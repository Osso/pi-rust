//! Transport-neutral protocol primitives ported from the pinned Pi TypeScript source.

mod framing;
mod schemas;

pub use framing::{
    DEFAULT_MAX_FRAME_LENGTH, FrameDecodeError, FrameDecoder, FrameEncodeError, FrameError, assert_complete_frame,
    encode_frame,
};
pub use schemas::{
    AbortCommand, AttachCommand, Command, CommandError, CreateCommand, CreateCommandInput, DetachCommand, ImageContent,
    ImageContentError, ListCommand, ModelCost, ModelCostError, ModelInput, ModelMetadata, ModelMetadataError,
    ModelMetadataInput, ModelRef, ModelRefError, ParseModelInputError, ParseProtocolErrorCodeError,
    ParseSessionPhaseError, ParseThinkingLevelError, PromptCommand, ProtocolErrorCode, SessionMetadata,
    SessionMetadataError, SessionMetadataInput, SessionPhase, SetModelCommand, SetThinkingCommand, SteerCommand,
    TextContent, ThinkingContent, ThinkingLevel, Usage, UsageCost, UsageCostError, UsageInput,
};
