use std::io;
use std::mem;
use std::str;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pi_js_compat::callback_error::CallbackError;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::task::JoinHandle;

const READ_CHUNK_BYTES: usize = 64 * 1024;

/// Serializes one compact JSON value followed by exactly one line feed.
pub fn serialize_json_line<T>(value: &T) -> Result<String, serde_json::Error>
where
    T: Serialize + ?Sized,
{
    let mut line = serde_json::to_string(value)?;
    line.push('\n');
    Ok(line)
}

/// Incrementally decodes UTF-8 bytes and emits LF-delimited records.
pub struct JsonlLineDecoder {
    buffer: String,
    pending_utf8: Vec<u8>,
}

impl Default for JsonlLineDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonlLineDecoder {
    /// Creates an empty line decoder.
    pub const fn new() -> Self {
        Self {
            buffer: String::new(),
            pending_utf8: Vec::new(),
        }
    }

    /// Accepts a byte chunk using Node `StringDecoder("utf8")` replacement semantics.
    pub fn push_bytes(&mut self, chunk: &[u8]) -> Vec<String> {
        let decoded = self.decode_bytes(chunk);
        self.push_decoded(&decoded)
    }

    /// Accepts an already-decoded string chunk without changing pending byte-decoder state.
    pub fn push_str(&mut self, chunk: &str) -> Vec<String> {
        self.push_decoded(chunk)
    }

    /// Flushes incomplete UTF-8 and emits one final non-empty unterminated record.
    pub fn end(mut self) -> Vec<String> {
        if !self.pending_utf8.is_empty() {
            self.buffer.push(char::REPLACEMENT_CHARACTER);
            self.pending_utf8.clear();
        }

        let mut lines = self.take_complete_lines();
        if !self.buffer.is_empty() {
            lines.push(strip_one_carriage_return(mem::take(&mut self.buffer)));
        }
        lines
    }

    fn decode_bytes(&mut self, chunk: &[u8]) -> String {
        let mut bytes = mem::take(&mut self.pending_utf8);
        bytes.extend_from_slice(chunk);

        let mut decoded = String::new();
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() {
            match str::from_utf8(remaining) {
                Ok(valid) => {
                    decoded.push_str(valid);
                    break;
                }
                Err(error) => {
                    let valid_length = error.valid_up_to();
                    let valid = str::from_utf8(&remaining[..valid_length]).expect("UTF-8 error prefix must be valid");
                    decoded.push_str(valid);

                    let Some(invalid_length) = error.error_len() else {
                        self.pending_utf8.extend_from_slice(&remaining[valid_length..]);
                        break;
                    };
                    decoded.push(char::REPLACEMENT_CHARACTER);
                    remaining = &remaining[valid_length + invalid_length..];
                }
            }
        }
        decoded
    }

    fn push_decoded(&mut self, decoded: &str) -> Vec<String> {
        self.buffer.push_str(decoded);
        self.take_complete_lines()
    }

    fn take_complete_lines(&mut self) -> Vec<String> {
        let Some(last_line_feed) = self.buffer.rfind('\n') else {
            return Vec::new();
        };

        let remainder = self.buffer[last_line_feed + 1..].to_owned();
        let complete_records = self.buffer[..last_line_feed].to_owned();
        self.buffer = remainder;
        complete_records
            .split('\n')
            .map(str::to_owned)
            .map(strip_one_carriage_return)
            .collect()
    }
}

/// Background LF-only JSONL reader returned by [`attach_jsonl_line_reader`].
pub struct JsonlLineReader {
    detached: Arc<AtomicBool>,
    task: JoinHandle<io::Result<()>>,
}

impl JsonlLineReader {
    /// Removes delivery like Node's data/end listeners without destroying the live pipe.
    pub fn detach(&self) {
        self.detached.store(true, Ordering::SeqCst);
    }

    /// Waits for stream end; detach disables delivery but still drains the live pipe.
    pub async fn finished(self) -> io::Result<()> {
        match self.task.await {
            Ok(result) => result,
            Err(error) if error.is_cancelled() => Ok(()),
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }
}

/// Reads LF-delimited records from `stream` on a Tokio task and passes each one to `on_line`.
///
/// Unlike line readers that also split on U+2028/U+2029, framing is strict JSONL.
pub fn attach_jsonl_line_reader<R, F>(stream: R, on_line: F) -> JsonlLineReader
where
    R: AsyncRead + Unpin + Send + 'static,
    F: FnMut(String) -> Result<(), CallbackError> + Send + 'static,
{
    let detached = Arc::new(AtomicBool::new(false));
    let task = tokio::spawn(read_lines(stream, on_line, Arc::clone(&detached)));
    JsonlLineReader { detached, task }
}

async fn read_lines<R, F>(mut stream: R, mut on_line: F, detached: Arc<AtomicBool>) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    F: FnMut(String) -> Result<(), CallbackError>,
{
    let mut decoder = JsonlLineDecoder::new();
    let mut chunk = vec![0; READ_CHUNK_BYTES];
    loop {
        let length = stream.read(&mut chunk).await.unwrap_or_else(|error| {
            crate::utils::uncaught::throw_native_uncaught(
                pi_js_compat::node_fs::node_stream_error_record(&error, "read").into(),
            )
        });
        if detached.load(Ordering::SeqCst) {
            if length == 0 {
                return Ok(());
            }
            continue;
        }
        let lines = if length == 0 {
            mem::take(&mut decoder).end()
        } else {
            decoder.push_bytes(&chunk[..length])
        };
        for line in lines {
            on_line(line).unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        }
        if length == 0 {
            return Ok(());
        }
    }
}

fn strip_one_carriage_return(mut line: String) -> String {
    if line.ends_with('\r') {
        line.pop();
    }
    line
}
