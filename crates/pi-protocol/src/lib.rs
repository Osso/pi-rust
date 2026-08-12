//! Transport-neutral protocol primitives ported from the pinned Pi TypeScript source.

mod framing;
mod schemas;

pub use framing::{
    DEFAULT_MAX_FRAME_LENGTH, FrameDecodeError, FrameDecoder, FrameEncodeError, FrameError, assert_complete_frame,
    encode_frame,
};
pub use schemas::{
    ImageContent, ImageContentError, ModelCost, ModelCostError, ModelInput, ModelMetadata, ModelMetadataError,
    ModelMetadataInput, ModelRef, ModelRefError, ParseModelInputError, ParseSessionPhaseError, ParseThinkingLevelError,
    SessionMetadata, SessionMetadataError, SessionMetadataInput, SessionPhase, TextContent, ThinkingContent,
    ThinkingLevel, Usage, UsageCost, UsageCostError, UsageInput,
};
