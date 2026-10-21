//! Stdin buffering for partial escape sequences.
//!
//! Ported from `packages/tui/src/stdin-buffer.ts`. TypeScript schedules a
//! `setTimeout` internally; the Rust port exposes the pending deadline through
//! [`StdinBuffer::next_deadline`] and flushes via [`StdinBuffer::poll`] so the
//! terminal driver owns timer execution.

use std::time::{Duration, Instant};

use pi_js_compat::string::utf16_len;

const ESC: &str = "\x1b";
const BRACKETED_PASTE_START: &str = "\x1b[200~";
const BRACKETED_PASTE_END: &str = "\x1b[201~";

/// Buffered stdin event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdinBufferEvent {
    Data(String),
    Paste(String),
}

/// Timeouts for incomplete sequences.
#[derive(Debug, Clone, Copy)]
pub struct StdinBufferOptions {
    /// Maximum wait for an incomplete CSI/OSC/APC or mouse sequence.
    pub timeout: Duration,
}

impl Default for StdinBufferOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_millis(10),
        }
    }
}

/// Buffers stdin input and emits complete sequences.
#[derive(Debug)]
pub struct StdinBuffer {
    buffer: String,
    deadline: Option<Instant>,
    timeout: Duration,
    paste_mode: bool,
    paste_buffer: String,
    pending_kitty_printable_codepoint: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SequenceStatus {
    Complete,
    Incomplete,
    NotEscape,
}

impl StdinBuffer {
    pub fn new() -> Self {
        Self::with_options(StdinBufferOptions::default())
    }

    pub fn with_options(options: StdinBufferOptions) -> Self {
        Self {
            buffer: String::new(),
            deadline: None,
            timeout: options.timeout,
            paste_mode: false,
            paste_buffer: String::new(),
            pending_kitty_printable_codepoint: None,
        }
    }

    /// Feed a UTF-8 string chunk and return the events it completes.
    pub fn process(&mut self, data: &str) -> Vec<StdinBufferEvent> {
        self.deadline = None;

        if data.is_empty() && self.buffer.is_empty() {
            return self.emit_data_sequence("");
        }

        self.buffer.push_str(data);

        if self.paste_mode {
            return self.process_paste_chunk();
        }

        if let Some(start) = self.buffer.find(BRACKETED_PASTE_START) {
            return self.begin_paste(start);
        }

        let (sequences, remainder) = extract_complete_sequences(&self.buffer);
        self.buffer = remainder;
        let mut events = Vec::new();
        for sequence in sequences {
            events.extend(self.emit_data_sequence(&sequence));
        }

        if !self.buffer.is_empty() {
            self.deadline = Some(Instant::now() + self.timeout);
        }
        events
    }

    /// Feed raw bytes, applying the source's high-byte conversion.
    pub fn process_bytes(&mut self, data: &[u8]) -> Vec<StdinBufferEvent> {
        if data.len() == 1 && data[0] > 127 {
            let byte = data[0] - 128;
            return self.process(&format!("\x1b{}", char::from(byte)));
        }
        let text = String::from_utf8_lossy(data).into_owned();
        self.process(&text)
    }

    fn process_paste_chunk(&mut self) -> Vec<StdinBufferEvent> {
        self.paste_buffer.push_str(&self.buffer);
        self.buffer.clear();
        let Some(end) = self.paste_buffer.find(BRACKETED_PASTE_END) else {
            return Vec::new();
        };
        let content = self.paste_buffer[..end].to_string();
        let remaining = self.paste_buffer[end + BRACKETED_PASTE_END.len()..].to_string();
        self.paste_mode = false;
        self.paste_buffer.clear();
        self.pending_kitty_printable_codepoint = None;

        let mut events = vec![StdinBufferEvent::Paste(content)];
        if !remaining.is_empty() {
            events.extend(self.process(&remaining));
        }
        events
    }

    fn begin_paste(&mut self, start: usize) -> Vec<StdinBufferEvent> {
        let mut events = Vec::new();
        if start > 0 {
            let before_paste = &self.buffer[..start];
            let (sequences, _) = extract_complete_sequences(before_paste);
            for sequence in sequences {
                events.extend(self.emit_data_sequence(&sequence));
            }
        }

        self.pending_kitty_printable_codepoint = None;
        self.buffer = self.buffer[start + BRACKETED_PASTE_START.len()..].to_string();
        self.paste_mode = true;
        self.paste_buffer = std::mem::take(&mut self.buffer);

        if let Some(end) = self.paste_buffer.find(BRACKETED_PASTE_END) {
            let content = self.paste_buffer[..end].to_string();
            let remaining = self.paste_buffer[end + BRACKETED_PASTE_END.len()..].to_string();
            self.paste_mode = false;
            self.paste_buffer.clear();
            self.pending_kitty_printable_codepoint = None;
            events.push(StdinBufferEvent::Paste(content));
            if !remaining.is_empty() {
                events.extend(self.process(&remaining));
            }
        }
        events
    }

    fn emit_data_sequence(&mut self, sequence: &str) -> Vec<StdinBufferEvent> {
        let raw_codepoint = if utf16_len(sequence) == 1 {
            sequence.chars().next().map(|character| character as u32)
        } else {
            None
        };
        if raw_codepoint.is_some() && raw_codepoint == self.pending_kitty_printable_codepoint {
            self.pending_kitty_printable_codepoint = None;
            return Vec::new();
        }

        self.pending_kitty_printable_codepoint = parse_unmodified_kitty_printable_codepoint(sequence);
        vec![StdinBufferEvent::Data(sequence.to_string())]
    }

    /// Flush and return any buffered sequence.
    pub fn flush(&mut self) -> Vec<String> {
        self.deadline = None;
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let sequences = vec![std::mem::take(&mut self.buffer)];
        self.pending_kitty_printable_codepoint = None;
        sequences
    }

    /// Emit the buffered sequence if its deadline has elapsed.
    pub fn poll(&mut self, now: Instant) -> Vec<StdinBufferEvent> {
        let Some(deadline) = self.deadline else {
            return Vec::new();
        };
        if now < deadline {
            return Vec::new();
        }

        let mut events = Vec::new();
        for sequence in self.flush() {
            events.extend(self.emit_data_sequence(&sequence));
        }
        events
    }

    /// Deadline for the currently buffered sequence.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Clear all buffered state without emitting.
    pub fn clear(&mut self) {
        self.deadline = None;
        self.buffer.clear();
        self.paste_mode = false;
        self.paste_buffer.clear();
        self.pending_kitty_printable_codepoint = None;
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn destroy(&mut self) {
        self.clear();
    }
}

impl Default for StdinBuffer {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_unmodified_kitty_printable_codepoint(sequence: &str) -> Option<u32> {
    let body = sequence.strip_prefix("\x1b[")?.strip_suffix('u')?;
    let bytes = body.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let codepoint: u32 = body[..index].parse().ok()?;

    if bytes.get(index) == Some(&b':') {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if bytes.get(index) == Some(&b':') {
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index == start {
                return None;
            }
        }
    }

    if index != bytes.len() {
        return None;
    }
    (codepoint >= 32).then_some(codepoint)
}

fn next_char_len(value: &str, index: usize) -> usize {
    value[index..].chars().next().map_or(1, char::len_utf8)
}

fn is_complete_sequence(data: &str) -> SequenceStatus {
    if !data.starts_with(ESC) {
        return SequenceStatus::NotEscape;
    }
    if utf16_len(data) == 1 {
        return SequenceStatus::Incomplete;
    }

    let after_escape = &data[1..];
    if after_escape.starts_with('[') {
        if after_escape.starts_with("[M") {
            return if utf16_len(data) >= 6 {
                SequenceStatus::Complete
            } else {
                SequenceStatus::Incomplete
            };
        }
        return is_complete_csi_sequence(data);
    }
    if after_escape.starts_with(']') {
        return is_complete_terminated_sequence(data, ESC, "\x1b\\", true);
    }
    if after_escape.starts_with('P') {
        return is_complete_terminated_sequence(data, "\x1bP", "\x1b\\", false);
    }
    if after_escape.starts_with('_') {
        return is_complete_terminated_sequence(data, "\x1b_", "\x1b\\", false);
    }
    if after_escape.starts_with('O') {
        return if utf16_len(after_escape) >= 2 {
            SequenceStatus::Complete
        } else {
            SequenceStatus::Incomplete
        };
    }
    if utf16_len(after_escape) == 1 {
        return SequenceStatus::Complete;
    }
    SequenceStatus::Complete
}

fn is_complete_terminated_sequence(data: &str, prefix: &str, terminator: &str, allow_bel: bool) -> SequenceStatus {
    if !data.starts_with(prefix) {
        return SequenceStatus::Complete;
    }
    if data.ends_with(terminator) || (allow_bel && data.ends_with('\x07')) {
        SequenceStatus::Complete
    } else {
        SequenceStatus::Incomplete
    }
}

fn is_complete_csi_sequence(data: &str) -> SequenceStatus {
    if !data.starts_with("\x1b[") {
        return SequenceStatus::Complete;
    }
    if utf16_len(data) < 3 {
        return SequenceStatus::Incomplete;
    }

    let payload = &data[2..];
    let Some(last) = payload.chars().last() else {
        return SequenceStatus::Incomplete;
    };
    let last_code = last as u32;
    if !(0x40..=0x7e).contains(&last_code) {
        return SequenceStatus::Incomplete;
    }

    if !payload.starts_with('<') {
        return SequenceStatus::Complete;
    }

    if is_sgr_mouse_payload(payload) {
        return SequenceStatus::Complete;
    }
    if last == 'M' || last == 'm' {
        let inner = &payload[1..payload.len() - 1];
        let parts: Vec<&str> = inner.split(';').collect();
        if parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        {
            return SequenceStatus::Complete;
        }
    }
    SequenceStatus::Incomplete
}

fn is_sgr_mouse_payload(payload: &str) -> bool {
    if !payload.starts_with('<') || !(payload.ends_with('M') || payload.ends_with('m')) {
        return false;
    }
    let inner = &payload[1..payload.len() - 1];
    let parts: Vec<&str> = inner.split(';').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn extract_complete_sequences(buffer: &str) -> (Vec<String>, String) {
    let mut sequences = Vec::new();
    let mut pos = 0;

    while pos < buffer.len() {
        let remaining = &buffer[pos..];
        if remaining.starts_with(ESC) {
            let mut seq_end = 1;
            let mut emitted = false;

            while seq_end <= remaining.len() {
                let candidate = &remaining[..seq_end];
                match is_complete_sequence(candidate) {
                    SequenceStatus::Complete => {
                        if candidate == "\x1b\x1b" {
                            let next = remaining[seq_end..].chars().next();
                            if matches!(next, Some('[' | ']' | 'O' | 'P' | '_')) {
                                sequences.push(ESC.to_string());
                                pos += 1;
                                emitted = true;
                                break;
                            }
                        }
                        sequences.push(candidate.to_string());
                        pos += seq_end;
                        emitted = true;
                        break;
                    }
                    SequenceStatus::Incomplete => {
                        if seq_end == remaining.len() {
                            break;
                        }
                        seq_end += next_char_len(remaining, seq_end);
                    }
                    SequenceStatus::NotEscape => {
                        sequences.push(candidate.to_string());
                        pos += seq_end;
                        emitted = true;
                        break;
                    }
                }
            }

            if !emitted {
                return (sequences, remaining.to_string());
            }
        } else {
            let length = next_char_len(remaining, 0);
            sequences.push(remaining[..length].to_string());
            pos += length;
        }
    }

    (sequences, String::new())
}
