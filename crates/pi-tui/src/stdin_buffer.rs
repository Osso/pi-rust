//! Stdin buffering for partial escape sequences.
//!
//! Ported from `packages/tui/src/stdin-buffer.ts`. TypeScript schedules a
//! `setTimeout` internally; the Rust port exposes the pending deadline through
//! [`StdinBuffer::next_deadline`] and flushes via [`StdinBuffer::poll`] so the
//! terminal driver owns timer execution.

use std::time::{Duration, Instant};

use pi_js_compat::js_json::JsString;

const ESC: &str = "\x1b";
const BRACKETED_PASTE_START: &str = "\x1b[200~";
const BRACKETED_PASTE_END: &str = "\x1b[201~";

/// Buffered stdin event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdinBufferEvent {
    Data(JsString),
    Paste(JsString),
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
    buffer: JsString,
    deadline: Option<Instant>,
    timeout: Duration,
    paste_mode: bool,
    paste_buffer: JsString,
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
            buffer: JsString::default(),
            deadline: None,
            timeout: options.timeout,
            paste_mode: false,
            paste_buffer: JsString::default(),
            pending_kitty_printable_codepoint: None,
        }
    }

    /// Feed a native UTF-16 string chunk and return the events it completes.
    pub fn process(&mut self, data: impl Into<JsString>) -> Vec<StdinBufferEvent> {
        let data = data.into();
        self.deadline = None;

        if data.is_empty() && self.buffer.is_empty() {
            return self.emit_data_sequence(&JsString::default());
        }

        self.buffer.push_str(&data);

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
            return self.process(format!("\x1b{}", char::from(byte)));
        }
        let text = String::from_utf8_lossy(data).into_owned();
        self.process(&text)
    }

    fn process_paste_chunk(&mut self) -> Vec<StdinBufferEvent> {
        self.paste_buffer.push_str(&self.buffer);
        self.buffer = JsString::default();
        let Some(end) = self.paste_buffer.find(BRACKETED_PASTE_END) else {
            return Vec::new();
        };
        let content = self.paste_buffer.slice(0, end);
        let remaining = self
            .paste_buffer
            .slice(end + BRACKETED_PASTE_END.len(), self.paste_buffer.len());
        self.paste_mode = false;
        self.paste_buffer = JsString::default();
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
            let before_paste = self.buffer.slice(0, start);
            let (sequences, _) = extract_complete_sequences(&before_paste);
            for sequence in sequences {
                events.extend(self.emit_data_sequence(&sequence));
            }
        }

        self.pending_kitty_printable_codepoint = None;
        self.buffer = self
            .buffer
            .slice(start + BRACKETED_PASTE_START.len(), self.buffer.len());
        self.paste_mode = true;
        self.paste_buffer = std::mem::take(&mut self.buffer);

        if let Some(end) = self.paste_buffer.find(BRACKETED_PASTE_END) {
            let content = self.paste_buffer.slice(0, end);
            let remaining = self
                .paste_buffer
                .slice(end + BRACKETED_PASTE_END.len(), self.paste_buffer.len());
            self.paste_mode = false;
            self.paste_buffer = JsString::default();
            self.pending_kitty_printable_codepoint = None;
            events.push(StdinBufferEvent::Paste(content));
            if !remaining.is_empty() {
                events.extend(self.process(&remaining));
            }
        }
        events
    }

    fn emit_data_sequence(&mut self, sequence: &JsString) -> Vec<StdinBufferEvent> {
        let raw_codepoint = if sequence.len() == 1 {
            sequence.units().first().map(|unit| u32::from(*unit))
        } else {
            None
        };
        if raw_codepoint.is_some() && raw_codepoint == self.pending_kitty_printable_codepoint {
            self.pending_kitty_printable_codepoint = None;
            return Vec::new();
        }

        self.pending_kitty_printable_codepoint = parse_unmodified_kitty_printable_codepoint(sequence);
        vec![StdinBufferEvent::Data(sequence.clone())]
    }

    /// Flush and return any buffered sequence.
    pub fn flush(&mut self) -> Vec<JsString> {
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
        self.buffer = JsString::default();
        self.paste_mode = false;
        self.paste_buffer = JsString::default();
        self.pending_kitty_printable_codepoint = None;
    }

    pub fn buffer(&self) -> &JsString {
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

fn parse_unmodified_kitty_printable_codepoint(sequence: &JsString) -> Option<u32> {
    let body = sequence.strip_prefix("\x1b[")?.strip_suffix('u')?;
    let bytes = body.units();
    let mut index = 0;
    while index < bytes.len() && (48..=57).contains(&bytes[index]) {
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let codepoint = bytes[..index].iter().try_fold(0u32, |number, unit| {
        number.checked_mul(10)?.checked_add(u32::from(*unit - 48))
    })?;

    if bytes.get(index) == Some(&58) {
        index += 1;
        while index < bytes.len() && (48..=57).contains(&bytes[index]) {
            index += 1;
        }
        if bytes.get(index) == Some(&58) {
            index += 1;
            let start = index;
            while index < bytes.len() && (48..=57).contains(&bytes[index]) {
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

fn is_complete_sequence(data: &JsString) -> SequenceStatus {
    if !data.starts_with(ESC) {
        return SequenceStatus::NotEscape;
    }
    if data.len() == 1 {
        return SequenceStatus::Incomplete;
    }

    let after_escape = data.slice(1, data.len());
    if after_escape.starts_with('[') {
        if after_escape.starts_with("[M") {
            return if data.len() >= 6 {
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
        return if after_escape.len() >= 2 {
            SequenceStatus::Complete
        } else {
            SequenceStatus::Incomplete
        };
    }
    if after_escape.len() == 1 {
        return SequenceStatus::Complete;
    }
    SequenceStatus::Complete
}

fn is_complete_terminated_sequence(data: &JsString, prefix: &str, terminator: &str, allow_bel: bool) -> SequenceStatus {
    if !data.starts_with(prefix) {
        return SequenceStatus::Complete;
    }
    if data.ends_with(terminator) || (allow_bel && data.ends_with('\x07')) {
        SequenceStatus::Complete
    } else {
        SequenceStatus::Incomplete
    }
}

fn is_complete_csi_sequence(data: &JsString) -> SequenceStatus {
    if !data.starts_with("\x1b[") {
        return SequenceStatus::Complete;
    }
    if data.len() < 3 {
        return SequenceStatus::Incomplete;
    }

    let payload = data.slice(2, data.len());
    let Some(last) = payload.units().last().copied() else {
        return SequenceStatus::Incomplete;
    };
    let last_code = last as u32;
    if !(0x40..=0x7e).contains(&last_code) {
        return SequenceStatus::Incomplete;
    }

    if !payload.starts_with('<') {
        return SequenceStatus::Complete;
    }

    if is_sgr_mouse_payload(&payload) {
        return SequenceStatus::Complete;
    }
    if last == 77 || last == 109 {
        let inner = payload.slice(1, payload.len() - 1);
        let parts: Vec<JsString> = inner.split(';').collect();
        if parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.units().iter().all(|unit| (48..=57).contains(unit)))
        {
            return SequenceStatus::Complete;
        }
    }
    SequenceStatus::Incomplete
}

fn is_sgr_mouse_payload(payload: &JsString) -> bool {
    if !payload.starts_with('<') || !(payload.ends_with('M') || payload.ends_with('m')) {
        return false;
    }
    let inner = payload.slice(1, payload.len() - 1);
    let parts: Vec<JsString> = inner.split(';').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.units().iter().all(|unit| (48..=57).contains(unit)))
}

fn extract_complete_sequences(buffer: &JsString) -> (Vec<JsString>, JsString) {
    let mut sequences = Vec::new();
    let mut pos = 0;

    while pos < buffer.len() {
        let remaining = buffer.slice(pos, buffer.len());
        if remaining.starts_with(ESC) {
            let mut seq_end = 1;
            let mut emitted = false;

            while seq_end <= remaining.len() {
                let candidate = remaining.slice(0, seq_end);
                match is_complete_sequence(&candidate) {
                    SequenceStatus::Complete => {
                        if candidate == "\x1b\x1b" {
                            let next = remaining.units().get(seq_end).copied();
                            if matches!(next, Some(91 | 93 | 79 | 80 | 95)) {
                                sequences.push(JsString::from(ESC));
                                pos += 1;
                                emitted = true;
                                break;
                            }
                        }
                        sequences.push(candidate.clone());
                        pos += seq_end;
                        emitted = true;
                        break;
                    }
                    SequenceStatus::Incomplete => {
                        if seq_end == remaining.len() {
                            break;
                        }
                        seq_end += 1;
                    }
                    SequenceStatus::NotEscape => {
                        sequences.push(candidate.clone());
                        pos += seq_end;
                        emitted = true;
                        break;
                    }
                }
            }

            if !emitted {
                return (sequences, remaining.clone());
            }
        } else {
            let length = 1;
            sequences.push(remaining.slice(0, length));
            pos += length;
        }
    }

    (sequences, JsString::default())
}
