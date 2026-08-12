use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[cfg(test)]
mod tests;

const FRAME_HEADER_LENGTH: usize = 4;
const PAYLOAD_BLOCK_SIZE: usize = 64 * 1024;

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

/// Failure while incrementally decoding framed payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameDecodeError {
    /// Source-compatible framing failure.
    Frame(FrameError),
    /// Rust could not reserve memory for decoder-owned data.
    Allocation,
}

impl Display for FrameDecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(error) => Display::fmt(error, formatter),
            Self::Allocation => formatter.write_str("Frame decoder allocation failed"),
        }
    }
}

impl Error for FrameDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Frame(error) => Some(error),
            Self::Allocation => None,
        }
    }
}

impl From<FrameError> for FrameDecodeError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

#[derive(Default)]
struct AllocationControl {
    #[cfg(test)]
    fail_next: bool,
}

impl AllocationControl {
    fn try_reserve_exact<T>(&mut self, values: &mut Vec<T>, additional: usize) -> Result<(), FrameDecodeError> {
        #[cfg(test)]
        if std::mem::take(&mut self.fail_next) {
            return Err(FrameDecodeError::Allocation);
        }

        values
            .try_reserve_exact(additional)
            .map_err(|_| FrameDecodeError::Allocation)
    }

    #[cfg(test)]
    fn failing_once() -> Self {
        Self { fail_next: true }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecoderState {
    Open,
    Ended,
    Failed,
}

/// Incrementally splits arbitrary byte chunks into length-prefixed payloads.
pub struct FrameDecoder {
    header: [u8; FRAME_HEADER_LENGTH],
    header_length: usize,
    max_frame_length: u32,
    payload_blocks: Vec<Vec<u8>>,
    current_payload_block_length: usize,
    expected_payload_length: Option<usize>,
    payload_length: usize,
    state: DecoderState,
    allocations: AllocationControl,
}

impl FrameDecoder {
    /// Creates an open decoder with the default or configured maximum payload length.
    pub fn new(max_frame_length: Option<u32>) -> Self {
        Self::with_allocation_control(max_frame_length, AllocationControl::default())
    }

    #[cfg(test)]
    fn with_forced_allocation_failure(max_frame_length: Option<u32>) -> Self {
        Self::with_allocation_control(max_frame_length, AllocationControl::failing_once())
    }

    fn with_allocation_control(max_frame_length: Option<u32>, allocations: AllocationControl) -> Self {
        Self {
            header: [0; FRAME_HEADER_LENGTH],
            header_length: 0,
            max_frame_length: max_frame_length.unwrap_or(DEFAULT_MAX_FRAME_LENGTH),
            payload_blocks: Vec::new(),
            current_payload_block_length: 0,
            expected_payload_length: None,
            payload_length: 0,
            state: DecoderState::Open,
            allocations,
        }
    }

    /// Decodes every complete payload available in one arbitrary input chunk.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, FrameDecodeError> {
        self.ensure_open().map_err(FrameDecodeError::Frame)?;
        let mut frames = Vec::new();
        let mut chunk_offset = 0;

        while chunk_offset < chunk.len() {
            if self.expected_payload_length.is_none() {
                if !self.read_frame_header(chunk, &mut chunk_offset) {
                    continue;
                }
                let frame_length = self.decode_frame_length()?;
                if frame_length == 0 {
                    self.append_frame(&mut frames, Vec::new())?;
                    continue;
                }
                self.begin_payload(frame_length);
            }

            let Some(expected_payload_length) = self.expected_payload_length else {
                continue;
            };
            self.copy_payload_bytes(chunk, &mut chunk_offset, expected_payload_length)?;
            if self.payload_length == expected_payload_length {
                let payload = self.complete_payload(expected_payload_length)?;
                self.append_frame(&mut frames, payload)?;
            }
        }

        Ok(frames)
    }

    /// Ends the stream, rejecting any retained partial header or payload.
    pub fn end(&mut self) -> Result<(), FrameError> {
        self.ensure_open()?;
        if self.header_length != 0 || self.expected_payload_length.is_some() {
            return Err(self.fail("Truncated frame at end of stream"));
        }
        self.state = DecoderState::Ended;
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), FrameError> {
        match self.state {
            DecoderState::Open => Ok(()),
            DecoderState::Ended => Err(FrameError::new("Frame decoder has ended")),
            DecoderState::Failed => Err(FrameError::new("Frame decoder has failed")),
        }
    }

    fn read_frame_header(&mut self, chunk: &[u8], chunk_offset: &mut usize) -> bool {
        let remaining_header = FRAME_HEADER_LENGTH - self.header_length;
        let remaining_chunk = chunk.len() - *chunk_offset;
        let header_bytes = remaining_header.min(remaining_chunk);
        let header_end = self.header_length + header_bytes;
        let chunk_end = *chunk_offset + header_bytes;
        self.header[self.header_length..header_end].copy_from_slice(&chunk[*chunk_offset..chunk_end]);
        self.header_length = header_end;
        *chunk_offset = chunk_end;
        self.header_length == FRAME_HEADER_LENGTH
    }

    fn decode_frame_length(&mut self) -> Result<u32, FrameDecodeError> {
        let frame_length = u32::from_be_bytes(self.header);
        self.header_length = 0;
        if frame_length > self.max_frame_length {
            let message = format!(
                "Frame length {frame_length} exceeds configured limit of {}",
                self.max_frame_length
            );
            return Err(FrameDecodeError::Frame(self.fail(message)));
        }
        Ok(frame_length)
    }

    fn begin_payload(&mut self, frame_length: u32) {
        let payload_length = usize::try_from(frame_length).expect("u32 frame lengths fit supported 64-bit targets");
        self.expected_payload_length = Some(payload_length);
        self.payload_blocks.clear();
        self.current_payload_block_length = 0;
        self.payload_length = 0;
    }

    fn copy_payload_bytes(
        &mut self,
        chunk: &[u8],
        chunk_offset: &mut usize,
        expected_payload_length: usize,
    ) -> Result<(), FrameDecodeError> {
        while *chunk_offset < chunk.len() && self.payload_length < expected_payload_length {
            if self.needs_payload_block() {
                self.allocate_payload_block(expected_payload_length)?;
            }
            self.copy_into_current_block(chunk, chunk_offset);
        }
        Ok(())
    }

    fn needs_payload_block(&self) -> bool {
        self.payload_blocks
            .last()
            .is_none_or(|block| self.current_payload_block_length == block.len())
    }

    fn allocate_payload_block(&mut self, expected_payload_length: usize) -> Result<(), FrameDecodeError> {
        let remaining_payload = expected_payload_length - self.payload_length;
        let block_length = PAYLOAD_BLOCK_SIZE.min(remaining_payload);
        let mut block = Vec::new();
        if self.allocations.try_reserve_exact(&mut block, block_length).is_err() {
            return Err(self.fail_allocation());
        }
        block.resize(block_length, 0);
        if self.allocations.try_reserve_exact(&mut self.payload_blocks, 1).is_err() {
            return Err(self.fail_allocation());
        }
        self.payload_blocks.push(block);
        self.current_payload_block_length = 0;
        Ok(())
    }

    fn copy_into_current_block(&mut self, chunk: &[u8], chunk_offset: &mut usize) {
        let block = self.payload_blocks.last_mut().expect("payload block must exist");
        let remaining_block = block.len() - self.current_payload_block_length;
        let remaining_chunk = chunk.len() - *chunk_offset;
        let payload_bytes = remaining_block.min(remaining_chunk);
        let block_end = self.current_payload_block_length + payload_bytes;
        let chunk_end = *chunk_offset + payload_bytes;
        block[self.current_payload_block_length..block_end].copy_from_slice(&chunk[*chunk_offset..chunk_end]);
        self.current_payload_block_length = block_end;
        self.payload_length += payload_bytes;
        *chunk_offset = chunk_end;
    }

    fn complete_payload(&mut self, expected_payload_length: usize) -> Result<Vec<u8>, FrameDecodeError> {
        let payload = if self.payload_blocks.len() == 1 {
            self.payload_blocks.pop().expect("completed payload block must exist")
        } else {
            self.merge_payload_blocks(expected_payload_length)?
        };
        self.reset_payload();
        Ok(payload)
    }

    fn merge_payload_blocks(&mut self, expected_payload_length: usize) -> Result<Vec<u8>, FrameDecodeError> {
        let mut payload = Vec::new();
        if self
            .allocations
            .try_reserve_exact(&mut payload, expected_payload_length)
            .is_err()
        {
            return Err(self.fail_allocation());
        }
        for payload_block in &self.payload_blocks {
            payload.extend_from_slice(payload_block);
        }
        Ok(payload)
    }

    fn append_frame(&mut self, frames: &mut Vec<Vec<u8>>, payload: Vec<u8>) -> Result<(), FrameDecodeError> {
        if self.allocations.try_reserve_exact(frames, 1).is_err() {
            return Err(self.fail_allocation());
        }
        frames.push(payload);
        Ok(())
    }

    fn fail(&mut self, message: impl Into<String>) -> FrameError {
        self.state = DecoderState::Failed;
        self.clear_partial_frame();
        FrameError::new(message)
    }

    fn fail_allocation(&mut self) -> FrameDecodeError {
        self.state = DecoderState::Failed;
        self.clear_partial_frame();
        FrameDecodeError::Allocation
    }

    fn clear_partial_frame(&mut self) {
        self.header_length = 0;
        self.reset_payload();
    }

    fn reset_payload(&mut self) {
        self.payload_blocks.clear();
        self.current_payload_block_length = 0;
        self.expected_payload_length = None;
        self.payload_length = 0;
    }
}
