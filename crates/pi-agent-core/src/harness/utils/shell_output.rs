use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use super::truncate::{DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncationResult, truncate_tail};
use crate::harness::types::{
    CreateTempFileOptions, ExecutionEnv, ExecutionError, ExecutionErrorCode, FileContent, FileError, ShellExecOptions,
};

const MAX_OUTPUT_BYTES: usize = DEFAULT_MAX_BYTES * 2;

/// Progress snapshot produced while shell output is being captured.
#[derive(Debug, Clone)]
pub struct ShellCaptureProgress {
    pub output: String,
    pub truncation: TruncationResult,
    pub full_output_path: Option<String>,
    pub last_line_bytes: usize,
}

/// Options for [`execute_shell_with_capture`].
#[derive(Clone, Default)]
pub struct ShellCaptureOptions {
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub inherit_env: Option<bool>,
    pub timeout: Option<f64>,
    pub abort: Option<CancellationToken>,
    /// Called with each sanitized chunk and the progress snapshot produced after accounting.
    pub on_chunk: Option<Arc<dyn Fn(&str, &ShellCaptureProgress) + Send + Sync>>,
    /// Return shell execution failures with captured output instead of as a failed `Result`.
    pub return_execution_errors: bool,
}

impl std::fmt::Debug for ShellCaptureOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellCaptureOptions")
            .field("cwd", &self.cwd)
            .field("env", &self.env)
            .field("inherit_env", &self.inherit_env)
            .field("timeout", &self.timeout)
            .field("abort", &self.abort)
            .field("on_chunk", &self.on_chunk.as_ref().map(|_| "<callback>"))
            .field("return_execution_errors", &self.return_execution_errors)
            .finish()
    }
}

/// Captured shell result with truncation metadata.
#[derive(Debug)]
pub struct ShellCaptureResult {
    pub output: String,
    pub truncation: TruncationResult,
    pub full_output_path: Option<String>,
    pub last_line_bytes: usize,
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub truncated: bool,
    pub execution_error: Option<ExecutionError>,
}

/// Removes binary control characters that should not appear in captured shell text.
pub fn sanitize_binary_output(input: &str) -> String {
    input.chars().filter(|character| keep_code_point(*character)).collect()
}

fn keep_code_point(character: char) -> bool {
    let code_point = u32::from(character);
    matches!(code_point, 0x09 | 0x0a | 0x0d) || !(code_point <= 0x1f || (0xfff9..=0xfffb).contains(&code_point))
}

/// Executes a command while sanitizing, truncating, and optionally persisting its output.
pub async fn execute_shell_with_capture(
    env: &Arc<dyn ExecutionEnv>,
    command: &str,
    options: ShellCaptureOptions,
) -> Result<ShellCaptureResult, ExecutionError> {
    let state = Arc::new(Mutex::new(CaptureState::default()));
    let (sender, receiver) = mpsc::unbounded_channel();
    let writer = tokio::spawn(write_full_output(Arc::clone(env), Arc::clone(&state), receiver));

    let on_chunk = options.on_chunk.clone();
    let callback_state = Arc::clone(&state);
    let callback_sender = sender.clone();
    let exec_options = ShellExecOptions {
        cwd: options.cwd.clone(),
        env: options.env.clone(),
        inherit_env: options.inherit_env,
        timeout: options.timeout,
        abort: options.abort.clone(),
        on_stdout: Some(Arc::new(move |chunk: &str| {
            Ok(account_chunk(
                &callback_state,
                chunk,
                &callback_sender,
                on_chunk.as_deref(),
            ))
        })),
        on_stderr: None,
    };
    let stderr_state = Arc::clone(&state);
    let stderr_sender = sender.clone();
    let stderr_on_chunk = options.on_chunk.clone();
    let exec_options = ShellExecOptions {
        on_stderr: Some(Arc::new(move |chunk: &str| {
            Ok(account_chunk(
                &stderr_state,
                chunk,
                &stderr_sender,
                stderr_on_chunk.as_deref(),
            ))
        })),
        ..exec_options
    };

    let result = env.exec(command, exec_options).await;
    {
        let mut state = state.lock().expect("capture state lock");
        state.accepting_output = false;
        let truncated = capture_truncation(&state).truncated;
        if truncated && !state.full_output_requested {
            state.full_output_requested = true;
            let _ = sender.send(WriteOperation::Create(state.tail_output.clone()));
        }
    }
    drop(sender);
    let write_result = writer.await.map_err(|error| {
        ExecutionError::new(
            ExecutionErrorCode::Unknown,
            format!("Full output writer failed: {error}"),
            None,
        )
    })?;
    write_result?;

    let progress = progress_snapshot(&state.lock().expect("capture state lock"));
    let truncated = progress.truncation.truncated;

    match result {
        Ok(result) => {
            let cancelled = options.abort.as_ref().is_some_and(CancellationToken::is_cancelled);
            Ok(ShellCaptureResult {
                output: progress.output,
                truncation: progress.truncation,
                full_output_path: progress.full_output_path,
                last_line_bytes: progress.last_line_bytes,
                exit_code: if cancelled { None } else { Some(result.exit_code) },
                cancelled,
                truncated,
                execution_error: None,
            })
        }
        Err(error) => {
            let aborted = error.code == ExecutionErrorCode::Aborted
                || options.abort.as_ref().is_some_and(CancellationToken::is_cancelled);
            if aborted {
                return Ok(ShellCaptureResult {
                    output: progress.output,
                    truncation: progress.truncation,
                    full_output_path: progress.full_output_path,
                    last_line_bytes: progress.last_line_bytes,
                    exit_code: None,
                    cancelled: true,
                    truncated,
                    execution_error: None,
                });
            }
            if options.return_execution_errors {
                return Ok(ShellCaptureResult {
                    output: progress.output,
                    truncation: progress.truncation,
                    full_output_path: progress.full_output_path,
                    last_line_bytes: progress.last_line_bytes,
                    exit_code: None,
                    cancelled: false,
                    truncated,
                    execution_error: Some(error),
                });
            }
            Err(error)
        }
    }
}

#[derive(Debug)]
struct CaptureState {
    tail_output: String,
    total_bytes: usize,
    completed_lines: usize,
    has_open_line: bool,
    current_line_bytes: usize,
    full_output_path: Option<String>,
    full_output_requested: bool,
    accepting_output: bool,
}

impl Default for CaptureState {
    fn default() -> Self {
        Self {
            tail_output: String::new(),
            total_bytes: 0,
            completed_lines: 0,
            has_open_line: false,
            current_line_bytes: 0,
            full_output_path: None,
            full_output_requested: false,
            accepting_output: true,
        }
    }
}

enum WriteOperation {
    Create(String),
    Append(String),
}

fn account_chunk(
    state: &Arc<Mutex<CaptureState>>,
    chunk: &str,
    sender: &UnboundedSender<WriteOperation>,
    on_chunk: Option<&(dyn Fn(&str, &ShellCaptureProgress) + Send + Sync)>,
) {
    let mut state = state.lock().expect("capture state lock");
    if !state.accepting_output {
        return;
    }
    let text = sanitize_binary_output(chunk).replace('\r', "");
    let text_bytes = text.len();
    state.total_bytes += text_bytes;
    state.completed_lines += text.matches('\n').count();
    if let Some(last_newline) = text.rfind('\n') {
        let trailing = &text[last_newline + 1..];
        state.current_line_bytes = trailing.len();
        state.has_open_line = !trailing.is_empty();
    } else if !text.is_empty() {
        state.current_line_bytes += text_bytes;
        state.has_open_line = true;
    }

    state.tail_output.push_str(&text);
    let total_lines = state.completed_lines + usize::from(state.has_open_line);
    if (state.total_bytes > DEFAULT_MAX_BYTES || total_lines > DEFAULT_MAX_LINES) && !state.full_output_requested {
        state.full_output_requested = true;
        let _ = sender.send(WriteOperation::Create(state.tail_output.clone()));
    } else if state.full_output_requested {
        let _ = sender.send(WriteOperation::Append(text.clone()));
    }
    state.tail_output = trim_to_last_utf8_bytes(&state.tail_output, MAX_OUTPUT_BYTES);
    let progress = progress_snapshot(&state);
    drop(state);
    if let Some(on_chunk) = on_chunk {
        on_chunk(&text, &progress);
    }
}

async fn write_full_output(
    env: Arc<dyn ExecutionEnv>,
    state: Arc<Mutex<CaptureState>>,
    mut receiver: UnboundedReceiver<WriteOperation>,
) -> Result<(), ExecutionError> {
    while let Some(operation) = receiver.recv().await {
        match operation {
            WriteOperation::Create(initial_content) => {
                let temp_file = env
                    .create_temp_file(CreateTempFileOptions {
                        prefix: Some("bash-".to_owned()),
                        suffix: Some(".log".to_owned()),
                    })
                    .await
                    .map_err(to_execution_error)?;
                state.lock().expect("capture state lock").full_output_path = Some(temp_file.clone());
                env.append_file(&temp_file, FileContent::Text(&initial_content))
                    .await
                    .map_err(to_execution_error)?;
            }
            WriteOperation::Append(text) => {
                let path = state
                    .lock()
                    .expect("capture state lock")
                    .full_output_path
                    .clone()
                    .ok_or_else(|| {
                        ExecutionError::new(ExecutionErrorCode::Unknown, "Full output path was not created", None)
                    })?;
                env.append_file(&path, FileContent::Text(&text))
                    .await
                    .map_err(to_execution_error)?;
            }
        }
    }
    Ok(())
}

fn to_execution_error(error: FileError) -> ExecutionError {
    let message = error.message.clone();
    ExecutionError::new(ExecutionErrorCode::Unknown, message, Some(Box::new(error)))
}

fn progress_snapshot(state: &CaptureState) -> ShellCaptureProgress {
    let truncation = capture_truncation(state);
    let output = if truncation.truncated {
        truncation.content.clone()
    } else {
        state.tail_output.clone()
    };
    ShellCaptureProgress {
        output,
        truncation,
        full_output_path: state.full_output_path.clone(),
        last_line_bytes: state.current_line_bytes,
    }
}

fn capture_truncation(state: &CaptureState) -> TruncationResult {
    let mut truncation = truncate_tail(&state.tail_output, Default::default());
    let total_lines = state.completed_lines + usize::from(state.has_open_line);
    let truncated = total_lines > DEFAULT_MAX_LINES || state.total_bytes > DEFAULT_MAX_BYTES;
    truncation.truncated = truncated;
    truncation.truncated_by = if truncated {
        truncation
            .truncated_by
            .or(Some(if state.total_bytes > DEFAULT_MAX_BYTES {
                super::TruncatedBy::Bytes
            } else {
                super::TruncatedBy::Lines
            }))
    } else {
        None
    };
    truncation.total_lines = total_lines;
    truncation.total_bytes = state.total_bytes;
    truncation
}

fn trim_to_last_utf8_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let mut start = text.len() - max_bytes;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_owned()
}
