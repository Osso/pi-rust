//! Bash execution with streaming and truncation (`core/bash-executor.ts`).

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use pi_agent_core::agent::ToolError;
use tokio_util::sync::CancellationToken;

use crate::core::tools::bash::{BashExecOptions, BashOperations};
use crate::core::tools::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_tail};
use crate::utils::ansi::strip_ansi;
use crate::utils::shell::sanitize_binary_output;

/// Streaming chunk callback.
pub type BashChunkCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// `BashExecutorOptions`.
#[derive(Clone, Default)]
pub struct BashExecutorOptions {
    /// Callback for streaming output chunks (already sanitized).
    pub on_chunk: Option<BashChunkCallback>,
    /// Cancellation token mapped from `AbortSignal`.
    pub signal: Option<CancellationToken>,
}

/// `BashResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BashResult {
    /// Combined stdout + stderr output (sanitized, possibly truncated).
    pub output: String,
    /// Process exit code; `None` when killed or cancelled.
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub truncated: bool,
    /// Temp file containing the full output when it exceeded the truncation threshold.
    pub full_output_path: Option<String>,
}

/// `executeBashWithOperations`.
pub async fn execute_bash_with_operations(
    command: &str,
    cwd: &str,
    operations: Arc<dyn BashOperations>,
    options: &BashExecutorOptions,
) -> Result<BashResult, ToolError> {
    let collector = OutputCollector::new(options.on_chunk.clone());
    let on_data: crate::core::tools::bash::BashDataCallback = {
        let collector = collector.clone();
        Arc::new(move |data: &[u8]| collector.push(data))
    };
    let result = operations
        .exec(
            command,
            cwd,
            BashExecOptions {
                on_data,
                signal: options.signal.clone(),
                timeout: None,
                env: crate::utils::shell::get_shell_env(),
                detach: None,
            },
        )
        .await;
    let cancelled = options.signal.as_ref().is_some_and(CancellationToken::is_cancelled);
    match result {
        Ok(result) => Ok(collector.finish(cancelled, result.exit_code)),
        Err(error) => {
            if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Ok(collector.finish(true, None));
            }
            Err(error)
        }
    }
}

#[derive(Clone)]
struct OutputCollector {
    state: Arc<std::sync::Mutex<CollectorState>>,
}

struct CollectorState {
    on_chunk: Option<BashChunkCallback>,
    output_chunks: Vec<String>,
    output_bytes: usize,
    total_bytes: usize,
    temp_file_path: Option<PathBuf>,
    temp_file: Option<File>,
    decoder: StreamingUtf8Decoder,
}

impl OutputCollector {
    fn new(on_chunk: Option<BashChunkCallback>) -> Self {
        Self {
            state: Arc::new(std::sync::Mutex::new(CollectorState {
                on_chunk,
                output_chunks: Vec::new(),
                output_bytes: 0,
                total_bytes: 0,
                temp_file_path: None,
                temp_file: None,
                decoder: StreamingUtf8Decoder::default(),
            })),
        }
    }

    fn push(&self, data: &[u8]) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.total_bytes += data.len();
        let decoded = state.decoder.decode(data);
        let text = sanitize_binary_output(&strip_ansi(&decoded)).replace('\r', "");
        if state.total_bytes > DEFAULT_MAX_BYTES {
            state.ensure_temp_file();
        }
        if let Some(file) = state.temp_file.as_mut() {
            let _ = file.write_all(text.as_bytes());
        }
        state.output_bytes += pi_js_compat::string::utf16_len(&text);
        state.output_chunks.push(text.clone());
        while state.output_bytes > DEFAULT_MAX_BYTES * 2 && state.output_chunks.len() > 1 {
            let removed = state.output_chunks.remove(0);
            state.output_bytes -= pi_js_compat::string::utf16_len(&removed);
        }
        if let Some(on_chunk) = &state.on_chunk {
            on_chunk(&text);
        }
    }

    fn finish(&self, cancelled: bool, exit_code: Option<i32>) -> BashResult {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let full_output: String = state.output_chunks.concat();
        let truncation = truncate_tail(&full_output, TruncationOptions::default());
        if truncation.truncated {
            state.ensure_temp_file();
        }
        state.temp_file.take();
        BashResult {
            output: if truncation.truncated {
                truncation.content
            } else {
                full_output
            },
            exit_code: if cancelled { None } else { exit_code },
            cancelled,
            truncated: truncation.truncated,
            full_output_path: state
                .temp_file_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
        }
    }
}

impl CollectorState {
    fn ensure_temp_file(&mut self) {
        if self.temp_file_path.is_some() {
            return;
        }
        let id = random_hex_8();
        let path = std::env::temp_dir().join(format!("pi-bash-{id}.log"));
        let mut file = File::create(&path).expect("bash output temp file");
        for chunk in &self.output_chunks {
            let _ = file.write_all(chunk.as_bytes());
        }
        self.temp_file_path = Some(path);
        self.temp_file = Some(file);
    }
}

fn random_hex_8() -> String {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes).expect("OS random source must be available");
    let mut hex = String::with_capacity(16);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Streaming UTF-8 decoder matching `TextDecoder.decode(data, { stream: true })`.
#[derive(Default)]
struct StreamingUtf8Decoder {
    pending: Vec<u8>,
}

impl StreamingUtf8Decoder {
    fn decode(&mut self, data: &[u8]) -> String {
        let mut combined = std::mem::take(&mut self.pending);
        combined.extend_from_slice(data);
        let mut output = String::new();
        let mut remaining = combined.as_slice();
        loop {
            match std::str::from_utf8(remaining) {
                Ok(valid) => {
                    output.push_str(valid);
                    break;
                }
                Err(error) => {
                    let valid_up_to = error.valid_up_to();
                    output.push_str(std::str::from_utf8(&remaining[..valid_up_to]).expect("valid prefix"));
                    match error.error_len() {
                        Some(length) => {
                            output.push('\u{fffd}');
                            remaining = &remaining[valid_up_to + length..];
                        }
                        None => {
                            self.pending = remaining[valid_up_to..].to_vec();
                            break;
                        }
                    }
                }
            }
        }
        output
    }
}
