//! Port of the core of `packages/agent`.

pub mod agent_loop;
pub mod proxy;
#[path = "agent.rs"]
pub mod runtime;
mod stream_fn;
pub mod types;

pub use agent_loop::{
    AgentEventSink, AgentEventStream, agent_loop, agent_loop_continue, restored_pending_tool_message, run_agent_loop,
    run_agent_loop_continue,
};
pub use proxy::{ProxyAssistantMessageEvent, ProxyStreamOptions, stream_proxy};
pub use runtime::{
    Agent, AgentInitialState, AgentListener, AgentOptions, ContinueOptions, PrepareNextTurnSignalHook, PromptInput,
};
pub use types::{
    AfterToolCallContext, AfterToolCallHook, AfterToolCallResult, AgentContext, AgentError, AgentEvent,
    AgentLoopConfig, AgentLoopTurnUpdate, AgentMessage, AgentState, AgentTool, AgentToolCall, AgentToolResult,
    AgentToolUpdateCallback, BeforeToolCallContext, BeforeToolCallHook, ConvertToLlm, GetApiKey, MessageQueueHook,
    PrepareNextTurnContext, PrepareNextTurnHook, ShouldStopAfterTurnContext, ShouldStopAfterTurnHook, StreamFn,
    ToolError, TransformContext, reasoning_for,
};
