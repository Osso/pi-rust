//! AWS `application/vnd.amazon.eventstream` framing used by Bedrock ConverseStream.
//!
//! The wire format is a prelude (total length, headers length, prelude CRC32),
//! header entries, payload, and a message CRC32. Both CRC32 values are checked
//! before a message is surfaced, matching the Smithy event-stream decoder.

use std::collections::BTreeMap;

/// One decoded event-stream message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStreamMessage {
    pub headers: BTreeMap<String, String>,
    pub payload: Vec<u8>,
}

/// Framing/decoding failure. Messages match the Smithy decoder where observable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStreamError {
    message: String,
}

impl EventStreamError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for EventStreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EventStreamError {}

/// Incremental decoder: feed arbitrary byte chunks, pull complete messages.
#[derive(Debug, Default)]
pub struct EventStreamDecoder {
    buffer: Vec<u8>,
}

impl EventStreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    /// Next complete message, `Ok(None)` when more bytes are needed.
    pub fn next_message(&mut self) -> Result<Option<EventStreamMessage>, EventStreamError> {
        if self.buffer.len() < 12 {
            return Ok(None);
        }
        let total_length = u32::from_be_bytes(self.buffer[0..4].try_into().expect("prelude length")) as usize;
        let headers_length = u32::from_be_bytes(self.buffer[4..8].try_into().expect("prelude length")) as usize;
        let prelude_crc = u32::from_be_bytes(self.buffer[8..12].try_into().expect("prelude crc"));
        if total_length < 16 || headers_length > total_length.saturating_sub(16) {
            return Err(EventStreamError::new("Invalid event stream message length"));
        }
        if self.buffer.len() < total_length {
            return Ok(None);
        }

        let calculated_prelude_crc = crc32(&self.buffer[0..8]);
        if prelude_crc != calculated_prelude_crc {
            return Err(EventStreamError::new(format!(
                "The prelude checksum specified in the message ({prelude_crc}) does not match the calculated CRC32 checksum ({calculated_prelude_crc})"
            )));
        }

        let message = &self.buffer[..total_length];
        let expected_message_crc =
            u32::from_be_bytes(message[total_length - 4..total_length].try_into().expect("message crc"));
        let calculated_message_crc = crc32(&message[..total_length - 4]);
        if expected_message_crc != calculated_message_crc {
            return Err(EventStreamError::new(format!(
                "The message checksum ({calculated_message_crc}) did not match the expected value of {expected_message_crc}"
            )));
        }

        let headers_end = 12 + headers_length;
        let headers = decode_headers(&message[12..headers_end])?;
        let payload = message[headers_end..total_length - 4].to_vec();
        self.buffer.drain(..total_length);
        Ok(Some(EventStreamMessage { headers, payload }))
    }
}

fn decode_headers(bytes: &[u8]) -> Result<BTreeMap<String, String>, EventStreamError> {
    let mut headers = BTreeMap::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let name_length = *bytes.get(index).ok_or_else(header_error)? as usize;
        index += 1;
        let name_bytes = bytes.get(index..index + name_length).ok_or_else(header_error)?;
        let name = std::str::from_utf8(name_bytes).map_err(|_| header_error())?.to_owned();
        index += name_length;
        let value_type = *bytes.get(index).ok_or_else(header_error)?;
        index += 1;
        let (value, consumed) = decode_header_value(value_type, &bytes[index..])?;
        index += consumed;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn decode_header_value(value_type: u8, bytes: &[u8]) -> Result<(String, usize), EventStreamError> {
    match value_type {
        0 => Ok(("true".to_owned(), 0)),
        1 => Ok(("false".to_owned(), 0)),
        2 => Ok((
            i8::from_be_bytes([*bytes.first().ok_or_else(header_error)?]).to_string(),
            1,
        )),
        3 => Ok((
            i16::from_be_bytes(bytes.get(0..2).ok_or_else(header_error)?.try_into().expect("short")).to_string(),
            2,
        )),
        4 => Ok((
            i32::from_be_bytes(bytes.get(0..4).ok_or_else(header_error)?.try_into().expect("int")).to_string(),
            4,
        )),
        5 => Ok((
            i64::from_be_bytes(bytes.get(0..8).ok_or_else(header_error)?.try_into().expect("long")).to_string(),
            8,
        )),
        6 => {
            let length =
                u16::from_be_bytes(bytes.get(0..2).ok_or_else(header_error)?.try_into().expect("len")) as usize;
            let value = bytes.get(2..2 + length).ok_or_else(header_error)?;
            Ok((crate::utils::base64::encode(value), 2 + length))
        }
        7 => {
            let length =
                u16::from_be_bytes(bytes.get(0..2).ok_or_else(header_error)?.try_into().expect("len")) as usize;
            let value =
                std::str::from_utf8(bytes.get(2..2 + length).ok_or_else(header_error)?).map_err(|_| header_error())?;
            Ok((value.to_owned(), 2 + length))
        }
        8 => Ok((
            i64::from_be_bytes(bytes.get(0..8).ok_or_else(header_error)?.try_into().expect("timestamp")).to_string(),
            8,
        )),
        9 => Ok((format!("{:?}", bytes.get(0..16).ok_or_else(header_error)?), 16)),
        _ => Err(EventStreamError::new(format!(
            "Unsupported event stream header value type {value_type}"
        ))),
    }
}

fn header_error() -> EventStreamError {
    EventStreamError::new("Invalid event stream header")
}

/// CRC32 (IEEE) used by event-stream framing.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        let index = ((crc ^ u32::from(*byte)) & 0xff) as usize;
        crc = (crc >> 8) ^ CRC_TABLE[index];
    }
    !crc
}

const CRC_TABLE: [u32; 256] = build_crc_table();

const fn build_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut index = 0usize;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 == 1 {
                0xedb8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
}
