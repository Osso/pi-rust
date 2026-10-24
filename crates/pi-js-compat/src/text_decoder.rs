//! Streaming UTF-8 TextDecoder semantics, shared by bash output collectors.

const REPLACEMENT_CHARACTER: char = '\u{FFFD}';

/// Streaming UTF-8 decoder matching Node's `TextDecoder` with `{ stream: true }`.
#[derive(Debug, Default)]
pub struct Utf8StreamDecoder {
    pending: Vec<u8>,
    at_stream_start: bool,
}

impl Utf8StreamDecoder {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            at_stream_start: true,
        }
    }

    pub fn decode(&mut self, chunk: &[u8]) -> String {
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

    pub fn finish(&mut self) -> String {
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
