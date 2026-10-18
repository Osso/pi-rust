//! Port of `packages/coding-agent/src/core/agent-session.ts`.
//!
//! Extension-runner, permission-reviewer, supervisor, multi-agent, and
//! control-db metadata integration are owned by parallel slices; every call
//! site that would reach them is recorded as a gap in
//! `docs/porting/traceability/agent-session.md`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use pi_agent_core::agent::types::{
    AfterToolCallContext, AfterToolCallResult, AgentError, AgentEvent, AgentMessage, AgentState, AgentTool,
    AgentToolCall, BeforeToolCallContext, PrepareNextTurnContext, ShouldStopAfterTurnContext,
};
use pi_agent_core::agent::{Agent, AgentListener};
use pi_agent_core::harness::types::BeforeToolCallResult;
use pi_agent_core::harness::compaction::{
    ContextUsageEstimate, calculate_context_tokens, estimate_context_tokens, estimate_tokens,
};
use pi_agent_core::harness::messages::{
    BashExecutionMessage, BashExecutionRole, CustomMessageContent, create_custom_message,
};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::types::{
    AssistantContent, AssistantMessage, ImageContent, InputSource, Model, ProviderHeaders, ProviderRetryEvent,
    StopReason, TextContent, TextOrImageContent, Usage, UserContent, UserMessage, UserRole,
};
use serde::Serialize;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::core::defaults::DEFAULT_THINKING_LEVEL;
use crate::core::model_registry::ModelRegistry;
use crate::core::model_resolver::{
    ScopedModel, find_exact_model_reference_match, resolve_model_scope,
};
use crate::core::model_runtime::ModelRuntime;
use crate::core::permissions::policy::{ApprovalPolicy, evaluate_approval_policy};
use crate::core::prompt_templates::expand_prompt_template;
use crate::core::resource_loader::{LoadedPrompts, ResourceLoader};
use crate::core::session_manager::{
    SessionEntry, SessionManager, SessionManagerError, get_latest_compaction_entry,
};
use crate::core::settings_manager::SettingsManager;
use crate::core::slash_commands::builtin_slash_commands;
use crate::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
use crate::core::system_prompt::{BuildSystemPromptOptions, build_system_prompt};
use crate::utils::frontmatter::strip_frontmatter;

const BUILT_IN_COMPACTION_DISABLED_MESSAGE: &str =
    "Built-in compaction is disabled; enable compaction or configure a compaction extension";
const DUPLICATE_TURN_GUARD_PROMPT: &str =
    "You repeated the same response. Stop looping. Call `end_turn` with a concise reason describing the current state.";
const THINKING_PHASE_TIMEOUT_MS: u64 = 15 * 60 * 1000;

/// Standard thinking levels.
pub const THINKING_LEVELS: [ThinkingLevel; 5] = [
    ThinkingLevel::Off,
    ThinkingLevel::Minimal,
    ThinkingLevel::Low,
    ThinkingLevel::Medium,
    ThinkingLevel::High,
];

/// Failure surfaced by [`AgentSession`] operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionError {
    message: String,
}

impl AgentSessionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for AgentSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AgentSessionError {}

impl From<SessionManagerError> for AgentSessionError {
    fn from(error: SessionManagerError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<AgentError> for AgentSessionError {
    fn from(error: AgentError) -> Self {
        Self::new(error.message())
    }
}

fn error(message: impl Into<String>) -> AgentSessionError {
    AgentSessionError::new(message)
}

/// `MultiAgentRuntimeRole`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiAgentRuntimeRole {
    Standalone,
    Orchestrator,
    Child,
    Observer,
}

/// `MultiAgentExecutionCapability` (opaque capability token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiAgentExecutionCapability {
    _private: (),
}

/// `createMultiAgentExecutionCapability()`.
pub fn create_multi_agent_execution_capability() -> MultiAgentExecutionCapability {
    MultiAgentExecutionCapability { _private: () }
}

/// `streamingBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingBehavior {
    Steer,
    FollowUp,
}

/// One tool definition plus its executable.
///
/// Mirrors `ToolDefinition` from `extensions/types.ts`; it moves to
/// `ca-extensions-api` when that slice merges.
#[derive(Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub prompt_snippet: Option<String>,
    pub prompt_guidelines: Vec<String>,
    pub approval_required: bool,
    pub tool: Arc<dyn AgentTool>,
}

impl ToolDefinition {
    /// Builds a definition around an existing agent tool.
    pub fn from_tool(tool: Arc<dyn AgentTool>) -> Self {
        Self {
            name: tool.name().to_owned(),
            description: tool.description().to_owned(),
            parameters: tool.parameters().clone(),
            prompt_snippet: None,
            prompt_guidelines: Vec::new(),
            approval_required: true,
            tool,
        }
    }
}

/// `ToolInfo` returned by `getAllTools`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub prompt_guidelines: Vec<String>,
    pub source_info: crate::core::source_info::SourceInfo,
}

#[derive(Clone)]
struct ToolDefinitionEntry {
    definition: ToolDefinition,
    source_info: crate::core::source_info::SourceInfo,
}

/// `AgentSessionConfig`.
pub struct AgentSessionConfig {
    pub agent: Agent,
    pub session_manager: Arc<Mutex<SessionManager>>,
    pub settings_manager: Arc<Mutex<SettingsManager>>,
    pub cwd: String,
    pub scoped_models: Vec<ScopedModel>,
    pub resource_loader: Arc<dyn ResourceLoader>,
    pub model_runtime: ModelRuntime,
    pub initial_active_tool_names: Option<Vec<String>>,
    pub allowed_tool_names: Option<Vec<String>>,
    pub excluded_tool_names: Option<Vec<String>>,
    pub permission_prompt_tool: Option<String>,
    pub agent_dir: Option<String>,
    pub control_db_path: Option<String>,
    pub base_tools: Vec<Arc<dyn AgentTool>>,
    pub custom_tools: Vec<ToolDefinition>,
    pub multi_agent_runtime_role: Option<MultiAgentRuntimeRole>,
    pub multi_agent_execution_capability: Option<MultiAgentExecutionCapability>,
    pub thinking_phase_timeout_ms: Option<u64>,
}

/// `PromptOptions`.
#[derive(Default)]
pub struct PromptOptions {
    pub expand_prompt_templates: Option<bool>,
    pub images: Vec<ImageContent>,
    pub streaming_behavior: Option<StreamingBehavior>,
    pub source: Option<InputSource>,
    pub preflight_result: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    pub on_user_message_prepared: Option<Arc<dyn Fn(&AgentMessage) + Send + Sync>>,
}

/// `ModelCycleResult`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCycleResult {
    pub model: Model,
    #[serde(serialize_with = "serialize_thinking_level")]
    pub thinking_level: ThinkingLevel,
    pub is_scoped: bool,
}

/// Session statistics for `/session`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub session_file: Option<String>,
    pub session_id: String,
    pub user_messages: u64,
    pub assistant_messages: u64,
    pub tool_calls: u64,
    pub tool_results: u64,
    pub total_messages: u64,
    pub tokens: SessionTokenStats,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub cost: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_usage: Option<ContextUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTokenStats {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub input: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub output: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub cache_read: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub cache_write: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub total: f64,
}

/// `ContextUsage`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub tokens: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub context_window: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub percent: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct SessionUsageTotals {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
    cost: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct SessionStatsTotals {
    user_messages: u64,
    assistant_messages: u64,
    tool_calls: u64,
    tool_results: u64,
    total_messages: u64,
    usage: SessionUsageTotals,
}

fn add_usage(mut total: SessionUsageTotals, usage: &Usage) -> SessionUsageTotals {
    total.input += usage.input;
    total.output += usage.output;
    total.cache_read += usage.cache_read;
    total.cache_write += usage.cache_write;
    total.cost += usage.cost.total;
    total
}

fn add_session_entry(mut total: SessionStatsTotals, entry: &SessionEntry) -> SessionStatsTotals {
    if entry.entry_type() == "branch_summary"
        && let Some(usage) = branch_summary_usage(entry)
    {
        total.usage = add_usage(total.usage, &usage);
        return total;
    }
    if entry.entry_type() != "message" {
        return total;
    }
    let Some(message) = entry.as_message().map(|entry| entry.message.as_ref()) else {
        return total;
    };
    total.total_messages += 1;
    match message {
        AgentMessage::User(_) => total.user_messages += 1,
        AgentMessage::ToolResult(result) => {
            total.tool_results += 1;
            if let Some(usage) = &result.usage {
                total.usage = add_usage(total.usage, usage);
            }
        }
        AgentMessage::Assistant(assistant) => {
            total.assistant_messages += 1;
            total.tool_calls += assistant
                .content
                .iter()
                .filter(|content| matches!(content, AssistantContent::ToolCall(_)))
                .count() as u64;
            total.usage = add_usage(total.usage, &assistant.usage);
        }
        AgentMessage::Custom(_) => {}
    }
    total
}

fn branch_summary_usage(entry: &SessionEntry) -> Option<Usage> {
    let SessionEntry::Known(crate::core::session_manager::KnownSessionEntry::BranchSummary(summary)) = entry else {
        return None;
    };
    summary.usage.as_deref().cloned()
}

/// `AgentSessionEvent`.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum AgentSessionEvent {
    Agent(AgentEvent),
    AgentEnd(AgentEndEvent),
    QueueUpdate(QueueUpdateEvent),
    SteeringMessageQueued(SteeringMessageQueuedEvent),
    AgentSettled(AgentSettledEvent),
    CompactionStart(CompactionStartEvent),
    CompactionEnd(CompactionEndEvent),
    EntryAppended(EntryAppendedEvent),
    BashMessagesCommitted(BashMessagesCommittedEvent),
    SessionInfoChanged(SessionInfoChangedEvent),
    ThinkingLevelChanged(ThinkingLevelChangedEvent),
    AutoRetryStart(AutoRetryStartEvent),
    AutoRetryEnd(AutoRetryEndEvent),
    ProviderStreamRetry(ProviderStreamRetryEvent),
    BashExecutionUpdate(BashExecutionUpdateEvent),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEndEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub messages: Vec<AgentMessage>,
    pub will_retry: bool,
}

impl AgentEndEvent {
    fn new(messages: Vec<AgentMessage>, will_retry: bool) -> Self {
        Self {
            event_type: "agent_end",
            messages,
            will_retry,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueUpdateEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub steering: Vec<String>,
    #[serde(rename = "followUp")]
    pub follow_up: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SteeringMessageQueuedEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSettledEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionStartEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hint: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionEndEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub reason: String,
    pub result: Option<Value>,
    pub aborted: bool,
    pub will_retry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntryAppendedEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub entry: SessionEntry,
}

#[derive(Debug, Clone, Serialize)]
pub struct BashMessagesCommittedEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfoChangedEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThinkingLevelChangedEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    #[serde(serialize_with = "serialize_thinking_level")]
    pub level: ThinkingLevel,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRetryStartEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub attempt: u64,
    pub max_attempts: u64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub delay_ms: f64,
    pub error_message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRetryEndEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub success: bool,
    pub attempt: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStreamRetryEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub retry: ProviderRetryEvent,
}

#[derive(Debug, Clone, Serialize)]
pub struct BashExecutionUpdateEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub delta: String,
}

/// Listener function for agent session events.
pub type AgentSessionEventListener = Arc<dyn Fn(&AgentSessionEvent) + Send + Sync>;

/// Options for [`AgentSession::navigate_tree`].
#[derive(Debug, Clone, Default)]
pub struct NavigateTreeOptions {
    pub summarize: Option<bool>,
    pub custom_instructions: Option<String>,
    pub replace_instructions: Option<bool>,
    pub label: Option<String>,
}

/// Result of [`AgentSession::navigate_tree`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateTreeResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_text: Option<String>,
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aborted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_entry: Option<SessionEntry>,
}

// =============================================================================
// Helpers
// =============================================================================

/// `getAssistantMessageText`.
pub fn get_assistant_message_text(message: &AssistantMessage) -> String {
    message
        .content
        .iter()
        .filter_map(|content| match content {
            AssistantContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect()
}

fn user_message_text(content: &UserContent) -> String {
    match content {
        UserContent::Text(text) => text.clone(),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                pi_ai::types::UserContentBlock::Text(text) => Some(text.text.as_str()),
                pi_ai::types::UserContentBlock::Image(_) => None,
            })
            .collect(),
    }
}

fn find_last_user_text(messages: &[AgentMessage]) -> Option<String> {
    for message in messages.iter().rev() {
        let AgentMessage::User(user) = message else {
            continue;
        };
        return Some(user_message_text(&user.content));
    }
    None
}

fn is_record(value: &Value) -> bool {
    value.is_object()
}

fn to_record(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn without_deleted_headers(headers: Option<&ProviderHeaders>) -> Option<ProviderHeaders> {
    headers.map(|headers| {
        headers
            .iter()
            .filter(|(_, value)| value.is_some())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    })
}

fn is_allowed_session_tool(
    name: &str,
    allowed: Option<&HashSet<String>>,
    excluded: Option<&HashSet<String>>,
) -> bool {
    if name == "end_turn" {
        return true;
    }
    allowed.is_none_or(|allowed| allowed.contains(name)) && !excluded.is_some_and(|excluded| excluded.contains(name))
}

fn normalize_message_content(message: AgentMessage) -> AgentMessage {
    match message {
        AgentMessage::Custom(mut object) => {
            if object.get("content").is_some_and(Value::is_null) {
                object.insert("content".to_owned(), Value::Array(Vec::new()));
            }
            AgentMessage::Custom(object)
        }
        other => other,
    }
}

fn thinking_level_from_model(level: pi_ai::types::ModelThinkingLevel) -> ThinkingLevel {
    match level {
        pi_ai::types::ModelThinkingLevel::Off => ThinkingLevel::Off,
        pi_ai::types::ModelThinkingLevel::Minimal => ThinkingLevel::Minimal,
        pi_ai::types::ModelThinkingLevel::Low => ThinkingLevel::Low,
        pi_ai::types::ModelThinkingLevel::Medium => ThinkingLevel::Medium,
        pi_ai::types::ModelThinkingLevel::High => ThinkingLevel::High,
        pi_ai::types::ModelThinkingLevel::Xhigh => ThinkingLevel::XHigh,
        pi_ai::types::ModelThinkingLevel::Max => ThinkingLevel::Max,
        pi_ai::types::ModelThinkingLevel::Ultra => ThinkingLevel::Ultra,
    }
}

fn model_thinking_level(level: ThinkingLevel) -> pi_ai::types::ModelThinkingLevel {
    match level {
        ThinkingLevel::Off => pi_ai::types::ModelThinkingLevel::Off,
        ThinkingLevel::Minimal => pi_ai::types::ModelThinkingLevel::Minimal,
        ThinkingLevel::Low => pi_ai::types::ModelThinkingLevel::Low,
        ThinkingLevel::Medium => pi_ai::types::ModelThinkingLevel::Medium,
        ThinkingLevel::High => pi_ai::types::ModelThinkingLevel::High,
        ThinkingLevel::XHigh => pi_ai::types::ModelThinkingLevel::Xhigh,
        ThinkingLevel::Max => pi_ai::types::ModelThinkingLevel::Max,
        ThinkingLevel::Ultra => pi_ai::types::ModelThinkingLevel::Ultra,
    }
}

fn assistant_tool_calls(message: &AssistantMessage) -> Vec<&AgentToolCall> {
    message
        .content
        .iter()
        .filter_map(|content| match content {
            AssistantContent::ToolCall(call) => Some(call),
            _ => None,
        })
        .collect()
}

fn find_trailing_assistant_tool_batch(messages: &[AgentMessage]) -> Option<&AssistantMessage> {
    for message in messages.iter().rev() {
        match message {
            AgentMessage::ToolResult(_) => continue,
            AgentMessage::Assistant(assistant) => return Some(assistant),
            _ => return None,
        }
    }
    None
}

fn is_completed_end_turn_batch(messages: &[AgentMessage], tool_calls: &[&AgentToolCall]) -> bool {
    if tool_calls.is_empty() || tool_calls.iter().any(|call| call.name != "end_turn") {
        return false;
    }
    tool_calls.iter().all(|call| {
        messages.iter().any(|message| {
            let AgentMessage::ToolResult(result) = message else {
                return false;
            };
            result.tool_call_id == call.id && result.tool_name == "end_turn" && !result.is_error
        })
    })
}

/// `shouldContinueInterruptedSession`.
pub fn should_continue_interrupted_session(messages: &[AgentMessage]) -> bool {
    let mut last_message_index = messages.len() as isize - 1;
    while last_message_index >= 0 {
        let message = &messages[last_message_index as usize];
        let is_self_restart = matches!(
            message,
            AgentMessage::Custom(object)
                if object.get("customType").and_then(Value::as_str) == Some("self_restart")
        );
        if !is_self_restart {
            break;
        }
        last_message_index -= 1;
    }
    let relevant = &messages[..=last_message_index.max(-1) as usize];
    let Some(last) = relevant.last() else {
        return false;
    };
    if matches!(last, AgentMessage::User(_)) {
        return true;
    }
    if !matches!(last, AgentMessage::Assistant(_) | AgentMessage::ToolResult(_)) {
        return false;
    }
    let assistant = match last {
        AgentMessage::Assistant(assistant) => Some(assistant.as_ref()),
        AgentMessage::ToolResult(_) => find_trailing_assistant_tool_batch(&relevant[..relevant.len() - 1]),
        _ => None,
    };
    let Some(assistant) = assistant else {
        return matches!(last, AgentMessage::ToolResult(_));
    };
    if assistant.stop_reason == StopReason::Aborted {
        return true;
    }
    let tool_calls = assistant_tool_calls(assistant);
    if is_completed_end_turn_batch(relevant, &tool_calls) {
        return false;
    }
    !tool_calls.is_empty() && tool_calls.iter().all(|call| call.name != "resume_session")
}

fn estimate_messages_tokens(messages: &[AgentMessage]) -> f64 {
    messages.iter().map(estimate_tokens).sum()
}

fn normalize_required_request_auth_error(message: String, provider: &str) -> AgentSessionError {
    if message == "authHeader requires a resolved API key" {
        return error(crate::core::auth_guidance::format_no_api_key_found_message(provider));
    }
    error(message)
}

fn session_model_message(provider: &str, model_id: &str) -> String {
    format!("Could not restore model {provider}/{model_id}")
}

// =============================================================================
// AgentSession
// =============================================================================

struct Inner {
    agent: Agent,
    session_manager: Arc<Mutex<SessionManager>>,
    settings_manager: Arc<Mutex<SettingsManager>>,
    cwd: String,
    agent_dir: String,
    control_db_path: Option<String>,
    scoped_models: Mutex<Vec<ScopedModel>>,
    resource_loader: Arc<dyn ResourceLoader>,
    model_runtime: ModelRuntime,
    model_registry: ModelRegistry,
    custom_tools: Vec<ToolDefinition>,
    base_tool_definitions: Vec<(String, ToolDefinition)>,
    tool_definitions: Mutex<Vec<(String, ToolDefinitionEntry)>>,
    tool_prompt_snippets: Mutex<HashMap<String, String>>,
    tool_prompt_guidelines: Mutex<HashMap<String, Vec<String>>>,
    tool_registry: Mutex<HashMap<String, Arc<dyn AgentTool>>>,
    initial_active_tool_names: Option<Vec<String>>,
    allowed_tool_names: Option<HashSet<String>>,
    excluded_tool_names: Option<HashSet<String>>,
    base_system_prompt: Mutex<String>,
    base_system_prompt_options: Mutex<Option<BuildSystemPromptOptions>>,
    system_prompt_override: Mutex<Option<String>>,
    event_listeners: Mutex<Vec<(u64, AgentSessionEventListener)>>,
    next_listener_id: AtomicU64,
    agent_subscription: Mutex<Option<u64>>,
    steering_messages: Mutex<Vec<String>>,
    follow_up_messages: Mutex<Vec<String>>,
    last_assistant_turn_fingerprint: Mutex<Option<String>>,
    duplicate_turn_guard_injected: AtomicBool,
    duplicate_turn_loop_detected: AtomicBool,
    pending_next_turn_messages: Mutex<Vec<AgentMessage>>,
    external_user_input_reservations: AtomicUsize,
    resume_continuation_requested: AtomicBool,
    turn_start_lock: tokio::sync::Mutex<()>,
    last_assistant_message: Mutex<Option<AssistantMessage>>,
    retry_attempt: AtomicU64,
    retry_abort_controller: Mutex<Option<CancellationToken>>,
    quota_fallback_attempted: AtomicBool,
    compaction_abort_controller: Mutex<Option<CancellationToken>>,
    auto_compaction_abort_controller: Mutex<Option<CancellationToken>>,
    compaction_summary_materialization_abort_controller: Mutex<Option<CancellationToken>>,
    branch_summary_abort_controller: Mutex<Option<CancellationToken>>,
    pending_bash_messages: Mutex<Vec<BashExecutionMessage>>,
    bash_abort_controllers: Mutex<Vec<CancellationToken>>,
    disposed: AtomicBool,
}

/// Core session abstraction shared by all run modes.
#[derive(Clone)]
pub struct AgentSession {
    inner: Arc<Inner>,
}

impl AgentSession {
    /// Builds a session and installs agent hooks, subscriptions, and tools.
    pub fn new(config: AgentSessionConfig) -> Result<Self, AgentSessionError> {
        validate_multi_agent_runtime_role(
            config.multi_agent_runtime_role,
            config.multi_agent_execution_capability,
        )?;

        let model_runtime = config.model_runtime.clone();
        let model_registry = ModelRegistry::new(model_runtime.clone());
        let base_tool_definitions = config
            .base_tools
            .iter()
            .map(|tool| (tool.name().to_owned(), ToolDefinition::from_tool(Arc::clone(tool))))
            .collect::<Vec<_>>();
        let agent_dir = config
            .agent_dir
            .clone()
            .unwrap_or_else(|| crate::config::get_agent_dir().display().to_string());

        let inner = Arc::new(Inner {
            agent: config.agent,
            session_manager: config.session_manager,
            settings_manager: config.settings_manager,
            cwd: config.cwd,
            agent_dir,
            control_db_path: config.control_db_path,
            scoped_models: Mutex::new(config.scoped_models),
            resource_loader: config.resource_loader,
            model_runtime,
            model_registry,
            custom_tools: config.custom_tools,
            base_tool_definitions,
            tool_definitions: Mutex::new(Vec::new()),
            tool_prompt_snippets: Mutex::new(HashMap::new()),
            tool_prompt_guidelines: Mutex::new(HashMap::new()),
            tool_registry: Mutex::new(HashMap::new()),
            initial_active_tool_names: config.initial_active_tool_names,
            allowed_tool_names: config
                .allowed_tool_names
                .map(|names| names.into_iter().collect()),
            excluded_tool_names: config
                .excluded_tool_names
                .map(|names| names.into_iter().collect()),
            base_system_prompt: Mutex::new(String::new()),
            base_system_prompt_options: Mutex::new(None),
            system_prompt_override: Mutex::new(None),
            event_listeners: Mutex::new(Vec::new()),
            next_listener_id: AtomicU64::new(1),
            agent_subscription: Mutex::new(None),
            steering_messages: Mutex::new(Vec::new()),
            follow_up_messages: Mutex::new(Vec::new()),
            last_assistant_turn_fingerprint: Mutex::new(None),
            duplicate_turn_guard_injected: AtomicBool::new(false),
            duplicate_turn_loop_detected: AtomicBool::new(false),
            pending_next_turn_messages: Mutex::new(Vec::new()),
            external_user_input_reservations: AtomicUsize::new(0),
            resume_continuation_requested: AtomicBool::new(false),
            turn_start_lock: tokio::sync::Mutex::new(()),
            last_assistant_message: Mutex::new(None),
            retry_attempt: AtomicU64::new(0),
            retry_abort_controller: Mutex::new(None),
            quota_fallback_attempted: AtomicBool::new(false),
            compaction_abort_controller: Mutex::new(None),
            auto_compaction_abort_controller: Mutex::new(None),
            compaction_summary_materialization_abort_controller: Mutex::new(None),
            branch_summary_abort_controller: Mutex::new(None),
            pending_bash_messages: Mutex::new(Vec::new()),
            bash_abort_controllers: Mutex::new(Vec::new()),
            disposed: AtomicBool::new(false),
        });

        let session = Self { inner };
        session.install_agent_hooks();
        session.install_agent_next_turn_refresh();
        session.install_duplicate_turn_stop();
        session.subscribe_agent();
        session.build_runtime(
            session.inner.initial_active_tool_names.clone(),
            None,
            true,
        );
        Ok(session)
    }

    fn subscribe_agent(&self) {
        let weak = Arc::downgrade(&self.inner);
        let listener: AgentListener = Arc::new(move |event: AgentEvent, _signal: CancellationToken| {
            let weak = weak.clone();
            Box::pin(async move {
                if let Some(inner) = weak.upgrade() {
                    let session = AgentSession { inner };
                    session.handle_agent_event(event).await;
                }
                Ok(())
            })
        });
        let id = self.inner.agent.subscribe(listener);
        *lock(&self.inner.agent_subscription) = Some(id);
    }

    fn install_agent_hooks(&self) {
        let weak = Arc::downgrade(&self.inner);
        self.inner.agent.set_before_tool_call(Some(Arc::new(
            move |context: BeforeToolCallContext, _signal: Option<CancellationToken>| {
                let weak = weak.clone();
                Box::pin(async move {
                    let Some(inner) = weak.upgrade() else {
                        return Ok(None);
                    };
                    let session = AgentSession { inner };
                    Ok(session.before_tool_call(context))
                })
            },
        )));
        let weak = Arc::downgrade(&self.inner);
        self.inner.agent.set_after_tool_call(Some(Arc::new(
            move |context: AfterToolCallContext, _signal: Option<CancellationToken>| {
                let weak = weak.clone();
                Box::pin(async move {
                    let Some(inner) = weak.upgrade() else {
                        return Ok(None);
                    };
                    let session = AgentSession { inner };
                    Ok(session.after_tool_call(context))
                })
            },
        )));
    }

    fn install_agent_next_turn_refresh(&self) {
        let previous_with_context = self.inner.agent.prepare_next_turn_with_context_hook();
        let previous_signal_only = self.inner.agent.prepare_next_turn_hook();
        let weak = Arc::downgrade(&self.inner);
        self.inner.agent.set_prepare_next_turn_with_context(Some(Arc::new(
            move |turn: PrepareNextTurnContext, signal: Option<CancellationToken>| {
                let weak = weak.clone();
                let previous_with_context = previous_with_context.clone();
                let previous_signal_only = previous_signal_only.clone();
                Box::pin(async move {
                    let previous_snapshot = if let Some(hook) = previous_with_context {
                        hook(turn.clone(), signal.clone()).await?
                    } else if let Some(hook) = previous_signal_only {
                        hook(signal.clone()).await?
                    } else {
                        None
                    };
                    let Some(inner) = weak.upgrade() else {
                        return Ok(previous_snapshot);
                    };
                    let previous_context = previous_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.context.clone())
                        .unwrap_or(turn.context);
                    let state = inner.agent.state();
                    Ok(Some(pi_agent_core::agent::types::AgentLoopTurnUpdate {
                        context: Some(pi_agent_core::agent::types::AgentContext {
                            system_prompt: lock(&inner.system_prompt_override)
                                .clone()
                                .unwrap_or_else(|| lock(&inner.base_system_prompt).clone()),
                            messages: previous_context.messages,
                            tools: Some(state.tools),
                        }),
                        model: Some(state.model),
                        thinking_level: Some(state.thinking_level),
                    }))
                })
            },
        )));
    }

    fn install_duplicate_turn_stop(&self) {
        let previous = self.inner.agent.should_stop_after_turn_hook();
        let weak = Arc::downgrade(&self.inner);
        self.inner.agent.set_should_stop_after_turn(Some(Arc::new(
            move |turn: ShouldStopAfterTurnContext, signal: Option<CancellationToken>| {
                let weak = weak.clone();
                let previous = previous.clone();
                Box::pin(async move {
                    let loop_detected = weak
                        .upgrade()
                        .is_some_and(|inner| inner.duplicate_turn_loop_detected.load(Ordering::SeqCst));
                    if loop_detected {
                        return Ok(true);
                    }
                    match previous {
                        Some(hook) => hook(turn, signal).await,
                        None => Ok(false),
                    }
                })
            },
        )));
    }

    fn build_runtime(
        &self,
        active_tool_names: Option<Vec<String>>,
        _flag_values: Option<BTreeMap<String, Value>>,
        include_all_extension_tools: bool,
    ) {
        self.refresh_tool_registry(active_tool_names, include_all_extension_tools);
    }

    fn refresh_tool_registry(
        &self,
        active_tool_names: Option<Vec<String>>,
        include_all_extension_tools: bool,
    ) {
        let previous_registry_names: HashSet<String> = lock(&self.inner.tool_registry).keys().cloned().collect();
        let previous_active_tool_names = self.get_active_tool_names();
        let allowed = self.inner.allowed_tool_names.clone();
        let excluded = self.inner.excluded_tool_names.clone();
        let is_allowed = |name: &str| is_allowed_session_tool(name, allowed.as_ref(), excluded.as_ref());

        let mut definition_registry: Vec<(String, ToolDefinitionEntry)> = self
            .inner
            .base_tool_definitions
            .iter()
            .filter(|(name, _)| is_allowed(name))
            .map(|(name, definition)| {
                (
                    name.clone(),
                    ToolDefinitionEntry {
                        definition: definition.clone(),
                        source_info: create_synthetic_source_info(
                            &format!("<builtin:{name}>"),
                            &SyntheticSourceInfoOptions {
                                source: "builtin".to_owned(),
                                scope: None,
                                origin: None,
                                base_dir: None,
                            },
                        ),
                    },
                )
            })
            .collect();

        let custom_tools = self
            .inner
            .custom_tools
            .iter()
            .filter(|definition| is_allowed(&definition.name))
            .map(|definition| ToolDefinitionEntry {
                definition: definition.clone(),
                source_info: create_synthetic_source_info(
                    &format!("<sdk:{}>", definition.name),
                    &SyntheticSourceInfoOptions {
                        source: "sdk".to_owned(),
                        scope: None,
                        origin: None,
                        base_dir: None,
                    },
                ),
            })
            .collect::<Vec<_>>();
        for entry in &custom_tools {
            if let Some(existing) = definition_registry
                .iter_mut()
                .find(|(name, _)| *name == entry.definition.name)
            {
                existing.1 = entry.clone();
            } else {
                definition_registry.push((entry.definition.name.clone(), entry.clone()));
            }
        }

        let mut snippets = HashMap::new();
        let mut guidelines = HashMap::new();
        for (_, entry) in &definition_registry {
            if let Some(snippet) = normalize_prompt_snippet(entry.definition.prompt_snippet.as_deref()) {
                snippets.insert(entry.definition.name.clone(), snippet);
            }
            let normalized = normalize_prompt_guidelines(&entry.definition.prompt_guidelines);
            if !normalized.is_empty() {
                guidelines.insert(entry.definition.name.clone(), normalized);
            }
        }

        let mut tool_registry = HashMap::new();
        for (name, entry) in &definition_registry {
            tool_registry.insert(name.clone(), Arc::clone(&entry.definition.tool));
        }

        let mut next_active_tool_names = active_tool_names
            .clone()
            .unwrap_or_else(|| previous_active_tool_names.clone())
            .into_iter()
            .filter(|name| is_allowed(name))
            .collect::<Vec<_>>();

        if let Some(allowed) = &allowed {
            for name in tool_registry.keys() {
                if allowed.contains(name) {
                    next_active_tool_names.push(name.clone());
                }
            }
        } else if include_all_extension_tools {
            for entry in &custom_tools {
                next_active_tool_names.push(entry.definition.name.clone());
            }
        } else if active_tool_names.is_none() {
            for name in tool_registry.keys() {
                if !previous_registry_names.contains(name) {
                    next_active_tool_names.push(name.clone());
                }
            }
        }

        next_active_tool_names.push("end_turn".to_owned());
        let unique: Vec<String> = next_active_tool_names
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        *lock(&self.inner.tool_definitions) = definition_registry;
        *lock(&self.inner.tool_prompt_snippets) = snippets;
        *lock(&self.inner.tool_prompt_guidelines) = guidelines;
        *lock(&self.inner.tool_registry) = tool_registry;
        self.apply_active_tools(&unique);
    }

    fn apply_active_tools(&self, tool_names: &[String]) {
        let registry = lock(&self.inner.tool_registry);
        let mut tools: Vec<Arc<dyn AgentTool>> = Vec::new();
        let mut valid_names: Vec<String> = Vec::new();
        for name in tool_names {
            if let Some(tool) = registry.get(name) {
                tools.push(Arc::clone(tool));
                valid_names.push(name.clone());
            }
        }
        drop(registry);
        self.inner.agent.set_tools(tools);
        let rebuilt = self.rebuild_system_prompt(&valid_names);
        *lock(&self.inner.base_system_prompt) = rebuilt.clone();
        let prompt = lock(&self.inner.system_prompt_override).clone().unwrap_or(rebuilt);
        self.inner.agent.set_system_prompt(prompt);
    }

    fn rebuild_system_prompt(&self, tool_names: &[String]) -> String {
        let registry = lock(&self.inner.tool_registry);
        let valid_tool_names: Vec<String> = tool_names
            .iter()
            .filter(|name| registry.contains_key(*name))
            .cloned()
            .collect();
        drop(registry);
        let mut tool_snippets = HashMap::new();
        let mut prompt_guidelines = Vec::new();
        for name in &valid_tool_names {
            if let Some(snippet) = lock(&self.inner.tool_prompt_snippets).get(name) {
                tool_snippets.insert(name.clone(), snippet.clone());
            }
            if let Some(guidelines) = lock(&self.inner.tool_prompt_guidelines).get(name) {
                prompt_guidelines.extend(guidelines.iter().cloned());
            }
        }

        let loader_system_prompt = self.inner.resource_loader.get_system_prompt();
        let append = self.inner.resource_loader.get_append_system_prompt();
        let append_system_prompt = (!append.is_empty()).then(|| append.join("\n\n"));
        let skills = self.inner.resource_loader.get_skills().skills;
        let context_files = self.inner.resource_loader.get_agents_files().agents_files;
        let rules_content = self.inner.resource_loader.get_rules_content();

        let options = BuildSystemPromptOptions {
            cwd: self.inner.cwd.clone(),
            skills: Some(skills),
            context_files: Some(context_files),
            rules_content,
            custom_prompt: loader_system_prompt,
            append_system_prompt,
            selected_tools: Some(valid_tool_names),
            tool_snippets: Some(tool_snippets),
            prompt_guidelines: Some(prompt_guidelines),
        };
        let prompt = build_system_prompt(&options);
        *lock(&self.inner.base_system_prompt_options) = Some(options);
        prompt
    }

    fn before_tool_call(&self, context: BeforeToolCallContext) -> Option<BeforeToolCallResult> {
        let tool_name = context.tool_call.name.clone();
        let approval_required = self
            .find_tool_definition(&tool_name)
            .is_none_or(|entry| entry.definition.approval_required);
        let policy = lock(&self.inner.settings_manager).get_approval_policy();
        let decision = evaluate_approval_policy(
            policy,
            crate::core::permissions::policy::ApprovalPolicyContext { approval_required },
        );
        match decision {
            crate::core::permissions::policy::ApprovalPolicyDecision::Allow => None,
            crate::core::permissions::policy::ApprovalPolicyDecision::Block { reason } => {
                Some(BeforeToolCallResult {
                    block: Some(true),
                    reason: Some(reason),
                    terminate: None,
                })
            }
            crate::core::permissions::policy::ApprovalPolicyDecision::Review => None,
        }
    }

    fn after_tool_call(&self, context: AfterToolCallContext) -> Option<AfterToolCallResult> {
        let _ = context;
        None
    }

    fn find_tool_definition(&self, name: &str) -> Option<ToolDefinitionEntry> {
        lock(&self.inner.tool_definitions)
            .iter()
            .find(|(tool_name, _)| tool_name == name)
            .map(|(_, entry)| entry.clone())
    }

    fn emit(&self, event: AgentSessionEvent) {
        let listeners: Vec<AgentSessionEventListener> = lock(&self.inner.event_listeners)
            .iter()
            .map(|(_, listener)| Arc::clone(listener))
            .collect();
        for listener in listeners {
            listener(&event);
        }
    }

    fn emit_queue_update(&self) {
        self.emit(AgentSessionEvent::QueueUpdate(QueueUpdateEvent {
            event_type: "queue_update",
            steering: lock(&self.inner.steering_messages).clone(),
            follow_up: lock(&self.inner.follow_up_messages).clone(),
        }));
    }

    async fn emit_agent_settled(&self) {
        self.emit(AgentSessionEvent::AgentSettled(AgentSettledEvent {
            event_type: "agent_settled",
        }));
    }

    fn reset_duplicate_turn_guard(&self) {
        *lock(&self.inner.last_assistant_turn_fingerprint) = None;
        self.inner
            .duplicate_turn_guard_injected
            .store(false, Ordering::SeqCst);
        self.inner
            .duplicate_turn_loop_detected
            .store(false, Ordering::SeqCst);
    }

    fn mark_duplicate_assistant_message_for_presentation(&self, message: &mut AssistantMessage) {
        if message.stop_reason == StopReason::Aborted || message.stop_reason == StopReason::Error {
            return;
        }
        if message
            .content
            .iter()
            .any(|content| matches!(content, AssistantContent::ToolCall(_)))
        {
            return;
        }
        let fingerprint = duplicate_fingerprint(message);
        if lock(&self.inner.last_assistant_turn_fingerprint).as_deref() == Some(fingerprint.as_str()) {
            mark_duplicate_turn_assistant_message(message);
        }
    }

    fn track_duplicate_assistant_turn(&self, message: &mut AssistantMessage) {
        if message.stop_reason == StopReason::Aborted {
            if !self.inner.duplicate_turn_guard_injected.load(Ordering::SeqCst) {
                self.reset_duplicate_turn_guard();
            }
            return;
        }
        let has_tool_call = message
            .content
            .iter()
            .any(|content| matches!(content, AssistantContent::ToolCall(_)));
        if has_tool_call || message.stop_reason == StopReason::Error {
            self.reset_duplicate_turn_guard();
            return;
        }
        let fingerprint = duplicate_fingerprint(message);
        let mut last = lock(&self.inner.last_assistant_turn_fingerprint);
        if last.as_deref() != Some(fingerprint.as_str()) {
            *last = Some(fingerprint);
            drop(last);
            self.inner
                .duplicate_turn_guard_injected
                .store(false, Ordering::SeqCst);
            return;
        }
        drop(last);
        mark_duplicate_turn_assistant_message(message);
        if self.inner.duplicate_turn_guard_injected.load(Ordering::SeqCst) {
            self.inner
                .duplicate_turn_loop_detected
                .store(true, Ordering::SeqCst);
            return;
        }
        self.inner
            .duplicate_turn_guard_injected
            .store(true, Ordering::SeqCst);
        let guard = create_custom_message(
            "duplicate_turn_guard",
            CustomMessageContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(
                DUPLICATE_TURN_GUARD_PROMPT,
            ))]),
            true,
            None,
            now_millis(),
        );
        self.steer_agent(guard);
    }

    fn steer_agent(&self, message: AgentMessage) {
        self.inner.agent.steer(message);
    }

    async fn handle_agent_event(&self, event: AgentEvent) {
        let original_tool_call_id = match &event {
            AgentEvent::MessageEnd { message } => match message {
                AgentMessage::ToolResult(result) => Some(result.tool_call_id.clone()),
                _ => None,
            },
            _ => None,
        };
        if matches!(event, AgentEvent::ToolExecutionStart { .. }) {
            self.reset_duplicate_turn_guard();
        }
        if let AgentEvent::MessageEnd { message } = &event
            && let AgentMessage::Assistant(assistant) = message
        {
            let mut assistant = (**assistant).clone();
            self.mark_duplicate_assistant_message_for_presentation(&mut assistant);
        }
        self.remove_started_message_from_queue(&event);
        let listener_event = match &event {
            AgentEvent::AgentEnd { messages } => AgentSessionEvent::AgentEnd(AgentEndEvent::new(
                messages.clone(),
                self.will_retry_after_agent_end(messages),
            )),
            other => AgentSessionEvent::Agent(other.clone()),
        };
        self.emit(listener_event);
        if let AgentEvent::MessageEnd { message } = event {
            self.persist_completed_message(&message, original_tool_call_id.as_deref());
        }
    }

    fn remove_started_message_from_queue(&self, event: &AgentEvent) {
        let AgentEvent::MessageStart { message } = event else {
            return;
        };
        match message {
            AgentMessage::Custom(object) => {
                let is_shared_channel = object.get("customType").and_then(Value::as_str) == Some("shared_channel");
                let Some(content) = object.get("content").and_then(Value::as_str) else {
                    return;
                };
                if !is_shared_channel {
                    return;
                }
                let mut follow_ups = lock(&self.inner.follow_up_messages);
                if let Some(index) = follow_ups.iter().position(|text| text == content) {
                    follow_ups.remove(index);
                    drop(follow_ups);
                    self.emit_queue_update();
                }
            }
            AgentMessage::User(user) => {
                let is_internal = is_duplicate_turn_guard_message(message);
                if !is_internal {
                    self.reset_duplicate_turn_guard();
                }
                self.inner
                    .quota_fallback_attempted
                    .store(false, Ordering::SeqCst);
                let text = user_message_text(&user.content);
                if text.is_empty() {
                    return;
                }
                let mut steering = lock(&self.inner.steering_messages);
                if let Some(index) = steering.iter().position(|message| message == &text) {
                    steering.remove(index);
                    drop(steering);
                    self.emit_queue_update();
                    return;
                }
                drop(steering);
                let mut follow_ups = lock(&self.inner.follow_up_messages);
                if let Some(index) = follow_ups.iter().position(|message| message == &text) {
                    follow_ups.remove(index);
                    drop(follow_ups);
                    self.emit_queue_update();
                }
            }
            _ => {}
        }
    }

    fn track_completed_assistant_message(&self, message: &AssistantMessage) {
        *lock(&self.inner.last_assistant_message) = Some(message.clone());
        self.write_last_assistant_control_message(message);
        let mut message = message.clone();
        self.track_duplicate_assistant_turn(&mut message);
        if message.stop_reason == StopReason::Error || message.stop_reason == StopReason::Length {
            // Overflow/length recovery flags are cleared below, matching the source.
        }
        if message.stop_reason == StopReason::Error {
            return;
        }
        let attempt = self.inner.retry_attempt.load(Ordering::SeqCst);
        if attempt == 0 {
            return;
        }
        self.emit(AgentSessionEvent::AutoRetryEnd(AutoRetryEndEvent {
            event_type: "auto_retry_end",
            success: true,
            attempt,
            final_error: None,
        }));
        self.inner.retry_attempt.store(0, Ordering::SeqCst);
    }

    fn write_last_assistant_control_message(&self, message: &AssistantMessage) {
        let content = get_assistant_message_text(message).trim().to_owned();
        if content.is_empty() {
            return;
        }
        let Some(control_db_path) = &self.inner.control_db_path else {
            return;
        };
        let _ = crate::core::session_control_db::write_last_message(
            control_db_path,
            "assistant",
            &content,
        );
    }

    fn persist_completed_message(&self, message: &AgentMessage, original_tool_call_id: Option<&str>) {
        let is_internal = is_duplicate_turn_guard_message(message);
        let mut manager = lock(&self.inner.session_manager);
        let result = match message {
            AgentMessage::Custom(object) => append_custom_agent_message(&mut manager, object),
            AgentMessage::User(_) | AgentMessage::Assistant(_) | AgentMessage::ToolResult(_) => {
                if is_internal {
                    Ok(None)
                } else {
                    manager.append_message(message.clone()).map(Some)
                }
            }
        };
        drop(manager);
        if let Err(error) = result {
            eprintln!("Failed to persist session message: {error}");
        }
        let _ = original_tool_call_id;
        if let AgentMessage::Assistant(assistant) = message {
            self.track_completed_assistant_message(assistant);
        }
    }

    fn will_retry_after_agent_end(&self, messages: &[AgentMessage]) -> bool {
        for message in messages.iter().rev() {
            let AgentMessage::Assistant(assistant) = message else {
                continue;
            };
            if self.find_quota_fallback_model(assistant).is_some() {
                return true;
            }
            let settings = lock(&self.inner.settings_manager).get_retry_settings();
            return settings.enabled
                && self.inner.retry_attempt.load(Ordering::SeqCst) < settings.max_retries as u64
                && self.is_retryable_error(assistant);
        }
        false
    }

    fn find_last_assistant_message(&self) -> Option<AssistantMessage> {
        self.inner
            .agent
            .messages()
            .iter()
            .rev()
            .find_map(|message| message.as_assistant().cloned())
    }

    async fn continue_agent_with_thinking_timeout(
        &self,
        process_queued_messages_first: bool,
    ) -> Result<(), AgentSessionError> {
        self.inner
            .agent
            .continue_run(pi_agent_core::agent::ContinueOptions {
                process_queued_messages_first,
            })
            .await?;
        Ok(())
    }

    async fn run_agent_prompt(&self, messages: Vec<AgentMessage>) -> Result<(), AgentSessionError> {
        let result = self.inner.agent.prompt(messages).await;
        let post = match result {
            Ok(()) => self.continue_post_agent_runs().await,
            Err(error) => Err(error.into()),
        };
        *lock(&self.inner.system_prompt_override) = None;
        self.flush_pending_bash_messages();
        self.emit_agent_settled().await;
        post
    }

    async fn run_agent_continuation(&self) -> Result<(), AgentSessionError> {
        let result = self.inner.agent.continue_run(Default::default()).await;
        let post = match result {
            Ok(()) => self.continue_post_agent_runs().await,
            Err(error) => Err(error.into()),
        };
        *lock(&self.inner.system_prompt_override) = None;
        self.flush_pending_bash_messages();
        self.emit_agent_settled().await;
        post
    }

    async fn continue_post_agent_runs(&self) -> Result<(), AgentSessionError> {
        if self.inner.disposed.load(Ordering::SeqCst) {
            return Ok(());
        }
        loop {
            let guard = self.inner.turn_start_lock.lock().await;
            let Some(kind) = self.handle_post_agent_run().await else {
                return Ok(());
            };
            drop(guard);
            self.continue_agent_with_thinking_timeout(kind == "queued")
                .await?;
        }
    }

    async fn handle_post_agent_run(&self) -> Option<&'static str> {
        if self.inner.disposed.load(Ordering::SeqCst) {
            return None;
        }
        let message = lock(&self.inner.last_assistant_message).take()?;
        if self.prepare_quota_fallback(&message).await {
            return Some("normal");
        }
        if self.is_retryable_error(&message) && self.prepare_retry(&message).await {
            return Some("normal");
        }
        self.finish_exhausted_retry(&message);
        if self.check_compaction(&message, true).await {
            return Some("normal");
        }
        if self.inner.agent.has_queued_messages() {
            return Some("queued");
        }
        None
    }

    /// Validates that the active model has configured auth.
    pub async fn validate_model_authentication(&self) -> Result<(), AgentSessionError> {
        let model = self.inner.agent.model();
        let has_configured_auth = self.inner.model_runtime.has_configured_auth(&model.provider)
            || self
                .inner
                .model_runtime
                .check_auth(&model.provider, Default::default())
                .await
                .ok()
                .flatten()
                .is_some();
        if has_configured_auth {
            return Ok(());
        }
        if self.inner.model_runtime.is_using_oauth(&model.provider) {
            return Err(error(format!(
                "Authentication failed for \"{}\". Credentials may have expired or network is unavailable. Run '/login {}' to re-authenticate.",
                model.provider, model.provider
            )));
        }
        Err(error(crate::core::auth_guidance::format_no_api_key_found_message(
            &model.provider,
        )))
    }

    fn finish_exhausted_retry(&self, message: &AssistantMessage) {
        if message.stop_reason != StopReason::Error {
            return;
        }
        let attempt = self.inner.retry_attempt.load(Ordering::SeqCst);
        if attempt == 0 {
            return;
        }
        self.emit(AgentSessionEvent::AutoRetryEnd(AutoRetryEndEvent {
            event_type: "auto_retry_end",
            success: false,
            attempt,
            final_error: message.error_message.clone(),
        }));
        self.inner.retry_attempt.store(0, Ordering::SeqCst);
    }

    /// Continues the agent from the current transcript.
    pub async fn continue_run(&self) -> Result<(), AgentSessionError> {
        let guard = self.inner.turn_start_lock.lock().await;
        if self.is_streaming() {
            return Err(error(
                "Agent is already processing. Wait for completion before continuing.",
            ));
        }
        self.flush_pending_bash_messages();
        if self.messages().is_empty() {
            return Ok(());
        }
        self.validate_model_authentication().await?;
        if let Some(last_assistant) = self.find_last_assistant_message() {
            self.check_compaction(&last_assistant, false).await;
        }
        if self.is_streaming() {
            return Err(error(
                "Agent is already processing. Wait for completion before continuing.",
            ));
        }
        drop(guard);
        self.run_agent_continuation().await
    }

    /// Sends a prompt to the agent.
    pub async fn prompt(&self, text: &str, options: PromptOptions) -> Result<(), AgentSessionError> {
        let expand = options.expand_prompt_templates.unwrap_or(true);
        if expand && text.starts_with('/') {
            if !self.is_known_slash_command(text) {
                return Err(error(format!(
                    "Unknown slash command: {}",
                    slash_command_name(text)
                )));
            }
        }
        let guard = self.inner.turn_start_lock.lock().await;
        self.prompt_turn(text, options, guard).await
    }

    async fn prompt_turn(
        &self,
        text: &str,
        options: PromptOptions,
        guard: tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<(), AgentSessionError> {
        let preflight = options.preflight_result.clone();
        let result = self.prompt_turn_inner(text, &options).await;
        match result {
            Ok(Some(messages)) => {
                if let Some(preflight) = &preflight {
                    preflight(true);
                }
                drop(guard);
                self.run_agent_prompt(messages).await
            }
            Ok(None) => {
                if let Some(preflight) = &preflight {
                    preflight(true);
                }
                Ok(())
            }
            Err(error) => {
                if let Some(preflight) = &preflight {
                    preflight(false);
                }
                Err(error)
            }
        }
    }

    async fn prompt_turn_inner(
        &self,
        text: &str,
        options: &PromptOptions,
    ) -> Result<Option<Vec<AgentMessage>>, AgentSessionError> {
        if lock(&self.inner.compaction_abort_controller).is_some() {
            return Err(error(
                "Cannot submit a prompt while compaction is in progress. Wait for compaction to finish and retry.",
            ));
        }
        let input = self.prepare_prompt_input(text, options);
        let Some((text, images)) = input else {
            return Ok(None);
        };
        if self.queue_prompt_if_streaming(&text, &images, options).await? {
            return Ok(None);
        }
        self.prepare_prompt_turn().await?;
        if self.queue_prompt_if_streaming(&text, &images, options).await? {
            return Ok(None);
        }
        let messages = self.build_prompt_messages(&text, &images, options);
        Ok(Some(messages))
    }

    fn prepare_prompt_input(
        &self,
        text: &str,
        options: &PromptOptions,
    ) -> Option<(String, Vec<ImageContent>)> {
        let mut current_text = text.to_owned();
        if options.expand_prompt_templates.unwrap_or(true) {
            current_text = self.expand_skill_command(&current_text);
            current_text = expand_prompt_template(&current_text, &self.prompt_templates());
        }
        Some((current_text, options.images.clone()))
    }

    async fn queue_prompt_if_streaming(
        &self,
        text: &str,
        images: &[ImageContent],
        options: &PromptOptions,
    ) -> Result<bool, AgentSessionError> {
        if !self.is_streaming() {
            return Ok(false);
        }
        match options.streaming_behavior {
            Some(StreamingBehavior::FollowUp) => {
                self.queue_follow_up(text, images, options.source).await;
            }
            Some(StreamingBehavior::Steer) => {
                self.queue_steer(text, images, options.source).await;
            }
            None => {
                return Err(error(
                    "Agent is already processing. Specify streamingBehavior ('steer' or 'followUp') to queue the message.",
                ));
            }
        }
        Ok(true)
    }

    async fn prepare_prompt_turn(&self) -> Result<(), AgentSessionError> {
        self.flush_pending_bash_messages();
        self.validate_model_authentication().await?;
        if let Some(last_assistant) = self.find_last_assistant_message() {
            self.check_compaction(&last_assistant, false).await;
        }
        Ok(())
    }

    fn build_prompt_messages(
        &self,
        text: &str,
        images: &[ImageContent],
        options: &PromptOptions,
    ) -> Vec<AgentMessage> {
        let mut content = vec![TextOrImageContent::Text(TextContent::new(text))];
        content.extend(images.iter().cloned().map(TextOrImageContent::Image));
        let user_message = AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(content),
            input_source: options.source,
            provider_native: None,
            timestamp: now_millis(),
        });
        if let Some(callback) = &options.on_user_message_prepared {
            callback(&user_message);
        }
        let mut messages = vec![user_message];
        messages.extend(lock(&self.inner.pending_next_turn_messages).drain(..));
        *lock(&self.inner.system_prompt_override) = None;
        let prompt = lock(&self.inner.base_system_prompt).clone();
        self.inner.agent.set_system_prompt(prompt);
        messages
    }

    fn is_known_slash_command(&self, text: &str) -> bool {
        let command_name = slash_command_name(text);
        let name = command_name.trim_start_matches('/');
        if builtin_slash_commands()
            .iter()
            .any(|command| command.name == name)
        {
            return true;
        }
        if let Some(skill_name) = name.strip_prefix("skill:") {
            return self
                .inner
                .resource_loader
                .get_skills()
                .skills
                .iter()
                .any(|skill| skill.name == skill_name);
        }
        self.prompt_templates()
            .iter()
            .any(|template| template.name == name)
    }

    fn expand_skill_command(&self, text: &str) -> String {
        let Some(rest) = text.strip_prefix("/skill:") else {
            return text.to_owned();
        };
        let (skill_name, args) = match rest.split_once(' ') {
            Some((name, args)) => (name, args.trim()),
            None => (rest, ""),
        };
        let skills = self.inner.resource_loader.get_skills().skills;
        let Some(skill) = skills.iter().find(|skill| skill.name == skill_name) else {
            return text.to_owned();
        };
        match std::fs::read_to_string(&skill.file_path) {
            Ok(content) => {
                let body = strip_frontmatter(&content)
                    .map(|body| body.trim().to_owned())
                    .unwrap_or_else(|_| content.trim().to_owned());
                let skill_block = format!(
                    "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
                    skill.name, skill.file_path, skill.base_dir, body
                );
                if args.is_empty() {
                    skill_block
                } else {
                    format!("{skill_block}\n\n{args}")
                }
            }
            Err(_) => text.to_owned(),
        }
    }

    /// Queues a steering message while the agent is running.
    pub async fn steer(&self, text: &str, images: &[ImageContent]) -> Result<(), AgentSessionError> {
        if text.starts_with('/') {
            self.throw_if_extension_command(text);
        }
        let mut expanded = self.expand_skill_command(text);
        expanded = expand_prompt_template(&expanded, &self.prompt_templates());
        let guard = self.inner.turn_start_lock.lock().await;
        if self.is_streaming() {
            self.queue_steer(&expanded, images, None).await;
            return Ok(());
        }
        self.validate_model_authentication().await?;
        self.queue_steer(&expanded, images, None).await;
        drop(guard);
        if let Err(error) = self.run_agent_continuation().await {
            eprintln!("Failed to continue idle steering: {error}");
        }
        Ok(())
    }

    /// Queues a follow-up message delivered after the agent finishes.
    pub async fn follow_up(
        &self,
        text: &str,
        images: &[ImageContent],
    ) -> Result<(), AgentSessionError> {
        if text.starts_with('/') {
            self.throw_if_extension_command(text);
        }
        let mut expanded = self.expand_skill_command(text);
        expanded = expand_prompt_template(&expanded, &self.prompt_templates());
        self.queue_follow_up(&expanded, images, None).await;
        Ok(())
    }

    fn throw_if_extension_command(&self, _text: &str) {
        // Extension commands are owned by `ca-extensions-api`; no command can
        // be registered until that slice merges.
    }

    async fn queue_steer(&self, text: &str, images: &[ImageContent], source: Option<InputSource>) {
        lock(&self.inner.steering_messages).push(text.to_owned());
        self.emit_queue_update();
        let mut content = vec![TextOrImageContent::Text(TextContent::new(text))];
        content.extend(images.iter().cloned().map(TextOrImageContent::Image));
        self.steer_agent(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(content),
            input_source: source,
            provider_native: None,
            timestamp: now_millis(),
        }));
        self.emit(AgentSessionEvent::SteeringMessageQueued(
            SteeringMessageQueuedEvent {
                event_type: "steering_message_queued",
            },
        ));
    }

    async fn queue_follow_up(&self, text: &str, images: &[ImageContent], source: Option<InputSource>) {
        lock(&self.inner.follow_up_messages).push(text.to_owned());
        self.emit_queue_update();
        let mut content = vec![TextOrImageContent::Text(TextContent::new(text))];
        content.extend(images.iter().cloned().map(TextOrImageContent::Image));
        self.inner.agent.follow_up(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(content),
            input_source: source,
            provider_native: None,
            timestamp: now_millis(),
        }));
    }

    /// Sends a custom message to the session.
    pub async fn send_custom_message(
        &self,
        custom_type: &str,
        content: CustomMessageContent,
        display: bool,
        details: Option<Value>,
        trigger_turn: bool,
        deliver_as: Option<StreamingBehavior>,
    ) -> Result<(), AgentSessionError> {
        let app_message = create_custom_message(custom_type, content, display, details, now_millis());
        if self.is_streaming() {
            if deliver_as == Some(StreamingBehavior::FollowUp) {
                self.inner.agent.follow_up(app_message);
            } else {
                self.steer_agent(app_message);
            }
            return Ok(());
        }
        if trigger_turn {
            return self.trigger_custom_message_turn(app_message).await;
        }
        self.append_custom_message(&app_message);
        Ok(())
    }

    /// Queues a custom message for the next user prompt turn.
    pub fn send_custom_message_next_turn(&self, custom_type: &str, content: CustomMessageContent, display: bool, details: Option<Value>) {
        lock(&self.inner.pending_next_turn_messages).push(create_custom_message(
            custom_type,
            content,
            display,
            details,
            now_millis(),
        ));
    }

    async fn trigger_custom_message_turn(&self, message: AgentMessage) -> Result<(), AgentSessionError> {
        let guard = self.inner.turn_start_lock.lock().await;
        if self.is_streaming() {
            self.steer_agent(message);
            return Ok(());
        }
        drop(guard);
        self.run_agent_prompt(vec![message]).await
    }

    fn append_custom_message(&self, app_message: &AgentMessage) {
        let mut messages = self.inner.agent.messages();
        messages.push(app_message.clone());
        self.inner.agent.set_messages(messages);
        if let AgentMessage::Custom(object) = app_message {
            let mut manager = lock(&self.inner.session_manager);
            if let Err(error) = append_custom_agent_message(&mut manager, object) {
                eprintln!("Failed to persist custom message: {error}");
            }
        }
        self.emit(AgentSessionEvent::Agent(AgentEvent::MessageStart {
            message: app_message.clone(),
        }));
        self.emit(AgentSessionEvent::Agent(AgentEvent::MessageEnd {
            message: app_message.clone(),
        }));
    }

    /// Sends a user message to the agent. Always triggers a turn.
    pub async fn send_user_message(
        &self,
        content: UserContent,
        deliver_as: Option<StreamingBehavior>,
        expand_prompt_templates: bool,
    ) -> Result<(), AgentSessionError> {
        let (text, images) = match content {
            UserContent::Text(text) => (text, Vec::new()),
            UserContent::Blocks(blocks) => {
                let mut text_parts = Vec::new();
                let mut images = Vec::new();
                for block in blocks {
                    match block {
                        pi_ai::types::UserContentBlock::Text(text) => text_parts.push(text.text),
                        pi_ai::types::UserContentBlock::Image(image) => images.push(image),
                    }
                }
                (text_parts.join("\n"), images)
            }
        };
        self.prompt(
            &text,
            PromptOptions {
                expand_prompt_templates: Some(expand_prompt_templates),
                images,
                streaming_behavior: deliver_as,
                source: Some(InputSource::Extension),
                preflight_result: None,
                on_user_message_prepared: None,
            },
        )
        .await
    }

    /// Clears all queued messages and returns them.
    pub fn clear_queue(&self) -> (Vec<String>, Vec<String>) {
        let steering = std::mem::take(&mut *lock(&self.inner.steering_messages));
        let follow_up = std::mem::take(&mut *lock(&self.inner.follow_up_messages));
        self.inner.agent.clear_all_queues();
        self.emit_queue_update();
        (steering, follow_up)
    }

    pub fn pending_message_count(&self) -> usize {
        lock(&self.inner.steering_messages).len() + lock(&self.inner.follow_up_messages).len()
    }

    pub fn has_pending_messages(&self) -> bool {
        self.pending_message_count() > 0
            || self
                .inner
                .external_user_input_reservations
                .load(Ordering::SeqCst)
                > 0
    }

    pub fn reserve_external_user_input(&self) -> ExternalUserInputReservation {
        self.inner
            .external_user_input_reservations
            .fetch_add(1, Ordering::SeqCst);
        ExternalUserInputReservation {
            inner: Arc::downgrade(&self.inner),
            released: AtomicBool::new(false),
        }
    }

    pub fn get_steering_messages(&self) -> Vec<String> {
        lock(&self.inner.steering_messages).clone()
    }

    pub fn get_follow_up_messages(&self) -> Vec<String> {
        lock(&self.inner.follow_up_messages).clone()
    }

    /// Aborts the current operation, then submits preserved user input.
    pub async fn interrupt(
        &self,
        message: &str,
        source: InputSource,
    ) -> Result<(), AgentSessionError> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return self.abort().await;
        }
        let release = self.reserve_external_user_input();
        let result = async {
            self.abort().await?;
            let release_for_preflight = release.clone();
            self.prompt(
                trimmed,
                PromptOptions {
                    expand_prompt_templates: None,
                    images: Vec::new(),
                    streaming_behavior: Some(StreamingBehavior::Steer),
                    source: Some(source),
                    preflight_result: Some(Arc::new(move |accepted| {
                        if accepted {
                            release_for_preflight.release();
                        }
                    })),
                    on_user_message_prepared: None,
                },
            )
            .await
        }
        .await;
        release.release();
        result
    }

    /// Aborts the current operation and waits for the agent to become idle.
    pub async fn abort(&self) -> Result<(), AgentSessionError> {
        self.abort_retry();
        lock(&self.inner.steering_messages).clear();
        lock(&self.inner.follow_up_messages).clear();
        self.inner.agent.clear_all_queues();
        self.emit_queue_update();
        self.inner.agent.abort();
        self.inner.agent.wait_for_idle().await;
        Ok(())
    }

    // =========================================================================
    // Model management
    // =========================================================================

    /// Sets the active model after validating auth.
    pub async fn set_model(&self, model: Model) -> Result<(), AgentSessionError> {
        if self
            .inner
            .model_runtime
            .check_auth(&model.provider, Default::default())
            .await
            .ok()
            .flatten()
            .is_none()
        {
            return Err(error(format!(
                "No API key for {}/{}",
                model.provider, model.id
            )));
        }
        let previous_model = self.inner.agent.model();
        let thinking_level = self.get_thinking_level_for_model_switch(None);
        self.inner.agent.set_model(model.clone());
        lock(&self.inner.settings_manager)
            .set_default_model_and_provider(&model.provider, &model.id);
        self.set_thinking_level(thinking_level);
        if previous_model.id != model.id || previous_model.provider != model.provider {
            // `model_select` extension event is a recorded gap.
        }
        Ok(())
    }

    /// Cycles to the next/previous scoped model.
    pub async fn cycle_model(
        &self,
        direction: CycleDirection,
    ) -> Result<Option<ModelCycleResult>, AgentSessionError> {
        let scoped_models = self.get_scoped_models_for_cycle().await;
        if scoped_models.is_empty() {
            return Ok(None);
        }
        self.cycle_scoped_model(direction, &scoped_models).await
    }

    async fn get_scoped_models_for_cycle(&self) -> Vec<ScopedModel> {
        let configured = lock(&self.inner.scoped_models).clone();
        if !configured.is_empty() {
            return configured;
        }
        let enabled_models = lock(&self.inner.settings_manager).get_enabled_models();
        let Some(enabled_models) = enabled_models.filter(|models| !models.is_empty()) else {
            return Vec::new();
        };
        let resolved =
            resolve_model_scope(&enabled_models, &self.inner.model_runtime, Default::default()).await;
        self.filter_narrow_model_scope(resolved).await
    }

    async fn filter_narrow_model_scope(&self, models: Vec<ScopedModel>) -> Vec<ScopedModel> {
        let available_models = self
            .inner
            .model_runtime
            .get_available(None, Default::default())
            .await
            .unwrap_or_default();
        let covers_every_model = available_models
            .iter()
            .all(|model| models.iter().any(|scoped| &scoped.model == model));
        if covers_every_model {
            return Vec::new();
        }
        models
    }

    async fn cycle_scoped_model(
        &self,
        direction: CycleDirection,
        models: &[ScopedModel],
    ) -> Result<Option<ModelCycleResult>, AgentSessionError> {
        let available_ids: HashSet<String> = self
            .inner
            .model_runtime
            .get_available_snapshot()
            .iter()
            .map(|model| format!("{}\0{}", model.provider, model.id))
            .collect();
        let available_scoped: Vec<&ScopedModel> = models
            .iter()
            .filter(|scoped| {
                available_ids.contains(&format!("{}\0{}", scoped.model.provider, scoped.model.id))
            })
            .collect();
        let mut authed = Vec::new();
        for scoped in available_scoped {
            if self
                .inner
                .model_runtime
                .check_auth(&scoped.model.provider, Default::default())
                .await
                .ok()
                .flatten()
                .is_some()
            {
                authed.push(scoped.clone());
            }
        }
        if authed.len() <= 1 {
            return Ok(None);
        }
        let current_model = self.inner.agent.model();
        let mut current_index = authed
            .iter()
            .position(|scoped| scoped.model == current_model)
            .unwrap_or(0);
        let length = authed.len();
        current_index = match direction {
            CycleDirection::Forward => (current_index + 1) % length,
            CycleDirection::Backward => (current_index + length - 1) % length,
        };
        let next = authed[current_index].clone();
        let thinking_level = self.get_thinking_level_for_model_switch(next.thinking_level);
        self.inner.agent.set_model(next.model.clone());
        lock(&self.inner.settings_manager)
            .set_default_model_and_provider(&next.model.provider, &next.model.id);
        self.set_thinking_level(thinking_level);
        Ok(Some(ModelCycleResult {
            model: next.model,
            thinking_level: self.thinking_level(),
            is_scoped: true,
        }))
    }

    // =========================================================================
    // Thinking level management
    // =========================================================================

    /// Sets the thinking level, clamped to the model capabilities.
    pub fn set_thinking_level(&self, level: ThinkingLevel) {
        let available = self.get_available_thinking_levels();
        let effective = if available.contains(&level) {
            level
        } else {
            self.clamp_thinking_level(level)
        };
        let previous = self.inner.agent.thinking_level();
        let is_changing = effective != previous;
        self.inner.agent.set_thinking_level(effective);
        if is_changing {
            if self.supports_thinking() || effective != ThinkingLevel::Off {
                lock(&self.inner.settings_manager).set_default_thinking_level(effective);
            }
            self.emit(AgentSessionEvent::ThinkingLevelChanged(
                ThinkingLevelChangedEvent {
                    event_type: "thinking_level_changed",
                    level: effective,
                },
            ));
        }
    }

    /// Cycles to the next thinking level.
    pub fn cycle_thinking_level(&self) -> Option<ThinkingLevel> {
        if !self.supports_thinking() {
            return None;
        }
        let levels = self.get_available_thinking_levels();
        let current = self.thinking_level();
        let index = levels.iter().position(|level| *level == current).unwrap_or(0);
        let next = levels[(index + 1) % levels.len()];
        self.set_thinking_level(next);
        Some(next)
    }

    /// Available thinking levels for the current model.
    pub fn get_available_thinking_levels(&self) -> Vec<ThinkingLevel> {
        pi_ai::models::get_supported_thinking_levels(&self.inner.agent.model())
            .into_iter()
            .map(thinking_level_from_model)
            .collect()
    }

    pub fn supports_thinking(&self) -> bool {
        self.inner.agent.model().reasoning
    }

    fn get_thinking_level_for_model_switch(&self, explicit: Option<ThinkingLevel>) -> ThinkingLevel {
        if let Some(level) = explicit {
            return level;
        }
        if !self.supports_thinking() {
            return lock(&self.inner.settings_manager)
                .get_default_thinking_level()
                .unwrap_or(DEFAULT_THINKING_LEVEL);
        }
        self.thinking_level()
    }

    fn clamp_thinking_level(&self, level: ThinkingLevel) -> ThinkingLevel {
        let model = self.inner.agent.model();
        thinking_level_from_model(pi_ai::models::clamp_thinking_level(
            &model,
            model_thinking_level(level),
        ))
    }

    pub fn set_steering_mode(&self, mode: pi_agent_core::harness::QueueMode) {
        self.inner.agent.set_steering_mode(mode);
        lock(&self.inner.settings_manager).set_steering_mode(mode);
    }

    pub fn set_follow_up_mode(&self, mode: pi_agent_core::harness::QueueMode) {
        self.inner.agent.set_follow_up_mode(mode);
        lock(&self.inner.settings_manager).set_follow_up_mode(mode);
    }

    // =========================================================================
    // Retry
    // =========================================================================

    fn is_retryable_error(&self, message: &AssistantMessage) -> bool {
        let context_window = self.inner.agent.model().context_window;
        if pi_ai::utils::overflow::is_context_overflow(message, Some(context_window)) {
            return false;
        }
        pi_ai::utils::retry::is_retryable_assistant_error(message)
    }

    fn find_quota_fallback_model(&self, message: &AssistantMessage) -> Option<Model> {
        if self.inner.quota_fallback_attempted.load(Ordering::SeqCst)
            || message.stop_reason != StopReason::Error
        {
            return None;
        }
        let error_message = message.error_message.as_deref()?;
        if !quota_exhaustion(error_message) {
            return None;
        }
        let current_model = self.inner.agent.model();
        if message.provider != current_model.provider || message.model != current_model.id {
            return None;
        }
        let paired = codex_provider_pair(&message.provider)?;
        let fallback = self.inner.model_runtime.get_model(paired, &message.model)?;
        self.inner
            .model_runtime
            .has_configured_auth(&fallback.provider)
            .then_some(fallback)
    }

    async fn prepare_quota_fallback(&self, message: &AssistantMessage) -> bool {
        let Some(fallback_model) = self.find_quota_fallback_model(message) else {
            return false;
        };
        self.inner
            .quota_fallback_attempted
            .store(true, Ordering::SeqCst);
        let mut messages = self.inner.agent.messages();
        if matches!(messages.last(), Some(AgentMessage::Assistant(_))) {
            messages.pop();
            self.inner.agent.set_messages(messages);
        }
        self.inner.agent.set_model(fallback_model);
        true
    }

    async fn prepare_retry(&self, message: &AssistantMessage) -> bool {
        let settings = lock(&self.inner.settings_manager).get_retry_settings();
        if !settings.enabled {
            return false;
        }
        let attempt = self.inner.retry_attempt.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt > settings.max_retries as u64 {
            self.inner.retry_attempt.store(attempt - 1, Ordering::SeqCst);
            return false;
        }
        let delay_ms = settings.base_delay_ms;
        self.emit(AgentSessionEvent::AutoRetryStart(AutoRetryStartEvent {
            event_type: "auto_retry_start",
            attempt,
            max_attempts: settings.max_retries as u64,
            delay_ms,
            error_message: message
                .error_message
                .clone()
                .unwrap_or_else(|| "Unknown error".to_owned()),
        }));
        let mut messages = self.inner.agent.messages();
        if matches!(messages.last(), Some(AgentMessage::Assistant(_))) {
            messages.pop();
            self.inner.agent.set_messages(messages);
        }
        let token = CancellationToken::new();
        *lock(&self.inner.retry_abort_controller) = Some(token.clone());
        let cancelled = tokio::select! {
            () = tokio::time::sleep(std::time::Duration::from_millis(delay_ms.max(0.0) as u64)) => false,
            () = token.cancelled() => true,
        };
        *lock(&self.inner.retry_abort_controller) = None;
        if cancelled {
            let attempt = self.inner.retry_attempt.swap(0, Ordering::SeqCst);
            self.emit(AgentSessionEvent::AutoRetryEnd(AutoRetryEndEvent {
                event_type: "auto_retry_end",
                success: false,
                attempt,
                final_error: Some("Retry cancelled".to_owned()),
            }));
            return false;
        }
        true
    }

    /// Cancels an in-progress retry.
    pub fn abort_retry(&self) {
        if let Some(token) = lock(&self.inner.retry_abort_controller).as_ref() {
            token.cancel();
        }
    }

    /// Surfaces a provider-internal retry as a session event.
    pub fn notify_provider_retry(&self, retry: ProviderRetryEvent) {
        self.emit(AgentSessionEvent::ProviderStreamRetry(
            ProviderStreamRetryEvent {
                event_type: "provider_stream_retry",
                retry,
            },
        ));
    }

    pub fn has_active_retry(&self) -> bool {
        self.inner.retry_attempt.load(Ordering::SeqCst) > 0
    }

    pub fn is_retrying(&self) -> bool {
        lock(&self.inner.retry_abort_controller).is_some()
    }

    pub fn auto_retry_enabled(&self) -> bool {
        lock(&self.inner.settings_manager).get_retry_enabled()
    }

    pub fn set_auto_retry_enabled(&self, enabled: bool) {
        lock(&self.inner.settings_manager).set_retry_enabled(enabled);
    }
}

/// Direction for [`AgentSession::cycle_model`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleDirection {
    Forward,
    Backward,
}

/// Release handle returned by [`AgentSession::reserve_external_user_input`].
pub struct ExternalUserInputReservation {
    inner: Weak<Inner>,
    released: AtomicBool,
}

impl ExternalUserInputReservation {
    pub fn release(&self) {
        if self.released.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(inner) = self.inner.upgrade() {
            let current = inner
                .external_user_input_reservations
                .load(Ordering::SeqCst);
            inner
                .external_user_input_reservations
                .store(current.saturating_sub(1), Ordering::SeqCst);
        }
    }
}

impl Clone for ExternalUserInputReservation {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            released: AtomicBool::new(false),
        }
    }
}

impl Drop for ExternalUserInputReservation {
    fn drop(&mut self) {
        self.release();
    }
}

fn serialize_thinking_level<S: serde::Serializer>(
    level: &ThinkingLevel,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(level.as_str())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn normalize_prompt_snippet(text: Option<&str>) -> Option<String> {
    let text = text?;
    let one_line = text
        .split(['\r', '\n'])
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!one_line.is_empty()).then_some(one_line)
}

fn normalize_prompt_guidelines(guidelines: &[String]) -> Vec<String> {
    let mut unique = Vec::new();
    for guideline in guidelines {
        let normalized = guideline.trim();
        if !normalized.is_empty() && !unique.iter().any(|item| item == normalized) {
            unique.push(normalized.to_owned());
        }
    }
    unique
}

fn slash_command_name(text: &str) -> String {
    let rest = text.strip_prefix('/').unwrap_or(text);
    let name = rest.split_whitespace().next().unwrap_or("");
    format!("/{name}")
}

fn duplicate_fingerprint(message: &AssistantMessage) -> String {
    let text = get_assistant_message_text(message);
    let stop_reason = serde_json::to_value(message.stop_reason).unwrap_or(Value::Null);
    let mut object = Map::new();
    object.insert("text".to_owned(), Value::String(text));
    object.insert("stopReason".to_owned(), stop_reason);
    Value::Object(object).to_string()
}

fn is_duplicate_turn_guard_message(message: &AgentMessage) -> bool {
    matches!(
        message,
        AgentMessage::Custom(object)
            if object.get("customType").and_then(Value::as_str) == Some("duplicate_turn_guard")
    )
}

fn mark_duplicate_turn_assistant_message(message: &mut AssistantMessage) {
    // The runtime-message marker module is owned by a parallel slice; the
    // fingerprint tracking above still suppresses the repeated turn.
    let _ = message;
}

fn append_custom_agent_message(
    manager: &mut SessionManager,
    object: &Map<String, Value>,
) -> Result<Option<String>, SessionManagerError> {
    let custom_type = object
        .get("customType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let content = object
        .get("content")
        .cloned()
        .and_then(|content| serde_json::from_value::<CustomMessageContent>(content).ok())
        .unwrap_or(CustomMessageContent::Blocks(Vec::new()));
    let display = object
        .get("display")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let details = object.get("details").cloned();
    manager
        .append_custom_message_entry(custom_type, content, display, details)
        .map(Some)
}

fn validate_multi_agent_runtime_role(
    role: Option<MultiAgentRuntimeRole>,
    capability: Option<MultiAgentExecutionCapability>,
) -> Result<(), AgentSessionError> {
    let role = role.unwrap_or(MultiAgentRuntimeRole::Standalone);
    if role == MultiAgentRuntimeRole::Orchestrator && capability.is_none() {
        return Err(error(
            "Multi-agent orchestrator requires an issued execution capability",
        ));
    }
    if role != MultiAgentRuntimeRole::Orchestrator && capability.is_some() {
        return Err(error(format!(
            "Multi-agent {} runtime cannot receive execution capability",
            match role {
                MultiAgentRuntimeRole::Standalone => "standalone",
                MultiAgentRuntimeRole::Child => "child",
                MultiAgentRuntimeRole::Observer => "observer",
                MultiAgentRuntimeRole::Orchestrator => "orchestrator",
            }
        )));
    }
    Ok(())
}

fn codex_provider_pair(provider: &str) -> Option<&'static str> {
    match provider {
        "openai-codex" => Some("openai-codex-gc"),
        "openai-codex-gc" => Some("openai-codex"),
        _ => None,
    }
}

fn quota_exhaustion(message: &str) -> bool {
    let lowercase = message.to_lowercase();
    const NEEDLES: [&str; 7] = [
        "gousagelimiterror",
        "freeusagelimiterror",
        "usage limit",
        "available balance",
        "insufficient_quota",
        "out of budget",
        "quota exceeded",
    ];
    if NEEDLES.iter().any(|needle| lowercase.contains(needle)) {
        return true;
    }
    lowercase.contains("billing limit")
        || lowercase.contains("billing quota")
        || lowercase.contains("billing exhausted")
}

// =============================================================================
// State access
// =============================================================================

impl AgentSession {
    pub fn agent(&self) -> &Agent {
        &self.inner.agent
    }

    pub fn session_manager(&self) -> MutexGuard<'_, SessionManager> {
        lock(&self.inner.session_manager)
    }

    pub fn settings_manager(&self) -> MutexGuard<'_, SettingsManager> {
        lock(&self.inner.settings_manager)
    }

    pub fn model_runtime(&self) -> &ModelRuntime {
        &self.inner.model_runtime
    }

    pub fn model_registry(&self) -> &ModelRegistry {
        &self.inner.model_registry
    }

    pub fn cwd(&self) -> &str {
        &self.inner.cwd
    }

    pub fn agent_dir(&self) -> &str {
        &self.inner.agent_dir
    }

    pub fn state(&self) -> AgentState {
        self.inner.agent.state()
    }

    pub fn model(&self) -> Model {
        self.inner.agent.model()
    }

    pub fn thinking_level(&self) -> ThinkingLevel {
        self.inner.agent.thinking_level()
    }

    pub fn is_streaming(&self) -> bool {
        self.inner.agent.is_streaming()
    }

    pub fn system_prompt(&self) -> String {
        self.inner.agent.system_prompt()
    }

    pub fn retry_attempt(&self) -> u64 {
        self.inner.retry_attempt.load(Ordering::SeqCst)
    }

    pub fn request_resume_continuation(&self) {
        self.inner
            .resume_continuation_requested
            .store(true, Ordering::SeqCst);
    }

    pub fn consume_resume_continuation_request(&self) -> bool {
        self.inner
            .resume_continuation_requested
            .swap(false, Ordering::SeqCst)
    }

    pub fn get_active_tool_names(&self) -> Vec<String> {
        self.inner
            .agent
            .tools()
            .iter()
            .map(|tool| tool.name().to_owned())
            .collect()
    }

    pub fn get_all_tools(&self) -> Vec<ToolInfo> {
        lock(&self.inner.tool_definitions)
            .iter()
            .map(|(_, entry)| ToolInfo {
                name: entry.definition.name.clone(),
                description: entry.definition.description.clone(),
                parameters: entry.definition.parameters.clone(),
                prompt_guidelines: entry.definition.prompt_guidelines.clone(),
                source_info: entry.source_info.clone(),
            })
            .collect()
    }

    pub fn get_tool_definition(&self, name: &str) -> Option<ToolDefinition> {
        self.find_tool_definition(name)
            .map(|entry| entry.definition)
    }

    pub fn set_active_tools_by_name(&self, tool_names: &[String]) {
        self.apply_active_tools(tool_names);
    }

    pub fn is_compacting(&self) -> bool {
        lock(&self.inner.auto_compaction_abort_controller).is_some()
            || lock(&self.inner.compaction_abort_controller).is_some()
            || lock(&self.inner.compaction_summary_materialization_abort_controller).is_some()
            || lock(&self.inner.branch_summary_abort_controller).is_some()
    }

    pub fn messages(&self) -> Vec<AgentMessage> {
        self.inner.agent.messages()
    }

    pub fn steering_mode(&self) -> pi_agent_core::harness::QueueMode {
        self.inner.agent.steering_mode()
    }

    pub fn follow_up_mode(&self) -> pi_agent_core::harness::QueueMode {
        self.inner.agent.follow_up_mode()
    }

    pub fn session_file(&self) -> Option<String> {
        lock(&self.inner.session_manager)
            .get_session_file()
            .map(str::to_owned)
    }

    pub fn session_id(&self) -> String {
        lock(&self.inner.session_manager)
            .get_session_id()
            .to_owned()
    }

    pub fn session_name(&self) -> Option<String> {
        lock(&self.inner.session_manager).get_session_name()
    }

    pub fn scoped_models(&self) -> Vec<ScopedModel> {
        lock(&self.inner.scoped_models).clone()
    }

    pub fn set_scoped_models(&self, scoped_models: Vec<ScopedModel>) {
        *lock(&self.inner.scoped_models) = scoped_models;
    }

    pub fn prompt_templates(&self) -> Vec<crate::core::prompt_templates::PromptTemplate> {
        let LoadedPrompts { prompts, .. } = self.inner.resource_loader.get_prompts();
        prompts
    }

    pub fn resource_loader(&self) -> Arc<dyn ResourceLoader> {
        Arc::clone(&self.inner.resource_loader)
    }

    /// Subscribes to session events; returns a subscription id.
    pub fn subscribe(&self, listener: AgentSessionEventListener) -> u64 {
        let id = self.inner.next_listener_id.fetch_add(1, Ordering::SeqCst);
        lock(&self.inner.event_listeners).push((id, listener));
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        lock(&self.inner.event_listeners).retain(|(listener_id, _)| *listener_id != id);
    }

    /// Removes all listeners and disconnects from the agent.
    pub fn dispose(&self) {
        if self.inner.disposed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.abort_retry();
        self.abort_compaction();
        self.abort_compaction_summary_materialization();
        self.abort_branch_summary();
        self.abort_bash();
        self.inner.agent.abort();
        if let Some(subscription) = lock(&self.inner.agent_subscription).take() {
            self.inner.agent.unsubscribe(subscription);
        }
        lock(&self.inner.event_listeners).clear();
    }

    // =========================================================================
    // Compaction
    // =========================================================================

    /// Manually compacts the session context.
    ///
    /// Blocked on the coding-agent compaction module (`ca-compaction-export`);
    /// the source path is not reproduced here until that slice merges.
    pub async fn compact(&self, _custom_instructions: Option<&str>) -> Result<Value, AgentSessionError> {
        Err(error(
            "Compaction is unavailable until the coding-agent compaction module is ported",
        ))
    }

    /// Cancels in-progress compaction (manual or auto).
    pub fn abort_compaction(&self) {
        if let Some(token) = lock(&self.inner.compaction_abort_controller).as_ref() {
            token.cancel();
        }
        if let Some(token) = lock(&self.inner.auto_compaction_abort_controller).as_ref() {
            token.cancel();
        }
    }

    /// Cancels in-progress compaction-summary materialization.
    pub fn abort_compaction_summary_materialization(&self) {
        if let Some(token) = lock(&self.inner.compaction_summary_materialization_abort_controller).as_ref() {
            token.cancel();
        }
    }

    /// Cancels in-progress branch summarization.
    pub fn abort_branch_summary(&self) {
        if let Some(token) = lock(&self.inner.branch_summary_abort_controller).as_ref() {
            token.cancel();
        }
    }

    async fn check_compaction(&self, _assistant_message: &AssistantMessage, _post_run_check: bool) -> bool {
        // Blocked on `ca-compaction-export`: threshold/overflow recovery needs the
        // coding-agent compaction module.
        false
    }

    pub fn set_auto_compaction_enabled(&self, enabled: bool) {
        lock(&self.inner.settings_manager).set_compaction_enabled(enabled);
    }

    pub fn auto_compaction_enabled(&self) -> bool {
        lock(&self.inner.settings_manager).get_compaction_enabled()
    }

    // =========================================================================
    // Bash execution
    // =========================================================================

    /// Records a bash execution result in session history.
    pub fn record_bash_result(
        &self,
        command: &str,
        output: &str,
        exit_code: Option<f64>,
        cancelled: bool,
        truncated: bool,
        full_output_path: Option<String>,
        exclude_from_context: Option<bool>,
    ) {
        let bash_message = BashExecutionMessage {
            role: BashExecutionRole::BashExecution,
            command: command.to_owned(),
            output: output.to_owned(),
            exit_code,
            cancelled,
            truncated,
            full_output_path,
            timestamp: now_millis() as f64,
            exclude_from_context,
        };
        if self.is_streaming() {
            lock(&self.inner.pending_bash_messages).push(bash_message);
            return;
        }
        let message = bash_message_to_agent_message(&bash_message);
        let mut messages = self.inner.agent.messages();
        messages.push(message.clone());
        self.inner.agent.set_messages(messages);
        if let Err(error) = lock(&self.inner.session_manager).append_message(message.clone()) {
            eprintln!("Failed to persist bash message: {error}");
        }
        self.emit(AgentSessionEvent::BashMessagesCommitted(
            BashMessagesCommittedEvent {
                event_type: "bash_messages_committed",
                messages: vec![message],
            },
        ));
    }

    /// Cancels a running bash command.
    pub fn abort_bash(&self) {
        let tokens: Vec<CancellationToken> = lock(&self.inner.bash_abort_controllers).clone();
        for token in tokens {
            token.cancel();
        }
    }

    pub fn is_bash_running(&self) -> bool {
        !lock(&self.inner.bash_abort_controllers).is_empty()
    }

    pub fn has_pending_bash_messages(&self) -> bool {
        !lock(&self.inner.pending_bash_messages).is_empty()
    }

    fn flush_pending_bash_messages(&self) {
        let mut pending = lock(&self.inner.pending_bash_messages);
        if pending.is_empty() {
            return;
        }
        let messages = std::mem::take(&mut *pending);
        drop(pending);
        let mut agent_messages = self.inner.agent.messages();
        let mut committed = Vec::new();
        for bash_message in messages {
            let message = bash_message_to_agent_message(&bash_message);
            agent_messages.push(message.clone());
            if let Err(error) = lock(&self.inner.session_manager).append_message(message.clone()) {
                eprintln!("Failed to persist bash message: {error}");
            }
            committed.push(message);
        }
        self.inner.agent.set_messages(agent_messages);
        self.emit(AgentSessionEvent::BashMessagesCommitted(
            BashMessagesCommittedEvent {
                event_type: "bash_messages_committed",
                messages: committed,
            },
        ));
    }

    // =========================================================================
    // Session management
    // =========================================================================

    /// Sets a display name for the current session.
    pub fn set_session_name(&self, name: &str) {
        let session_file = self.session_file();
        if let (Some(control_db_path), Some(session_file)) = (&self.inner.control_db_path, session_file) {
            let _ = crate::core::session_control_db::set_named_session(
                control_db_path,
                &session_file,
                name,
            );
        }
        if let Err(error) = lock(&self.inner.session_manager).append_session_info(name) {
            eprintln!("Failed to persist session name: {error}");
        }
        let name = lock(&self.inner.session_manager).get_session_name();
        self.emit(AgentSessionEvent::SessionInfoChanged(
            SessionInfoChangedEvent {
                event_type: "session_info_changed",
                name,
            },
        ));
    }

    pub fn clear_session_name(&self) {
        let session_file = self.session_file();
        if let (Some(control_db_path), Some(session_file)) = (&self.inner.control_db_path, session_file) {
            let _ = crate::core::session_control_db::remove_named_session(
                control_db_path,
                &session_file,
            );
        }
        if let Err(error) = lock(&self.inner.session_manager).append_session_info("") {
            eprintln!("Failed to clear session name: {error}");
        }
        self.emit(AgentSessionEvent::SessionInfoChanged(
            SessionInfoChangedEvent {
                event_type: "session_info_changed",
                name: None,
            },
        ));
    }

    pub fn update_compaction_summary(&self, entry_id: &str, summary: &str) {
        let messages = {
            let mut manager = lock(&self.inner.session_manager);
            if let Err(error) = manager.update_compaction_summary(entry_id, summary) {
                eprintln!("Failed to update compaction summary: {error}");
            }
            manager.build_session_context().messages
        };
        self.inner.agent.set_messages(messages);
    }

    /// Materializes a provider-native compaction summary.
    ///
    /// Blocked on the coding-agent compaction module (`ca-compaction-export`).
    pub async fn materialize_compaction_summary(
        &self,
        entry_id: &str,
    ) -> Result<Value, AgentSessionError> {
        Err(error(format!(
            "Compaction summary materialization is unavailable until the compaction module is ported (entry {entry_id})"
        )))
    }

    // =========================================================================
    // Tree navigation
    // =========================================================================

    /// Navigates to a different node in the session tree.
    pub async fn navigate_tree(
        &self,
        target_id: &str,
        options: NavigateTreeOptions,
    ) -> Result<NavigateTreeResult, AgentSessionError> {
        if self.is_streaming() {
            return Err(error(
                "Wait for the current response to finish before navigating the session tree.",
            ));
        }
        if options.summarize == Some(true) {
            return Err(error(
                "Branch summarization is unavailable until the coding-agent compaction module is ported",
            ));
        }
        let (old_leaf_id, target_entry) = {
            let manager = lock(&self.inner.session_manager);
            (manager.get_leaf_id().map(str::to_owned), manager.get_entry(target_id).cloned())
        };
        if old_leaf_id.as_deref() == Some(target_id) {
            return Ok(NavigateTreeResult {
                editor_text: None,
                cancelled: false,
                aborted: None,
                summary_entry: None,
            });
        }
        let Some(target_entry) = target_entry else {
            return Err(error(format!("Entry {target_id} not found")));
        };

        let (new_leaf_id, editor_text) = match &target_entry {
            SessionEntry::Known(crate::core::session_manager::KnownSessionEntry::Message(message))
                if message.message.role() == "user" =>
            {
                let text = match message.message.as_ref() {
                    AgentMessage::User(user) => user_message_text(&user.content),
                    _ => String::new(),
                };
                (message.parent_id.clone(), Some(text))
            }
            SessionEntry::Known(crate::core::session_manager::KnownSessionEntry::CustomMessage(message)) => {
                let text = match &message.content {
                    CustomMessageContent::Text(text) => text.clone(),
                    CustomMessageContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|block| match block {
                            TextOrImageContent::Text(text) => Some(text.text.as_str()),
                            TextOrImageContent::Image(_) => None,
                        })
                        .collect(),
                };
                (message.parent_id.clone(), Some(text))
            }
            _ => (Some(target_id.to_owned()), None),
        };

        {
            let mut manager = lock(&self.inner.session_manager);
            if let Some(new_leaf_id) = &new_leaf_id {
                manager.branch(new_leaf_id)?;
            } else {
                manager.reset_leaf();
            }
            if let Some(label) = &options.label {
                manager.append_label_change(target_id, Some(label))?;
            }
        }
        let session_context = lock(&self.inner.session_manager).build_session_context();
        self.inner.agent.set_messages(session_context.messages);
        Ok(NavigateTreeResult {
            editor_text,
            cancelled: false,
            aborted: None,
            summary_entry: None,
        })
    }

    /// All user messages from the session, for the fork selector.
    pub fn get_user_messages_for_forking(&self) -> Vec<(String, String)> {
        let entries = lock(&self.inner.session_manager).get_entries();
        let mut result = Vec::new();
        for entry in entries {
            let Some(message) = entry.as_message() else {
                continue;
            };
            let AgentMessage::User(user) = message.message.as_ref() else {
                continue;
            };
            let text = user_message_text(&user.content);
            if !text.is_empty() {
                result.push((message.id.clone(), text));
            }
        }
        result
    }

    // =========================================================================
    // Statistics / export
    // =========================================================================

    pub fn get_session_stats(&self) -> SessionStats {
        let entries = lock(&self.inner.session_manager).get_entries();
        let totals = entries
            .iter()
            .fold(SessionStatsTotals::default(), |total, entry| {
                add_session_entry(total, entry)
            });
        let usage = totals.usage;
        SessionStats {
            session_file: self.session_file(),
            session_id: self.session_id(),
            user_messages: totals.user_messages,
            assistant_messages: totals.assistant_messages,
            tool_calls: totals.tool_calls,
            tool_results: totals.tool_results,
            total_messages: totals.total_messages,
            tokens: SessionTokenStats {
                input: usage.input,
                output: usage.output,
                cache_read: usage.cache_read,
                cache_write: usage.cache_write,
                total: usage.input + usage.output + usage.cache_read + usage.cache_write,
            },
            cost: usage.cost,
            context_usage: self.get_context_usage(),
        }
    }

    pub fn get_context_usage(&self) -> Option<ContextUsage> {
        let model = self.inner.agent.model();
        let context_window = model.context_window;
        if context_window <= 0.0 {
            return None;
        }
        let branch_entries = lock(&self.inner.session_manager).get_branch(None);
        let latest_compaction = get_latest_compaction_entry(&branch_entries).cloned();
        if let Some(latest_compaction) = latest_compaction {
            let compaction_index = branch_entries
                .iter()
                .position(|entry| entry == &latest_compaction)
                .unwrap_or(0);
            let has_post_compaction_usage = branch_entries
                .iter()
                .skip(compaction_index + 1)
                .rev()
                .find_map(|entry| {
                    let message = entry.as_message()?;
                    let AgentMessage::Assistant(assistant) = message.message.as_ref() else {
                        return None;
                    };
                    if assistant.stop_reason == StopReason::Aborted
                        || assistant.stop_reason == StopReason::Error
                    {
                        return None;
                    }
                    let context_tokens = calculate_context_tokens(&assistant.usage);
                    (context_tokens > 0.0).then_some(())
                })
                .is_some();
            if !has_post_compaction_usage {
                let tokens = estimate_messages_tokens(&self.messages());
                return Some(ContextUsage {
                    tokens,
                    context_window,
                    percent: (tokens / context_window) * 100.0,
                });
            }
        }
        let ContextUsageEstimate { tokens, .. } = estimate_context_tokens(&self.messages());
        Some(ContextUsage {
            tokens,
            context_window,
            percent: (tokens / context_window) * 100.0,
        })
    }

    /// Exports the current session branch to a JSONL file.
    pub fn export_to_jsonl(&self, output_path: Option<&str>) -> Result<String, AgentSessionError> {
        let default_name = format!(
            "session-{}.jsonl",
            crate::utils::iso_time::now_iso_string().replace([':', '.'], "-")
        );
        let file_path = crate::utils::paths::resolve_path(
            output_path.unwrap_or(&default_name),
            &std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            &Default::default(),
        )
        .map_err(|path_error| error(path_error.to_string()))?;
        if let Some(parent) = std::path::Path::new(&file_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|io_error| error(format!("Failed to create export directory: {io_error}")))?;
        }
        let (session_id, cwd, branch_entries) = {
            let manager = lock(&self.inner.session_manager);
            (
                manager.get_session_id().to_owned(),
                manager.get_cwd().to_owned(),
                manager.get_branch(None),
            )
        };
        let mut header = Map::new();
        header.insert("type".to_owned(), Value::String("session".to_owned()));
        header.insert(
            "version".to_owned(),
            serde_json::json!(crate::core::session_manager::CURRENT_SESSION_VERSION),
        );
        header.insert("id".to_owned(), Value::String(session_id));
        header.insert(
            "timestamp".to_owned(),
            Value::String(crate::utils::iso_time::now_iso_string()),
        );
        header.insert("cwd".to_owned(), Value::String(cwd));
        let mut lines = vec![Value::Object(header).to_string()];
        let mut previous_id: Option<String> = None;
        for entry in branch_entries {
            let mut value = serde_json::to_value(&entry)
                .map_err(|json_error| error(json_error.to_string()))?;
            if let Value::Object(object) = &mut value {
                object.insert(
                    "parentId".to_owned(),
                    previous_id.clone().map(Value::String).unwrap_or(Value::Null),
                );
            }
            previous_id = entry.id().map(str::to_owned);
            lines.push(value.to_string());
        }
        std::fs::write(&file_path, format!("{}\n", lines.join("\n")))
            .map_err(|io_error| error(format!("Failed to write session export: {io_error}")))?;
        Ok(file_path)
    }

    /// Text content of the last assistant message.
    pub fn get_last_assistant_text(&self) -> Option<String> {
        let last_assistant = self.messages().into_iter().rev().find(|message| {
            let AgentMessage::Assistant(assistant) = message else {
                return false;
            };
            !(assistant.stop_reason == StopReason::Aborted && assistant.content.is_empty())
        })?;
        let AgentMessage::Assistant(assistant) = last_assistant else {
            return None;
        };
        let text = get_assistant_message_text(&assistant);
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }

    /// Whether extensions have handlers for a given event type.
    ///
    /// Always false until `ca-extensions-api` merges.
    pub fn has_extension_handlers(&self, _event_type: &str) -> bool {
        false
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BashExecutionWire<'a> {
    role: &'static str,
    command: &'a str,
    output: &'a str,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "pi_js_compat::number::serialize_option")]
    exit_code: Option<f64>,
    cancelled: bool,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    full_output_path: Option<&'a str>,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    exclude_from_context: Option<bool>,
}

fn bash_message_to_agent_message(message: &BashExecutionMessage) -> AgentMessage {
    let wire = BashExecutionWire {
        role: "bashExecution",
        command: &message.command,
        output: &message.output,
        exit_code: message.exit_code,
        cancelled: message.cancelled,
        truncated: message.truncated,
        full_output_path: message.full_output_path.as_deref(),
        timestamp: message.timestamp,
        exclude_from_context: message.exclude_from_context,
    };
    match serde_json::to_value(wire) {
        Ok(Value::Object(object)) => AgentMessage::Custom(object),
        _ => AgentMessage::Custom(Map::new()),
    }
}

/// Re-export used by `sdk` and `agent-session-services`.
pub use crate::core::resource_loader::{
    ExtendedResourcePath, LoadedAgentsFiles, LoadedSkills, ResourceExtensionPaths, StaticResourceLoader,
};

/// Marker so unused imports in partial ports stay visible during review.
#[allow(dead_code)]
fn _unused_import_markers() {
    let _ = is_record;
    let _ = to_record;
    let _ = without_deleted_headers;
    let _ = normalize_message_content;
    let _ = find_last_user_text;
    let _ = session_model_message;
    let _ = normalize_required_request_auth_error;
    let _ = should_continue_interrupted_session;
    let _ = find_exact_model_reference_match;
    let _ = ApprovalPolicy::OnRequest;
    let _ = THINKING_PHASE_TIMEOUT_MS;
    let _ = BUILT_IN_COMPACTION_DISABLED_MESSAGE;
}
