//! Incrementally tracks streaming output with bounded memory.

use super::node_fs::node_fs_error;
use crate::utils::paths::node_join;
use crate::utils::uncaught::{ERROR, throw_uncaught};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::fs::File;
use std::io::{self, Write};

use super::truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncatedBy, TruncationOptions, TruncationResult, truncate_tail,
};

const DEFAULT_TEMP_FILE_PREFIX: &str = "pi-output";
const REPLACEMENT_CHARACTER: char = '\u{FFFD}';

/// Limits and temp-file naming for [`OutputAccumulator`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputAccumulatorOptions {
    pub max_lines: usize,
    pub max_bytes: usize,
    pub temp_file_prefix: JsString,
}

impl Default for OutputAccumulatorOptions {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MAX_LINES,
            max_bytes: DEFAULT_MAX_BYTES,
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
    max_lines: usize,
    max_bytes: usize,
    max_rolling_bytes: usize,
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
    configured_temp_file_path: JsString,
    temp_file_path: Option<JsString>,
    temp_file: Option<File>,
    close_file: fn(File) -> io::Result<()>,
}

impl OutputAccumulator {
    /// Creates an accumulator with the source defaults for unset fields.
    pub fn new(options: OutputAccumulatorOptions) -> Self {
        let temp_file_path = default_temp_file_path(&options.temp_file_prefix);
        Self::new_with_temp_file_path(options, temp_file_path)
    }

    /// Creates an accumulator that persists full output to `temp_file_path`.
    pub fn new_with_temp_file_path(options: OutputAccumulatorOptions, temp_file_path: impl Into<JsString>) -> Self {
        Self {
            max_lines: options.max_lines,
            max_bytes: options.max_bytes,
            max_rolling_bytes: (options.max_bytes * 2).max(1),
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
            configured_temp_file_path: temp_file_path.into(),
            temp_file_path: None,
            temp_file: None,
            close_file: |file| nix::unistd::close(file).map_err(io::Error::from),
        }
    }

    /// Appends one raw chunk, decoding it as streaming UTF-8.
    ///
    /// # Panics
    ///
    /// Panics when called after [`Self::finish`], matching the source error.
    pub fn append(&mut self, data: &[u8]) {
        assert!(!self.finished, "Cannot append to a finished output accumulator");

        self.total_raw_bytes += data.len();
        let decoded = self.decoder.decode(data);
        self.append_decoded_text(JsString::from(decoded));

        if self.temp_file.is_some() || self.should_use_temp_file() {
            self.ensure_temp_file();
            self.write_to_temp_file(data);
        } else if !data.is_empty() {
            self.raw_chunks.push(data.to_vec());
        }
    }

    /// Flushes pending decoder state and stops accepting chunks.
    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let decoded = self.decoder.finish();
        self.append_decoded_text(JsString::from(decoded));
        if self.should_use_temp_file() {
            self.ensure_temp_file();
        }
    }

    /// Returns the current display snapshot.
    pub fn snapshot(&mut self, options: SnapshotOptions) -> OutputSnapshot {
        let tail_truncation = truncate_tail(
            &self.snapshot_text(),
            TruncationOptions {
                max_lines: self.max_lines as f64,
                max_bytes: self.max_bytes as f64,
            },
        );
        let truncated = self.total_lines > self.max_lines || self.total_decoded_bytes > self.max_bytes;
        let truncated_by = truncated.then(|| {
            tail_truncation
                .truncated_by
                .unwrap_or(if self.total_decoded_bytes > self.max_bytes {
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
            max_lines: self.max_lines as f64,
            max_bytes: self.max_bytes as f64,
            ..tail_truncation
        };

        if options.persist_if_truncated && truncation.truncated {
            self.ensure_temp_file();
        }

        OutputSnapshot {
            content: truncation.content.clone(),
            truncation,
            full_output_path: self.temp_file_path.clone(),
        }
    }

    /// Flushes and closes the temp file, if one is open.
    pub fn close_temp_file(&mut self) -> io::Result<()> {
        let Some(mut file) = self.temp_file.take() else {
            return Ok(());
        };
        file.flush()?;
        (self.close_file)(file)
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
        if self.tail_bytes > self.max_rolling_bytes * 2 {
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
        if buffer.len() <= self.max_rolling_bytes {
            self.tail_bytes = buffer.len();
            return;
        }

        let mut start = buffer.len() - self.max_rolling_bytes;
        while start < buffer.len() && (buffer[start] & 0xC0) == 0x80 {
            start += 1;
        }

        if start > 0 {
            self.tail_starts_at_line_boundary = buffer[start - 1] == b'\n';
        }
        self.tail_text = String::from_utf8_lossy(&buffer[start..]).into_owned().into();
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
        self.total_raw_bytes > self.max_bytes
            || self.total_decoded_bytes > self.max_bytes
            || self.total_lines > self.max_lines
    }

    fn ensure_temp_file(&mut self) {
        if self.temp_file.is_some() {
            return;
        }
        let path = self.configured_temp_file_path.clone();
        let file = File::create(encode_node_utf8(&path))
            .unwrap_or_else(|error| throw_uncaught(ERROR, node_fs_error(&error, "open", &path)));
        self.temp_file = Some(file);
        self.temp_file_path = Some(path);
        let chunks = std::mem::take(&mut self.raw_chunks);
        for chunk in chunks {
            self.write_to_temp_file(&chunk);
        }
    }

    fn write_to_temp_file(&mut self, data: &[u8]) {
        if let Some(file) = self.temp_file.as_mut() {
            file.write_all(data)
                .unwrap_or_else(|error| throw_uncaught(ERROR, node_fs_error(&error, "write", &JsString::default())));
        }
    }
}

fn default_temp_file_path(prefix: &JsString) -> JsString {
    let id = unique_id();
    node_join(
        std::env::temp_dir().to_string_lossy().into_owned(),
        js_format!("{}-{}.log", prefix, format!("{id:016x}")),
    )
}

fn unique_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    nanos ^ COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Streaming UTF-8 decoder matching Node's `TextDecoder` with `{ stream: true }`.
#[derive(Debug, Default)]
struct Utf8StreamDecoder {
    pending: Vec<u8>,
    at_stream_start: bool,
}

impl Utf8StreamDecoder {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            at_stream_start: true,
        }
    }

    fn decode(&mut self, chunk: &[u8]) -> String {
        let mut bytes = std::mem::take(&mut self.pending);
        bytes.extend_from_slice(chunk);
        let mut decoded = String::new();
        let mut index = 0;
        while index < bytes.len() {
            let (text, consumed) = decode_scalar(&bytes[index..]);
            if consumed == 0 {
                self.pending.extend_from_slice(&bytes[index..]);
                break;
            }
            decoded.push_str(&text);
            index += consumed;
        }
        if self.at_stream_start && !decoded.is_empty() {
            self.at_stream_start = false;
            if decoded.starts_with('\u{FEFF}') {
                decoded.remove(0);
            }
        }
        decoded
    }

    fn finish(&mut self) -> String {
        let pending = std::mem::take(&mut self.pending);
        if pending.is_empty() {
            return String::new();
        }
        REPLACEMENT_CHARACTER.to_string()
    }
}

/// Decodes one scalar from the head of `bytes`.
///
/// Returns the text and the number of bytes consumed. Incomplete trailing
/// sequences consume nothing; invalid sequences consume one byte per error,
/// matching the WHATWG UTF-8 decoder.
fn decode_scalar(bytes: &[u8]) -> (String, usize) {
    let first = bytes[0];
    let (length, second_range) = match first {
        0x00..=0x7F => return ((first as char).to_string(), 1),
        0xC2..=0xDF => (2, 0x80..=0xBF),
        0xE0 => (3, 0xA0..=0xBF),
        0xE1..=0xEC | 0xEE..=0xEF => (3, 0x80..=0xBF),
        0xED => (3, 0x80..=0x9F),
        0xF0 => (4, 0x90..=0xBF),
        0xF1..=0xF3 => (4, 0x80..=0xBF),
        0xF4 => (4, 0x80..=0x8F),
        _ => return (REPLACEMENT_CHARACTER.to_string(), 1),
    };

    if bytes.len() == 1 {
        return (String::new(), 0);
    }
    if !second_range.contains(&bytes[1]) {
        return (REPLACEMENT_CHARACTER.to_string(), 1);
    }
    for index in 2..length {
        if bytes.len() <= index {
            return (String::new(), 0);
        }
        if !is_continuation(bytes[index]) {
            // One error consumes the lead byte and the continuations before the bad byte.
            return (REPLACEMENT_CHARACTER.to_string(), index);
        }
    }

    let mut value = u32::from(first & (0x7F >> length));
    for byte in &bytes[1..length] {
        value = (value << 6) | u32::from(byte & 0x3F);
    }
    match char::from_u32(value) {
        Some(character) => (character.to_string(), length),
        None => (REPLACEMENT_CHARACTER.to_string(), 1),
    }
}

fn is_continuation(byte: u8) -> bool {
    (byte & 0xC0) == 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_syscall_failure_is_returned_without_crashing() {
        let directory = tempfile::tempdir().unwrap();
        let path = JsString::from(directory.path().to_str().unwrap()) + "/spill.log";
        let mut output = OutputAccumulator::new_with_temp_file_path(
            OutputAccumulatorOptions {
                max_bytes: 1,
                ..OutputAccumulatorOptions::default()
            },
            &path,
        );
        output.append(b"payload");
        // A real close error requires an unreliable filesystem or invalid ownership.
        // Inject only the close syscall result; the real file has been written and closed.
        output.close_file = |file| {
            drop(file);
            Err(io::Error::from_raw_os_error(rustix::io::Errno::IO.raw_os_error()))
        };
        let error = output.close_temp_file().unwrap_err();
        assert_eq!(error.raw_os_error(), Some(rustix::io::Errno::IO.raw_os_error()));
        assert_eq!(std::fs::read(encode_node_utf8(&path)).unwrap(), b"payload");
    }

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
