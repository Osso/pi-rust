//! Port of the core of `packages/agent`.

pub mod stream_fn;
pub mod types;

pub use stream_fn::{get_default_stream_fn, set_default_stream_fn};
pub use types::{
    AfterToolCallContext, AfterToolCallHook, AfterToolCallResult, AgentContext, AgentError, AgentEvent,
    AgentLoopConfig, AgentLoopTurnUpdate, AgentMessage, AgentState, AgentTool, AgentToolCall, AgentToolResult,
    AgentToolUpdateCallback, BeforeToolCallContext, BeforeToolCallHook, ConvertToLlm, GetApiKey, MessageQueueHook,
    PrepareNextTurnContext, PrepareNextTurnHook, ShouldStopAfterTurnContext, ShouldStopAfterTurnHook, StreamFn,
    ToolError, TransformContext, reasoning_for,
};
