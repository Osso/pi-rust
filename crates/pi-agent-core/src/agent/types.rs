//! Port of `packages/agent/src/types.ts`.
//!
//! TypeScript declaration-merging extension of `AgentMessage` maps to
//! [`AgentMessage::Custom`], which keeps the full JSON object (including its
//! `role`) so higher layers can parse their typed messages from it.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, Mutex};

use pi_ai::request::{BoxFuture, SimpleStreamOptions};
use pi_ai::types::{
    AssistantMessage, AssistantMessageEvent, ConstrainedSampling, Context, JsonObject, Message, Model,
    TextOrImageContent, ToolCall, ToolCallExecution, ToolResultMessage, Usage, UserMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::harness::types::{AgentToolExecutionContext, BeforeToolCallResult, ThinkingLevel, ToolExecutionMode};

/// `StreamFn`: produces an assistant message event stream for one request.
///
/// Contract: must not reject; request failures are encoded in the stream as a
/// final `error`/`aborted` assistant message.
pub type StreamFn = Arc<
    dyn Fn(
            Model,
            Context,
            SimpleStreamOptions,
        ) -> BoxFuture<'static, pi_ai::utils::event_stream::AssistantMessageEventStream>
        + Send
        + Sync,
>;

/// Failure surfaced by agent and loop APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError {
    message: String,
}

impl AgentError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn default_stream_fn_missing() -> Self {
        Self::new("No default stream function configured. Pass streamFn explicitly or call setDefaultStreamFn().")
    }

    pub fn already_processing_prompt() -> Self {
        Self::new(
            "Agent is already processing a prompt. Use steer() or followUp() to queue messages, or wait for completion.",
        )
    }

    pub fn already_processing_continue() -> Self {
        Self::new("Agent is already processing. Wait for completion before continuing.")
    }

    pub fn already_processing_reset() -> Self {
        Self::new("Agent is already processing. Wait for completion before resetting.")
    }

    pub fn already_processing() -> Self {
        Self::new("Agent is already processing.")
    }

    pub fn no_messages_to_continue() -> Self {
        Self::new("No messages to continue from")
    }

    pub fn cannot_continue_empty_context() -> Self {
        Self::new("Cannot continue: no messages in context")
    }

    pub fn run_aborted() -> Self {
        Self::new("Agent run aborted")
    }

    pub fn listener_outside_run() -> Self {
        Self::new("Agent listener invoked outside active run")
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AgentError {}

/// Failure returned by [`AgentTool::execute`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ToolError {}

impl From<ToolError> for AgentError {
    fn from(error: ToolError) -> Self {
        Self::new(error.message)
    }
}

/// `Message | CustomAgentMessages[keyof CustomAgentMessages]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentMessage {
    User(UserMessage),
    Assistant(Box<AssistantMessage>),
    ToolResult(ToolResultMessage),
    /// Custom app message: the full JSON object, including `role`.
    Custom(JsonObject),
}

impl AgentMessage {
    /// The message `role` discriminator, including custom roles.
    pub fn role(&self) -> &str {
        match self {
            Self::User(_) => "user",
            Self::Assistant(_) => "assistant",
            Self::ToolResult(_) => "toolResult",
            Self::Custom(object) => object.get("role").and_then(Value::as_str).unwrap_or_default(),
        }
    }

    pub fn as_assistant(&self) -> Option<&AssistantMessage> {
        match self {
            Self::Assistant(message) => Some(message),
            _ => None,
        }
    }

    pub fn as_tool_result(&self) -> Option<&ToolResultMessage> {
        match self {
            Self::ToolResult(message) => Some(message),
            _ => None,
        }
    }

    pub fn as_user(&self) -> Option<&UserMessage> {
        match self {
            Self::User(message) => Some(message),
            _ => None,
        }
    }

    /// Message timestamp when the standard roles carry one.
    pub fn timestamp(&self) -> Option<i64> {
        match self {
            Self::User(message) => Some(message.timestamp),
            Self::Assistant(message) => Some(message.timestamp),
            Self::ToolResult(message) => Some(message.timestamp),
            Self::Custom(object) => object.get("timestamp").and_then(Value::as_i64),
        }
    }
}

impl From<Message> for AgentMessage {
    fn from(message: Message) -> Self {
        match message {
            Message::User(message) => Self::User(message),
            Message::Assistant(message) => Self::Assistant(message),
            Message::ToolResult(message) => Self::ToolResult(message),
        }
    }
}

impl From<UserMessage> for AgentMessage {
    fn from(message: UserMessage) -> Self {
        Self::User(message)
    }
}

impl From<AssistantMessage> for AgentMessage {
    fn from(message: AssistantMessage) -> Self {
        Self::Assistant(Box::new(message))
    }
}

impl From<ToolResultMessage> for AgentMessage {
    fn from(message: ToolResultMessage) -> Self {
        Self::ToolResult(message)
    }
}

/// `AgentToolCall`: the `toolCall` content block of an assistant message.
pub type AgentToolCall = ToolCall;

/// Final or partial result produced by a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolResult {
    pub content: Vec<TextOrImageContent>,
    /// Arbitrary structured details; absent when the tool returned `undefined`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_tool_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Hint that the agent should stop after the current tool batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate: Option<bool>,
}

impl AgentToolResult {
    /// Text-only result with empty details (`createErrorToolResult` shape).
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(text))],
            details: Some(Value::Object(JsonObject::new())),
            usage: None,
            added_tool_names: None,
            is_error: None,
            terminate: None,
        }
    }
}

/// Callback used by tools to stream partial execution updates.
pub type AgentToolUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>;

/// Tool definition used by the agent runtime.
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// JSON Schema for the tool arguments (TypeBox `TSchema` in TypeScript).
    fn parameters(&self) -> &Value;
    /// Human-readable label for UI display.
    fn label(&self) -> &str;
    fn constrained_sampling(&self) -> Option<ConstrainedSampling> {
        None
    }
    /// Per-tool execution mode override.
    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        None
    }
    /// Compatibility shim for raw tool-call arguments before schema validation.
    fn prepare_arguments(&self, args: Value) -> Value {
        args
    }
    /// Prepare durable execution metadata before the assistant tool call is persisted.
    fn prepare_execution(
        &self,
        _tool_call_id: String,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<'static, Option<ToolCallExecution>> {
        Box::pin(async { None })
    }
    /// Execute the tool call. Failures are returned as [`ToolError`].
    fn execute(
        &self,
        tool_call_id: String,
        params: Value,
        signal: Option<CancellationToken>,
        on_update: AgentToolUpdateCallback,
        execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>>;
}

/// Context snapshot passed into the low-level agent loop.
#[derive(Clone)]
pub struct AgentContext {
    pub system_prompt: String,
    pub messages: Vec<AgentMessage>,
    pub tools: Option<Vec<Arc<dyn AgentTool>>>,
}

impl AgentContext {
    pub fn find_tool(&self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.as_ref()?.iter().find(|tool| tool.name() == name).cloned()
    }
}

/// Events emitted by the Agent for UI updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum AgentEvent {
    AgentStart,
    AgentEnd {
        messages: Vec<AgentMessage>,
    },
    TurnStart,
    TurnEnd {
        message: AgentMessage,
        tool_results: Vec<ToolResultMessage>,
    },
    ModelRequestStart,
    ModelRequestEnd,
    MessageStart {
        message: AgentMessage,
    },
    MessageUpdate {
        message: AgentMessage,
        assistant_message_event: Box<AssistantMessageEvent>,
    },
    MessageEnd {
        message: AgentMessage,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        args: Value,
        started_at: f64,
    },
    ToolExecutionUpdate {
        tool_call_id: String,
        tool_name: String,
        args: Value,
        partial_result: AgentToolResult,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: AgentToolResult,
        is_error: bool,
        started_at: f64,
        finished_at: f64,
    },
}

/// Context passed to `beforeToolCall`.
#[derive(Clone)]
pub struct BeforeToolCallContext {
    pub assistant_message: AssistantMessage,
    pub tool_call: AgentToolCall,
    /// Validated arguments. Shared so the hook can mutate them in place, matching
    /// the TypeScript object-reference semantics; the loop does not revalidate.
    pub args: Arc<Mutex<Value>>,
    pub context: AgentContext,
}

impl BeforeToolCallContext {
    pub fn args_snapshot(&self) -> Value {
        self.args
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_args(&self, args: Value) {
        *self.args.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = args;
    }
}

/// Partial override returned from `afterToolCall`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AfterToolCallResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<TextOrImageContent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate: Option<bool>,
}

/// Context passed to `afterToolCall`.
#[derive(Clone)]
pub struct AfterToolCallContext {
    pub assistant_message: AssistantMessage,
    pub tool_call: AgentToolCall,
    pub args: Value,
    pub result: AgentToolResult,
    pub is_error: bool,
    pub context: AgentContext,
}

/// Context passed to `shouldStopAfterTurn` and `prepareNextTurn`.
#[derive(Clone)]
pub struct ShouldStopAfterTurnContext {
    pub message: AssistantMessage,
    pub tool_results: Vec<ToolResultMessage>,
    pub context: AgentContext,
    pub new_messages: Vec<AgentMessage>,
}

/// Alias matching the TypeScript `PrepareNextTurnContext`.
pub type PrepareNextTurnContext = ShouldStopAfterTurnContext;

/// Replacement runtime state used before starting another provider request.
#[derive(Clone)]
pub struct AgentLoopTurnUpdate {
    pub context: Option<AgentContext>,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
}

/// `convertToLlm`: maps agent messages to provider messages before each request.
pub type ConvertToLlm =
    Arc<dyn Fn(Vec<AgentMessage>) -> BoxFuture<'static, Result<Vec<Message>, AgentError>> + Send + Sync>;
/// `transformContext`: agent-message-level context transform before conversion.
pub type TransformContext = Arc<
    dyn Fn(Vec<AgentMessage>, Option<CancellationToken>) -> BoxFuture<'static, Result<Vec<AgentMessage>, AgentError>>
        + Send
        + Sync,
>;
/// `getApiKey`: resolves a key for each provider request.
pub type GetApiKey = Arc<dyn Fn(String) -> BoxFuture<'static, Option<String>> + Send + Sync>;
/// `beforeToolCall` hook.
pub type BeforeToolCallHook = Arc<
    dyn Fn(
            BeforeToolCallContext,
            Option<CancellationToken>,
        ) -> BoxFuture<'static, Result<Option<BeforeToolCallResult>, AgentError>>
        + Send
        + Sync,
>;
/// `afterToolCall` hook.
pub type AfterToolCallHook = Arc<
    dyn Fn(
            AfterToolCallContext,
            Option<CancellationToken>,
        ) -> BoxFuture<'static, Result<Option<AfterToolCallResult>, AgentError>>
        + Send
        + Sync,
>;
/// `shouldStopAfterTurn` hook.
pub type ShouldStopAfterTurnHook = Arc<
    dyn Fn(ShouldStopAfterTurnContext, Option<CancellationToken>) -> BoxFuture<'static, Result<bool, AgentError>>
        + Send
        + Sync,
>;
/// `prepareNextTurn` hook.
pub type PrepareNextTurnHook = Arc<
    dyn Fn(PrepareNextTurnContext) -> BoxFuture<'static, Result<Option<AgentLoopTurnUpdate>, AgentError>> + Send + Sync,
>;
/// Queue poll hook (`getSteeringMessages`, `getFollowUpMessages`).
pub type MessageQueueHook = Arc<dyn Fn() -> BoxFuture<'static, Vec<AgentMessage>> + Send + Sync>;

/// Configuration for the low-level agent loop.
#[derive(Clone)]
pub struct AgentLoopConfig {
    pub model: Model,
    /// `SimpleStreamOptions` forwarded to the stream function.
    pub stream_options: SimpleStreamOptions,
    pub convert_to_llm: ConvertToLlm,
    pub transform_context: Option<TransformContext>,
    pub get_api_key: Option<GetApiKey>,
    pub should_stop_after_turn: Option<ShouldStopAfterTurnHook>,
    pub prepare_next_turn: Option<PrepareNextTurnHook>,
    pub get_steering_messages: Option<MessageQueueHook>,
    pub get_follow_up_messages: Option<MessageQueueHook>,
    /// Default: [`ToolExecutionMode::Parallel`].
    pub tool_execution: ToolExecutionMode,
    pub before_tool_call: Option<BeforeToolCallHook>,
    pub after_tool_call: Option<AfterToolCallHook>,
}

impl AgentLoopConfig {
    pub fn new(model: Model, convert_to_llm: ConvertToLlm) -> Self {
        Self {
            model,
            stream_options: SimpleStreamOptions::default(),
            convert_to_llm,
            transform_context: None,
            get_api_key: None,
            should_stop_after_turn: None,
            prepare_next_turn: None,
            get_steering_messages: None,
            get_follow_up_messages: None,
            tool_execution: ToolExecutionMode::Parallel,
            before_tool_call: None,
            after_tool_call: None,
        }
    }
}

/// Maps the agent thinking level onto the provider reasoning level (`off` is absent).
pub fn reasoning_for(level: ThinkingLevel) -> Option<pi_ai::types::ThinkingLevel> {
    match level {
        ThinkingLevel::Off => None,
        ThinkingLevel::Minimal => Some(pi_ai::types::ThinkingLevel::Minimal),
        ThinkingLevel::Low => Some(pi_ai::types::ThinkingLevel::Low),
        ThinkingLevel::Medium => Some(pi_ai::types::ThinkingLevel::Medium),
        ThinkingLevel::High => Some(pi_ai::types::ThinkingLevel::High),
        ThinkingLevel::XHigh => Some(pi_ai::types::ThinkingLevel::Xhigh),
        ThinkingLevel::Max => Some(pi_ai::types::ThinkingLevel::Max),
        ThinkingLevel::Ultra => Some(pi_ai::types::ThinkingLevel::Ultra),
    }
}

/// Public agent state snapshot.
#[derive(Clone)]
pub struct AgentState {
    pub system_prompt: String,
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub tools: Vec<Arc<dyn AgentTool>>,
    pub messages: Vec<AgentMessage>,
    pub is_streaming: bool,
    pub is_model_request_active: bool,
    pub streaming_message: Option<AgentMessage>,
    pub pending_tool_calls: BTreeSet<String>,
    pub error_message: Option<String>,
}
