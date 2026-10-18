use std::error::Error;
use std::fmt::{self, Display, Formatter};

mod parse;
mod serialize;

use crate::framing::{DEFAULT_MAX_FRAME_LENGTH, FrameDecoder, assert_complete_frame, encode_frame};
use crate::{CborOptions, CborValue, ClientMessage, PROTOCOL_VERSION, ServerMessage, decode_cbor, encode_cbor};

/// Failure while validating, framing, or decoding a protocol message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolValidationError {
    message: String,
}

impl ProtocolValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ProtocolValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ProtocolValidationError {}

/// Validates one decoded CBOR value as a client protocol message.
pub fn parse_client_message(value: &CborValue) -> Result<ClientMessage, ProtocolValidationError> {
    parse::client_message(value).ok_or_else(|| ProtocolValidationError::new("Invalid client protocol message"))
}

/// Validates one decoded CBOR value as a server protocol message.
pub fn parse_server_message(value: &CborValue) -> Result<ServerMessage, ProtocolValidationError> {
    parse::server_message(value).ok_or_else(|| ProtocolValidationError::new("Invalid server protocol message"))
}

/// Validates and encodes one complete length-prefixed client message.
pub fn encode_client_message(
    message: &ClientMessage,
    max_frame_length: Option<u32>,
) -> Result<Vec<u8>, ProtocolValidationError> {
    encode_message(&serialize::client_message(message), "client", max_frame_length)
}

/// Validates and encodes one complete length-prefixed server message.
pub fn encode_server_message(
    message: &ServerMessage,
    max_frame_length: Option<u32>,
) -> Result<Vec<u8>, ProtocolValidationError> {
    encode_message(&serialize::server_message(message), "server", max_frame_length)
}

fn encode_message(
    value: &CborValue,
    kind: &str,
    max_frame_length: Option<u32>,
) -> Result<Vec<u8>, ProtocolValidationError> {
    let maximum = max_frame_length.unwrap_or(DEFAULT_MAX_FRAME_LENGTH);
    encode_message_inner(value, maximum).map_err(|error| {
        ProtocolValidationError::new(format!(
            "Unable to encode {kind} protocol message: {}",
            bounded_message(&error)
        ))
    })
}

fn encode_message_inner(value: &CborValue, maximum: u32) -> Result<Vec<u8>, String> {
    let options = CborOptions {
        max_byte_length: Some(maximum),
        ..CborOptions::default()
    };
    let payload = encode_cbor(value, Some(options)).map_err(|error| error.to_string())?;
    let frame = encode_frame(&payload).map_err(|error| error.to_string())?;
    assert_complete_frame(&frame, Some(maximum)).map_err(|error| error.to_string())?;
    Ok(frame)
}

/// Incrementally decodes and validates framed client messages.
pub struct ClientMessageDecoder {
    decoder: ValidatedMessageDecoder<ClientMessage>,
}

impl ClientMessageDecoder {
    pub fn new(max_frame_length: Option<u32>) -> Self {
        Self {
            decoder: ValidatedMessageDecoder::new("client", parse::client_message, max_frame_length),
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<ClientMessage>, ProtocolValidationError> {
        self.decoder.push(chunk)
    }

    pub fn end(&mut self) -> Result<(), ProtocolValidationError> {
        self.decoder.end()
    }
}

/// Incrementally decodes and validates framed server messages.
pub struct ServerMessageDecoder {
    decoder: ValidatedMessageDecoder<ServerMessage>,
}

impl ServerMessageDecoder {
    pub fn new(max_frame_length: Option<u32>) -> Self {
        Self {
            decoder: ValidatedMessageDecoder::new("server", parse::server_message, max_frame_length),
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<ServerMessage>, ProtocolValidationError> {
        self.decoder.push(chunk)
    }

    pub fn end(&mut self) -> Result<(), ProtocolValidationError> {
        self.decoder.end()
    }
}

/// Creates an incremental client-message decoder.
pub fn create_client_message_decoder(max_frame_length: Option<u32>) -> ClientMessageDecoder {
    ClientMessageDecoder::new(max_frame_length)
}

/// Creates an incremental server-message decoder.
pub fn create_server_message_decoder(max_frame_length: Option<u32>) -> ServerMessageDecoder {
    ServerMessageDecoder::new(max_frame_length)
}

/// Returns whether an integer version is the supported protocol version.
pub const fn is_supported_protocol_version(version: u64) -> bool {
    version == PROTOCOL_VERSION
}

struct ValidatedMessageDecoder<T> {
    frames: FrameDecoder,
    kind: &'static str,
    max_frame_length: u32,
    failed: bool,
    parse: fn(&CborValue) -> Option<T>,
}

impl<T> ValidatedMessageDecoder<T> {
    fn new(kind: &'static str, parse: fn(&CborValue) -> Option<T>, max_frame_length: Option<u32>) -> Self {
        Self {
            frames: FrameDecoder::new(max_frame_length),
            kind,
            max_frame_length: max_frame_length.unwrap_or(DEFAULT_MAX_FRAME_LENGTH),
            failed: false,
            parse,
        }
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<T>, ProtocolValidationError> {
        if self.failed {
            return Err(self.failed_error());
        }

        let frames = match self.frames.push(chunk) {
            Ok(frames) => frames,
            Err(error) => {
                self.failed = true;
                return Err(self.frame_error(&error.to_string()));
            }
        };

        let mut messages = Vec::new();
        for frame in frames {
            let value = match decode_cbor(&frame, Some(self.cbor_options())) {
                Ok(value) => value,
                Err(error) => {
                    self.failed = true;
                    return Err(self.frame_error(&error.to_string()));
                }
            };
            match (self.parse)(&value) {
                Some(message) => messages.push(message),
                None => {
                    self.failed = true;
                    return Err(ProtocolValidationError::new(format!(
                        "Invalid {} protocol message",
                        self.kind
                    )));
                }
            }
        }
        Ok(messages)
    }

    fn end(&mut self) -> Result<(), ProtocolValidationError> {
        if self.failed {
            return Err(self.failed_error());
        }
        match self.frames.end() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.failed = true;
                Err(ProtocolValidationError::new(format!(
                    "Invalid {} protocol framing: {}",
                    self.kind,
                    bounded_message(&error.to_string())
                )))
            }
        }
    }

    fn cbor_options(&self) -> CborOptions {
        CborOptions {
            max_byte_length: Some(self.max_frame_length),
            ..CborOptions::default()
        }
    }

    fn failed_error(&self) -> ProtocolValidationError {
        ProtocolValidationError::new(format!("{} message decoder has failed", self.kind))
    }

    fn frame_error(&self, message: &str) -> ProtocolValidationError {
        ProtocolValidationError::new(format!(
            "Invalid {} protocol frame: {}",
            self.kind,
            bounded_message(message)
        ))
    }
}

fn bounded_message(message: &str) -> String {
    if message.chars().count() <= 500 {
        return message.to_string();
    }
    format!("{}...", message.chars().take(497).collect::<String>())
}
