use std::error::Error;
use std::fmt::{self, Display, Formatter};

const FRAME_HEADER_LENGTH: usize = 4;

/// Default upper bound for one framed protocol payload.
pub const DEFAULT_MAX_FRAME_LENGTH: u32 = 16 * 1024 * 1024;

/// Failure while constructing a length-prefixed frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameEncodeError {
    /// The payload length cannot be represented by the unsigned 32-bit frame header.
    PayloadTooLarge,
}

impl Display for FrameEncodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge => formatter.write_str("Frame payload exceeds the unsigned 32-bit length limit"),
        }
    }
}

impl Error for FrameEncodeError {}

/// Validation failure for one complete frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameError {
    message: String,
}

impl FrameError {
    /// Creates a framing error with the source-compatible message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for FrameError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for FrameError {}

/// Prefixes a payload with its unsigned 32-bit big-endian byte length.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, FrameEncodeError> {
    let payload_length = u32::try_from(payload.len()).map_err(|_| FrameEncodeError::PayloadTooLarge)?;
    let frame_length = FRAME_HEADER_LENGTH
        .checked_add(payload.len())
        .ok_or(FrameEncodeError::PayloadTooLarge)?;
    let mut frame = Vec::with_capacity(frame_length);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Validates that bytes contain exactly one complete frame within the configured limit.
pub fn assert_complete_frame(frame: &[u8], max_frame_length: Option<u32>) -> Result<(), FrameError> {
    if frame.len() < FRAME_HEADER_LENGTH {
        return Err(FrameError::new("Frame does not contain a complete length prefix"));
    }

    let payload_length = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]);
    let maximum = max_frame_length.unwrap_or(DEFAULT_MAX_FRAME_LENGTH);
    if payload_length > maximum {
        return Err(FrameError::new(format!(
            "Frame length {payload_length} exceeds configured limit of {maximum}"
        )));
    }

    let expected_frame_length = usize::try_from(payload_length)
        .ok()
        .and_then(|length| FRAME_HEADER_LENGTH.checked_add(length));
    if expected_frame_length != Some(frame.len()) {
        return Err(FrameError::new("Frame must contain exactly one complete payload"));
    }

    Ok(())
}
