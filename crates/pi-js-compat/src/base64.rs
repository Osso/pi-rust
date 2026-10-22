//! Base64 helpers with the two distinct Node semantics: `atob`/`btoa` and
//! `Buffer.from(value, "base64")`/`Buffer#toString("base64")`.

use crate::js_json::JsString;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Decode standard base64 with `atob` leniency: ASCII whitespace is ignored and
/// missing padding is accepted.
pub fn atob(input: &JsString) -> Result<Vec<u8>, Base64Error> {
    let mut symbols = Vec::with_capacity(input.len());
    let mut padding = 0usize;
    let units = input
        .units()
        .iter()
        .copied()
        .filter(|unit| !matches!(unit, 0x09 | 0x0a | 0x0c | 0x0d | 0x20));
    for unit in units {
        if unit > 255 {
            return Err(Base64Error::InvalidCharacter);
        }
        let byte = unit as u8;
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' => {
                if padding > 0 {
                    return Err(Base64Error::InvalidCharacter);
                }
                symbols.push(byte);
            }
            b'=' => padding += 1,
            _ => return Err(Base64Error::InvalidCharacter),
        }
    }
    if padding > 2 {
        return Err(Base64Error::InvalidCharacter);
    }
    if symbols.len() % 4 == 1 {
        return Err(Base64Error::IncorrectEncoding);
    }
    if padding > 0 && !(symbols.len() + padding).is_multiple_of(4) {
        return Err(Base64Error::InvalidCharacter);
    }

    let mut output = Vec::with_capacity(symbols.len() * 3 / 4);
    for chunk in symbols.chunks(4) {
        let values = chunk
            .iter()
            .map(|byte| symbol_value(*byte))
            .collect::<Result<Vec<_>, _>>()?;
        output.push((values[0] << 2) | (values[1] >> 4));
        if values.len() > 2 {
            output.push((values[1] << 4) | (values[2] >> 2));
        }
        if values.len() > 3 {
            output.push((values[2] << 6) | values[3]);
        }
    }
    Ok(output)
}

/// `btoa(bytes)`: standard padded base64.
pub fn btoa(bytes: &[u8]) -> String {
    buffer_to_base64(bytes)
}

/// Decode Base64 with `Buffer.from(value, "base64")` semantics: the standard
/// and URL-safe alphabets are accepted. Node reads each native UTF-16 unit's
/// low byte; other low bytes are ignored and the first `=` stops decoding.
pub fn buffer_from_base64(value: impl Into<JsString>) -> Vec<u8> {
    let value = value.into();
    let mut values = Vec::new();
    for unit in value.units() {
        let byte = *unit as u8;
        if byte == b'=' {
            break;
        }
        if let Some(decoded) = buffer_symbol_value(byte) {
            values.push(decoded);
        }
    }

    let mut output = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        let first = chunk[0];
        if chunk.len() >= 2 {
            output.push((first << 2) | (chunk[1] >> 4));
        }
        if chunk.len() >= 3 {
            output.push(((chunk[1] & 0x0f) << 4) | (chunk[2] >> 2));
        }
        if chunk.len() == 4 {
            output.push(((chunk[2] & 0x03) << 6) | chunk[3]);
        }
    }
    output
}

/// `Buffer#toString("base64")`: standard padded base64.
pub fn buffer_to_base64(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied();
        let third = chunk.get(2).copied();
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0x03) << 4) | (second.unwrap_or(0) >> 4)) as usize] as char);
        output.push(second.map_or('=', |second| {
            ALPHABET[(((second & 0x0f) << 2) | (third.unwrap_or(0) >> 6)) as usize] as char
        }));
        output.push(third.map_or('=', |third| ALPHABET[(third & 0x3f) as usize] as char));
    }
    output
}

/// Decoded byte length of a Base64 payload, counting only valid standard
/// alphabet symbols and stopping at the first `=`.
pub fn buffer_decoded_len(value: &str) -> usize {
    let mut count = 0;
    for byte in value.bytes() {
        if byte == b'=' {
            break;
        }
        if symbol_value(byte).is_ok() {
            count += 1;
        }
    }
    count / 4 * 3
        + match count % 4 {
            2 => 1,
            3 => 2,
            _ => 0,
        }
}

fn symbol_value(byte: u8) -> Result<u8, Base64Error> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(Base64Error::InvalidCharacter),
    }
}

fn buffer_symbol_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base64Error {
    InvalidCharacter,
    IncorrectEncoding,
}

impl Base64Error {
    pub fn name(&self) -> &'static str {
        "InvalidCharacterError"
    }
    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidCharacter => "Invalid character",
            Self::IncorrectEncoding => "The string to be decoded is not correctly encoded.",
        }
    }
}

impl std::fmt::Display for Base64Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}
impl std::error::Error for Base64Error {}
