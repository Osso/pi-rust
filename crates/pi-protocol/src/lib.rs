//! Transport-neutral protocol primitives ported from the pinned Pi TypeScript source.

mod framing;

pub use framing::{
    DEFAULT_MAX_FRAME_LENGTH, FrameDecodeError, FrameDecoder, FrameEncodeError, FrameError, assert_complete_frame,
    encode_frame,
};
