//! Incrementally tracks streaming output with bounded memory.

use crate::utils::paths::node_join;
use crate::utils::uncaught::{PendingNativeError, defer_native_error};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use pi_js_compat::number::{js_math_max, js_slice_index};
use std::fs::File;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use super::truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, TruncationOptions, TruncationResult, truncate_tail,
};

const DEFAULT_TEMP_FILE_PREFIX: &str = "pi-output";
use pi_js_compat::text_decoder::Utf8StreamDecoder;

/// Limits and temp-file naming for [`OutputAccumulator`].
#[derive(Debug, Clone, PartialEq)]
pub struct OutputAccumulatorOptions {
    pub max_lines: f64,
    pub max_bytes: f64,
    pub temp_file_prefix: JsString,
}

impl Default for OutputAccumulatorOptions {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MAX_LINES as f64,
            max_bytes: DEFAULT_MAX_BYTES as f64,
            temp_file_prefix: DEFAULT_TEMP_FILE_PREFIX.into(),
        }
    }
}

/// One display snapshot of the accumulated output.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSnapshot {
    pub content: JsString,
    pub truncation: TruncationResult,
    pub full_output_path: Option<JsString>,
}

/// Snapshot options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SnapshotOptions {
    pub persist_if_truncated: bool,
}

/// Incrementally tracks streaming output, keeping only a decoded tail for display.
#[derive(Debug)]
pub struct OutputAccumulator {
    max_lines: f64,
    max_bytes: f64,
    max_rolling_bytes: f64,
    decoder: Utf8StreamDecoder,
    raw_chunks: Vec<Vec<u8>>,
    tail_text: JsString,
    tail_bytes: usize,
    tail_starts_at_line_boundary: bool,
    total_raw_bytes: usize,
    total_decoded_bytes: usize,
    completed_lines: usize,
    total_lines: usize,
    current_line_bytes: usize,
    has_open_line: bool,
    finished: bool,
    temp_file_prefix: JsString,
    configured_temp_file_path: Option<JsString>,
    temp_file_path: Option<JsString>,
    temp_file: Option<File>,
    temp_file_stream_exists: bool,
    pending_error: PendingNativeError,
    runtime: Option<tokio::runtime::Handle>,
}

impl OutputAccumulator {
    /// Creates an accumulator with the source defaults for unset fields.
    pub fn new(options: OutputAccumulatorOptions) -> Self {
        Self {
            max_lines: options.max_lines,
            max_bytes: options.max_bytes,
            max_rolling_bytes: js_math_max(options.max_bytes * 2.0, 1.0),
            decoder: Utf8StreamDecoder::new(),
            raw_chunks: Vec::new(),
            tail_text: JsString::default(),
            tail_bytes: 0,
            tail_starts_at_line_boundary: true,
            total_raw_bytes: 0,
            total_decoded_bytes: 0,
            completed_lines: 0,
            total_lines: 0,
            current_line_bytes: 0,
            has_open_line: false,
            finished: false,
            temp_file_prefix: options.temp_file_prefix,
            configured_temp_file_path: None,
            temp_file_path: None,
            temp_file: None,
            temp_file_stream_exists: false,
            pending_error: Arc::new(Mutex::new(None)),
            runtime: tokio::runtime::Handle::try_current().ok(),
        }
    }

    /// Creates an accumulator that persists full output to `temp_file_path`.
    pub fn new_with_temp_file_path(options: OutputAccumulatorOptions, temp_file_path: impl Into<JsString>) -> Self {
        let mut output = Self::new(options);
        output.configured_temp_file_path = Some(temp_file_path.into());
        output
    }

    /// Appends one raw chunk, decoding it as streaming UTF-8.
    ///
    /// Returns the original synchronous source error; stream failures are deferred.
    pub fn append(&mut self, data: &[u8]) -> Result<(), NativeNodeError> {
        if self.finished {
            return Err(NativeNodeError::from_constructor(
                NativeErrorClass::Error,
                "Cannot append to a finished output accumulator",
            ));
        }

        self.total_raw_bytes += data.len();
        let decoded = self.decoder.decode(data);
        self.append_decoded_text(JsString::from(decoded));

        if self.temp_file.is_some() || self.should_use_temp_file() {
            self.ensure_temp_file()?;
            self.write_to_temp_file(data);
        } else if !data.is_empty() {
            self.raw_chunks.push(data.to_vec());
        }
        Ok(())
    }

    /// Flushes pending decoder state and stops accepting chunks.
    pub fn finish(&mut self) -> Result<(), NativeNodeError> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let decoded = self.decoder.finish();
        self.append_decoded_text(JsString::from(decoded));
        if self.should_use_temp_file() {
            self.ensure_temp_file()?;
        }
        Ok(())
    }

    /// Returns the current display snapshot.
    pub fn snapshot(&mut self, options: SnapshotOptions) -> Result<OutputSnapshot, NativeNodeError> {
        let tail_truncation = truncate_tail(
            self.snapshot_text(),
            TruncationOptions {
                max_lines: self.max_lines,
                max_bytes: self.max_bytes,
            },
        );
        let truncated = self.total_lines as f64 > self.max_lines || self.total_decoded_bytes as f64 > self.max_bytes;
        let truncated_by = truncated.then(|| {
            tail_truncation
                .truncated_by
                .unwrap_or(if self.total_decoded_bytes as f64 > self.max_bytes {
                    TruncatedBy::Bytes
                } else {
                    TruncatedBy::Lines
                })
        });
        let truncation = TruncationResult {
            truncated,
            truncated_by,
            total_lines: self.total_lines,
            total_bytes: self.total_decoded_bytes,
            max_lines: self.max_lines,
            max_bytes: self.max_bytes,
            ..tail_truncation
        };

        if options.persist_if_truncated && truncation.truncated {
            self.ensure_temp_file()?;
        }

        Ok(OutputSnapshot {
            content: truncation.content.clone(),
            truncation,
            full_output_path: self.temp_file_path.clone(),
        })
    }

    /// Flushes and closes the temp file, if one is open.
    pub fn close_temp_file(&mut self) -> Result<(), Box<NativeNodeError>> {
        if !self.temp_file_stream_exists {
            return Ok(());
        }
        self.temp_file_stream_exists = false;
        let error = self
            .pending_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        // The stream's finish removes its error listener before descriptor close.
        if let Some(file) = self.temp_file.take()
            && let Err(error) = nix::unistd::close(file).map_err(io::Error::from)
        {
            self.defer_stream_error(NativeNodeError::from(pi_js_compat::node_fs::node_fs_error_record(
                &error,
                "close",
                &JsString::default(),
            )));
        }
        error.map_or(Ok(()), Err)
    }

    /// Returns the decoded byte length of the current, not yet terminated line.
    pub fn last_line_bytes(&self) -> usize {
        self.current_line_bytes
    }

    fn append_decoded_text(&mut self, text: JsString) {
        if text.is_empty() {
            return;
        }

        let bytes = encode_node_utf8(&text).len();
        self.total_decoded_bytes += bytes;
        self.tail_text.push_str(&text);
        self.tail_bytes += bytes;
        if self.tail_bytes as f64 > self.max_rolling_bytes * 2.0 {
            self.trim_tail();
        }

        let newlines = text.units().iter().filter(|unit| **unit == 0x0a).count();
        match text.units().iter().rposition(|unit| *unit == 0x0a) {
            None => {
                self.current_line_bytes += bytes;
                self.has_open_line = true;
            }
            Some(last_newline) => {
                self.completed_lines += newlines;
                let tail = text.slice(last_newline + 1, text.len());
                self.current_line_bytes = encode_node_utf8(&tail).len();
                self.has_open_line = !tail.is_empty();
            }
        }
        self.total_lines = self.completed_lines + usize::from(self.has_open_line);
    }

    fn trim_tail(&mut self) {
        let encoded = encode_node_utf8(&self.tail_text);
        let buffer = encoded.as_bytes();
        if buffer.len() as f64 <= self.max_rolling_bytes {
            self.tail_bytes = buffer.len();
            return;
        }

        let mut start = buffer.len() as f64 - self.max_rolling_bytes;
        while start < buffer.len() as f64 && start.fract() == 0.0 && (buffer[start as usize] & 0xC0) == 0x80 {
            start += 1.0;
        }

        if start > 0.0 {
            self.tail_starts_at_line_boundary = start.fract() == 0.0 && buffer[start as usize - 1] == b'\n';
        }
        let offset = js_slice_index(start, buffer.len());
        self.tail_text = String::from_utf8_lossy(&buffer[offset..]).into_owned().into();
        self.tail_bytes = encode_node_utf8(&self.tail_text).len();
    }

    fn snapshot_text(&self) -> JsString {
        if self.tail_starts_at_line_boundary {
            return self.tail_text.clone();
        }

        match self.tail_text.find('\n') {
            Some(first_newline) => self.tail_text.slice(first_newline + 1, self.tail_text.len()),
            None => self.tail_text.clone(),
        }
    }

    fn should_use_temp_file(&self) -> bool {
        self.total_raw_bytes as f64 > self.max_bytes
            || self.total_decoded_bytes as f64 > self.max_bytes
            || self.total_lines as f64 > self.max_lines
    }

    fn ensure_temp_file(&mut self) -> Result<(), NativeNodeError> {
        if self.temp_file_path.is_some() {
            return Ok(());
        }
        let path = self
            .configured_temp_file_path
            .clone()
            .unwrap_or_else(|| default_temp_file_path(&self.temp_file_prefix));
        self.temp_file_path = Some(path.clone());
        if let Some(error) = pi_js_compat::node_fs::node_path_argument_error(&path) {
            return Err(NativeNodeError::from(error));
        }
        self.temp_file_stream_exists = true;
        match File::create(encode_node_utf8(&path)) {
            Ok(file) => self.temp_file = Some(file),
            Err(error) => {
                self.defer_stream_error(NativeNodeError::from(pi_js_compat::node_fs::node_fs_error_record(
                    &error, "open", &path,
                )));
                return Ok(());
            }
        }
        let chunks = std::mem::take(&mut self.raw_chunks);
        for chunk in chunks {
            self.write_to_temp_file(&chunk);
        }
        Ok(())
    }

    fn write_to_temp_file(&mut self, data: &[u8]) {
        let error = self.temp_file.as_mut().and_then(|file| file.write_all(data).err());
        if let Some(error) = error {
            self.defer_stream_error(NativeNodeError::from(
                pi_js_compat::node_fs::node_file_write_error_record(&error),
            ));
        }
    }

    fn defer_stream_error(&mut self, mut error: NativeNodeError) {
        if let Some(file) = self.temp_file.take()
            && let Err(close) = nix::unistd::close(file).map_err(io::Error::from)
        {
            error = pi_js_compat::node_fs::node_fs_error_record(&close, "close", &JsString::default()).into();
        }
        *self
            .pending_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new(error));
        let runtime = self
            .runtime
            .as_ref()
            .expect("stream error delivery requires the owning Tokio runtime");
        defer_native_error(self.pending_error.clone(), runtime);
    }
}

fn default_temp_file_path(prefix: &JsString) -> JsString {
    let id = generate_random_id();
    node_join(
        pi_js_compat::node_os::tmpdir(),
        js_format!("{}-{}.log", prefix, format!("{id:016x}")),
    )
}

fn generate_random_id() -> u64 {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS random source must be available");
    u64::from_be_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_trailing_bytes_are_buffered() {
        let mut decoder = Utf8StreamDecoder::new();
        assert_eq!(decoder.decode(&[0xE2]), "");
        assert_eq!(decoder.decode(&[0x82]), "");
        assert_eq!(decoder.decode(&[0xAC]), "€");
        assert_eq!(decoder.finish(), "");
    }

    #[test]
    fn invalid_sequences_reduce_errors_per_byte() {
        let mut decoder = Utf8StreamDecoder::new();
        assert_eq!(decoder.decode(&[0xFF, 0x61]), "\u{FFFD}a");
        assert_eq!(decoder.decode(&[0xE2, 0x28, 0xA1]), "\u{FFFD}(\u{FFFD}");
    }
}
