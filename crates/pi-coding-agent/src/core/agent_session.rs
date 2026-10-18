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
use pi_agent_core::harness::compaction::{
    ContextUsageEstimate, calculate_context_tokens, estimate_context_tokens, estimate_tokens,
};
use pi_agent_core::harness::messages::{
    BashExecutionMessage, BashExecutionRole, CustomMessageContent, create_custom_message,
};
use pi_agent_core::harness::types::BeforeToolCallResult;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::request::BoxFuture;
use pi_ai::types::{
    AssistantContent, AssistantMessage, ImageContent, InputSource, Model, ProviderHeaders, ProviderRetryEvent,
    StopReason, TextContent, TextOrImageContent, Usage, UserContent, UserMessage, UserRole,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::core::defaults::DEFAULT_THINKING_LEVEL;
use crate::core::model_registry::ModelRegistry;
use crate::core::model_resolver::{ScopedModel, find_exact_model_reference_match, resolve_model_scope};
use crate::core::model_runtime::ModelRuntime;
use crate::core::permissions::policy::{ApprovalPolicy, evaluate_approval_policy};
use crate::core::prompt_templates::expand_prompt_template;
use crate::core::resource_loader::{LoadedPrompts, ResourceLoader};
use crate::core::session_manager::{SessionEntry, SessionManager, SessionManagerError, get_latest_compaction_entry};
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
    pub extension_runner_ref: Option<Arc<Mutex<Option<ExtensionRunner>>>>,
    pub session_start_event: Option<SessionStartEvent>,
}

/// `onUserMessagePrepared` callback.
pub type UserMessagePreparedCallback = Arc<dyn Fn(&AgentMessage) + Send + Sync>;
/// `preflightResult` callback.
pub type PromptPreflightCallback = Arc<dyn Fn(bool) + Send + Sync>;
/// Provider registration action types.
pub type RegisterProviderAction =
    Arc<dyn Fn(&str, &crate::core::provider_composer::ProviderConfigInput) -> Result<(), String> + Send + Sync>;
pub type RegisterNativeProviderAction = Arc<dyn Fn(Arc<dyn pi_ai::models::Provider>) + Send + Sync>;
pub type UnregisterProviderAction = Arc<dyn Fn(&str) + Send + Sync>;

/// `PromptOptions`.
#[derive(Default)]
pub struct PromptOptions {
    pub expand_prompt_templates: Option<bool>,
    pub images: Vec<ImageContent>,
    pub streaming_behavior: Option<StreamingBehavior>,
    pub source: Option<InputSource>,
    pub preflight_result: Option<PromptPreflightCallback>,
    pub on_user_message_prepared: Option<UserMessagePreparedCallback>,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

fn flag_values_map(
    values: &indexmap::IndexMap<String, crate::core::extensions::types::FlagValue>,
) -> BTreeMap<String, Value> {
    values
        .iter()
        .map(|(name, value)| {
            let value = match value {
                crate::core::extensions::types::FlagValue::Boolean(boolean) => Value::Bool(*boolean),
                crate::core::extensions::types::FlagValue::String(text) => Value::String(text.clone()),
            };
            (name.clone(), value)
        })
        .collect()
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

fn is_allowed_session_tool(name: &str, allowed: Option<&HashSet<String>>, excluded: Option<&HashSet<String>>) -> bool {
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

struct CompactionRequestAuth {
    model: Model,
    api_key: Option<String>,
    headers: Option<ProviderHeaders>,
    env: Option<pi_ai::types::ProviderEnv>,
}

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
    extension_runner: Mutex<ExtensionRunner>,
    extension_runner_ref: Option<Arc<Mutex<Option<ExtensionRunner>>>>,
    session_start_event: SessionStartEvent,
    extension_ui_context: Mutex<Option<Arc<dyn ExtensionUiContext>>>,
    extension_footer_data: Mutex<Option<Arc<FooterDataProvider>>>,
    extension_mode: Mutex<ExtensionMode>,
    extension_control_db_path: Mutex<Option<String>>,
    extension_command_context_actions: Mutex<Option<ExtensionCommandContextActions>>,
    extension_abort_handler: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    extension_shutdown_handler: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    extension_error_listener: Mutex<Option<ExtensionErrorListener>>,
    extension_error_unsubscriber: Mutex<Option<ErrorSubscription>>,
    turn_index: AtomicU64,
    overflow_recovery_attempted: AtomicBool,
    length_recovery_attempted: AtomicBool,
}

/// Core session abstraction shared by all run modes.
#[derive(Clone)]
pub struct AgentSession {
    inner: Arc<Inner>,
}

impl AgentSession {
    /// Builds a session and installs agent hooks, subscriptions, and tools.
    pub fn new(config: AgentSessionConfig) -> Result<Self, AgentSessionError> {
        validate_multi_agent_runtime_role(config.multi_agent_runtime_role, config.multi_agent_execution_capability)?;

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
        let extension_runner_ref = config.extension_runner_ref.clone();
        let session_start_event = config.session_start_event.clone().unwrap_or(SessionStartEvent {
            reason: crate::core::extensions::types::SessionStartReason::Startup,
            previous_session_file: None,
        });
        let placeholder_runner = ExtensionRunner::new(
            Vec::new(),
            crate::core::extensions::extension_runtime::create_extension_runtime(),
            config.cwd.clone(),
            Arc::new(
                config
                    .session_manager
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone(),
            ),
            Arc::new(ModelRegistry::new(model_runtime.clone())),
            Some(Arc::clone(&config.settings_manager)),
        );

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
            allowed_tool_names: config.allowed_tool_names.map(|names| names.into_iter().collect()),
            excluded_tool_names: config.excluded_tool_names.map(|names| names.into_iter().collect()),
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
            extension_runner: Mutex::new(placeholder_runner),
            extension_runner_ref,
            session_start_event,
            extension_ui_context: Mutex::new(None),
            extension_footer_data: Mutex::new(None),
            extension_mode: Mutex::new(ExtensionMode::Print),
            extension_control_db_path: Mutex::new(None),
            extension_command_context_actions: Mutex::new(None),
            extension_abort_handler: Mutex::new(None),
            extension_shutdown_handler: Mutex::new(None),
            extension_error_listener: Mutex::new(None),
            extension_error_unsubscriber: Mutex::new(None),
            turn_index: AtomicU64::new(0),
            overflow_recovery_attempted: AtomicBool::new(false),
            length_recovery_attempted: AtomicBool::new(false),
        });

        let session = Self { inner };
        session.install_agent_hooks();
        session.install_agent_next_turn_refresh();
        session.install_duplicate_turn_stop();
        session.subscribe_agent();
        session.build_runtime(session.inner.initial_active_tool_names.clone(), None, true);
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
                    Ok(session.before_tool_call(context).await)
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
                    Ok(session.after_tool_call(context).await)
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
        let extensions = self.inner.resource_loader.get_extensions();
        let mut runner = ExtensionRunner::new(
            extensions.extensions,
            extensions.runtime,
            self.inner.cwd.clone(),
            Arc::new(
                self.inner
                    .session_manager
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .clone(),
            ),
            Arc::new(self.inner.model_registry.clone()),
            Some(Arc::clone(&self.inner.settings_manager)),
        );
        runner.set_internal_commands(self.create_internal_commands());
        self.bind_extension_core(&mut runner);
        *lock(&self.inner.extension_runner) = runner.clone();
        if let Some(reference) = &self.inner.extension_runner_ref {
            *lock(reference) = Some(runner.clone());
        }
        self.apply_extension_bindings(&runner);
        self.refresh_tool_registry(active_tool_names, include_all_extension_tools);
    }

    fn refresh_tool_registry(&self, active_tool_names: Option<Vec<String>>, include_all_extension_tools: bool) {
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

        let extension_tools = {
            let runner = self.runner();
            runner
                .get_all_registered_tools()
                .into_iter()
                .filter(|tool| is_allowed(&tool.definition.name))
                .map(|tool| {
                    let name = tool.definition.name.clone();
                    let wrapped = wrap_registered_tool(tool.clone(), &runner);
                    let definition = ToolDefinition {
                        name: name.clone(),
                        description: tool.definition.description.clone(),
                        parameters: tool.definition.parameters.clone(),
                        prompt_snippet: tool.definition.prompt_snippet.clone(),
                        prompt_guidelines: tool.definition.prompt_guidelines.clone().unwrap_or_default(),
                        approval_required: tool.definition.approval_required.unwrap_or(true),
                        tool: wrapped,
                    };
                    (
                        name,
                        ToolDefinitionEntry {
                            definition,
                            source_info: tool.source_info.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        for (name, entry) in extension_tools {
            if let Some(existing) = definition_registry.iter_mut().find(|(tool_name, _)| *tool_name == name) {
                existing.1 = entry;
            } else {
                definition_registry.push((name, entry));
            }
        }

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

    async fn before_tool_call(&self, context: BeforeToolCallContext) -> Option<BeforeToolCallResult> {
        let runner = self.runner();
        let event = ToolCallEvent {
            tool_name: crate::core::extensions::types::ToolName::Custom(context.tool_call.name.clone()),
            tool_call_id: context.tool_call.id.clone(),
            bypass_permissions: false,
            input: context.args_snapshot().as_object().cloned().unwrap_or_default(),
        };
        if runner.has_tool_gates() {
            let mut gated = event.clone();
            if let Ok(Some(result)) = runner.emit_tool_gates(&mut gated).await {
                return Some(tool_call_result_to_before(result));
            }
        }

        let approval_required = self
            .find_tool_definition(&context.tool_call.name)
            .is_none_or(|entry| entry.definition.approval_required);
        let policy = lock(&self.inner.settings_manager).get_approval_policy();
        let decision = evaluate_approval_policy(
            policy,
            crate::core::permissions::policy::ApprovalPolicyContext { approval_required },
        );
        match decision {
            crate::core::permissions::policy::ApprovalPolicyDecision::Allow => None,
            crate::core::permissions::policy::ApprovalPolicyDecision::Block { reason } => Some(BeforeToolCallResult {
                block: Some(true),
                reason: Some(reason),
                terminate: None,
            }),
            crate::core::permissions::policy::ApprovalPolicyDecision::Review => {
                if approval_required && runner.has_approval_reviewers() {
                    let mut reviewed = event.clone();
                    match runner.emit_approval_reviewers(&mut reviewed).await {
                        Ok(Some(crate::core::extensions::types::ApprovalReviewerResult::Deny { reason })) => {
                            return Some(BeforeToolCallResult {
                                block: Some(true),
                                reason: Some(reason),
                                terminate: None,
                            });
                        }
                        Ok(Some(crate::core::extensions::types::ApprovalReviewerResult::Allow { .. })) => {
                            return None;
                        }
                        _ => {}
                    }
                }
                if !runner.has_handlers("tool_call") {
                    return None;
                }
                let mut called = event;
                match runner.emit_tool_call(&mut called).await {
                    Ok(Some(result)) => Some(tool_call_result_to_before(result)),
                    Ok(None) => None,
                    Err(error) => Some(BeforeToolCallResult {
                        block: Some(true),
                        reason: Some(format!("Extension failed, blocking execution: {error}")),
                        terminate: None,
                    }),
                }
            }
        }
    }

    async fn after_tool_call(&self, context: AfterToolCallContext) -> Option<AfterToolCallResult> {
        let runner = self.runner();
        if !runner.has_handlers("tool_result") {
            return None;
        }
        let event = crate::core::extensions::types::ToolResultEvent {
            tool_name: crate::core::extensions::types::ToolName::Custom(context.tool_call.name.clone()),
            tool_call_id: context.tool_call.id.clone(),
            input: context.args.as_object().cloned().unwrap_or_default(),
            content: context.result.content.clone(),
            is_error: context.is_error,
            usage: context.result.usage.clone(),
            details: context.result.details.clone(),
        };
        let result = runner.emit_tool_result(event).await?;
        Some(AfterToolCallResult {
            content: result.content,
            details: result.details,
            is_error: result.is_error,
            usage: result.usage,
            terminate: None,
        })
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
        self.emit_agent_settled_extension().await;
        self.emit(AgentSessionEvent::AgentSettled(AgentSettledEvent {
            event_type: "agent_settled",
        }));
    }

    fn reset_duplicate_turn_guard(&self) {
        *lock(&self.inner.last_assistant_turn_fingerprint) = None;
        self.inner.duplicate_turn_guard_injected.store(false, Ordering::SeqCst);
        self.inner.duplicate_turn_loop_detected.store(false, Ordering::SeqCst);
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
            self.inner.duplicate_turn_guard_injected.store(false, Ordering::SeqCst);
            return;
        }
        drop(last);
        mark_duplicate_turn_assistant_message(message);
        if self.inner.duplicate_turn_guard_injected.load(Ordering::SeqCst) {
            self.inner.duplicate_turn_loop_detected.store(true, Ordering::SeqCst);
            return;
        }
        self.inner.duplicate_turn_guard_injected.store(true, Ordering::SeqCst);
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

    async fn handle_agent_event(&self, mut event: AgentEvent) {
        let original_tool_call_id = match &event {
            AgentEvent::MessageEnd {
                message: AgentMessage::ToolResult(result),
            } => Some(result.tool_call_id.clone()),
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
        let replacement = self.emit_extension_event(&mut event).await;
        if let (Some(replacement), AgentEvent::MessageEnd { message }) = (replacement, &mut event) {
            *message = replacement;
        }
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
                self.inner.quota_fallback_attempted.store(false, Ordering::SeqCst);
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
        let _ = crate::core::session_control_db::write_last_message(control_db_path, "assistant", &content);
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

    fn remove_trailing_interrupted_assistant(&self, remove_tool_use_assistant: bool) {
        let mut messages = self.inner.agent.messages();
        let Some(AgentMessage::Assistant(assistant)) = messages.last() else {
            return;
        };
        let stop = assistant.stop_reason;
        if stop == StopReason::Aborted
            || stop == StopReason::Error
            || (remove_tool_use_assistant && stop == StopReason::ToolUse)
        {
            messages.pop();
            self.inner.agent.set_messages(messages);
        }
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
            self.continue_agent_with_thinking_timeout(kind == "queued").await?;
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
            if self.try_execute_extension_command(text).await {
                self.record_handled_slash_command_history(text);
                if let Some(preflight) = &options.preflight_result {
                    preflight(true);
                }
                return Ok(());
            }
            if !self.is_known_slash_command(text) {
                return Err(error(format!("Unknown slash command: {}", slash_command_name(text))));
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
        let input = self.prepare_prompt_input(text, options).await;
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
        let messages = self.build_prompt_messages(&text, &images, options).await;
        Ok(Some(messages))
    }

    async fn prepare_prompt_input(&self, text: &str, options: &PromptOptions) -> Option<(String, Vec<ImageContent>)> {
        let mut current_text = text.to_owned();
        let mut current_images = options.images.clone();
        match self
            .emit_extension_input(
                &current_text,
                &current_images,
                options.source,
                if self.is_streaming() {
                    options.streaming_behavior
                } else {
                    None
                },
            )
            .await
        {
            InputEventResult::Handled => return None,
            InputEventResult::Transform { text, images } => {
                current_text = text;
                if let Some(images) = images {
                    current_images = images;
                }
            }
            InputEventResult::Continue => {}
        }
        if options.expand_prompt_templates.unwrap_or(true) {
            current_text = self.expand_skill_command(&current_text);
            current_text = expand_prompt_template(&current_text, &self.prompt_templates());
        }
        Some((current_text, current_images))
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

    async fn build_prompt_messages(
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
        let base_prompt = lock(&self.inner.base_system_prompt).clone();
        let base_options =
            lock(&self.inner.base_system_prompt_options)
                .clone()
                .unwrap_or_else(|| BuildSystemPromptOptions {
                    cwd: self.inner.cwd.clone(),
                    ..Default::default()
                });
        let result = self.emit_before_agent_start(text, images, &base_options).await;
        if let Some(result) = &result {
            for payload in result.messages.clone().unwrap_or_default() {
                let content = match payload.content {
                    crate::core::extensions::types::CustomMessageContent::Text(text) => {
                        CustomMessageContent::Text(text)
                    }
                    crate::core::extensions::types::CustomMessageContent::Blocks(blocks) => {
                        CustomMessageContent::Blocks(blocks)
                    }
                };
                messages.push(create_custom_message(
                    &payload.custom_type,
                    content,
                    payload.display,
                    payload.details,
                    now_millis(),
                ));
            }
        }
        let system_prompt_override = result.and_then(|result| result.system_prompt);
        *lock(&self.inner.system_prompt_override) = system_prompt_override.clone();
        self.inner
            .agent
            .set_system_prompt(system_prompt_override.unwrap_or(base_prompt));
        messages
    }

    fn is_known_slash_command(&self, text: &str) -> bool {
        let command_name = slash_command_name(text);
        let name = command_name.trim_start_matches('/');
        if builtin_slash_commands().iter().any(|command| command.name == name) {
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
        self.prompt_templates().iter().any(|template| template.name == name)
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
    pub async fn follow_up(&self, text: &str, images: &[ImageContent]) -> Result<(), AgentSessionError> {
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
        self.emit(AgentSessionEvent::SteeringMessageQueued(SteeringMessageQueuedEvent {
            event_type: "steering_message_queued",
        }));
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
    pub fn send_custom_message_next_turn(
        &self,
        custom_type: &str,
        content: CustomMessageContent,
        display: bool,
        details: Option<Value>,
    ) {
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
        self.pending_message_count() > 0 || self.inner.external_user_input_reservations.load(Ordering::SeqCst) > 0
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
    pub async fn interrupt(&self, message: &str, source: InputSource) -> Result<(), AgentSessionError> {
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
            return Err(error(format!("No API key for {}/{}", model.provider, model.id)));
        }
        let previous_model = self.inner.agent.model();
        let thinking_level = self.get_thinking_level_for_model_switch(None);
        self.inner.agent.set_model(model.clone());
        lock(&self.inner.settings_manager).set_default_model_and_provider(&model.provider, &model.id);
        self.set_thinking_level(thinking_level);
        self.emit_model_select(&model, Some(&previous_model), ModelSelectSource::Set)
            .await;
        Ok(())
    }

    /// Cycles to the next/previous scoped model.
    pub async fn cycle_model(&self, direction: CycleDirection) -> Result<Option<ModelCycleResult>, AgentSessionError> {
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
        let resolved = resolve_model_scope(&enabled_models, &self.inner.model_runtime, Default::default()).await;
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
            .filter(|scoped| available_ids.contains(&format!("{}\0{}", scoped.model.provider, scoped.model.id)))
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
        lock(&self.inner.settings_manager).set_default_model_and_provider(&next.model.provider, &next.model.id);
        self.set_thinking_level(thinking_level);
        let previous_model = self.inner.agent.model();
        self.emit_model_select(&next.model, Some(&previous_model), ModelSelectSource::Cycle)
            .await;
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
            self.emit(AgentSessionEvent::ThinkingLevelChanged(ThinkingLevelChangedEvent {
                event_type: "thinking_level_changed",
                level: effective,
            }));
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let session = self.clone();
                handle.spawn(async move {
                    session.emit_thinking_level_select(effective, previous).await;
                });
            }
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
        thinking_level_from_model(pi_ai::models::clamp_thinking_level(&model, model_thinking_level(level)))
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
        if self.inner.quota_fallback_attempted.load(Ordering::SeqCst) || message.stop_reason != StopReason::Error {
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
        self.inner.quota_fallback_attempted.store(true, Ordering::SeqCst);
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
        self.emit(AgentSessionEvent::ProviderStreamRetry(ProviderStreamRetryEvent {
            event_type: "provider_stream_retry",
            retry,
        }));
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

/// Options for [`AgentSession::execute_bash`].
#[derive(Clone, Default)]
pub struct ExecuteBashOptions {
    pub exclude_from_context: Option<bool>,
    /// Identifier included in `bash_execution_update` events.
    pub id: Option<String>,
    /// Custom operations for remote execution.
    pub operations: Option<Arc<dyn crate::core::tools::bash::BashOperations>>,
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
            let current = inner.external_user_input_reservations.load(Ordering::SeqCst);
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

fn serialize_thinking_level<S: serde::Serializer>(level: &ThinkingLevel, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(level.as_str())
}

fn compaction_settings(settings_manager: &Arc<Mutex<SettingsManager>>) -> crate::core::compaction::CompactionSettings {
    let view = lock(settings_manager).get_compaction_settings();
    crate::core::compaction::CompactionSettings {
        enabled: view.enabled,
        reserve_tokens: view.reserve_tokens,
        keep_recent_tokens: view.keep_recent_tokens,
    }
}

struct CompactedContextEstimate {
    estimated_tokens_after: f64,
    kept_from_previous_context_tokens: f64,
}

fn estimate_compacted_context_tokens(
    messages: &[AgentMessage],
    summary: &str,
    tokens_before: f64,
    compacted_result_tokens: Option<f64>,
) -> CompactedContextEstimate {
    let synthetic_tokens_after = estimate_messages_tokens(messages);
    let synthetic_summary = create_compaction_summary_probe(summary, tokens_before);
    let synthetic_summary_tokens = estimate_tokens(&synthetic_summary);
    let compacted_result_tokens = compacted_result_tokens.unwrap_or(synthetic_summary_tokens);
    let kept_from_previous_context_tokens = (synthetic_tokens_after - synthetic_summary_tokens).max(0.0);
    CompactedContextEstimate {
        estimated_tokens_after: kept_from_previous_context_tokens + compacted_result_tokens,
        kept_from_previous_context_tokens,
    }
}

fn create_compaction_summary_probe(summary: &str, tokens_before: f64) -> AgentMessage {
    pi_agent_core::harness::messages::create_compaction_summary_message(
        summary,
        tokens_before,
        0,
        &pi_agent_core::harness::messages::CompactionSummaryOptions::default(),
    )
}

fn should_continue_after_manual_compaction(messages: &[AgentMessage]) -> bool {
    let mut latest_user_index: isize = -1;
    let mut latest_assistant_index: isize = -1;
    let mut latest_completed_assistant_index: isize = -1;
    for index in (0..messages.len()).rev() {
        let message = &messages[index];
        let index = index as isize;
        if latest_user_index == -1 && matches!(message, AgentMessage::User(_)) {
            latest_user_index = index;
        }
        if latest_assistant_index == -1
            && let AgentMessage::Assistant(assistant) = message
        {
            latest_assistant_index = index;
            if assistant.stop_reason == StopReason::Aborted || assistant.stop_reason == StopReason::Error {
                return true;
            }
        }
        if latest_completed_assistant_index == -1
            && let AgentMessage::Assistant(assistant) = message
            && assistant.stop_reason != StopReason::Aborted
            && assistant.stop_reason != StopReason::Error
        {
            latest_completed_assistant_index = index;
        }
        if latest_user_index != -1 && latest_assistant_index != -1 && latest_completed_assistant_index != -1 {
            break;
        }
    }
    latest_user_index != -1 && latest_completed_assistant_index < latest_user_index
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
    let custom_type = object.get("customType").and_then(Value::as_str).unwrap_or_default();
    let content = object
        .get("content")
        .cloned()
        .and_then(|content| serde_json::from_value::<CustomMessageContent>(content).ok())
        .unwrap_or(CustomMessageContent::Blocks(Vec::new()));
    let display = object.get("display").and_then(Value::as_bool).unwrap_or(false);
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

    pub fn session_manager_handle(&self) -> Arc<Mutex<SessionManager>> {
        Arc::clone(&self.inner.session_manager)
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
        self.inner.resume_continuation_requested.store(true, Ordering::SeqCst);
    }

    pub fn consume_resume_continuation_request(&self) -> bool {
        self.inner.resume_continuation_requested.swap(false, Ordering::SeqCst)
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
        self.find_tool_definition(name).map(|entry| entry.definition)
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
        lock(&self.inner.session_manager).get_session_file().map(str::to_owned)
    }

    pub fn session_id(&self) -> String {
        lock(&self.inner.session_manager).get_session_id().to_owned()
    }

    pub fn control_db_path(&self) -> Option<String> {
        self.inner.control_db_path.clone()
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
    pub async fn compact(
        &self,
        custom_instructions: Option<&str>,
    ) -> Result<crate::core::compaction::CompactionResult, AgentSessionError> {
        let guard = self.inner.turn_start_lock.lock().await;
        let result = self.compact_inner(custom_instructions).await;
        drop(guard);
        result
    }

    async fn compact_inner(
        &self,
        custom_instructions: Option<&str>,
    ) -> Result<crate::core::compaction::CompactionResult, AgentSessionError> {
        let was_running_agent_turn = self.is_streaming();
        let compaction_started_during_tool_execution =
            was_running_agent_turn && !self.inner.agent.pending_tool_calls().is_empty();
        self.disconnect_from_agent();
        self.abort().await?;
        let abort = CancellationToken::new();
        *lock(&self.inner.compaction_abort_controller) = Some(abort.clone());
        let source_hint = self.get_compaction_source_hint("manual", false).await;
        self.emit(AgentSessionEvent::CompactionStart(CompactionStartEvent {
            event_type: "compaction_start",
            reason: "manual".to_owned(),
            source_hint,
        }));
        let started_at = now_millis();
        let result = self
            .compact_with_settings(
                custom_instructions,
                &abort,
                was_running_agent_turn,
                compaction_started_during_tool_execution,
                started_at,
                "manual",
            )
            .await;
        match result {
            Ok((compaction_result, should_continue)) => {
                *lock(&self.inner.compaction_abort_controller) = None;
                self.emit(AgentSessionEvent::CompactionEnd(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: "manual".to_owned(),
                    result: Some(serde_json::to_value(&compaction_result).unwrap_or(Value::Null)),
                    aborted: false,
                    will_retry: should_continue,
                    error_message: None,
                }));
                self.reconnect_to_agent();
                if should_continue {
                    self.continue_after_manual_compaction(was_running_agent_turn).await?;
                }
                Ok(compaction_result)
            }
            Err(error) => {
                let aborted = error.message() == "Compaction cancelled";
                *lock(&self.inner.compaction_abort_controller) = None;
                self.emit(AgentSessionEvent::CompactionEnd(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: "manual".to_owned(),
                    result: None,
                    aborted,
                    will_retry: false,
                    error_message: (!aborted).then(|| format!("Compaction failed: {}", error.message())),
                }));
                self.reconnect_to_agent();
                Err(error)
            }
        }
    }

    async fn continue_after_manual_compaction(&self, remove_tool_use_assistant: bool) -> Result<(), AgentSessionError> {
        if self.is_streaming() {
            return Err(error(
                "Agent is already processing. Wait for completion before continuing.",
            ));
        }
        self.remove_trailing_interrupted_assistant(remove_tool_use_assistant);
        self.continue_agent_with_thinking_timeout(false).await?;
        self.continue_post_agent_runs().await
    }

    #[allow(clippy::too_many_arguments)]
    async fn compact_with_settings(
        &self,
        custom_instructions: Option<&str>,
        abort: &CancellationToken,
        was_running_agent_turn: bool,
        compaction_started_during_tool_execution: bool,
        started_at: i64,
        reason: &str,
    ) -> Result<(crate::core::compaction::CompactionResult, bool), AgentSessionError> {
        let model = self.inner.agent.model();
        let settings = compaction_settings(&self.inner.settings_manager);
        let auth = self.get_compaction_request_auth(&model).await;
        let path_entries = lock(&self.inner.session_manager).get_branch(None);
        let Some(preparation) = crate::core::compaction::prepare_compaction(&path_entries, settings) else {
            if path_entries
                .last()
                .is_some_and(|entry| entry.entry_type() == "compaction")
            {
                return Err(error("Already compacted"));
            }
            return Err(error("Nothing to compact (session too small)"));
        };
        if !settings.enabled {
            return Err(error(BUILT_IN_COMPACTION_DISABLED_MESSAGE));
        }
        let retry_policy = self.retry_policy();
        let stream_fn = self.inner.agent.stream_fn();
        let stream_fn = Some(&stream_fn);
        let summary_options = crate::core::compaction::CompactionSummaryOptions {
            model: &auth.model,
            api_key: auth.api_key.clone(),
            headers: auth.headers.clone(),
            custom_instructions,
            signal: Some(abort.clone()),
            thinking_level: Some(self.thinking_level()),
            stream_fn,
            env: auth.env.clone(),
            retry: Some(&retry_policy),
        };
        let mut compact_result = crate::core::compaction::compact(
            &preparation,
            &auth.model,
            &summary_options,
            None::<&mut pi_agent_core::harness::compaction::NoRetryCallbacks>,
        )
        .await
        .map_err(|compaction_error| error(compaction_error.message()))?;
        if abort.is_cancelled() {
            return Err(error("Compaction cancelled"));
        }
        let duration_ms = (now_millis() - started_at) as f64;
        let source = compact_result.source.clone();
        let usage = compact_result.usage.clone();
        let details = compact_result.details.clone();
        let compacted_result_tokens = compact_result.compacted_result_tokens;
        let provider_native = compact_result.provider_native.clone();
        {
            let mut manager = lock(&self.inner.session_manager);
            manager
                .append_compaction(
                    &compact_result.summary,
                    &compact_result.first_kept_entry_id,
                    compact_result.tokens_before,
                    crate::core::session_manager::CompactionAppendOptions {
                        details,
                        from_hook: None,
                        usage,
                        duration_ms: Some(duration_ms),
                        provider_native,
                    },
                )
                .map_err(|manager_error| error(manager_error.to_string()))?;
        }
        let session_context = lock(&self.inner.session_manager).build_session_context();
        self.inner.agent.set_messages(session_context.messages.clone());
        let estimate = estimate_compacted_context_tokens(
            &session_context.messages,
            &compact_result.summary,
            compact_result.tokens_before,
            compacted_result_tokens,
        );
        let will_retry = reason == "manual"
            && !compaction_started_during_tool_execution
            && (was_running_agent_turn || should_continue_after_manual_compaction(&session_context.messages));
        compact_result.duration_ms = Some(duration_ms);
        compact_result.estimated_tokens_after = Some(estimate.estimated_tokens_after);
        compact_result.kept_from_previous_context_tokens = Some(estimate.kept_from_previous_context_tokens);
        compact_result.compacted_result_tokens = compacted_result_tokens;
        compact_result.source = source;
        Ok((compact_result, will_retry))
    }

    fn disconnect_from_agent(&self) {
        if let Some(subscription) = lock(&self.inner.agent_subscription).take() {
            self.inner.agent.unsubscribe(subscription);
        }
    }

    fn reconnect_to_agent(&self) {
        if lock(&self.inner.agent_subscription).is_some() {
            return;
        }
        self.subscribe_agent();
    }

    fn retry_policy(&self) -> pi_ai::utils::retry::RetryPolicy {
        let settings = lock(&self.inner.settings_manager).get_retry_settings();
        pi_ai::utils::retry::RetryPolicy {
            enabled: settings.enabled,
            max_retries: settings.max_retries.max(0.0) as u32,
            base_delay_ms: settings.base_delay_ms,
        }
    }

    async fn get_compaction_source_hint(&self, reason: &str, will_retry: bool) -> Option<Value> {
        let runner = self.runner();
        if runner.has_handlers("session_compaction_source") {
            let mut event =
                ExtensionEvent::SessionCompactionSource(crate::core::extensions::types::SessionCompactionSourceEvent {
                    reason: match reason {
                        "threshold" => crate::core::extensions::types::CompactionReason::Threshold,
                        "overflow" => crate::core::extensions::types::CompactionReason::Overflow,
                        _ => crate::core::extensions::types::CompactionReason::Manual,
                    },
                    will_retry,
                });
            let emit_result = runner.emit(&mut event).await;
            if let Some(source) = emit_result.session_compaction_source()
                && let Some(source) = &source.source
            {
                return serde_json::to_value(source).ok();
            }
        }
        if runner.has_handlers("compaction") {
            return None;
        }
        if !lock(&self.inner.settings_manager).get_compaction_enabled() {
            return None;
        }
        let model = self.inner.agent.model();
        serde_json::to_value(crate::core::compaction::CompactionSourceInfo {
            r#type: crate::core::compaction::CompactionSourceType::Local,
            provider: model.provider,
            model: model.id,
            endpoint: None,
        })
        .ok()
    }

    async fn get_compaction_request_auth(&self, model: &Model) -> CompactionRequestAuth {
        match self
            .inner
            .model_runtime
            .get_auth(&model.provider, Default::default())
            .await
        {
            Ok(Some(result)) => {
                let request_model = if let Some(base_url) = result.auth.base_url {
                    let mut request_model = model.clone();
                    request_model.base_url = base_url;
                    request_model
                } else {
                    model.clone()
                };
                CompactionRequestAuth {
                    model: request_model,
                    api_key: result.auth.api_key,
                    headers: without_deleted_headers(result.auth.headers.as_ref()),
                    env: result.env,
                }
            }
            _ => CompactionRequestAuth {
                model: model.clone(),
                api_key: None,
                headers: None,
                env: None,
            },
        }
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

    async fn check_compaction(&self, assistant_message: &AssistantMessage, post_run_check: bool) -> bool {
        let settings = compaction_settings(&self.inner.settings_manager);
        if post_run_check && assistant_message.stop_reason == StopReason::Aborted {
            return false;
        }
        let model = self.inner.agent.model();
        let context_window = model.context_window;
        let same_model = assistant_message.provider == model.provider && assistant_message.model == model.id;
        let branch = lock(&self.inner.session_manager).get_branch(None);
        let compaction_entry = get_latest_compaction_entry(&branch).cloned();
        let assistant_is_before_compaction = compaction_entry.as_ref().is_some_and(|entry| {
            let compaction_timestamp = entry.timestamp().and_then(pi_js_compat::date::date_parse_millis);
            compaction_timestamp.is_some_and(|timestamp| assistant_message.timestamp <= timestamp)
        });
        if assistant_is_before_compaction {
            return false;
        }
        let recoverable_length =
            same_model && pi_ai::utils::overflow::is_recoverable_length(assistant_message, model.max_tokens);
        if same_model
            && (pi_ai::utils::overflow::is_context_overflow(assistant_message, Some(context_window))
                || recoverable_length)
        {
            let should_retry_after_compaction = assistant_message.stop_reason != StopReason::Stop;
            let should_continue_after_compaction = post_run_check && should_retry_after_compaction;
            if !should_retry_after_compaction {
                return self.run_auto_compaction("overflow", false, false).await;
            }
            if self.inner.overflow_recovery_attempted.load(Ordering::SeqCst) {
                self.emit(AgentSessionEvent::CompactionEnd(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: "overflow".to_owned(),
                    result: None,
                    aborted: false,
                    will_retry: false,
                    error_message: Some(
                        "Context overflow recovery failed after one compact-and-retry attempt. Try reducing context or switching to a larger-context model.".to_owned(),
                    ),
                }));
                return false;
            }
            if post_run_check {
                self.inner.overflow_recovery_attempted.store(true, Ordering::SeqCst);
                let mut messages = self.inner.agent.messages();
                if matches!(messages.last(), Some(AgentMessage::Assistant(_))) {
                    messages.pop();
                    self.inner.agent.set_messages(messages);
                }
            }
            return self
                .run_auto_compaction(
                    "overflow",
                    should_continue_after_compaction,
                    should_continue_after_compaction,
                )
                .await;
        }

        let direct_context_tokens = crate::core::compaction::calculate_context_tokens(&assistant_message.usage);
        let mut context_tokens = direct_context_tokens;
        if assistant_message.stop_reason == StopReason::Error || direct_context_tokens == 0.0 {
            let messages = self.inner.agent.messages();
            let estimate = crate::core::compaction::estimate_context_tokens(&messages);
            let Some(last_usage_index) = estimate.last_usage_index else {
                return false;
            };
            let index = last_usage_index as usize;
            if let Some(compaction_entry) = &compaction_entry
                && let Some(AgentMessage::Assistant(usage_message)) = messages.get(index)
                && let Some(timestamp) = compaction_entry
                    .timestamp()
                    .and_then(pi_js_compat::date::date_parse_millis)
                && usage_message.timestamp <= timestamp
            {
                return false;
            }
            context_tokens = estimate.tokens;
        }
        if crate::core::compaction::should_compact(
            context_tokens,
            context_window,
            &settings,
            model.auto_compaction_threshold,
        ) {
            let turn_was_truncated = assistant_message.stop_reason == StopReason::Length;
            let will_retry =
                post_run_check && turn_was_truncated && !self.inner.length_recovery_attempted.load(Ordering::SeqCst);
            if will_retry {
                self.inner.length_recovery_attempted.store(true, Ordering::SeqCst);
            }
            return self.run_auto_compaction("threshold", will_retry, will_retry).await;
        }
        false
    }

    async fn run_auto_compaction(&self, reason: &str, will_retry: bool, continue_after_compaction: bool) -> bool {
        let abort = CancellationToken::new();
        *lock(&self.inner.auto_compaction_abort_controller) = Some(abort.clone());
        let source_hint = self.get_compaction_source_hint(reason, will_retry).await;
        self.emit(AgentSessionEvent::CompactionStart(CompactionStartEvent {
            event_type: "compaction_start",
            reason: reason.to_owned(),
            source_hint,
        }));
        let started_at = now_millis();
        let result = self
            .compact_with_settings(None, &abort, false, false, started_at, reason)
            .await;
        *lock(&self.inner.auto_compaction_abort_controller) = None;
        match result {
            Ok((compaction_result, _)) => {
                self.emit(AgentSessionEvent::CompactionEnd(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: reason.to_owned(),
                    result: Some(serde_json::to_value(&compaction_result).unwrap_or(Value::Null)),
                    aborted: false,
                    will_retry,
                    error_message: None,
                }));
                if continue_after_compaction {
                    let mut messages = self.inner.agent.messages();
                    if let Some(AgentMessage::Assistant(assistant)) = messages.last()
                        && (assistant.stop_reason == StopReason::Error || assistant.stop_reason == StopReason::Length)
                    {
                        messages.pop();
                        self.inner.agent.set_messages(messages);
                    }
                    return true;
                }
                self.inner.agent.has_queued_messages()
            }
            Err(error) => {
                let aborted = abort.is_cancelled() || error.message() == "Compaction cancelled";
                self.emit(AgentSessionEvent::CompactionEnd(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: reason.to_owned(),
                    result: None,
                    aborted,
                    will_retry: false,
                    error_message: (!aborted).then(|| {
                        if reason == "overflow" {
                            format!("Context overflow recovery failed: {}", error.message())
                        } else {
                            format!("Auto-compaction failed: {}", error.message())
                        }
                    }),
                }));
                false
            }
        }
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

    /// Executes a bash command, records the result, and streams output.
    pub async fn execute_bash(
        &self,
        command: &str,
        on_chunk: Option<crate::core::bash_executor::BashChunkCallback>,
        options: ExecuteBashOptions,
    ) -> Result<crate::core::bash_executor::BashResult, AgentSessionError> {
        use crate::core::bash_executor::{BashExecutorOptions, execute_bash_with_operations};

        let abort = CancellationToken::new();
        lock(&self.inner.bash_abort_controllers).push(abort.clone());
        let (prefix, shell_path) = {
            let settings = lock(&self.inner.settings_manager);
            (
                settings.get_shell_command_prefix().map(str::to_owned),
                settings.get_shell_path(),
            )
        };
        let resolved_command = match prefix {
            Some(prefix) => format!("{prefix}\n{command}"),
            None => command.to_owned(),
        };
        let operations = options.operations.clone().unwrap_or_else(|| {
            Arc::new(crate::core::tools::bash::create_local_bash_operations(
                shell_path.as_deref(),
            ))
        });
        let event_id = options.id.clone();
        let session = self.clone();
        let stream_chunk: crate::core::bash_executor::BashChunkCallback = Arc::new(move |delta: &str| {
            if let Some(on_chunk) = &on_chunk {
                on_chunk(delta);
            }
            session.emit(AgentSessionEvent::BashExecutionUpdate(BashExecutionUpdateEvent {
                event_type: "bash_execution_update",
                id: event_id.clone(),
                delta: delta.to_owned(),
            }));
        });
        let cwd = lock(&self.inner.session_manager).get_cwd().to_owned();
        let result = execute_bash_with_operations(
            &resolved_command,
            &cwd,
            operations,
            &BashExecutorOptions {
                on_chunk: Some(stream_chunk),
                signal: Some(abort.clone()),
            },
        )
        .await;
        lock(&self.inner.bash_abort_controllers).retain(|token| token != &abort);
        let result = result.map_err(|bash_error| error(bash_error.message))?;
        self.record_bash_result(command, &result, options.exclude_from_context);
        Ok(result)
    }

    /// Records a bash execution result in session history.
    pub fn record_bash_result(
        &self,
        command: &str,
        result: &crate::core::bash_executor::BashResult,
        exclude_from_context: Option<bool>,
    ) {
        let bash_message = BashExecutionMessage {
            role: BashExecutionRole::BashExecution,
            command: command.to_owned(),
            output: result.output.clone(),
            exit_code: result.exit_code.map(f64::from),
            cancelled: result.cancelled,
            truncated: result.truncated,
            full_output_path: result.full_output_path.clone(),
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
        self.emit(AgentSessionEvent::BashMessagesCommitted(BashMessagesCommittedEvent {
            event_type: "bash_messages_committed",
            messages: vec![message],
        }));
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
        self.emit(AgentSessionEvent::BashMessagesCommitted(BashMessagesCommittedEvent {
            event_type: "bash_messages_committed",
            messages: committed,
        }));
    }

    // =========================================================================
    // Session management
    // =========================================================================

    /// Sets a display name for the current session.
    pub fn set_session_name(&self, name: &str) {
        let session_file = self.session_file();
        if let (Some(control_db_path), Some(session_file)) = (&self.inner.control_db_path, session_file) {
            let _ = crate::core::session_control_db::set_named_session(control_db_path, &session_file, name);
        }
        if let Err(error) = lock(&self.inner.session_manager).append_session_info(name) {
            eprintln!("Failed to persist session name: {error}");
        }
        let name = lock(&self.inner.session_manager).get_session_name();
        self.emit(AgentSessionEvent::SessionInfoChanged(SessionInfoChangedEvent {
            event_type: "session_info_changed",
            name: name.clone(),
        }));
        let session = self.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                session.emit_session_info_changed(name).await;
            });
        }
    }

    pub fn clear_session_name(&self) {
        let session_file = self.session_file();
        if let (Some(control_db_path), Some(session_file)) = (&self.inner.control_db_path, session_file) {
            let _ = crate::core::session_control_db::remove_named_session(control_db_path, &session_file);
        }
        if let Err(error) = lock(&self.inner.session_manager).append_session_info("") {
            eprintln!("Failed to clear session name: {error}");
        }
        self.emit(AgentSessionEvent::SessionInfoChanged(SessionInfoChangedEvent {
            event_type: "session_info_changed",
            name: None,
        }));
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
    pub async fn materialize_compaction_summary(
        &self,
        entry_id: &str,
    ) -> Result<crate::core::compaction::MaterializeCompactionSummaryResult, AgentSessionError> {
        let (entry, model) = self.require_compaction_summary_materialization_target(entry_id)?;
        if lock(&self.inner.compaction_summary_materialization_abort_controller).is_some() {
            return Err(error("Compaction summary materialization is already running"));
        }
        let abort = CancellationToken::new();
        *lock(&self.inner.compaction_summary_materialization_abort_controller) = Some(abort.clone());
        let auth = self.get_required_request_auth(&model).await?;
        let settings = compaction_settings(&self.inner.settings_manager);
        let stream_fn = self.inner.agent.stream_fn();
        let result = crate::core::compaction::materialize_compaction_summary(
            &entry,
            &model,
            &crate::core::compaction::MaterializeCompactionSummaryOptions {
                reserve_tokens: settings.reserve_tokens,
                api_key: auth.api_key,
                headers: auth.headers,
                env: auth.env,
                signal: Some(abort),
                stream_fn: Some(&stream_fn),
            },
        )
        .await
        .map_err(|compaction_error| error(compaction_error.message()));
        *lock(&self.inner.compaction_summary_materialization_abort_controller) = None;
        result
    }

    fn require_compaction_summary_materialization_target(
        &self,
        entry_id: &str,
    ) -> Result<(crate::core::session_manager::CompactionEntry, Model), AgentSessionError> {
        let entry = lock(&self.inner.session_manager)
            .get_entry(entry_id)
            .cloned()
            .ok_or_else(|| error(format!("Entry {entry_id} not found")))?;
        let Some(compaction) = entry.as_compaction().cloned() else {
            return Err(error(format!("Entry {entry_id} is not a compaction")));
        };
        let Some(checkpoint) = &compaction.provider_native else {
            return Err(error(format!(
                "Compaction entry {entry_id} has no provider-native checkpoint"
            )));
        };
        let model = self.inner.agent.model();
        let matching_provider = checkpoint.provider == model.provider;
        let matching_api = checkpoint.api == model.api;
        let supported_format = checkpoint.format == pi_ai::types::ProviderNativeFormat::OpenAIResponsesInput;
        if !matching_provider || !matching_api || !supported_format {
            return Err(error(format!(
                "Compaction summary materialization requires an active {}/{} model",
                checkpoint.provider, checkpoint.api
            )));
        }
        Ok((compaction, model))
    }

    async fn get_required_request_auth(&self, model: &Model) -> Result<CompactionRequestAuth, AgentSessionError> {
        let result = self
            .inner
            .model_runtime
            .get_auth(&model.provider, Default::default())
            .await
            .map_err(|error| normalize_required_request_auth_error(error.to_string(), &model.provider))?;
        if let Some(result) = result
            && (result.auth.api_key.is_some() || result.auth.headers.is_some())
        {
            let request_model = if let Some(base_url) = result.auth.base_url {
                let mut request_model = model.clone();
                request_model.base_url = base_url;
                request_model
            } else {
                model.clone()
            };
            return Ok(CompactionRequestAuth {
                model: request_model,
                api_key: result.auth.api_key,
                headers: without_deleted_headers(result.auth.headers.as_ref()),
                env: result.env,
            });
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
            (
                manager.get_leaf_id().map(str::to_owned),
                manager.get_entry(target_id).cloned(),
            )
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
        let totals = entries.iter().fold(SessionStatsTotals::default(), |total, entry| {
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
                    if assistant.stop_reason == StopReason::Aborted || assistant.stop_reason == StopReason::Error {
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
            let mut value = serde_json::to_value(&entry).map_err(|json_error| error(json_error.to_string()))?;
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

    /// Exports the current session branch to an HTML file.
    pub async fn export_to_html(&self, output_path: Option<&str>) -> Result<String, AgentSessionError> {
        use crate::core::export_html::{
            ExportOptions, ToolHtmlRendererDeps, create_tool_html_renderer, export_session_to_html,
        };
        use crate::modes::interactive::theme::{current_theme_if_initialized, get_theme_by_name};

        let configured_theme_name = lock(&self.inner.settings_manager).get_theme().map(str::to_owned);
        let theme_name = configured_theme_name.filter(|name| get_theme_by_name(name).is_some());
        let Some(theme) = current_theme_if_initialized() else {
            return Err(error("Theme not initialized. Call initTheme() first."));
        };
        let weak = Arc::downgrade(&self.inner);
        let get_tool_definition = Arc::new(move |name: &str| {
            let inner = weak.upgrade()?;
            AgentSession { inner }
                .get_tool_definition(name)
                .map(extension_tool_definition_for_html)
        });
        let cwd = lock(&self.inner.session_manager).get_cwd().to_owned();
        let tool_renderer = create_tool_html_renderer(ToolHtmlRendererDeps {
            get_tool_definition,
            theme,
            cwd,
            width: None,
        });
        let state = self.state();
        let session_manager = self.session_manager().clone();
        export_session_to_html(
            &session_manager,
            Some(&state),
            Some(ExportOptions {
                output_path: output_path.map(str::to_owned),
                theme_name,
                tool_renderer: Some(tool_renderer),
            }),
        )
        .await
        .map_err(|export_error| error(export_error.to_string()))
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
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
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

fn extension_tool_definition_for_html(definition: ToolDefinition) -> crate::core::extensions::types::ToolDefinition {
    // Session tool definitions do not carry extension TUI renderers yet; the
    // HTML exporter falls back to structured rendering for those tools.
    crate::core::extensions::types::ToolDefinition {
        name: definition.name.clone(),
        label: definition.name,
        description: definition.description,
        prompt_snippet: definition.prompt_snippet,
        prompt_guidelines: Some(definition.prompt_guidelines),
        approval_required: Some(definition.approval_required),
        approval_kind: None,
        parameters: definition.parameters,
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_, _, _, _, _| {
            Box::pin(async {
                Err(crate::core::extensions::types::HandlerError::new(
                    "Tool execution is unavailable during HTML export",
                ))
            })
        }),
        render_call: None,
        render_result: None,
    }
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

// =============================================================================
// Extension integration
// =============================================================================

use crate::core::extensions::runner::{
    BeforeAgentStartCombinedResult, ErrorSubscription, ExtensionErrorListener, ExtensionRunner,
    emit_session_shutdown_event,
};
use crate::core::extensions::types::ToolCallEvent;
use crate::core::extensions::types::{
    AgentEndEvent as ExtensionAgentEndEvent, AgentSettledEvent as ExtensionAgentSettledEvent, AgentStartEvent,
    ExtensionActions, ExtensionCommandContext, ExtensionCommandContextActions, ExtensionContextActions, ExtensionEvent,
    ExtensionMode, ExtensionUiContext, InputEventResult, InputSource as ExtensionInputSource, MessageDelivery,
    MessageEndEvent, MessageStartEvent, MessageUpdateEvent, ModelSelectEvent, ModelSelectSource, ProviderActions,
    SessionShutdownEvent, SessionStartEvent, StreamingBehavior as ExtensionStreamingBehavior, ThinkingLevelSelectEvent,
    ToolExecutionEndEvent, ToolExecutionStartEvent, ToolExecutionUpdateEvent, ToolInfo as ExtensionToolInfo,
    TurnEndEvent, TurnStartEvent, UserMessageContent, UserMessageDelivery,
};
use crate::core::extensions::wrapper::wrap_registered_tool;
use crate::core::footer_data_provider::FooterDataProvider;
use crate::core::slash_commands::SlashCommandInfo;

/// `ExtensionBindings` passed to [`AgentSession::bind_extensions`].
#[derive(Default)]
pub struct ExtensionBindings {
    pub ui_context: Option<Arc<dyn ExtensionUiContext>>,
    pub footer_data: Option<Arc<FooterDataProvider>>,
    pub mode: Option<ExtensionMode>,
    pub control_db_path: Option<String>,
    pub command_context_actions: Option<ExtensionCommandContextActions>,
    pub abort_handler: Option<Arc<dyn Fn() + Send + Sync>>,
    pub shutdown_handler: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_error: Option<ExtensionErrorListener>,
}

impl AgentSession {
    fn runner(&self) -> ExtensionRunner {
        lock(&self.inner.extension_runner).clone()
    }

    /// The extension runner bound to this session (`session.extensionRunner`).
    pub fn extension_runner(&self) -> ExtensionRunner {
        self.runner()
    }

    /// Binds extension host callbacks and emits `session_start`.
    pub async fn bind_extensions(&self, bindings: ExtensionBindings) {
        if let Some(ui_context) = bindings.ui_context {
            *lock(&self.inner.extension_ui_context) = Some(ui_context);
        }
        if let Some(footer_data) = bindings.footer_data {
            *lock(&self.inner.extension_footer_data) = Some(footer_data);
        }
        if let Some(mode) = bindings.mode {
            *lock(&self.inner.extension_mode) = mode;
        }
        if let Some(control_db_path) = bindings.control_db_path {
            *lock(&self.inner.extension_control_db_path) = Some(control_db_path);
        }
        if let Some(actions) = bindings.command_context_actions {
            *lock(&self.inner.extension_command_context_actions) = Some(actions);
        }
        if let Some(handler) = bindings.abort_handler {
            *lock(&self.inner.extension_abort_handler) = Some(handler);
        }
        if let Some(handler) = bindings.shutdown_handler {
            *lock(&self.inner.extension_shutdown_handler) = Some(handler);
        }
        if let Some(listener) = bindings.on_error {
            *lock(&self.inner.extension_error_listener) = Some(listener);
        }
        let runner = self.runner();
        self.apply_extension_bindings(&runner);
        let mut event = ExtensionEvent::SessionStart(self.inner.session_start_event.clone());
        runner.emit(&mut event).await;
        self.extend_resources_from_extensions().await;
    }

    /// Reloads settings, resources, and extensions (`AgentSession.reload`).
    pub async fn reload(&self, before_session_start: Option<Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>) {
        let old_runner = self.runner();
        let previous_flag_values = old_runner.get_flag_values();
        emit_session_shutdown_event(
            &old_runner,
            SessionShutdownEvent {
                reason: crate::core::extensions::types::SessionShutdownReason::Reload,
                target_session_file: None,
            },
        )
        .await;
        old_runner.invalidate(None);
        lock(&self.inner.settings_manager).reload();
        let (steering_mode, follow_up_mode) = {
            let settings = lock(&self.inner.settings_manager);
            (settings.get_steering_mode(), settings.get_follow_up_mode())
        };
        self.inner.agent.set_steering_mode(steering_mode);
        self.inner.agent.set_follow_up_mode(follow_up_mode);
        pi_ai::compat::reset_api_providers();
        self.inner.resource_loader.reload().await;
        self.build_runtime(
            Some(self.get_active_tool_names()),
            Some(flag_values_map(&previous_flag_values)),
            true,
        );

        let has_bindings = lock(&self.inner.extension_ui_context).is_some()
            || lock(&self.inner.extension_command_context_actions).is_some()
            || lock(&self.inner.extension_shutdown_handler).is_some()
            || lock(&self.inner.extension_error_listener).is_some();
        if !has_bindings {
            return;
        }
        if let Some(before_session_start) = before_session_start {
            before_session_start().await;
        }
        let mut event = ExtensionEvent::SessionStart(crate::core::extensions::types::SessionStartEvent {
            reason: crate::core::extensions::types::SessionStartReason::Reload,
            previous_session_file: None,
        });
        self.runner().emit(&mut event).await;
        self.extend_resources_with_reason(crate::core::extensions::types::ResourcesDiscoverReason::Reload)
            .await;
    }

    fn apply_extension_bindings(&self, runner: &ExtensionRunner) {
        let mut runner = runner.clone();
        runner.set_ui_context(
            lock(&self.inner.extension_ui_context).clone(),
            *lock(&self.inner.extension_mode),
        );
        if let Some(actions) = lock(&self.inner.extension_command_context_actions).clone() {
            runner.bind_command_context(Some(actions));
        }
        let mut unsubscribe = lock(&self.inner.extension_error_unsubscriber).take();
        if let Some(subscription) = unsubscribe.take() {
            subscription.unsubscribe();
        }
        if let Some(listener) = lock(&self.inner.extension_error_listener).clone() {
            *lock(&self.inner.extension_error_unsubscriber) = Some(runner.on_error(listener));
        }
        *lock(&self.inner.extension_runner) = runner;
    }

    async fn extend_resources_from_extensions(&self) {
        let reason =
            if self.inner.session_start_event.reason == crate::core::extensions::types::SessionStartReason::Reload {
                crate::core::extensions::types::ResourcesDiscoverReason::Reload
            } else {
                crate::core::extensions::types::ResourcesDiscoverReason::Startup
            };
        self.extend_resources_with_reason(reason).await;
    }

    async fn extend_resources_with_reason(&self, reason: crate::core::extensions::types::ResourcesDiscoverReason) {
        let runner = self.runner();
        if !runner.has_handlers("resources_discover") {
            return;
        }
        let paths = runner.emit_resources_discover(&self.inner.cwd, reason).await;
        if paths.skill_paths.is_empty() && paths.prompt_paths.is_empty() && paths.theme_paths.is_empty() {
            return;
        }
        let convert = |entries: Vec<crate::core::extensions::runner::DiscoveredPath>| {
            entries
                .into_iter()
                .map(|entry| crate::core::resource_loader::ExtendedResourcePath {
                    path: entry.path,
                    metadata: crate::core::package_manager::PathMetadata {
                        source: extension_source_label(&entry.extension_path),
                        scope: crate::core::package_manager::SourceScope::Temporary,
                        origin: crate::core::package_manager::MetadataOrigin::TopLevel,
                        base_dir: (!entry.extension_path.starts_with('<'))
                            .then(|| {
                                std::path::Path::new(&entry.extension_path)
                                    .parent()
                                    .map(|path| path.display().to_string())
                            })
                            .flatten(),
                    },
                })
                .collect::<Vec<_>>()
        };
        self.inner
            .resource_loader
            .extend_resources(crate::core::resource_loader::ResourceExtensionPaths {
                skill_paths: convert(paths.skill_paths),
                prompt_paths: convert(paths.prompt_paths),
                theme_paths: convert(paths.theme_paths),
            });
        let prompt = self.rebuild_system_prompt(&self.get_active_tool_names());
        *lock(&self.inner.base_system_prompt) = prompt.clone();
        self.inner.agent.set_system_prompt(prompt);
    }

    fn create_internal_commands(
        &self,
    ) -> indexmap::IndexMap<String, crate::core::extensions::types::InternalCommandHandler> {
        let weak = Arc::downgrade(&self.inner);
        let handler: crate::core::extensions::types::InternalCommandHandler =
            Arc::new(move |args: String, ctx: ExtensionCommandContext| {
                let model_reference = args.trim().to_owned();
                let weak = weak.clone();
                Box::pin(async move {
                    if model_reference.is_empty() {
                        return Ok(());
                    }
                    let scoped = ctx.context.get_scoped_models()?;
                    let models = if scoped.is_empty() {
                        ctx.context.model_registry()?.get_available()
                    } else {
                        scoped.into_iter().map(|scoped| scoped.model).collect()
                    };
                    let Some(model) = find_exact_model_reference_match(&model_reference, &models) else {
                        return Err(crate::core::extensions::types::HandlerError::new(format!(
                            "Model not found or not authenticated: {model_reference}"
                        )));
                    };
                    let provider = model.provider.clone();
                    let model_id = model.id.clone();
                    let Some(inner) = weak.upgrade() else {
                        return Err(crate::core::extensions::types::HandlerError::new("Session is disposed"));
                    };
                    AgentSession { inner }
                        .set_model(model)
                        .await
                        .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message()))?;
                    let _ = (provider, model_id);
                    Ok(())
                })
            });
        let mut commands = indexmap::IndexMap::new();
        commands.insert("model".to_owned(), handler);
        commands
    }

    fn bind_extension_core(&self, runner: &mut ExtensionRunner) {
        let weak = Arc::downgrade(&self.inner);
        let actions = extension_actions(weak.clone());
        let context_actions = extension_context_actions(weak.clone());
        let provider_actions = extension_provider_actions(weak);
        runner.bind_core(actions, context_actions, Some(provider_actions));
    }

    async fn emit_extension_event(&self, event: &mut AgentEvent) -> Option<AgentMessage> {
        let runner = self.runner();
        match event {
            AgentEvent::AgentStart => {
                self.inner.turn_index.store(0, Ordering::SeqCst);
                let mut extension_event = ExtensionEvent::AgentStart(AgentStartEvent {});
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::AgentEnd { messages } => {
                let mut extension_event = ExtensionEvent::AgentEnd(ExtensionAgentEndEvent {
                    messages: messages.clone(),
                    session_continuation: None,
                });
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::TurnStart => {
                let mut extension_event = ExtensionEvent::TurnStart(TurnStartEvent {
                    turn_index: self.inner.turn_index.load(Ordering::SeqCst) as f64,
                    timestamp: now_millis() as f64,
                });
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::TurnEnd { message, tool_results } => {
                let mut extension_event = ExtensionEvent::TurnEnd(TurnEndEvent {
                    turn_index: self.inner.turn_index.load(Ordering::SeqCst) as f64,
                    message: message.clone(),
                    tool_results: tool_results.clone(),
                });
                runner.emit(&mut extension_event).await;
                self.inner.turn_index.fetch_add(1, Ordering::SeqCst);
                None
            }
            AgentEvent::MessageStart { message } => {
                let mut extension_event = ExtensionEvent::MessageStart(MessageStartEvent {
                    message: message.clone(),
                });
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::MessageUpdate {
                message,
                assistant_message_event,
            } => {
                let mut extension_event = ExtensionEvent::MessageUpdate(Box::new(MessageUpdateEvent {
                    message: message.clone(),
                    assistant_message_event: (**assistant_message_event).clone(),
                }));
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::MessageEnd { message } => {
                let replacement = runner
                    .emit_message_end(MessageEndEvent {
                        message: message.clone(),
                    })
                    .await;
                replacement.map(normalize_message_content)
            }
            AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
                ..
            } => {
                let mut extension_event = ExtensionEvent::ToolExecutionStart(ToolExecutionStartEvent {
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.clone(),
                    args: args.clone(),
                });
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::ToolExecutionUpdate {
                tool_call_id,
                tool_name,
                args,
                partial_result,
            } => {
                let mut extension_event = ExtensionEvent::ToolExecutionUpdate(ToolExecutionUpdateEvent {
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.clone(),
                    args: args.clone(),
                    partial_result: serde_json::to_value(partial_result).unwrap_or(Value::Null),
                });
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
                ..
            } => {
                let mut extension_event = ExtensionEvent::ToolExecutionEnd(ToolExecutionEndEvent {
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.clone(),
                    result: serde_json::to_value(result).unwrap_or(Value::Null),
                    is_error: *is_error,
                });
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::ModelRequestStart | AgentEvent::ModelRequestEnd => None,
        }
    }

    async fn emit_model_select(&self, next_model: &Model, previous_model: Option<&Model>, source: ModelSelectSource) {
        if pi_ai::models::models_are_equal(previous_model, Some(next_model)) {
            return;
        }
        let mut event = ExtensionEvent::ModelSelect(Box::new(ModelSelectEvent {
            model: next_model.clone(),
            previous_model: previous_model.cloned(),
            source,
        }));
        self.runner().emit(&mut event).await;
    }

    async fn emit_thinking_level_select(&self, level: ThinkingLevel, previous: ThinkingLevel) {
        let mut event = ExtensionEvent::ThinkingLevelSelect(ThinkingLevelSelectEvent {
            level,
            previous_level: previous,
        });
        self.runner().emit(&mut event).await;
    }

    async fn emit_session_info_changed(&self, name: Option<String>) {
        let mut event =
            ExtensionEvent::SessionInfoChanged(crate::core::extensions::types::SessionInfoChangedEvent { name });
        self.runner().emit(&mut event).await;
    }

    async fn emit_agent_settled_extension(&self) {
        let mut event = ExtensionEvent::AgentSettled(ExtensionAgentSettledEvent {});
        self.runner().emit(&mut event).await;
    }

    async fn try_execute_extension_command(&self, text: &str) -> bool {
        let (command_name, args) = match text.split_once(' ') {
            Some((name, args)) => (name.trim_start_matches('/'), args),
            None => (text.trim_start_matches('/'), ""),
        };
        let runner = self.runner();
        let Some(command) = runner.get_prompt_command(command_name) else {
            return false;
        };
        let context = self.create_command_context();
        if let Err(error) = (command.handler)(args.to_owned(), context).await {
            runner.emit_error(crate::core::extensions::types::ExtensionError {
                extension_path: format!("command:{command_name}"),
                event: "command".to_owned(),
                error: error.to_string(),
                stack: None,
            });
        }
        true
    }

    fn record_handled_slash_command_history(&self, text: &str) {
        let Some(control_db_path) = &self.inner.control_db_path else {
            return;
        };
        if let Err(error) = crate::core::session_control_db::record_prompt_history_entry(control_db_path, text) {
            eprintln!("Failed to persist prompt history for handled slash command: {error}");
        }
    }

    async fn emit_extension_input(
        &self,
        text: &str,
        images: &[ImageContent],
        source: Option<InputSource>,
        streaming_behavior: Option<StreamingBehavior>,
    ) -> InputEventResult {
        let runner = self.runner();
        if !runner.has_handlers("input") {
            return InputEventResult::Continue;
        }
        runner
            .emit_input(
                text,
                (!images.is_empty()).then(|| images.to_vec()),
                match source.unwrap_or(InputSource::Interactive) {
                    InputSource::Interactive => ExtensionInputSource::Interactive,
                    InputSource::Rpc => ExtensionInputSource::Rpc,
                    InputSource::Extension => ExtensionInputSource::Extension,
                },
                streaming_behavior.map(|behavior| match behavior {
                    StreamingBehavior::Steer => ExtensionStreamingBehavior::Steer,
                    StreamingBehavior::FollowUp => ExtensionStreamingBehavior::FollowUp,
                }),
            )
            .await
    }

    async fn emit_before_agent_start(
        &self,
        text: &str,
        images: &[ImageContent],
        options: &BuildSystemPromptOptions,
    ) -> Option<BeforeAgentStartCombinedResult> {
        let runner = self.runner();
        let base_prompt = lock(&self.inner.base_system_prompt).clone();
        runner
            .emit_before_agent_start(
                text,
                (!images.is_empty()).then(|| images.to_vec()),
                &base_prompt,
                options.clone(),
            )
            .await
    }
}

fn tool_call_result_to_before(result: crate::core::extensions::types::ToolCallEventResult) -> BeforeToolCallResult {
    BeforeToolCallResult {
        block: result.block,
        reason: result.reason,
        terminate: result.terminate,
    }
}

fn extension_source_label(extension_path: &str) -> String {
    if extension_path.starts_with('<') {
        return format!("extension:{}", extension_path.replace(['<', '>'], ""));
    }
    let base = std::path::Path::new(extension_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = base
        .strip_suffix(".ts")
        .or_else(|| base.strip_suffix(".js"))
        .unwrap_or(&base);
    format!("extension:{name}")
}

fn extension_tool_info(entry: &ToolDefinitionEntry) -> ExtensionToolInfo {
    ExtensionToolInfo {
        name: entry.definition.name.clone(),
        description: entry.definition.description.clone(),
        parameters: entry.definition.parameters.clone(),
        prompt_guidelines: (!entry.definition.prompt_guidelines.is_empty())
            .then(|| entry.definition.prompt_guidelines.clone()),
        source_info: entry.source_info.clone(),
    }
}

fn extension_actions(weak: Weak<Inner>) -> ExtensionActions {
    let send_weak = weak.clone();
    let send_message: crate::core::extensions::types::SendMessageHandler = Arc::new(
        move |message: crate::core::extensions::types::CustomMessagePayload,
              options: Option<crate::core::extensions::types::SendMessageOptions>| {
            let Some(inner) = send_weak.upgrade() else {
                return Ok(());
            };
            let session = AgentSession { inner };
            let content = match message.content {
                crate::core::extensions::types::CustomMessageContent::Text(text) => CustomMessageContent::Text(text),
                crate::core::extensions::types::CustomMessageContent::Blocks(blocks) => {
                    CustomMessageContent::Blocks(blocks)
                }
            };
            let trigger_turn = options
                .as_ref()
                .and_then(|options| options.trigger_turn)
                .unwrap_or(false);
            let deliver_as = options.as_ref().and_then(|options| options.deliver_as);
            if deliver_as == Some(MessageDelivery::NextTurn) {
                session.send_custom_message_next_turn(&message.custom_type, content, message.display, message.details);
                return Ok(());
            }
            let deliver = match deliver_as {
                Some(MessageDelivery::Steer) => Some(StreamingBehavior::Steer),
                Some(MessageDelivery::FollowUp) => Some(StreamingBehavior::FollowUp),
                _ => None,
            };
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = session
                        .send_custom_message(
                            &message.custom_type,
                            content,
                            message.display,
                            message.details,
                            trigger_turn,
                            deliver,
                        )
                        .await;
                });
            }
            Ok(())
        },
    );
    let send_user_weak = weak.clone();
    let send_user_message: crate::core::extensions::types::SendUserMessageHandler = Arc::new(
        move |content: UserMessageContent, options: Option<crate::core::extensions::types::SendUserMessageOptions>| {
            let Some(inner) = send_user_weak.upgrade() else {
                return Ok(());
            };
            let session = AgentSession { inner };
            let content = match content {
                UserMessageContent::Text(text) => UserContent::Text(text),
                UserMessageContent::Blocks(blocks) => UserContent::Blocks(blocks),
            };
            let deliver_as = options
                .as_ref()
                .and_then(|options| options.deliver_as)
                .map(|delivery| match delivery {
                    UserMessageDelivery::Steer => StreamingBehavior::Steer,
                    UserMessageDelivery::FollowUp => StreamingBehavior::FollowUp,
                });
            let expand = options
                .as_ref()
                .and_then(|options| options.expand_prompt_templates)
                .unwrap_or(false);
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = session.send_user_message(content, deliver_as, expand).await;
                });
            }
            Ok(())
        },
    );
    let append_weak = weak.clone();
    let append_entry: crate::core::extensions::types::AppendEntryHandler =
        Arc::new(move |custom_type: &str, data: Option<Value>| {
            let Some(inner) = append_weak.upgrade() else {
                return Ok(());
            };
            let session = AgentSession { inner };
            let entry_id = session
                .session_manager()
                .append_custom_entry(custom_type, data)
                .map_err(|error| crate::core::extensions::types::HandlerError::new(error.to_string()))?;
            let entry = session.session_manager().get_entry(&entry_id).cloned();
            if let Some(entry) = entry {
                session.emit(AgentSessionEvent::EntryAppended(EntryAppendedEvent {
                    event_type: "entry_appended",
                    entry,
                }));
            }
            Ok(())
        });
    let resume_weak = weak.clone();
    let request_resume_continuation: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Some(inner) = resume_weak.upgrade() {
            AgentSession { inner }.request_resume_continuation();
        }
    });
    let name_weak = weak.clone();
    let set_session_name: crate::core::extensions::types::SetSessionNameHandler = Arc::new(move |name: &str| {
        let Some(inner) = name_weak.upgrade() else {
            return Ok(());
        };
        AgentSession { inner }.set_session_name(name);
        Ok(())
    });
    let get_name_weak = weak.clone();
    let get_session_name: crate::core::extensions::types::GetSessionNameHandler =
        Arc::new(move || match get_name_weak.upgrade() {
            Some(inner) => Ok(AgentSession { inner }.session_name()),
            None => Ok(None),
        });
    let label_weak = weak.clone();
    let set_label: crate::core::extensions::types::SetLabelHandler =
        Arc::new(move |entry_id: &str, label: Option<&str>| {
            let Some(inner) = label_weak.upgrade() else {
                return Ok(());
            };
            AgentSession { inner }
                .session_manager()
                .append_label_change(entry_id, label)
                .map(|_| ())
                .map_err(|error| crate::core::extensions::types::HandlerError::new(error.to_string()))
        });
    let active_weak = weak.clone();
    let get_active_tools: crate::core::extensions::types::GetActiveToolsHandler =
        Arc::new(move || match active_weak.upgrade() {
            Some(inner) => Ok(AgentSession { inner }.get_active_tool_names()),
            None => Ok(Vec::new()),
        });
    let all_weak = weak.clone();
    let get_all_tools: crate::core::extensions::types::GetAllToolsHandler =
        Arc::new(move || match all_weak.upgrade() {
            Some(inner) => Ok(lock(&inner.tool_definitions)
                .iter()
                .map(|(_, entry)| extension_tool_info(entry))
                .collect()),
            None => Ok(Vec::new()),
        });
    let set_active_weak = weak.clone();
    let set_active_tools: crate::core::extensions::types::SetActiveToolsHandler =
        Arc::new(move |tool_names: &[String]| {
            if let Some(inner) = set_active_weak.upgrade() {
                AgentSession { inner }.set_active_tools_by_name(tool_names);
            }
            Ok(())
        });
    let call_tool_weak = weak.clone();
    let call_tool: crate::core::extensions::types::CallToolHandler = Arc::new(
        move |name: String, params: Value, signal: Option<CancellationToken>, tool_call_id: Option<String>| {
            let Some(inner) = call_tool_weak.upgrade() else {
                return Box::pin(async {
                    Err(crate::core::extensions::types::HandlerError::new("Session is disposed"))
                });
            };
            let session = AgentSession { inner };
            Box::pin(async move { session.call_active_tool(&name, params, signal, tool_call_id).await })
        },
    );
    let command_weak = weak.clone();
    let call_command: crate::core::extensions::types::CallCommandHandler =
        Arc::new(move |name: String, args: Option<String>| {
            let Some(inner) = command_weak.upgrade() else {
                return Box::pin(async {
                    Err(crate::core::extensions::types::HandlerError::new("Session is disposed"))
                });
            };
            let session = AgentSession { inner };
            Box::pin(async move {
                let runner = session.runner();
                let Some(command) = runner.get_prompt_command(&name) else {
                    return Err(crate::core::extensions::types::HandlerError::new(format!(
                        "Command not found: {name}"
                    )));
                };
                let context = session.create_command_context();
                (command.handler)(args.unwrap_or_default(), context)
                    .await
                    .map(|()| Value::Null)
            })
        });
    let refresh_weak = weak.clone();
    let refresh_tools: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Some(inner) = refresh_weak.upgrade() {
            AgentSession { inner }.refresh_tool_registry(None, false);
        }
    });
    let commands_weak = weak.clone();
    let get_commands: crate::core::extensions::types::GetCommandsHandler =
        Arc::new(move || match commands_weak.upgrade() {
            Some(inner) => Ok(AgentSession { inner }.slash_commands()),
            None => Ok(Vec::new()),
        });
    let set_model_weak = weak.clone();
    let set_model: crate::core::extensions::types::SetModelHandler = Arc::new(move |model: Model| {
        let Some(inner) = set_model_weak.upgrade() else {
            return Box::pin(async { Ok(false) });
        };
        let session = AgentSession { inner };
        Box::pin(async move {
            if !session.model_runtime().has_configured_auth(&model.provider) {
                return Ok(false);
            }
            session
                .set_model(model)
                .await
                .map(|()| true)
                .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message()))
        })
    });
    let thinking_weak = weak.clone();
    let get_thinking_level: crate::core::extensions::types::GetThinkingLevelHandler =
        Arc::new(move || match thinking_weak.upgrade() {
            Some(inner) => Ok(AgentSession { inner }.thinking_level()),
            None => Ok(ThinkingLevel::Off),
        });
    let set_thinking_weak = weak.clone();
    let set_thinking_level: crate::core::extensions::types::SetThinkingLevelHandler =
        Arc::new(move |level: ThinkingLevel| {
            if let Some(inner) = set_thinking_weak.upgrade() {
                AgentSession { inner }.set_thinking_level(level);
            }
            Ok(())
        });

    ExtensionActions {
        send_message,
        send_user_message,
        append_entry,
        request_resume_continuation,
        set_session_name,
        get_session_name,
        set_label,
        get_active_tools,
        get_all_tools,
        set_active_tools,
        call_tool,
        call_command,
        refresh_tools,
        get_commands,
        set_model,
        get_thinking_level,
        set_thinking_level,
    }
}

fn extension_context_actions(weak: Weak<Inner>) -> ExtensionContextActions {
    let model_weak = weak.clone();
    let get_model: Arc<dyn Fn() -> Option<Model> + Send + Sync> =
        Arc::new(move || model_weak.upgrade().map(|inner| AgentSession { inner }.model()));
    let thinking_weak = weak.clone();
    let get_thinking_level: Option<Arc<dyn Fn() -> ThinkingLevel + Send + Sync>> =
        Some(Arc::new(move || match thinking_weak.upgrade() {
            Some(inner) => AgentSession { inner }.thinking_level(),
            None => ThinkingLevel::Off,
        }));
    let scoped_weak = weak.clone();
    let get_scoped_models: Option<Arc<dyn Fn() -> Vec<ScopedModel> + Send + Sync>> =
        Some(Arc::new(move || match scoped_weak.upgrade() {
            Some(inner) => AgentSession { inner }.scoped_models(),
            None => Vec::new(),
        }));
    let footer_weak = weak.clone();
    let get_footer_data: Option<Arc<dyn Fn() -> Option<Arc<FooterDataProvider>> + Send + Sync>> =
        Some(Arc::new(move || match footer_weak.upgrade() {
            Some(inner) => lock(&inner.extension_footer_data).clone(),
            None => None,
        }));
    let idle_weak = weak.clone();
    let is_idle: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
        idle_weak
            .upgrade()
            .is_none_or(|inner| !AgentSession { inner }.is_streaming())
    });
    let retry_weak = weak.clone();
    let has_active_retry: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
        retry_weak
            .upgrade()
            .is_some_and(|inner| AgentSession { inner }.has_active_retry())
    });
    let trust_weak = weak.clone();
    let is_project_trusted: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
        trust_weak
            .upgrade()
            .is_some_and(|inner| lock(&inner.settings_manager).is_project_trusted())
    });
    let signal_weak = weak.clone();
    let get_signal: Arc<dyn Fn() -> Option<CancellationToken> + Send + Sync> =
        Arc::new(move || signal_weak.upgrade().and_then(|inner| inner.agent.signal()));
    let abort_weak = weak.clone();
    let abort: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let Some(inner) = abort_weak.upgrade() else {
            return;
        };
        let session = AgentSession { inner };
        if let Some(handler) = lock(&session.inner.extension_abort_handler).clone() {
            handler();
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = session.abort().await;
            });
        }
    });
    let pending_weak = weak.clone();
    let has_pending_messages: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
        pending_weak
            .upgrade()
            .is_some_and(|inner| AgentSession { inner }.has_pending_messages())
    });
    let shutdown_weak = weak.clone();
    let shutdown: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Some(inner) = shutdown_weak.upgrade()
            && let Some(handler) = lock(&inner.extension_shutdown_handler).clone()
        {
            handler();
        }
    });
    let restart_weak = weak.clone();
    let restart: Arc<
        dyn Fn(Option<crate::core::extensions::types::RestartOptions>) -> BoxFuture<'static, ()> + Send + Sync,
    > = Arc::new(move |options| {
        let Some(inner) = restart_weak.upgrade() else {
            return Box::pin(async {}) as BoxFuture<'static, ()>;
        };
        let session = AgentSession { inner };
        let actions = lock(&session.inner.extension_command_context_actions).clone();
        Box::pin(async move {
            if let Some(actions) = actions {
                (actions.restart)(options).await;
            }
        })
    });
    let control_weak = weak.clone();
    let get_control_db_path: Option<Arc<dyn Fn() -> Option<String> + Send + Sync>> = Some(Arc::new(move || {
        control_weak.upgrade().and_then(|inner| {
            lock(&inner.extension_control_db_path)
                .clone()
                .or_else(|| inner.control_db_path.clone())
        })
    }));
    let usage_weak = weak.clone();
    let get_context_usage: Arc<dyn Fn() -> Option<crate::core::extensions::types::ContextUsage> + Send + Sync> =
        Arc::new(move || {
            usage_weak.upgrade().and_then(|inner| {
                AgentSession { inner }
                    .get_context_usage()
                    .map(|usage| crate::core::extensions::types::ContextUsage {
                        tokens: Some(usage.tokens),
                        context_window: usage.context_window,
                        percent: Some(usage.percent),
                    })
            })
        });
    let compact_weak = weak.clone();
    let compact: Arc<dyn Fn(Option<crate::core::extensions::types::CompactOptions>) + Send + Sync> =
        Arc::new(move |options| {
            let Some(inner) = compact_weak.upgrade() else {
                return;
            };
            let session = AgentSession { inner };
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let custom = options.as_ref().and_then(|options| options.custom_instructions.clone());
                    match session.compact(custom.as_deref()).await {
                        Ok(_result) => {
                            // `on_complete` expects the coding-agent compaction result
                            // type (ca-compaction-export); recorded gap.
                        }
                        Err(error) => {
                            if let Some(on_error) = options.as_ref().and_then(|options| options.on_error.clone()) {
                                on_error(error.message());
                            }
                        }
                    }
                });
            }
        });
    let prompt_weak = weak.clone();
    let get_system_prompt: Arc<dyn Fn() -> String + Send + Sync> = Arc::new(move || {
        prompt_weak
            .upgrade()
            .map(|inner| AgentSession { inner }.system_prompt())
            .unwrap_or_default()
    });
    let options_weak = weak.clone();
    let get_system_prompt_options: Option<Arc<dyn Fn() -> BuildSystemPromptOptions + Send + Sync>> =
        Some(Arc::new(move || {
            options_weak
                .upgrade()
                .and_then(|inner| lock(&inner.base_system_prompt_options).clone())
                .unwrap_or_default()
        }));

    ExtensionContextActions {
        get_model,
        get_thinking_level,
        get_scoped_models,
        get_footer_data,
        is_idle,
        has_active_retry,
        is_project_trusted,
        get_signal,
        abort,
        has_pending_messages,
        shutdown,
        restart,
        get_control_db_path,
        get_context_usage,
        get_multi_agent_agent_id: None,
        get_multi_agent_parent_session_id: None,
        get_multi_agent_requires_agent_id: None,
        get_multi_agent_store: None,
        get_tool_detach_registry: None,
        compact,
        get_system_prompt,
        get_system_prompt_options,
    }
}

fn extension_provider_actions(weak: Weak<Inner>) -> ProviderActions {
    let register_weak = weak.clone();
    let register_provider: Option<RegisterProviderAction> = Some(Arc::new(
        move |name: &str, config: &crate::core::provider_composer::ProviderConfigInput| {
            let Some(inner) = register_weak.upgrade() else {
                return Err("Session is disposed".to_owned());
            };
            inner
                .model_runtime
                .register_provider(name, config.clone())
                .map_err(|error| error.to_string())?;
            refresh_current_model_from_registry(&inner);
            Ok(())
        },
    ));
    let native_weak = weak.clone();
    let register_native_provider: Option<RegisterNativeProviderAction> = Some(Arc::new(move |provider| {
        if let Some(inner) = native_weak.upgrade() {
            inner.model_runtime.register_native_provider(provider);
            refresh_current_model_from_registry(&inner);
        }
    }));
    let unregister_weak = weak;
    let unregister_provider: Option<UnregisterProviderAction> = Some(Arc::new(move |name: &str| {
        if let Some(inner) = unregister_weak.upgrade() {
            inner.model_runtime.unregister_provider(name);
            refresh_current_model_from_registry(&inner);
        }
    }));
    ProviderActions {
        register_provider,
        register_native_provider,
        unregister_provider,
    }
}

fn refresh_current_model_from_registry(inner: &Arc<Inner>) {
    let current = inner.agent.model();
    if let Some(refreshed) = inner.model_runtime.get_model(&current.provider, &current.id) {
        inner.agent.set_model(refreshed);
    }
}

impl AgentSession {
    fn create_command_context(&self) -> ExtensionCommandContext {
        self.runner().create_command_context()
    }

    fn slash_commands(&self) -> Vec<SlashCommandInfo> {
        let runner = self.runner();
        let mut commands: Vec<SlashCommandInfo> = runner
            .get_registered_commands()
            .into_iter()
            .map(|command| SlashCommandInfo {
                name: command.invocation_name,
                description: command.description,
                source: crate::core::slash_commands::SlashCommandSource::Extension,
                source_info: command.source_info,
            })
            .collect();
        for template in self.prompt_templates() {
            commands.push(SlashCommandInfo {
                name: template.name,
                description: Some(template.description),
                source: crate::core::slash_commands::SlashCommandSource::Prompt,
                source_info: template.source_info,
            });
        }
        for skill in self.inner.resource_loader.get_skills().skills {
            commands.push(SlashCommandInfo {
                name: format!("skill:{}", skill.name),
                description: Some(skill.description),
                source: crate::core::slash_commands::SlashCommandSource::Skill,
                source_info: skill.source_info,
            });
        }
        commands
    }

    async fn call_active_tool(
        &self,
        tool_name: &str,
        params: Value,
        signal: Option<CancellationToken>,
        active_tool_call_id: Option<String>,
    ) -> Result<pi_agent_core::agent::types::AgentToolResult, crate::core::extensions::types::HandlerError> {
        if !self.get_active_tool_names().iter().any(|name| name == tool_name) {
            return Err(crate::core::extensions::types::HandlerError::new(format!(
                "Tool is not active: {tool_name}"
            )));
        }
        if tool_name == "pyrun_eval" {
            return Err(crate::core::extensions::types::HandlerError::new(
                "pyrun_eval cannot call itself through pi.tools.call",
            ));
        }
        let tool = lock(&self.inner.tool_registry).get(tool_name).cloned().ok_or_else(|| {
            crate::core::extensions::types::HandlerError::new(format!("Tool is not registered: {tool_name}"))
        })?;
        let tool_call_id = active_tool_call_id.unwrap_or_else(|| format!("pyrun:{tool_name}:{}", now_millis()));
        let arguments = params.as_object().cloned().unwrap_or_else(|| {
            let mut object = Map::new();
            object.insert("value".to_owned(), params.clone());
            object
        });
        let tool_call = AgentToolCall {
            r#type: pi_ai::types::ToolCallTag::Tag,
            id: tool_call_id.clone(),
            name: tool_name.to_owned(),
            arguments,
            execution: None,
            thought_signature: None,
            namespace: None,
        };
        let tool_spec = pi_ai::types::Tool::new(tool_name, tool.description(), tool.parameters().clone());
        let validated = pi_ai::utils::validation::validate_tool_arguments(&tool_spec, &tool_call)
            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.to_string()))?;
        let before = self
            .before_tool_call(BeforeToolCallContext {
                assistant_message: synthetic_tool_call_assistant_message(&self.model(), &tool_call),
                tool_call: tool_call.clone(),
                args: Arc::new(Mutex::new(validated.clone())),
                context: pi_agent_core::agent::types::AgentContext {
                    system_prompt: self.system_prompt(),
                    messages: self.messages(),
                    tools: Some(self.inner.agent.tools()),
                },
            })
            .await;
        if before.as_ref().and_then(|result| result.block) == Some(true) {
            let reason = before
                .and_then(|result| result.reason)
                .unwrap_or_else(|| "Tool execution was blocked".to_owned());
            return Ok(pi_agent_core::agent::types::AgentToolResult::text(reason));
        }
        let result = tool
            .execute(tool_call_id, validated.clone(), signal, Arc::new(|_| {}), None)
            .await
            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message))?;
        Ok(result)
    }
}

fn synthetic_tool_call_assistant_message(model: &Model, tool_call: &AgentToolCall) -> AssistantMessage {
    let mut message = AssistantMessage::empty(model.api.clone(), model.provider.clone(), model.id.clone(), 0);
    message.content = vec![AssistantContent::ToolCall(tool_call.clone())];
    message.stop_reason = StopReason::ToolUse;
    message.timestamp = now_millis();
    message
}
