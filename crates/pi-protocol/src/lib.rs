//! Transport-neutral protocol primitives ported from the pinned Pi TypeScript source.

mod cbor;
mod codec;
mod framing;
mod schemas;

pub use cbor::{
    CborCodecError, CborError, CborOptions, CborOptionsError, CborValue, DEFAULT_MAX_CBOR_BYTE_LENGTH,
    DEFAULT_MAX_CBOR_CONTAINER_LENGTH, DEFAULT_MAX_CBOR_DEPTH, decode_cbor, encode_cbor,
};
pub use codec::{
    ClientMessageDecoder, ProtocolValidationError, ServerMessageDecoder, create_client_message_decoder,
    create_server_message_decoder, encode_client_message, encode_server_message, is_supported_protocol_version,
    parse_client_message, parse_server_message,
};
pub use framing::{
    DEFAULT_MAX_FRAME_LENGTH, FrameDecodeError, FrameDecoder, FrameEncodeError, FrameError, assert_complete_frame,
    encode_frame,
};
pub use schemas::{
    AbortCommand, AssistantContent, AssistantStopReason, AssistantTranscriptItem, AssistantTranscriptItemInput,
    AssistantTranscriptState, AttachCommand, ClientHello, ClientMessage, Command, CommandError, CommandResult,
    CreateCommand, CreateCommandInput, DetachCommand, DetachResult, ErrorResponse, EventEnvelope,
    FinishedTranscriptItem, ImageContent, ImageContentError, JsonValue, ListCommand, ListResult, ModelCost,
    ModelCostError, ModelInput, ModelMetadata, ModelMetadataError, ModelMetadataInput, ModelRef, ModelRefError,
    PROTOCOL_VERSION, ParseModelInputError, ParseProtocolErrorCodeError, ParseSessionPhaseError,
    ParseThinkingLevelError, PromptCommand, ProtocolError, ProtocolErrorCode, RequestEnvelope, RequestEnvelopeError,
    ResponseEnvelope, SchemaValueError, ServerEvent, ServerHello, ServerHelloError, ServerMessage, ServerSnapshot,
    SessionMetadata, SessionMetadataError, SessionMetadataInput, SessionPhase, SessionProgress, SessionRemoved,
    SessionSnapshot, SessionSnapshotInput, SetModelCommand, SetThinkingCommand, SteerCommand, SuccessResponse,
    TextContent, ThinkingContent, ThinkingLevel, ToolCallContent, ToolContent, ToolTranscriptItem,
    ToolTranscriptItemInput, ToolTranscriptState, TranscriptDeltaKind, TranscriptItem, TranscriptProgress,
    UpdatedTranscriptItem, Usage, UsageCost, UsageCostError, UsageInput, UserContent, UserTranscriptItem,
};
