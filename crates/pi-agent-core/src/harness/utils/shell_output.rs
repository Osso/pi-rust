//! Frozen shell capture: raw-byte spill threshold and whole-chunk UTF-16 retention.

use indexmap::IndexMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use super::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_tail};
use crate::harness::types::{
    CreateTempFileOptions, ExecutionEnv, ExecutionError, ExecutionErrorCode, FileContent, FileError, ShellExecOptions,
};

pub type ShellChunkHandler = dyn Fn(&JsString) -> Result<(), ExecutionError> + Send + Sync;

#[derive(Clone, Default)]
pub struct ShellCaptureOptions {
    pub cwd: Option<JsString>,
    pub env: Option<IndexMap<String, JsString>>,
    pub timeout: Option<f64>,
    pub abort: Option<CancellationToken>,
    pub on_chunk: Option<Arc<ShellChunkHandler>>,
}

#[derive(Debug)]
pub struct ShellCaptureResult {
    pub output: JsString,
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub truncated: bool,
    pub full_output_path: Option<JsString>,
}

/// Filters frozen control ranges without changing any other UTF-16 units.
pub fn sanitize_binary_output(input: impl Into<JsString>) -> JsString {
    let input = input.into();
    JsString::from_utf16(
        input
            .units()
            .iter()
            .copied()
            .filter(|unit| matches!(unit, 0x09 | 0x0a | 0x0d) || !(*unit <= 0x1f || (0xfff9..=0xfffb).contains(unit)))
            .collect(),
    )
}

#[derive(Default)]
struct CaptureState {
    chunks: VecDeque<JsString>,
    retained_units: usize,
    raw_bytes: usize,
    full_output_requested: bool,
    capture_error: Option<ExecutionError>,
}

impl CaptureState {
    fn output(&self) -> JsString {
        JsString::join(&self.chunks, "")
    }

    fn retain_chunk(&mut self, text: JsString) {
        self.retained_units += text.len();
        self.chunks.push_back(text);
        // DISCREPANCY: frozen calls this outputBytes but counts UTF-16 units, not bytes.
        while self.retained_units > DEFAULT_MAX_BYTES * 2 && self.chunks.len() > 1 {
            self.retained_units -= self.chunks.pop_front().expect("nonempty chunks").len();
        }
    }
}

enum WriteOperation {
    Create(JsString),
    Append(JsString),
}

/// Executes through the environment; waits for all ordered full-output writes before returning.
pub async fn execute_shell_with_capture(
    env: &Arc<dyn ExecutionEnv>,
    command: &JsString,
    options: ShellCaptureOptions,
) -> Result<ShellCaptureResult, ExecutionError> {
    let state = Arc::new(Mutex::new(CaptureState::default()));
    let (sender, receiver) = mpsc::unbounded_channel();
    let writer = tokio::spawn(write_full_output(Arc::clone(env), receiver, options.abort.clone()));
    let chunk_handler = capture_chunk_handler(Arc::clone(&state), sender.clone(), options.on_chunk.clone());
    let result = env
        .exec(
            command,
            ShellExecOptions {
                cwd: options.cwd,
                env: options.env,
                timeout: options.timeout,
                abort: options.abort.clone(),
                on_stdout: Some(Arc::clone(&chunk_handler)),
                on_stderr: Some(chunk_handler),
            },
        )
        .await;
    let (output, truncated, capture_error) = finalize_capture(&state, &sender);
    drop(sender);
    let path = writer.await.map_err(|error| {
        ExecutionError::new(
            ExecutionErrorCode::Unknown,
            format!("Full output writer failed: {error}"),
            None,
        )
    })??;
    if let Some(error) = capture_error {
        return Err(error);
    }
    let aborted = options.abort.as_ref().is_some_and(CancellationToken::is_cancelled);
    match result {
        Ok(result) => Ok(ShellCaptureResult {
            output,
            exit_code: (!aborted).then_some(result.exit_code),
            cancelled: aborted,
            truncated,
            full_output_path: path,
        }),
        Err(error) if error.code == ExecutionErrorCode::Aborted || aborted => Ok(ShellCaptureResult {
            output,
            exit_code: None,
            cancelled: true,
            truncated,
            full_output_path: path,
        }),
        Err(error) => Err(error),
    }
}

fn capture_chunk_handler(
    state: Arc<Mutex<CaptureState>>,
    sender: UnboundedSender<WriteOperation>,
    on_chunk: Option<Arc<ShellChunkHandler>>,
) -> Arc<crate::harness::types::ShellOutputHandler> {
    Arc::new(move |chunk| {
        let text = sanitize_binary_output(chunk).replace("\r", "");
        {
            let mut state = state.lock().expect("capture state lock");
            state.raw_bytes += encode_node_utf8(&JsString::from(chunk)).len();
            if state.capture_error.is_none() {
                let operation = if state.raw_bytes > DEFAULT_MAX_BYTES && !state.full_output_requested {
                    state.full_output_requested = true;
                    Some(WriteOperation::Create(state.output() + &text))
                } else if state.full_output_requested {
                    Some(WriteOperation::Append(text.clone()))
                } else {
                    None
                };
                send_write(&mut state, &sender, operation);
            }
            state.retain_chunk(text.clone());
        }
        if let Some(callback) = &on_chunk
            && let Err(error) = callback(&text)
        {
            state.lock().expect("capture state lock").capture_error = Some(error);
        }
        Ok(())
    })
}

fn send_write(state: &mut CaptureState, sender: &UnboundedSender<WriteOperation>, operation: Option<WriteOperation>) {
    if let Some(operation) = operation
        && sender.send(operation).is_err()
    {
        state.capture_error = Some(ExecutionError::new(
            ExecutionErrorCode::Unknown,
            "Full output writer stopped",
            None,
        ));
    }
}

fn finalize_capture(
    state: &Arc<Mutex<CaptureState>>,
    sender: &UnboundedSender<WriteOperation>,
) -> (JsString, bool, Option<ExecutionError>) {
    let mut state = state.lock().expect("capture state lock");
    let tail = state.output();
    let truncation = truncate_tail(&tail, TruncationOptions::default());
    if truncation.truncated && !state.full_output_requested && state.capture_error.is_none() {
        state.full_output_requested = true;
        send_write(&mut state, sender, Some(WriteOperation::Create(tail.clone())));
    }
    let output = if truncation.truncated { truncation.content } else { tail };
    (
        output,
        truncation.truncated || state.full_output_requested,
        state.capture_error.take(),
    )
}

async fn write_full_output(
    env: Arc<dyn ExecutionEnv>,
    mut receiver: UnboundedReceiver<WriteOperation>,
    abort: Option<CancellationToken>,
) -> Result<Option<JsString>, ExecutionError> {
    let mut path = None;
    while let Some(operation) = receiver.recv().await {
        let text = match operation {
            WriteOperation::Create(text) => {
                path = Some(
                    env.create_temp_file(CreateTempFileOptions {
                        prefix: Some("bash-".into()),
                        suffix: Some(".log".into()),
                        abort: abort.clone(),
                    })
                    .await
                    .map_err(to_execution_error)?,
                );
                text
            }
            WriteOperation::Append(text) => text,
        };
        env.append_file(
            path.as_ref().expect("create precedes append"),
            FileContent::Text(&text),
            abort.as_ref(),
        )
        .await
        .map_err(to_execution_error)?;
    }
    Ok(path)
}

fn to_execution_error(error: FileError) -> ExecutionError {
    ExecutionError::new(
        ExecutionErrorCode::Unknown,
        error.message.clone(),
        Some(Box::new(error)),
    )
}
