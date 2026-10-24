//! Bash execution with streaming and truncation (`core/bash-executor.ts`).

use crate::utils::paths::node_join;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use pi_agent_core::agent::ToolError;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::text_decoder::Utf8StreamDecoder;

use crate::core::tools::bash::{BashExecOptions, BashOperations};
use crate::core::tools::truncate::{DEFAULT_MAX_BYTES, TruncationOptions, truncate_tail};
use crate::utils::ansi::strip_ansi;
use crate::utils::shell::sanitize_binary_output;

/// Streaming chunk callback.
pub type BashChunkCallback = Arc<dyn Fn(&JsString) + Send + Sync>;

/// `BashExecutorOptions`.
#[derive(Clone, Default)]
pub struct BashExecutorOptions {
    /// Callback for streaming output chunks (already sanitized).
    pub on_chunk: Option<BashChunkCallback>,
    /// Cancellation token mapped from `AbortSignal`.
    pub signal: Option<AbortSignal>,
}

/// `BashResult`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BashResult {
    /// Combined stdout + stderr output (sanitized, possibly truncated).
    pub output: JsString,
    /// JavaScript-number exit code; `None` when killed or cancelled.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub exit_code: Option<f64>,
    pub cancelled: bool,
    pub truncated: bool,
    /// Temp file containing the full output when it exceeded the truncation threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_output_path: Option<JsString>,
}

/// `executeBashWithOperations`.
pub async fn execute_bash_with_operations(
    command: &JsString,
    cwd: &JsString,
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
                env: None,
                detach: None,
            },
        )
        .await;
    let cancelled = options.signal.as_ref().is_some_and(AbortSignal::aborted);
    match result {
        Ok(result) => Ok(collector.finish(cancelled, result.exit_code)),
        Err(error) => {
            if options.signal.as_ref().is_some_and(AbortSignal::aborted) {
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
    output_chunks: Vec<JsString>,
    output_bytes: usize,
    total_bytes: usize,
    temp_file_path: Option<JsString>,
    temp_file: Option<File>,
    decoder: Utf8StreamDecoder,
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
                decoder: Utf8StreamDecoder::new(),
            })),
        }
    }

    fn push(&self, data: &[u8]) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.total_bytes += data.len();
        let decoded = state.decoder.decode(data);
        let text = sanitize_binary_output(&strip_ansi(&JsString::from(decoded))).replace("\r", "");
        if state.total_bytes > DEFAULT_MAX_BYTES {
            state.ensure_temp_file();
        }
        if let Some(file) = state.temp_file.as_mut() {
            let _ = file.write_all(encode_node_utf8(&text).as_bytes());
        }
        state.output_bytes += text.len();
        state.output_chunks.push(text.clone());
        while state.output_bytes > DEFAULT_MAX_BYTES * 2 && state.output_chunks.len() > 1 {
            let removed = state.output_chunks.remove(0);
            state.output_bytes -= removed.len();
        }
        if let Some(on_chunk) = &state.on_chunk {
            on_chunk(&text);
        }
    }

    fn finish(&self, cancelled: bool, exit_code: Option<f64>) -> BashResult {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let full_output = JsString::join(&state.output_chunks, "");
        let truncation = truncate_tail(full_output.clone(), TruncationOptions::default());
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
            full_output_path: state.temp_file_path.clone(),
        }
    }
}

impl CollectorState {
    fn ensure_temp_file(&mut self) {
        if self.temp_file_path.is_some() {
            return;
        }
        let id = random_hex_8();
        let path = node_join(
            std::env::temp_dir().to_string_lossy().into_owned(),
            format!("pi-bash-{id}.log"),
        );
        let mut file = File::create(encode_node_utf8(&path)).expect("bash output temp file");
        for chunk in &self.output_chunks {
            let _ = file.write_all(encode_node_utf8(chunk).as_bytes());
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
