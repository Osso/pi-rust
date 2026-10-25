//! Port of `packages/coding-agent/src/core/agent-session.ts`.
//!
//! Gaps against the source are recorded in
//! `docs/porting/traceability/agent-session.md`.

mod restriction;
use restriction::{CodexRestrictionContextCache, CodexRestrictionValidation};

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
use pi_ai::request::{BoxFuture, CallbackError, now_millis};
use pi_ai::types::{
    AssistantContent, AssistantMessage, ImageContent, InputSource, Model, ProviderHeaders, ProviderRetryEvent,
    StopReason, TextContent, TextOrImageContent, Usage, UserContent, UserMessage, UserRole,
};
use pi_js_compat::abort::{AbortController, AbortSignal};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::defaults::DEFAULT_THINKING_LEVEL;
use crate::core::extensions::types::NoOpUiContext;
use crate::core::model_registry::{ModelRegistry, ResolvedRequestAuth};
use crate::core::model_resolver::{ScopedModel, find_exact_model_reference_match, resolve_model_scope};
use crate::core::multi_agent_store::{AgentLifecycle, SteeringCheckpoint};
use crate::core::permissions::policy::ApprovalPolicy;
use crate::core::permissions::presets::{ApprovalPresetName, SandboxProfileName, SandboxProfileScope};
use crate::core::prompt_templates::expand_prompt_template;
use crate::core::resource_loader::{LoadedPrompts, ResourceLoader};
use crate::core::runtime_message_markers::{
    RuntimeMessageMarker, is_duplicate_turn_guard_message as is_duplicate_turn_guard_message_marker,
};
use crate::core::session_control_db::RuntimeMailboxMessage;
use crate::core::session_manager::{SessionEntry, SessionManager, SessionManagerError, get_latest_compaction_entry};
use crate::core::settings_manager::{SettingsManager, SettingsManagerGuard, SettingsScope, lock_settings_manager};
use crate::core::slash_commands::builtin_slash_commands;
use crate::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
use crate::core::system_prompt::{BuildSystemPromptOptions, build_system_prompt};
use crate::core::tool_detach_registry::ToolDetachRegistry;
use crate::core::tools::bash::{BashBackgroundJobs, BashToolOptions};
use crate::core::tools::read::ReadToolOptions;
use crate::core::tools::{ToolsOptions, create_tool_definitions_for_available_executables};
use crate::modes::interactive::theme::ThemeColor;
use crate::utils::frontmatter::strip_frontmatter;

const BUILT_IN_COMPACTION_DISABLED_MESSAGE: &str =
    "Built-in compaction is disabled; enable compaction or configure a compaction extension";
const DUPLICATE_TURN_GUARD_PROMPT: &str =
    "You repeated the same response. Stop looping. Call `end_turn` with a concise reason describing the current state.";
const THINKING_PHASE_TIMEOUT_MS: f64 = 20.0 * 60.0 * 1000.0;
const RUNTIME_MAILBOX_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
const RUNTIME_MAILBOX_HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Standard thinking levels.
pub const THINKING_LEVELS: [ThinkingLevel; 5] = [
    ThinkingLevel::Off,
    ThinkingLevel::Minimal,
    ThinkingLevel::Low,
    ThinkingLevel::Medium,
    ThinkingLevel::High,
];

/// Failure surfaced by [`AgentSession`] operations.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentSessionError {
    message: JsString,
    original: Option<Box<AgentSessionErrorSource>>,
}

#[derive(Debug, Clone, PartialEq)]
enum AgentSessionErrorSource {
    Native(pi_js_compat::node_error::NativeNodeError),
    Callback(pi_ai::request::CallbackError),
}

impl AgentSessionError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            original: None,
        }
    }
    pub fn message(&self) -> &JsString {
        &self.message
    }
}
impl From<AgentSessionError> for pi_ai::request::CallbackError {
    fn from(error: AgentSessionError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}

impl From<AgentSessionError> for crate::core::extensions::types::HandlerError {
    fn from(error: AgentSessionError) -> Self {
        Self::from(pi_ai::request::CallbackError::from(error))
    }
}

impl std::fmt::Display for AgentSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for AgentSessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.original.as_deref().map(|source| match source {
            AgentSessionErrorSource::Native(error) => error as &dyn std::error::Error,
            AgentSessionErrorSource::Callback(error) => error as &dyn std::error::Error,
        })
    }
}

impl From<pi_js_compat::node_error::NativeNodeError> for AgentSessionError {
    fn from(error: pi_js_compat::node_error::NativeNodeError) -> Self {
        Self {
            message: error.message.clone(),
            original: Some(Box::new(AgentSessionErrorSource::Native(error))),
        }
    }
}
impl From<pi_ai::request::CallbackError> for AgentSessionError {
    fn from(error: pi_ai::request::CallbackError) -> Self {
        Self {
            message: error.message.clone(),
            original: Some(Box::new(AgentSessionErrorSource::Callback(error))),
        }
    }
}
impl From<crate::config::ConfigError> for AgentSessionError {
    fn from(error: crate::config::ConfigError) -> Self {
        error.into_native_error().into()
    }
}
impl From<SessionManagerError> for AgentSessionError {
    fn from(error: SessionManagerError) -> Self {
        match error.native_error() {
            Some(native) => Self::from(native.clone()),
            None => Self::new(error.message()),
        }
    }
}

impl From<crate::core::multi_agent_store::MultiAgentStoreError> for AgentSessionError {
    fn from(error: crate::core::multi_agent_store::MultiAgentStoreError) -> Self {
        Self::new(error.message)
    }
}

impl From<AgentError> for AgentSessionError {
    fn from(error: AgentError) -> Self {
        pi_ai::request::CallbackError::from_error(error.message().clone(), error).into()
    }
}

fn error(message: impl Into<JsString>) -> AgentSessionError {
    AgentSessionError::new(message)
}

fn handler_error_to_agent_error(error: crate::core::extensions::types::HandlerError) -> AgentError {
    let callback = pi_ai::request::CallbackError::from(error);
    match callback.error_or_string_message() {
        Ok(message) => AgentError::new(message).with_original(Arc::new(callback)),
        Err(error) => AgentError::from(error),
    }
}

/// Frozen `beforeRun`: optionally replace the session before starting work.
pub type BeforeWorkingDirectoryRecoveryRun =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Option<AgentSession>, pi_ai::request::CallbackError>> + Send + Sync>;
/// Frozen `afterTurn`: prepare recovery before the existing stop predicates run.
pub type AfterWorkingDirectoryRecoveryTurn =
    Arc<dyn Fn() -> BoxFuture<'static, Result<(), pi_ai::request::CallbackError>> + Send + Sync>;

#[derive(Clone)]
pub struct WorkingDirectoryRecoveryHandlers {
    pub before_run: BeforeWorkingDirectoryRecoveryRun,
    pub after_turn: AfterWorkingDirectoryRecoveryTurn,
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

/// `bindAgentSessionMutationTargetResolver`.
pub fn bind_agent_session_mutation_target_resolver(
    session: &AgentSession,
    resolver: crate::core::extensions::types::ViewedSessionMutationTargetResolver,
) {
    *lock(&session.inner.session_mutation_target_resolver) = Some(resolver);
}

/// `streamingBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingBehavior {
    Steer,
    FollowUp,
}

/// One tool definition plus its executable.
///
/// Mirrors `ToolDefinition` from `extensions/types.ts`.
#[derive(Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: JsString,
    pub parameters: Value,
    pub prompt_snippet: Option<JsString>,
    pub prompt_guidelines: Vec<JsString>,
    pub approval_required: bool,
    pub approval_kind: Option<crate::core::extensions::types::ApprovalKind>,
    pub tool: Arc<dyn AgentTool>,
}

impl ToolDefinition {
    /// Builds a definition around an existing agent tool.
    pub fn from_tool(tool: Arc<dyn AgentTool>) -> Self {
        Self {
            name: tool.name().to_owned(),
            description: tool.description().into(),
            parameters: tool.parameters().clone(),
            prompt_snippet: None,
            prompt_guidelines: Vec::new(),
            approval_required: true,
            approval_kind: None,
            tool,
        }
    }
}

/// Tools the Supervisor LLM reviewer auto-approves when the session definition
/// marks them `approvalKind: "read-only"`.
const SUPERVISOR_AUTO_APPROVED_READ_ONLY_TOOLS: [&str; 7] =
    ["find", "grep", "ls", "outline", "read", "references", "symbol"];

fn build_allow_always_agent_prompt(cwd: &JsString, input: &JsObject, tool_name: &str) -> JsString {
    let input = pi_js_compat::js_json::stringify_lossless_pretty(&JsValue::Object(input.clone()));
    JsString::join(
        [
            "Add a persistent allow rule for a command that was approved with Allow always in Pi.".into(),
            JsString::default(),
            "Target project: /syncthing/Sync/Projects/claude/claude-bash-hook".into(),
            js_format!("Original cwd: {}", cwd),
            js_format!("Tool name: {}", tool_name),
            "Tool input:".into(),
            input.into(),
            JsString::default(),
            "Requirements:".into(),
            "- Add the narrowest safe allow rule for this exact workflow.".into(),
            "- Do not broaden access beyond what the approved command requires.".into(),
            "- Preserve deny/protected-path precedence.".into(),
            "- Run the relevant claude-bash-hook tests/checks.".into(),
            "- Commit the verified config or code change if repository rules allow committing.".into(),
        ],
        "\n",
    )
}

fn builtin_source_info(name: &str) -> crate::core::source_info::SourceInfo {
    create_synthetic_source_info(
        &JsString::from(format!("<builtin:{name}>")),
        &SyntheticSourceInfoOptions {
            source: "builtin".into(),
            scope: None,
            origin: None,
            base_dir: None,
        },
    )
}

/// `ToolInfo` returned by `getAllTools`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: JsString,
    pub parameters: Value,
    pub prompt_guidelines: Vec<JsString>,
    pub source_info: crate::core::source_info::SourceInfo,
}

#[derive(Clone)]
struct ToolDefinitionEntry {
    definition: ToolDefinition,
    source_info: crate::core::source_info::SourceInfo,
}

/// Supervisor decision requester used by the LLM approval reviewer.
pub type SupervisorDecisionRequester = Arc<
    dyn Fn(
            crate::supervisor::client::RequestSupervisorDecisionInput,
        ) -> BoxFuture<'static, Result<crate::core::session_control_db::SupervisorResponse, JsString>>
        + Send
        + Sync,
>;

fn default_supervisor_decision_requester() -> SupervisorDecisionRequester {
    Arc::new(|input| {
        Box::pin(async move {
            crate::supervisor::client::request_supervisor_decision(&input)
                .await
                .map_err(|error| error.message)
        })
    })
}

/// `AgentSessionConfig`.
pub struct AgentSessionConfig {
    pub no_supervisor: Option<bool>,
    pub agent: Agent,
    pub session_manager: Arc<Mutex<SessionManager>>,
    pub settings_manager: Arc<Mutex<SettingsManager>>,
    pub cwd: JsString,
    pub scoped_models: Vec<ScopedModel>,
    pub resource_loader: Arc<dyn ResourceLoader>,
    pub model_registry: ModelRegistry,
    pub initial_active_tool_names: Option<Vec<String>>,
    pub allowed_tool_names: Option<Vec<String>>,
    pub excluded_tool_names: Option<Vec<String>>,
    pub permission_prompt_tool: Option<String>,
    pub agent_dir: Option<JsString>,
    pub control_db_path: Option<JsString>,
    pub base_tools: Vec<Arc<dyn AgentTool>>,
    pub custom_tools: Vec<ToolDefinition>,
    /// Rust-native carrier for SDK custom definitions executed with the runner context.
    pub custom_tool_definitions: Vec<crate::core::extensions::types::ToolDefinition>,
    pub multi_agent_runtime_role: Option<MultiAgentRuntimeRole>,
    pub multi_agent_execution_capability: Option<MultiAgentExecutionCapability>,
    pub thinking_phase_timeout_ms: Option<f64>,
    pub supervisor_decision_requester: Option<SupervisorDecisionRequester>,
    pub multi_agent_store: Option<Arc<crate::core::multi_agent_store::MultiAgentStore>>,
    pub multi_agent_agent_id: Option<JsString>,
    pub multi_agent_parent_session_id: Option<JsString>,
    pub multi_agent_requires_agent_id: bool,
    pub disable_runtime_coordination_inbound: bool,
    pub on_shared_channel_message_delivered: Option<Arc<dyn Fn(JsString) + Send + Sync>>,
    pub extension_runner_ref: Option<Arc<Mutex<Option<ExtensionRunner>>>>,
    pub session_start_event: Option<SessionStartEvent>,
}

/// `onUserMessagePrepared` callback.
pub type UserMessagePreparedCallback = Arc<dyn Fn(&AgentMessage) + Send + Sync>;
/// `preflightResult` callback.
pub type PromptPreflightCallback = Arc<dyn Fn(bool) + Send + Sync>;
/// Provider registration action types.
pub type RegisterProviderAction =
    Arc<dyn Fn(&str, &crate::core::model_registry::ProviderConfigInput) -> Result<(), CallbackError> + Send + Sync>;
pub type UnregisterProviderAction = Arc<dyn Fn(&str) -> Result<(), CallbackError> + Send + Sync>;

/// `PromptOptions`.
#[derive(Default, Clone)]
pub struct PromptOptions {
    pub expand_prompt_templates: Option<bool>,
    pub images: Vec<ImageContent>,
    pub streaming_behavior: Option<StreamingBehavior>,
    pub source: Option<InputSource>,
    pub preflight_result: Option<PromptPreflightCallback>,
    pub on_user_message_prepared: Option<UserMessagePreparedCallback>,
}

/// Display-only queue projection; Agent still owns delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedMessagePreview {
    pub text: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_source: Option<InputSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_type: Option<JsString>,
}

/// Independent snapshots of the two display queues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedMessagePreviews {
    pub steering: Vec<QueuedMessagePreview>,
    pub follow_up: Vec<QueuedMessagePreview>,
}

pub use pi_coding_agent_tools::core::agent_session::ModelCycleResult;
use pi_coding_agent_tools::core::agent_session::serialize_thinking_level;

/// Session statistics for `/session`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub session_file: Option<JsString>,
    pub session_id: JsString,
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

/// One user message in the fork selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessageForForking {
    pub entry_id: JsString,
    pub text: JsString,
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

fn add_session_message(mut total: SessionStatsTotals, message: &AgentMessage) -> SessionStatsTotals {
    total.total_messages += 1;
    match message {
        AgentMessage::User(_) => total.user_messages += 1,
        AgentMessage::ToolResult(_) => total.tool_results += 1,
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

/// `AgentSessionEvent`.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum AgentSessionEvent {
    Agent(AgentEvent),
    Message(AgentMessageEvent),
    AgentEnd(AgentEndEvent),
    QueueUpdate(QueueUpdateEvent),
    SteeringMessageQueued(SteeringMessageQueuedEvent),
    CompactionStart(CompactionStartEvent),
    CompactionEnd(Box<CompactionEndEvent>),
    EntryAppended(EntryAppendedEvent),
    BashMessagesCommitted(BashMessagesCommittedEvent),
    SessionInfoChanged(SessionInfoChangedEvent),
    ThinkingLevelChanged(ThinkingLevelChangedEvent),
    AutoRetryStart(AutoRetryStartEvent),
    AutoRetryEnd(AutoRetryEndEvent),
    ProviderStreamRetry(ProviderStreamRetryEvent),
}

#[derive(Debug, Clone)]
pub struct AgentMessageEvent {
    pub event: AgentEvent,
    pub runtime_message_marker: Option<crate::core::runtime_message_markers::RuntimeMessageMarker>,
}

impl Serialize for AgentMessageEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = pi_js_compat::js_json::to_value(&self.event).map_err(serde::ser::Error::custom)?;
        if let (Some(marker), JsValue::Object(object)) = (self.runtime_message_marker, &mut value) {
            object.insert(
                "runtimeMessageMarker".into(),
                pi_js_compat::js_json::to_value(&marker).map_err(serde::ser::Error::custom)?,
            );
        }
        value.serialize(serializer)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEndEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub messages: Vec<AgentMessage>,
    pub will_retry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_continuation: Option<crate::core::extensions::types::AgentEndSessionContinuation>,
}

impl AgentEndEvent {
    fn new(
        messages: Vec<AgentMessage>,
        will_retry: bool,
        session_continuation: Option<crate::core::extensions::types::AgentEndSessionContinuation>,
    ) -> Self {
        Self {
            event_type: "agent_end",
            messages,
            will_retry,
            session_continuation,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueUpdateEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub steering: Vec<JsString>,
    #[serde(rename = "followUp")]
    pub follow_up: Vec<JsString>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SteeringMessageQueuedEvent {
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
    pub source_hint: Option<crate::core::compaction::CompactionSourceInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionEndEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<crate::core::compaction::CompactionResult>,
    pub aborted: bool,
    pub will_retry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<JsString>,
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
    pub name: Option<JsString>,
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
    pub error_message: JsString,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoRetryEndEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub success: bool,
    pub attempt: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_error: Option<JsString>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStreamRetryEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub retry: ProviderRetryEvent,
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
    pub custom_instructions: Option<JsString>,
    pub replace_instructions: Option<bool>,
    pub label: Option<JsString>,
}

/// Result of [`AgentSession::navigate_tree`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateTreeResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_text: Option<JsString>,
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
pub fn get_assistant_message_text(message: &AssistantMessage) -> JsString {
    JsString::join(
        message.content.iter().filter_map(|content| match content {
            AssistantContent::Text(text) => Some(&text.text),
            _ => None,
        }),
        "",
    )
}

/// `_isTerminalMultiAgentLifecycle`.
fn is_terminal_multi_agent_lifecycle(lifecycle: AgentLifecycle) -> bool {
    matches!(
        lifecycle,
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
    )
}

fn lifecycle_name(lifecycle: AgentLifecycle) -> &'static str {
    match lifecycle {
        AgentLifecycle::Running => "running",
        AgentLifecycle::WaitingForInput => "waiting_for_input",
        AgentLifecycle::SteeringPending => "steering_pending",
        AgentLifecycle::Cancelling => "cancelling",
        AgentLifecycle::Completed => "completed",
        AgentLifecycle::Failed => "failed",
        AgentLifecycle::Aborted => "aborted",
    }
}

fn last_runtime_mailbox_assistant(messages: &[AgentMessage]) -> Option<&AssistantMessage> {
    messages.iter().rev().find_map(|message| match message {
        AgentMessage::Assistant(assistant) => Some(assistant.as_ref()),
        _ => None,
    })
}

fn runtime_mailbox_steering_lifecycle(messages: &[AgentMessage]) -> AgentLifecycle {
    match last_runtime_mailbox_assistant(messages).map(|assistant| assistant.stop_reason) {
        Some(StopReason::Aborted) => AgentLifecycle::Aborted,
        Some(StopReason::Error) => AgentLifecycle::Failed,
        _ => AgentLifecycle::Completed,
    }
}

fn last_runtime_mailbox_assistant_text(messages: &[AgentMessage]) -> Option<JsString> {
    let assistant = last_runtime_mailbox_assistant(messages)?;
    let text = get_assistant_message_text(assistant).trim();
    (!text.is_empty()).then_some(text)
}

fn user_message_text(content: &UserContent) -> JsString {
    match content {
        UserContent::Text(text) => text.clone(),
        UserContent::Blocks(blocks) => JsString::join(
            blocks.iter().filter_map(|block| match block {
                pi_ai::types::UserContentBlock::Text(text) => Some(&text.text),
                pi_ai::types::UserContentBlock::Image(_) => None,
            }),
            "",
        ),
    }
}

fn find_last_user_text(messages: &[AgentMessage]) -> Option<JsString> {
    for message in messages.iter().rev() {
        let AgentMessage::User(user) = message else {
            continue;
        };
        return Some(user_message_text(&user.content));
    }
    None
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
    no_supervisor: bool,
) -> bool {
    if no_supervisor && crate::core::tool_capabilities::STANDALONE_DISABLED_TOOL_NAMES.contains(&name) {
        return false;
    }
    if name == "end_turn" {
        return true;
    }
    allowed.is_none_or(|allowed| allowed.contains(name)) && !excluded.is_some_and(|excluded| excluded.contains(name))
}

fn normalize_message_content(message: AgentMessage) -> AgentMessage {
    match message {
        AgentMessage::Custom(mut object) => {
            if object.get("content").is_some_and(JsValue::is_null) {
                object.insert("content".to_owned().into(), Value::Array(Vec::new()).into());
            }
            AgentMessage::Custom(object)
        }
        other => other,
    }
}

pub fn thinking_level_from_model(level: pi_ai::types::ModelThinkingLevel) -> ThinkingLevel {
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

pub(crate) fn model_thinking_level(level: ThinkingLevel) -> pi_ai::types::ModelThinkingLevel {
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
                if object.get("customType").and_then(JsValue::as_string) == Some(&JsString::from("self_restart"))
        );
        if !is_self_restart {
            break;
        }
        last_message_index -= 1;
    }
    let relevant = &messages[..(last_message_index + 1).max(0) as usize];
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

fn normalize_required_request_auth_error(message: JsString, provider: &str) -> AgentSessionError {
    if message == "authHeader requires a resolved API key" {
        return crate::core::auth_guidance::format_no_api_key_found_message(provider)
            .map(error)
            .unwrap_or_else(AgentSessionError::from);
    }
    error(message)
}

// =============================================================================
// AgentSession
// =============================================================================

struct CompactionRequestAuth {
    model: Model,
    api_key: Option<JsString>,
    headers: Option<ProviderHeaders>,
    env: Option<pi_ai::types::ProviderEnv>,
}

#[derive(Default)]
struct ThinkingTimeoutDispatch {
    recovery_used: bool,
    cancelled: bool,
}

struct IdleCompactionTarget {
    identity: pi_agent_core::agent::AgentMessageIdentity,
    due_at: f64,
}

struct Inner {
    agent: Agent,
    session_manager: Arc<Mutex<SessionManager>>,
    settings_manager: Arc<Mutex<SettingsManager>>,
    cwd: JsString,
    agent_dir: JsString,
    control_db_path: Option<JsString>,
    scoped_models: Mutex<Vec<ScopedModel>>,
    resource_loader: Arc<dyn ResourceLoader>,
    model_registry: ModelRegistry,
    custom_tools: Vec<ToolDefinition>,
    custom_tool_definitions: Vec<crate::core::extensions::types::ToolDefinition>,
    /// `_baseToolsOverride`: `Some` replaces the built-in definitions.
    base_tools_override: Option<Vec<(String, crate::core::extensions::types::ToolDefinition)>>,
    base_tool_definitions: Mutex<Vec<(String, crate::core::extensions::types::ToolDefinition)>>,
    no_supervisor: bool,
    tool_detach_registry: Option<Arc<ToolDetachRegistry>>,
    tool_definitions: Mutex<Vec<(String, ToolDefinitionEntry)>>,
    tool_prompt_snippets: Mutex<HashMap<String, JsString>>,
    tool_prompt_guidelines: Mutex<HashMap<String, Vec<JsString>>>,
    tool_registry: Mutex<HashMap<String, Arc<dyn AgentTool>>>,
    initial_active_tool_names: Option<Vec<String>>,
    permission_prompt_tool: Option<String>,
    allowed_tool_names: Option<HashSet<String>>,
    excluded_tool_names: Option<HashSet<String>>,
    base_system_prompt: Mutex<JsString>,
    base_system_prompt_options: Mutex<Option<BuildSystemPromptOptions>>,
    system_prompt_override: Mutex<Option<JsString>>,
    event_listeners: Mutex<Vec<(u64, AgentSessionEventListener)>>,
    next_listener_id: AtomicU64,
    agent_subscription: Mutex<Option<u64>>,
    steering_messages: Mutex<Vec<QueuedMessagePreview>>,
    follow_up_messages: Mutex<Vec<QueuedMessagePreview>>,
    last_assistant_turn_fingerprint: Mutex<Option<String>>,
    duplicate_turn_guard_injected: AtomicBool,
    duplicate_turn_loop_detected: AtomicBool,
    pending_next_turn_messages: Mutex<Vec<AgentMessage>>,
    external_user_input_reservations: AtomicUsize,
    resume_continuation_requested: AtomicBool,
    turn_start_lock: tokio::sync::Mutex<()>,
    working_directory_recovery: Mutex<Option<WorkingDirectoryRecoveryHandlers>>,
    last_assistant_message: Mutex<Option<(AssistantMessage, pi_agent_core::agent::AgentMessageIdentity)>>,
    codex_restriction_context: Mutex<CodexRestrictionContextCache>,
    codex_restriction_abort: Mutex<Option<AbortController>>,
    codex_restriction_validation: Mutex<Option<CodexRestrictionValidation>>,
    codex_restriction_attempt_active: AtomicBool,
    active_tools_revision: AtomicU64,
    supervisor_review_cancellation_handler: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    retry_attempt: AtomicU64,
    retry_abort_controller: Mutex<Option<AbortController>>,
    quota_fallback_visited_providers: Mutex<HashSet<String>>,
    compaction_abort_controller: Mutex<Option<AbortController>>,
    auto_compaction_abort_controller: Mutex<Option<AbortController>>,
    compaction_summary_materialization_abort_controller: Mutex<Option<AbortController>>,
    branch_summary_abort_controller: Mutex<Option<AbortController>>,
    pending_bash_messages: Mutex<Vec<BashExecutionMessage>>,
    bash_abort_controllers: Mutex<Vec<AbortController>>,
    disposed: AtomicBool,
    extension_runner: Mutex<ExtensionRunner>,
    extension_runner_ref: Option<Arc<Mutex<Option<ExtensionRunner>>>>,
    session_start_event: SessionStartEvent,
    extension_ui_context: Mutex<Option<Arc<dyn ExtensionUiContext>>>,
    extension_footer_data: Mutex<Option<Arc<FooterDataProvider>>>,
    extension_mode: Mutex<ExtensionMode>,
    extension_control_db_path: Mutex<Option<JsString>>,
    extension_command_context_actions: Mutex<Option<ExtensionCommandContextActions>>,
    extension_abort_handler: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    extension_shutdown_handler: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    extension_error_listener: Mutex<Option<ExtensionErrorListener>>,
    extension_error_unsubscriber: Mutex<Option<ErrorSubscription>>,
    turn_index: AtomicU64,
    overflow_recovery_attempted: AtomicBool,
    length_recovery_attempted: AtomicBool,
    permission_rule_store: Arc<Mutex<crate::core::rule_store::PermissionRuleStore>>,
    supervisor_decision_requester: SupervisorDecisionRequester,
    multi_agent_store: Option<Arc<crate::core::multi_agent_store::MultiAgentStore>>,
    multi_agent_agent_id: Option<JsString>,
    multi_agent_parent_session_id: Option<JsString>,
    multi_agent_requires_agent_id: bool,
    multi_agent_runtime_role: Option<MultiAgentRuntimeRole>,
    thinking_phase_timeout_ms: f64,
    thinking_timeout_dispatch: Mutex<Option<Arc<Mutex<ThinkingTimeoutDispatch>>>>,
    thinking_phase_timer: Mutex<Option<tokio::task::JoinHandle<()>>>,
    idle_compaction_timer: Mutex<Option<tokio::task::JoinHandle<()>>>,
    thinking_phase_timeout_error: Mutex<Option<AgentSessionError>>,
    disable_runtime_coordination_inbound: bool,
    on_shared_channel_message_delivered: Option<Arc<dyn Fn(JsString) + Send + Sync>>,
    terminal_outbox_claim_id: String,
    terminal_outbox_last_cleanup_at: Mutex<Option<i64>>,
    multi_agent_active_tools: Mutex<Vec<(String, ActiveTool)>>,
    background_compaction_cache: Mutex<Option<BackgroundCompactionCache>>,
    runtime_mailbox_drain_lock: tokio::sync::Mutex<()>,
    runtime_mailbox_steering_agent_ids: Mutex<Vec<JsString>>,
    runtime_mailbox_acceptance: Mutex<crate::core::runtime_mailbox_acceptance::RuntimeMailboxAcceptance>,
    runtime_mailbox_pending_terminal: Mutex<indexmap::IndexMap<JsString, PendingSteeringTerminal>>,
    runtime_mailbox_pending_terminal_unsubscribe: Mutex<Option<crate::core::multi_agent_store::AgentSubscription>>,
    runtime_mailbox_poll_token: Mutex<Option<AbortController>>,
    runtime_mailbox_heartbeat_token: Mutex<Option<AbortController>>,
    runtime_mailbox_signal_token: Mutex<Option<AbortController>>,
    runtime_mailbox_control_db_path: Mutex<Option<JsString>>,
    runtime_mailbox_control_db_release: Mutex<Option<crate::core::session_control_db::RetainedControlDbGuard>>,
    session_mutation_target_resolver:
        Mutex<Option<crate::core::extensions::types::ViewedSessionMutationTargetResolver>>,
    shared_channel_drain_in_progress: AtomicBool,
}

#[derive(Clone)]
struct PendingSteeringTerminal {
    lifecycle: crate::core::multi_agent_store::AgentLifecycle,
    summary: Option<JsString>,
}

struct ActiveTool {
    started_at: String,
    tool_call_id: String,
    tool_name: String,
}

fn active_tool_activity(
    active: Option<&(String, ActiveTool)>,
) -> Option<crate::core::multi_agent_store::AgentCurrentActivity> {
    let (_, tool) = active?;
    Some(crate::core::multi_agent_store::AgentCurrentActivity::Tool {
        started_at: tool.started_at.clone().into(),
        tool_call_id: tool.tool_call_id.clone().into(),
        tool_name: tool.tool_name.clone().into(),
        detail: None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BackgroundCompactionState {
    Pending,
    Ready,
    Failed,
}

#[derive(Clone)]
struct BackgroundCompactionCache {
    token: u64,
    state: BackgroundCompactionState,
    session_id: JsString,
    snapshot_leaf_id: JsString,
    branch_entries: Vec<SessionEntry>,
    preparation: crate::core::compaction::CompactionPreparation,
    settings: crate::core::compaction::CompactionSettings,
    model_provider: String,
    model_id: String,
    model_api: String,
    thinking_level: ThinkingLevel,
    system_prompt: JsString,
    runner: ExtensionRunner,
    abort: AbortController,
    compaction: Option<crate::core::compaction::CompactionResult>,
    from_extension: bool,
    generation_duration_ms: Option<i64>,
}

const BACKGROUND_COMPACTION_CONTEXT_RATIO: f64 = 0.7;

enum CompactionExtensionDecision {
    None,
    Cancel,
    Compaction(Box<crate::core::compaction::CompactionResult>),
}

fn compaction_extension_decision(
    cancel: Option<bool>,
    compaction: Option<crate::core::compaction::CompactionResult>,
) -> CompactionExtensionDecision {
    if cancel == Some(true) {
        CompactionExtensionDecision::Cancel
    } else if let Some(compaction) = compaction {
        CompactionExtensionDecision::Compaction(Box::new(compaction))
    } else {
        CompactionExtensionDecision::None
    }
}

fn compaction_reason(reason: &str) -> CompactionReason {
    match reason {
        "threshold" => CompactionReason::Threshold,
        "overflow" => CompactionReason::Overflow,
        "idle" => CompactionReason::Idle,
        _ => CompactionReason::Manual,
    }
}

async fn emit_compaction_event(
    runner: &ExtensionRunner,
    preparation: &crate::core::compaction::CompactionPreparation,
    branch_entries: &[SessionEntry],
    custom_instructions: Option<&JsString>,
    reason: &str,
    will_retry: bool,
    abort: &AbortSignal,
) -> CompactionExtensionDecision {
    if !runner.has_handlers("compaction") {
        return CompactionExtensionDecision::None;
    }
    let mut event = ExtensionEvent::Compaction(CompactionEvent {
        preparation: preparation.clone(),
        branch_entries: branch_entries.to_vec(),
        custom_instructions: custom_instructions.cloned(),
        reason: compaction_reason(reason),
        will_retry,
        signal: Some(abort.clone()),
    });
    match runner.emit(&mut event).await {
        EmitResult::Compaction(result) => compaction_extension_decision(result.cancel, result.compaction),
        _ => CompactionExtensionDecision::None,
    }
}

/// `DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE`.
pub const DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE: &str = "detached_tool_call_completion";

/// Terminal outcome recorded for a detached tool call.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedToolCallCompletionEntryData {
    pub tool_call_id: JsString,
    pub agent_id: JsString,
    pub lifecycle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsString>,
}

/// `buildDetachedToolCallCompletionEntry`.
pub fn build_detached_tool_call_completion_entry(
    agent: Option<&crate::core::multi_agent_store::AgentSnapshot>,
) -> Option<DetachedToolCallCompletionEntryData> {
    use crate::core::multi_agent_store::AgentLifecycle;

    let agent = agent?;
    let tool_call_id = agent
        .worker
        .as_ref()
        .and_then(|worker| worker.tool_call_id.clone())
        .or_else(|| agent.result.as_ref().and_then(|result| result.tool_call_id.clone()))?;
    let lifecycle = match agent.lifecycle {
        AgentLifecycle::Completed => "completed",
        AgentLifecycle::Failed => "failed",
        AgentLifecycle::Aborted => "aborted",
        _ => return None,
    };
    Some(DetachedToolCallCompletionEntryData {
        tool_call_id,
        agent_id: agent.id.clone(),
        lifecycle: lifecycle.to_owned(),
        summary: agent.result.as_ref().and_then(|result| result.summary.clone()),
        error: agent.error.as_ref().map(|error| error.message.clone()),
    })
}

#[derive(Clone, Copy)]
struct DrainCoordinationOptions {
    checkpoint: Option<SteeringCheckpoint>,
    include_next_model_call: bool,
    trigger_if_idle: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MailboxDeliveryMode {
    Steer,
    Prompt,
}

fn is_runtime_mailbox_message_due(message: &RuntimeMailboxMessage, options: DrainCoordinationOptions) -> bool {
    if message.kind != crate::core::session_control_db::RuntimeMailboxMessageKind::Steer {
        return options.checkpoint != Some(SteeringCheckpoint::AfterToolResult) || options.include_next_model_call;
    }
    match message.target_checkpoint.unwrap_or(SteeringCheckpoint::NextModelCall) {
        SteeringCheckpoint::AfterToolResult => options.checkpoint == Some(SteeringCheckpoint::AfterToolResult),
        SteeringCheckpoint::WhenWaiting => options.trigger_if_idle,
        SteeringCheckpoint::NextModelCall => {
            options.checkpoint == Some(SteeringCheckpoint::NextModelCall)
                || options.include_next_model_call
                || options.trigger_if_idle
        }
    }
}

fn runtime_mailbox_user_message(
    text: impl Into<JsString>,
    images: &[ImageContent],
    source: Option<InputSource>,
) -> AgentMessage {
    let mut content = vec![TextOrImageContent::Text(TextContent::new(text))];
    content.extend(images.iter().cloned().map(TextOrImageContent::Image));
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(content),
        input_source: source,
        provider_native: None,
        timestamp: pi_js_compat::date::js_now_millis(),
    })
}

fn runtime_mailbox_checkpoint_priority(message: &RuntimeMailboxMessage) -> u8 {
    u8::from(message.target_checkpoint != Some(SteeringCheckpoint::AfterToolResult))
}

fn is_own_shared_channel_message(
    message: &crate::core::session_control_db::SharedChannelMessage,
    recipient: &crate::core::session_control_db::RuntimeMailboxAddress,
) -> bool {
    message.sender.session_id == recipient.session_id && message.sender.agent_id == recipient.agent_id
}

fn is_subagent_shared_channel_message(message: &crate::core::session_control_db::SharedChannelMessage) -> bool {
    message.sender.agent_id.is_some()
}

/// Core session abstraction shared by all run modes.
#[derive(Clone)]
pub struct AgentSession {
    inner: Arc<Inner>,
}

impl AgentSession {
    /// Builds a session and installs agent hooks, subscriptions, and tools.
    pub fn new(config: AgentSessionConfig) -> Result<Self, AgentSessionError> {
        assert_standalone_worker_options(&StandaloneWorkerOptions {
            no_supervisor: config.no_supervisor.unwrap_or(false),
            session_is_persisted: lock(&config.session_manager).is_persisted(),
            has_multi_agent_store: config.multi_agent_store.is_some(),
            multi_agent_runtime_role: config.multi_agent_runtime_role,
            has_execution_capability: config.multi_agent_execution_capability.is_some(),
            multi_agent_agent_id: config.multi_agent_agent_id.as_ref(),
            multi_agent_parent_session_id: config.multi_agent_parent_session_id.as_ref(),
            multi_agent_requires_agent_id: config.multi_agent_requires_agent_id,
        })?;
        let no_supervisor = config.no_supervisor.unwrap_or(false);
        validate_multi_agent_runtime_role(config.multi_agent_runtime_role, config.multi_agent_execution_capability)?;

        let model_registry = config.model_registry.clone();
        let base_tools_override = (!config.base_tools.is_empty()).then(|| {
            let mut definitions = config
                .base_tools
                .iter()
                .filter(|tool| tool.name() != crate::core::tools::end_turn::NAME)
                .map(|tool| {
                    (
                        tool.name().to_owned(),
                        crate::core::tools::tool_definition_wrapper::create_tool_definition_from_agent_tool(
                            Arc::clone(tool),
                        ),
                    )
                })
                .collect::<Vec<_>>();
            definitions.push((
                crate::core::tools::end_turn::NAME.to_owned(),
                crate::core::tools::end_turn::create_end_turn_tool_definition(),
            ));
            definitions
        });
        let tool_detach_registry = (!no_supervisor).then(|| Arc::new(ToolDetachRegistry::new()));
        let agent_dir = match config.agent_dir.clone() {
            Some(directory) => directory,
            None => crate::config::get_agent_dir()?,
        };
        let extension_runner_ref = config.extension_runner_ref.clone();
        let session_start_event = config.session_start_event.clone().unwrap_or(SessionStartEvent {
            reason: crate::core::extensions::types::SessionStartReason::Startup,
            previous_session_file: None,
        });
        let placeholder_runner = ExtensionRunner::new(
            Vec::new(),
            crate::core::extensions::extension_runtime::create_extension_runtime(),
            config.cwd.clone(),
            Arc::clone(&config.session_manager),
            Arc::new(model_registry.clone()),
            Some(Arc::clone(&config.settings_manager)),
        );

        let control_db_path = {
            let mut manager = lock(&config.session_manager);
            let path = config
                .control_db_path
                .clone()
                .or_else(|| manager.get_metadata_control_db_path().cloned())
                .map(Ok)
                .unwrap_or_else(|| {
                    crate::config::get_user_state_root()
                        .map(|root| crate::core::session_control_db::get_control_db_path(&root).into())
                })?;
            manager
                .set_metadata_control_db_path(Some(&path), Default::default())
                .map_err(AgentSessionError::from)?;
            path
        };

        let permission_prompt_tool = match config.permission_prompt_tool.clone() {
            Some(name) => Some(name),
            None => lock(&config.settings_manager)
                .get_permission_prompt_tool()
                .map(|name| {
                    name.to_utf8()
                        .map_err(|_| error("Permission prompt tool name contains unpaired UTF-16 units"))
                })
                .transpose()?,
        };

        let permission_rule_store = {
            let settings = JsValue::Object(lock(&config.settings_manager).get_merged_settings());
            let mut store = crate::core::rule_store::PermissionRuleStore::from_settings(Some(&settings));
            store.set_locations(Some(config.cwd.clone()), Some(agent_dir.clone()));
            store
        };
        let supervisor_decision_requester = config
            .supervisor_decision_requester
            .clone()
            .unwrap_or_else(default_supervisor_decision_requester);
        let multi_agent_store = config.multi_agent_store.clone();
        let multi_agent_agent_id = config.multi_agent_agent_id.clone();
        let multi_agent_parent_session_id = config.multi_agent_parent_session_id.clone();
        let multi_agent_requires_agent_id = config.multi_agent_requires_agent_id;
        let multi_agent_runtime_role = config.multi_agent_runtime_role;
        let thinking_phase_timeout_ms = config.thinking_phase_timeout_ms.unwrap_or(THINKING_PHASE_TIMEOUT_MS);
        let disable_runtime_coordination_inbound = no_supervisor || config.disable_runtime_coordination_inbound;
        let on_shared_channel_message_delivered = config.on_shared_channel_message_delivered.clone();

        let inner = Arc::new(Inner {
            agent: config.agent,
            session_manager: config.session_manager,
            settings_manager: config.settings_manager,
            cwd: config.cwd,
            agent_dir,
            control_db_path: Some(control_db_path),
            scoped_models: Mutex::new(config.scoped_models),
            resource_loader: config.resource_loader,
            model_registry,
            custom_tools: config.custom_tools,
            custom_tool_definitions: config.custom_tool_definitions,
            base_tools_override,
            base_tool_definitions: Mutex::new(Vec::new()),
            no_supervisor,
            tool_detach_registry,
            tool_definitions: Mutex::new(Vec::new()),
            tool_prompt_snippets: Mutex::new(HashMap::new()),
            tool_prompt_guidelines: Mutex::new(HashMap::new()),
            tool_registry: Mutex::new(HashMap::new()),
            initial_active_tool_names: config.initial_active_tool_names,
            permission_prompt_tool,
            allowed_tool_names: config.allowed_tool_names.map(|names| names.into_iter().collect()),
            excluded_tool_names: config.excluded_tool_names.map(|names| names.into_iter().collect()),
            base_system_prompt: Mutex::new(JsString::default()),
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
            working_directory_recovery: Mutex::new(None),
            last_assistant_message: Mutex::new(None),
            codex_restriction_context: Mutex::new(CodexRestrictionContextCache::default()),
            codex_restriction_abort: Mutex::new(None),
            codex_restriction_validation: Mutex::new(None),
            codex_restriction_attempt_active: AtomicBool::new(false),
            active_tools_revision: AtomicU64::new(0),
            supervisor_review_cancellation_handler: Mutex::new(None),
            retry_attempt: AtomicU64::new(0),
            retry_abort_controller: Mutex::new(None),
            quota_fallback_visited_providers: Mutex::new(HashSet::new()),
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
            permission_rule_store: Arc::new(Mutex::new(permission_rule_store)),
            supervisor_decision_requester,
            multi_agent_store,
            multi_agent_agent_id,
            multi_agent_parent_session_id,
            multi_agent_requires_agent_id,
            multi_agent_runtime_role,
            thinking_phase_timeout_ms,
            thinking_timeout_dispatch: Mutex::new(None),
            thinking_phase_timer: Mutex::new(None),
            idle_compaction_timer: Mutex::new(None),
            thinking_phase_timeout_error: Mutex::new(None),
            disable_runtime_coordination_inbound,
            on_shared_channel_message_delivered,
            terminal_outbox_claim_id: pi_ai::utils::random_uuid::random_uuid()?,
            terminal_outbox_last_cleanup_at: Mutex::new(None),
            multi_agent_active_tools: Mutex::new(Vec::new()),
            background_compaction_cache: Mutex::new(None),
            runtime_mailbox_drain_lock: tokio::sync::Mutex::new(()),
            runtime_mailbox_steering_agent_ids: Mutex::new(Vec::new()),
            runtime_mailbox_acceptance: Mutex::new(Default::default()),
            runtime_mailbox_pending_terminal: Mutex::new(indexmap::IndexMap::new()),
            runtime_mailbox_pending_terminal_unsubscribe: Mutex::new(None),
            runtime_mailbox_poll_token: Mutex::new(None),
            runtime_mailbox_heartbeat_token: Mutex::new(None),
            runtime_mailbox_signal_token: Mutex::new(None),
            runtime_mailbox_control_db_path: Mutex::new(None),
            runtime_mailbox_control_db_release: Mutex::new(None),
            session_mutation_target_resolver: Mutex::new(None),
            shared_channel_drain_in_progress: AtomicBool::new(false),
        });

        let session = Self { inner };
        session.install_agent_hooks();
        session.install_agent_next_turn_refresh();
        session.install_duplicate_turn_stop();
        session.subscribe_agent();
        let default_active_tool_names = match &session.inner.base_tools_override {
            Some(definitions) => definitions.iter().map(|(name, _)| name.clone()).collect(),
            None => crate::core::tools::DEFAULT_ACTIVE_TOOL_NAMES
                .iter()
                .map(|name| name.as_str().to_owned())
                .collect(),
        };
        session.build_runtime(
            Some(
                session
                    .inner
                    .initial_active_tool_names
                    .clone()
                    .unwrap_or(default_active_tool_names),
            ),
            None,
            true,
        )?;
        session.start_runtime_mailbox_signal_wake();
        session.start_runtime_mailbox_polling();
        Ok(session)
    }

    fn subscribe_agent(&self) {
        let weak = Arc::downgrade(&self.inner);
        let listener: AgentListener = Arc::new(move |event: AgentEvent, _signal: AbortSignal| {
            let weak = weak.clone();
            Box::pin(async move {
                if let Some(inner) = weak.upgrade() {
                    let session = AgentSession { inner };
                    session.handle_agent_event(event).await?;
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
            move |context: BeforeToolCallContext, _signal: Option<AbortSignal>| {
                let weak = weak.clone();
                Box::pin(async move {
                    let Some(inner) = weak.upgrade() else {
                        return Ok(None);
                    };
                    let session = AgentSession { inner };
                    session.before_tool_call(context).await
                })
            },
        )));
        let weak = Arc::downgrade(&self.inner);
        self.inner.agent.set_after_tool_call(Some(Arc::new(
            move |context: AfterToolCallContext, _signal: Option<AbortSignal>| {
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
            move |turn: PrepareNextTurnContext, signal: Option<AbortSignal>| {
                let weak = weak.clone();
                let previous_with_context = previous_with_context.clone();
                let previous_signal_only = previous_signal_only.clone();
                Box::pin(async move {
                    if let Some(inner) = weak.upgrade() {
                        lock(&inner.settings_manager)
                            .get_compaction_settings()
                            .map_err(|failure| AgentError::new(failure.message().clone()))?;
                    }
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
                    if !turn.tool_results.is_empty() {
                        AgentSession {
                            inner: Arc::clone(&inner),
                        }
                        .drain_runtime_coordination_messages(DrainCoordinationOptions {
                            checkpoint: Some(SteeringCheckpoint::AfterToolResult),
                            include_next_model_call: true,
                            trigger_if_idle: false,
                        })
                        .await
                        .map_err(|error| AgentError::new(error.message()))?;
                    }
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
        self.inner
            .agent
            .set_should_stop_after_turn(Some(Arc::new(move |turn: ShouldStopAfterTurnContext| {
                let weak = weak.clone();
                let previous = previous.clone();
                Box::pin(async move {
                    let after_turn = weak.upgrade().and_then(|inner| {
                        lock(&inner.working_directory_recovery)
                            .as_ref()
                            .map(|handlers| handlers.after_turn.clone())
                    });
                    if let Some(after_turn) = after_turn {
                        after_turn().await.map_err(AgentError::from)?;
                    }
                    let loop_detected = weak.upgrade().is_some_and(|inner| {
                        lock(&inner.codex_restriction_validation).is_some()
                            || lock(&inner.extension_runner).has_prepared_tool_result_relocation()
                            || inner.duplicate_turn_loop_detected.load(Ordering::SeqCst)
                    });
                    if loop_detected {
                        return Ok(true);
                    }
                    match previous {
                        Some(hook) => hook(turn).await,
                        None => Ok(false),
                    }
                })
            })));
    }

    fn build_runtime(
        &self,
        active_tool_names: Option<Vec<String>>,
        _flag_values: Option<BTreeMap<String, Value>>,
        include_all_extension_tools: bool,
    ) -> Result<(), AgentSessionError> {
        self.rebuild_base_tool_definitions()?;
        let extensions = self.inner.resource_loader.get_extensions();
        let mut runner = ExtensionRunner::new(
            extensions.extensions,
            extensions.runtime,
            self.inner.cwd.clone(),
            Arc::clone(&self.inner.session_manager),
            Arc::new(self.inner.model_registry.clone()),
            Some(Arc::clone(&self.inner.settings_manager)),
        );
        let lifecycle_weak = Arc::downgrade(&self.inner);
        runner.bind_detached_job_lifecycle(Arc::new(move || {
            lifecycle_weak
                .upgrade()
                .and_then(|inner| AgentSession { inner }.get_detached_job_lifecycle_controller())
        }));
        runner.set_internal_commands(self.create_internal_commands());
        self.bind_extension_core(&mut runner)?;
        *lock(&self.inner.extension_runner) = runner.clone();
        if let Some(reference) = &self.inner.extension_runner_ref {
            *lock(reference) = Some(runner.clone());
        }
        self.apply_extension_bindings(&runner);
        self.refresh_tool_registry(active_tool_names, include_all_extension_tools)?;
        Ok(())
    }

    /// `_buildRuntime` base definitions: overrides, or every available built-in tool with
    /// the current settings.
    fn rebuild_base_tool_definitions(&self) -> Result<(), AgentSessionError> {
        if self.inner.base_tools_override.is_some() {
            lock(&self.inner.base_tool_definitions).clear();
            return Ok(());
        }
        let (auto_resize_images, command_prefix, shell_path) = {
            let settings = lock(&self.inner.settings_manager);
            (
                settings.get_image_auto_resize(),
                settings.get_shell_command_prefix().cloned(),
                settings.get_shell_path(),
            )
        };
        let session = self.clone();
        let background_jobs = self.inner.multi_agent_store.as_ref().map(|_| {
            Arc::new(BashBackgroundJobs {
                get_lifecycle: Some(Arc::new(move || Ok(session.get_detached_job_lifecycle_controller()))),
                lifecycle: None,
            })
        });
        let definitions = create_tool_definitions_for_available_executables(
            &self.inner.cwd,
            ToolsOptions {
                read: Some(ReadToolOptions {
                    auto_resize_images: Some(auto_resize_images),
                    ..Default::default()
                }),
                bash: Some(BashToolOptions {
                    command_prefix,
                    shell_path,
                    detach_registry: self.inner.tool_detach_registry.clone(),
                    background_jobs,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .map_err(|error| AgentSessionError::new(error.message))?
        .into_iter()
        .map(|(name, definition)| (name.as_str().to_owned(), definition))
        .collect();
        *lock(&self.inner.base_tool_definitions) = definitions;
        Ok(())
    }

    fn refresh_tool_registry(
        &self,
        active_tool_names: Option<Vec<String>>,
        include_all_extension_tools: bool,
    ) -> Result<(), AgentSessionError> {
        let previous_registry_names: HashSet<String> = lock(&self.inner.tool_registry).keys().cloned().collect();
        let previous_active_tool_names = self.get_active_tool_names();
        let allowed = self.inner.allowed_tool_names.clone();
        let excluded = self.inner.excluded_tool_names.clone();
        let is_allowed =
            |name: &str| is_allowed_session_tool(name, allowed.as_ref(), excluded.as_ref(), self.inner.no_supervisor);

        let runner = self.runner();
        let base_definitions: Vec<(String, crate::core::extensions::types::ToolDefinition)> =
            match &self.inner.base_tools_override {
                Some(overrides) => overrides.clone(),
                None => lock(&self.inner.base_tool_definitions).clone(),
            };
        let mut definition_registry: Vec<(String, ToolDefinitionEntry)> = base_definitions
            .iter()
            .filter(|(name, _)| is_allowed(name))
            .map(|(name, definition)| {
                let source_info = builtin_source_info(name);
                let wrapped = wrap_registered_tool(
                    crate::core::extensions::types::RegisteredTool {
                        definition: definition.clone(),
                        source_info: source_info.clone(),
                    },
                    &runner,
                );
                (
                    name.clone(),
                    ToolDefinitionEntry {
                        definition: ToolDefinition {
                            name: definition.name.clone(),
                            description: definition.description.clone(),
                            parameters: definition.parameters.clone(),
                            prompt_snippet: definition.prompt_snippet.clone(),
                            prompt_guidelines: definition.prompt_guidelines.clone().unwrap_or_default(),
                            approval_required: definition.approval_required.unwrap_or(true),
                            approval_kind: definition.approval_kind,
                            tool: wrapped,
                        },
                        source_info,
                    },
                )
            })
            .collect();

        let extension_tools = {
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
                        approval_kind: tool.definition.approval_kind,
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
                    &JsString::from(format!("<sdk:{}>", definition.name)),
                    &SyntheticSourceInfoOptions {
                        source: "sdk".into(),
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

        let custom_definition_tools = self
            .inner
            .custom_tool_definitions
            .iter()
            .filter(|definition| is_allowed(&definition.name))
            .map(|definition| ToolDefinitionEntry {
                definition: ToolDefinition {
                    name: definition.name.clone(),
                    description: definition.description.clone(),
                    parameters: definition.parameters.clone(),
                    prompt_snippet: definition.prompt_snippet.clone(),
                    prompt_guidelines: definition.prompt_guidelines.clone().unwrap_or_default(),
                    approval_required: definition.approval_required.unwrap_or(true),
                    approval_kind: definition.approval_kind,
                    tool: wrap_definition_tool(definition.clone(), &runner),
                },
                source_info: create_synthetic_source_info(
                    &JsString::from(format!("<sdk:{}>", definition.name)),
                    &SyntheticSourceInfoOptions {
                        source: "sdk".into(),
                        scope: None,
                        origin: None,
                        base_dir: None,
                    },
                ),
            })
            .collect::<Vec<_>>();
        for entry in &custom_definition_tools {
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
            if let Some(snippet) = normalize_prompt_snippet(entry.definition.prompt_snippet.as_ref()) {
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
            for entry in &custom_definition_tools {
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
        let mut seen = HashSet::new();
        let unique: Vec<String> = next_active_tool_names
            .into_iter()
            .filter(|name| seen.insert(name.clone()))
            .collect();

        *lock(&self.inner.tool_definitions) = definition_registry;
        *lock(&self.inner.tool_prompt_snippets) = snippets;
        *lock(&self.inner.tool_prompt_guidelines) = guidelines;
        *lock(&self.inner.tool_registry) = tool_registry;
        self.apply_active_tools(&unique)
    }

    fn apply_active_tools(&self, tool_names: &[String]) -> Result<(), AgentSessionError> {
        self.inner.active_tools_revision.fetch_add(1, Ordering::SeqCst);
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
        let rebuilt = self.rebuild_system_prompt(&valid_names)?;
        *lock(&self.inner.base_system_prompt) = rebuilt.clone();
        let prompt = lock(&self.inner.system_prompt_override).clone().unwrap_or(rebuilt);
        self.inner.agent.set_system_prompt(prompt);
        Ok(())
    }

    fn rebuild_system_prompt(&self, tool_names: &[String]) -> Result<JsString, AgentSessionError> {
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
        let append_system_prompt = (!append.is_empty()).then(|| JsString::join(append, "\n\n"));
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
        let prompt = build_system_prompt(&options)?;
        *lock(&self.inner.base_system_prompt_options) = Some(options);
        Ok(prompt)
    }

    async fn before_tool_call(
        &self,
        context: BeforeToolCallContext,
    ) -> Result<Option<BeforeToolCallResult>, AgentError> {
        use crate::core::permissions::orchestrator::{ApprovalOrchestratorOptions, orchestrate_tool_approval};

        if self.inner.no_supervisor
            && crate::core::tool_capabilities::STANDALONE_DISABLED_TOOL_NAMES.contains(&context.tool_call.name.as_str())
        {
            return Ok(Some(BeforeToolCallResult {
                block: Some(true),
                reason: Some(js_format!(
                    "Tool {} is unavailable in standalone worker mode",
                    context.tool_call.name
                )),
            }));
        }
        let runner = self.runner();
        let event = ToolCallEvent {
            tool_name: crate::core::extensions::types::ToolName::Custom(context.tool_call.name.clone()),
            tool_call_id: context.tool_call.id.clone(),
            bypass_permissions: crate::core::permissions::presets::approval_preset_to_bypass_permissions(
                lock(&self.inner.settings_manager).get_approval_preset(),
            ),
            input: context.args_snapshot().as_object().cloned().unwrap_or_default(),
        };
        if runner.has_tool_gates() {
            let mut gated = event.clone();
            if let Some(result) = runner
                .emit_tool_gates(&mut gated)
                .await
                .map_err(handler_error_to_agent_error)?
            {
                return Ok(Some(tool_call_result_to_before(result)));
            }
        }

        let approval_required = self
            .find_tool_definition(&context.tool_call.name)
            .is_none_or(|entry| entry.definition.approval_required);
        let policy = lock(&self.inner.settings_manager).get_approval_policy();
        if policy == ApprovalPolicy::AutoApprove
            && let Some(result) = self
                .review_auto_approved_tool_call(&event, &runner, approval_required)
                .await
                .map_err(handler_error_to_agent_error)?
        {
            return Ok(Some(tool_call_result_to_before(result)));
        }

        let current_user_request = find_last_user_text(&context.context.messages);
        let result = orchestrate_tool_approval(&ApprovalOrchestratorOptions {
            policy,
            approval_required,
            hook_reviewer: self.create_tool_approval_hook_reviewer(&event, &runner, Arc::clone(&context.args)),
            reviewer: self.create_tool_approval_human_reviewer(&event, &runner, None, false),
            llm_reviewer: self.create_tool_approval_llm_reviewer(&event, current_user_request),
        })
        .await
        .map_err(handler_error_to_agent_error)?;
        Ok(result.map(tool_call_result_to_before))
    }

    async fn review_auto_approved_tool_call(
        &self,
        event: &ToolCallEvent,
        runner: &ExtensionRunner,
        approval_required: bool,
    ) -> Result<Option<crate::core::extensions::types::ToolCallEventResult>, crate::core::extensions::types::HandlerError>
    {
        if !approval_required || !runner.has_approval_reviewers() {
            return Ok(None);
        }
        let mut reviewed = event.clone();
        match runner.emit_approval_reviewers(&mut reviewed).await? {
            Some(crate::core::extensions::types::ApprovalReviewerResult::Deny { reason }) => {
                Ok(Some(crate::core::extensions::types::ToolCallEventResult {
                    block: Some(true),
                    input: None,
                    reason: Some(reason.into()),
                    terminate: None,
                }))
            }
            _ => Ok(None),
        }
    }

    fn resolve_permission_prompt_tool(&self) -> Option<String> {
        if let Some(tool) = &self.inner.permission_prompt_tool {
            return Some(tool.clone());
        }
        let discovered: Vec<String> = lock(&self.inner.tool_registry)
            .iter()
            .filter(|(name, tool)| is_permission_prompt_protocol_tool(name, tool.parameters()))
            .map(|(name, _)| name.clone())
            .collect();
        (discovered.len() == 1).then(|| discovered[0].clone())
    }

    fn create_tool_approval_hook_reviewer(
        &self,
        event: &ToolCallEvent,
        runner: &ExtensionRunner,
        args: Arc<Mutex<JsValue>>,
    ) -> Option<crate::core::permissions::orchestrator::ApprovalReviewer> {
        let permission_prompt_tool = self.resolve_permission_prompt_tool();
        if !runner.has_approval_reviewers() && permission_prompt_tool.is_none() && !runner.has_handlers("tool_call") {
            return None;
        }
        let session = self.clone();
        let runner = runner.clone();
        let event = event.clone();
        Some(Arc::new(move || {
            let session = session.clone();
            let runner = runner.clone();
            let event = event.clone();
            let args = Arc::clone(&args);
            let permission_prompt_tool = permission_prompt_tool.clone();
            Box::pin(async move {
                if let Some(result) = session.review_registered_approval_reviewers(&event, &runner).await? {
                    return Ok(Some(result));
                }
                let tool_inner = Arc::downgrade(&session.inner);
                let call_tool: crate::core::permissions::mcp_permission_prompt::PermissionPromptCaller =
                    Arc::new(move |tool_name, input| {
                        let tool_inner = tool_inner.clone();
                        Box::pin(async move {
                            let Some(inner) = tool_inner.upgrade() else {
                                return Err(JsString::from("Session is disposed"));
                            };
                            let tool = lock(&inner.tool_registry).get(tool_name).cloned().ok_or_else(|| {
                                JsString::from(format!("Permission prompt tool not found: {tool_name}"))
                            })?;
                            let tool_use_id = input.tool_use_id.clone();
                            let params = pi_js_compat::js_json::to_value(&input)
                                .map_err(|error| JsString::from(error.to_string()))?;
                            let result = tool
                                .execute(
                                    format!("permission-prompt:{tool_use_id}"),
                                    params.into(),
                                    None,
                                    Arc::new(|_| Ok(())),
                                    None,
                                )
                                .await
                                .map_err(|error| error.message)?;
                            pi_js_compat::js_json::to_value(&result).map_err(|error| JsString::from(error.to_string()))
                        })
                    });
                let cwd = session.inner.cwd.clone();
                let handler = crate::core::permissions::mcp_permission_prompt::create_permission_prompt_handler(
                    crate::core::permissions::mcp_permission_prompt::PermissionPromptHandlerOptions {
                        permission_prompt_tool,
                        cwd,
                        call_tool,
                        desktop_notifier: None,
                        rule_store: Some(Arc::clone(&session.inner.permission_rule_store)),
                    },
                );
                let mut reviewed = event.clone();
                if let Some(result) = handler(&mut reviewed).await {
                    write_back_tool_input(&args, &reviewed.input);
                    return Ok(Some(result));
                }
                write_back_tool_input(&args, &reviewed.input);
                if !runner.has_handlers("tool_call") {
                    return Ok(None);
                }
                let result = runner.emit_tool_call(&mut reviewed).await;
                write_back_tool_input(&args, &reviewed.input);
                result
            })
        }))
    }

    async fn review_registered_approval_reviewers(
        &self,
        event: &ToolCallEvent,
        runner: &ExtensionRunner,
    ) -> Result<Option<crate::core::extensions::types::ToolCallEventResult>, crate::core::extensions::types::HandlerError>
    {
        if !runner.has_approval_reviewers() {
            return Ok(None);
        }
        let mut reviewed = event.clone();
        let Some(result) = runner.emit_approval_reviewers(&mut reviewed).await? else {
            return Ok(None);
        };
        match result {
            crate::core::extensions::types::ApprovalReviewerResult::Allow { .. } => {
                Ok(Some(crate::core::extensions::types::ToolCallEventResult {
                    block: Some(false),
                    input: None,
                    reason: None,
                    terminate: None,
                }))
            }
            crate::core::extensions::types::ApprovalReviewerResult::Deny { reason } => {
                Ok(Some(crate::core::extensions::types::ToolCallEventResult {
                    block: Some(true),
                    input: None,
                    reason: Some(reason.into()),
                    terminate: None,
                }))
            }
            crate::core::extensions::types::ApprovalReviewerResult::Ask { reason } => {
                let human = self.create_tool_approval_human_reviewer(event, runner, reason.clone(), false);
                match human {
                    Some(human) => (human)().await,
                    None => Ok(Some(crate::core::extensions::types::ToolCallEventResult {
                        block: Some(true),
                        input: None,
                        reason: Some(reason.unwrap_or_else(|| "Approval required".into())),
                        terminate: None,
                    })),
                }
            }
        }
    }

    fn create_tool_approval_human_reviewer(
        &self,
        event: &ToolCallEvent,
        runner: &ExtensionRunner,
        reason: Option<JsString>,
        force: bool,
    ) -> Option<crate::core::permissions::orchestrator::ApprovalReviewer> {
        let approval_preset = lock(&self.inner.settings_manager).get_approval_preset();
        if (!force
            && approval_preset != ApprovalPresetName::AskMe
            && approval_preset != ApprovalPresetName::LlmApprovedAsk)
            || !runner.has_ui()
        {
            return None;
        }
        let event = event.clone();
        let ui = runner.get_ui_context();
        let session = self.clone();
        Some(Arc::new(move || {
            let event = event.clone();
            let ui = Arc::clone(&ui);
            let reason = reason.clone();
            let session = session.clone();
            Box::pin(async move {
                let reason_text = reason
                    .as_ref()
                    .map(|reason| JsString::from("\nReason: ") + reason)
                    .unwrap_or_default();
                let input = pi_js_compat::js_json::stringify_lossless_pretty(&JsValue::Object(event.input.clone()));
                let title =
                    JsString::from(format!("Approve {}?", event.tool_name.as_str())) + reason_text + "\n" + input;
                let selection = ui
                    .select(
                        &title,
                        &["Allow once".into(), "Allow always".into(), "Deny".into()],
                        None,
                    )
                    .await
                    .map_err(crate::core::extensions::types::HandlerError::from)?;
                match selection.as_ref() {
                    Some(choice) if choice == "Allow once" => Ok(None),
                    Some(choice) if choice == "Allow always" => {
                        session.spawn_allow_always_agent(&event);
                        Ok(None)
                    }
                    _ => Ok(Some(crate::core::extensions::types::ToolCallEventResult {
                        block: Some(true),
                        input: None,
                        reason: Some("Tool call rejected by user".into()),
                        terminate: None,
                    })),
                }
            })
        }))
    }

    fn is_read_only_auto_approved(&self, event: &ToolCallEvent) -> bool {
        let approval_kind = self
            .find_tool_definition(event.tool_name.as_str())
            .and_then(|entry| entry.definition.approval_kind);
        approval_kind == Some(crate::core::extensions::types::ApprovalKind::ReadOnly)
            && SUPERVISOR_AUTO_APPROVED_READ_ONLY_TOOLS.contains(&event.tool_name.as_str())
    }

    fn spawn_allow_always_agent(&self, event: &ToolCallEvent) {
        let Some(tool) = lock(&self.inner.tool_registry).get("spawn_agent").cloned() else {
            return;
        };
        let params = serde_json::json!({
            "agentType": "permission-config",
            "displayName": "Allow command",
            "prompt": build_allow_always_agent_prompt(&self.inner.cwd, &event.input, event.tool_name.as_str()),
        });
        let tool_call_id = format!("allow-always:{}", event.tool_call_id);
        tokio::spawn(async move {
            let result = tool
                .execute(tool_call_id, params.into(), None, Arc::new(|_| Ok(())), None)
                .await;
            if let Err(error) = result {
                eprintln!("Failed to spawn allow-always agent: {error}");
            }
        });
    }

    fn create_tool_approval_llm_reviewer(
        &self,
        event: &ToolCallEvent,
        current_user_request: Option<JsString>,
    ) -> Option<crate::core::permissions::orchestrator::ApprovalReviewer> {
        let approval_preset = lock(&self.inner.settings_manager).get_approval_preset();
        if approval_preset != ApprovalPresetName::LlmApprovedDeny
            && approval_preset != ApprovalPresetName::LlmApprovedAsk
        {
            return None;
        }
        if self.is_read_only_auto_approved(event) {
            return Some(Arc::new(|| Box::pin(async { Ok(None) })));
        }
        if self.inner.no_supervisor {
            return Some(Arc::new(|| {
                Box::pin(async {
                    Ok(Some(crate::core::extensions::types::ToolCallEventResult {
                        block: Some(true),
                        reason: Some("Supervisor approval is unavailable in standalone worker mode".into()),
                        ..Default::default()
                    }))
                })
            }));
        }
        let session = self.clone();
        let event = event.clone();
        Some(Arc::new(move || {
            let session = session.clone();
            let event = event.clone();
            let current_user_request = current_user_request.clone();
            Box::pin(async move {
                let decision_session = session.clone();
                let decision_event = event.clone();
                let request_decision: crate::supervisor::approval_reviewer::SupervisorApprovalCall =
                    Arc::new(move || {
                        let session = decision_session.clone();
                        let event = decision_event.clone();
                        let current_user_request = current_user_request.clone();
                        Box::pin(async move {
                            let control_db_path = session.inner.control_db_path.clone().unwrap_or_default();
                            let kb_dir =
                                JsString::from(pi_js_compat::process_env::get("PI_KB_DIR").unwrap_or_else(|| {
                                    crate::supervisor::project_resolver::DEFAULT_SUPERVISOR_KB_DIR.to_owned()
                                }));
                            let project_id = crate::supervisor::project_resolver::resolve_supervisor_project_for_cwd(
                                &session.inner.cwd,
                                &kb_dir,
                            )?;
                            let active_goal = match lock(&session.inner.session_manager).get_session_goal_json() {
                                Ok(goal) => goal,
                                Err(error) => {
                                    // The source throws here, aborting the tool call.
                                    return Err(error.message().clone());
                                }
                            };
                            let mut payload = JsObject::new();
                            payload.insert(
                                "activeGoal".into(),
                                active_goal
                                    .map(|goal| JsValue::String(goal.into()))
                                    .unwrap_or(JsValue::Null),
                            );
                            payload.insert(
                                "currentUserRequest".into(),
                                current_user_request.map(JsValue::String).unwrap_or(JsValue::Null),
                            );
                            payload.insert("input".into(), JsValue::Object(event.input.clone()));
                            payload.insert("preset".into(), JsValue::String(approval_preset.as_str().into()));
                            payload.insert("toolCallId".into(), JsValue::String(event.tool_call_id.clone().into()));
                            payload.insert("toolName".into(), JsValue::String(event.tool_name.as_str().into()));
                            let request = crate::supervisor::client::RequestSupervisorDecisionInput {
                                control_db_path,
                                kind: crate::core::session_control_db::SupervisorRequestKind::ApprovalReview,
                                payload,
                                project_id,
                                sender_session_id: session.session_id(),
                                timeout_ms: 30_000.0,
                                signal: None,
                                max_attempts: None,
                                poll_interval_ms: None,
                                retry_delay_ms: None,
                                retry_jitter_ratio: None,
                            };
                            let requester = Arc::clone(&session.inner.supervisor_decision_requester);
                            requester(request).await
                        })
                    });
                let human_session = session.clone();
                let human_event = event.clone();
                let ask_human: Option<crate::supervisor::approval_reviewer::SupervisorHumanReviewer> =
                    if human_session.runner().has_ui() {
                        Some(Arc::new(move |reason: JsString| {
                            let human_session = human_session.clone();
                            let human_event = human_event.clone();
                            Box::pin(async move {
                                let Some(reviewer) = human_session.create_tool_approval_human_reviewer(
                                    &human_event,
                                    &human_session.runner(),
                                    Some(reason),
                                    true,
                                ) else {
                                    return Ok(None);
                                };
                                (reviewer)().await.map_err(|error| error.message)
                            })
                        }))
                    } else {
                        None
                    };
                crate::supervisor::approval_reviewer::review_tool_call_with_supervisor(
                    request_decision,
                    ask_human,
                    approval_preset == ApprovalPresetName::LlmApprovedAsk,
                )
                .await
                .map_err(crate::core::extensions::types::HandlerError::new)
            })
        }))
    }

    async fn after_tool_call(&self, context: AfterToolCallContext) -> Option<AfterToolCallResult> {
        let runner = self.runner();
        let hook_result = if runner.has_handlers("tool_result") {
            let event = crate::core::extensions::types::ToolResultEvent {
                tool_name: crate::core::extensions::types::ToolName::Custom(context.tool_call.name.clone()),
                tool_call_id: context.tool_call.id.clone(),
                input: context.args.as_object().cloned().unwrap_or_default(),
                content: context.result.content.clone(),
                is_error: context.is_error,
                usage: None,
                details: context.result.details.clone(),
            };
            runner.emit_tool_result(event).await
        } else {
            None
        };
        let hook_result = hook_result?;
        Some(AfterToolCallResult {
            content: hook_result.content,
            details: hook_result.details,
            is_error: hook_result.is_error.or(Some(context.is_error)),
            terminate: None,
        })
    }

    fn find_tool_definition(&self, name: &str) -> Option<ToolDefinitionEntry> {
        lock(&self.inner.tool_definitions)
            .iter()
            .find(|(tool_name, _)| tool_name == name)
            .map(|(_, entry)| entry.clone())
    }

    /// Parent runtime session ID for supervisor-directed messages.
    pub fn multi_agent_parent_session_id(&self) -> Option<JsString> {
        self.inner.multi_agent_parent_session_id.clone()
    }

    /// Whether inbound runtime coordination is disabled.
    pub fn runtime_coordination_inbound_disabled(&self) -> bool {
        self.inner.disable_runtime_coordination_inbound
    }

    /// Callback fired after a shared-channel message is delivered.
    pub fn on_shared_channel_message_delivered(&self) -> Option<Arc<dyn Fn(JsString) + Send + Sync>> {
        self.inner.on_shared_channel_message_delivered.clone()
    }

    fn runtime_mailbox_control_db_path(&self) -> Option<JsString> {
        lock(&self.inner.extension_control_db_path).clone().or_else(|| {
            lock(&self.inner.session_manager)
                .get_metadata_control_db_path()
                .cloned()
        })
    }

    /// Builds the detached-job lifecycle controller from the multi-agent store.
    pub fn get_detached_job_lifecycle_controller(
        &self,
    ) -> Option<Arc<crate::core::detached_job_runner::DetachedJobLifecycleController>> {
        use crate::core::detached_job_runner::{DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions};
        use crate::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};

        let store = self.inner.multi_agent_store.as_ref()?;
        let persistence = store.get_persistence_target()?;
        let control_db_path = self.runtime_mailbox_control_db_path()?;
        let id_store = Arc::clone(store);
        // TS parses this when the session is constructed and lets a failure throw.
        let instance_id = crate::core::session_control_db::get_runtime_process_instance_id()
            .expect("runtime process identity must be readable");
        let process_identity: crate::core::runtime_process::ProcessIdentity =
            serde_json::from_str(instance_id).expect("runtime process identity must be valid JSON");
        let coordinator = Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
            control_db_path: control_db_path.clone(),
            create_agent_id: Arc::new(move || {
                id_store
                    .allocate_agent_id_for_lifecycle_coordinator("agent")
                    .map_err(|error| crate::core::sqlite::ControlDbError::new(error.message))
            }),
            now: Arc::new(crate::utils::iso_time::now_iso_string),
            process_identity,
            session_path: persistence.session_path.clone(),
        }));
        Some(Arc::new(DetachedJobLifecycleController::new(
            DetachedJobLifecycleControllerOptions {
                artifact_root: self.inner.agent_dir.clone(),
                control_db_path,
                coordinator,
                owner_agent_id: self.inner.multi_agent_agent_id.clone(),
                owner_session_id: self.session_id(),
                session_path: persistence.session_path,
                store: Arc::clone(store),
                write_bash_launch_manifest: None,
            },
        )))
    }

    /// Bound detached-job lifecycle controller, when a multi-agent store is configured.
    pub fn detached_job_lifecycle(
        &self,
    ) -> Option<Arc<crate::core::detached_job_runner::DetachedJobLifecycleController>> {
        self.get_detached_job_lifecycle_controller()
    }

    /// Delivers terminal-outbox projections to the multi-agent store.
    pub fn drain_terminal_outbox_projections(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        use crate::core::terminal_outbox_delivery::{
            DeliverTerminalOutboxOptions, deliver_terminal_outbox_projections, is_terminal_outbox_cleanup_due,
            terminal_outbox_retention_threshold,
        };

        let Some(control_db_path) = self.runtime_mailbox_control_db_path() else {
            return Ok(());
        };
        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return Ok(());
        };
        let now = now_millis();
        let last_cleanup = *lock(&self.inner.terminal_outbox_last_cleanup_at);
        // Frozen agent-session.ts:3503-3519 propagates all synchronous throws.
        if is_terminal_outbox_cleanup_due(last_cleanup.map(|time| time as f64), now as f64) {
            let threshold = terminal_outbox_retention_threshold(now as f64)
                .map_err(|error| pi_js_compat::callback_error::CallbackError::from(*error))?;
            crate::core::session_control_db::cleanup_multi_agent_terminal_outbox(&control_db_path, &threshold)?;
            *lock(&self.inner.terminal_outbox_last_cleanup_at) = Some(now);
        }
        deliver_terminal_outbox_projections(&DeliverTerminalOutboxOptions {
            artifact_root: Some(self.inner.agent_dir.clone()),
            claim_id: self.inner.terminal_outbox_claim_id.clone().into(),
            control_db_path,
            now: Arc::new(|| Ok(crate::utils::iso_time::now_iso_string().into())),
            store: Arc::clone(store),
        })?;
        Ok(())
    }

    /// Drains runtime mailbox and shared-channel messages (TS `drainRuntimeCoordination`).
    pub async fn drain_runtime_coordination(&self) -> Result<(), AgentSessionError> {
        self.drain_runtime_coordination_messages(DrainCoordinationOptions {
            checkpoint: None,
            include_next_model_call: false,
            trigger_if_idle: true,
        })
        .await
        .map(|_| ())
    }

    async fn drain_runtime_coordination_messages(
        &self,
        options: DrainCoordinationOptions,
    ) -> Result<bool, AgentSessionError> {
        if self.inner.disable_runtime_coordination_inbound || self.inner.disposed.load(Ordering::SeqCst) {
            return Ok(false);
        }
        if let Some(store) = &self.inner.multi_agent_store {
            store
                .expire_mailbox_messages(now_millis() as f64)
                .map_err(|cause| error(cause.message))?;
        }
        // Frozen agent-session.ts:3492-3495 catches and logs this drain only.
        if let Err(error) = self.drain_terminal_outbox_projections() {
            let message = error.error_or_string_message()?;
            eprintln!(
                "Failed to drain terminal outbox projections: {}",
                encode_node_utf8(&message)
            );
        }
        let mailbox_queued = self.drain_runtime_mailbox_messages(options).await?;
        let channel_queued = if options.checkpoint == Some(SteeringCheckpoint::AfterToolResult) {
            false
        } else {
            self.drain_shared_channel_messages(options).await?
        };
        Ok(mailbox_queued || channel_queued)
    }

    fn runtime_mailbox_recipient(&self) -> crate::core::session_control_db::RuntimeMailboxAddress {
        crate::core::session_control_db::RuntimeMailboxAddress {
            agent_id: self.inner.multi_agent_agent_id.clone(),
            session_id: self.session_id().into(),
        }
    }

    async fn drain_runtime_mailbox_messages(
        &self,
        options: DrainCoordinationOptions,
    ) -> Result<bool, AgentSessionError> {
        let can_steer_active_turn = self.is_streaming() && options.checkpoint.is_some();
        if self.is_streaming() {
            if !can_steer_active_turn {
                return Ok(false);
            }
        } else if !options.trigger_if_idle {
            return Ok(false);
        }
        let control_db_path = match self.runtime_mailbox_control_db_path() {
            Some(path) => path,
            None => return Ok(false),
        };
        let drain_guard = self.inner.runtime_mailbox_drain_lock.lock().await;
        if can_steer_active_turn {
            return self
                .deliver_ready_runtime_mailbox_messages(
                    &control_db_path,
                    options,
                    MailboxDeliveryMode::Steer,
                    None,
                    drain_guard,
                )
                .await;
        }
        let guard = self.inner.turn_start_lock.lock().await;
        self.deliver_ready_runtime_mailbox_messages(
            &control_db_path,
            options,
            MailboxDeliveryMode::Prompt,
            Some(guard),
            drain_guard,
        )
        .await
    }

    async fn deliver_ready_runtime_mailbox_messages(
        &self,
        control_db_path: &JsString,
        options: DrainCoordinationOptions,
        mode: MailboxDeliveryMode,
        guard: Option<tokio::sync::MutexGuard<'_, ()>>,
        drain_guard: tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<bool, AgentSessionError> {
        if mode == MailboxDeliveryMode::Prompt {
            let model = self.inner.agent.model();
            if !self.inner.model_registry.has_configured_auth(&model)? {
                return Ok(false);
            }
        }
        let recipient = self.runtime_mailbox_recipient();
        let listener = crate::core::session_control_db::read_runtime_mailbox_listener(control_db_path, &recipient)
            .map_err(|cause| AgentSessionError::new(cause.message()))?;
        if listener
            .as_ref()
            .is_none_or(|listener| listener.pid != std::process::id())
        {
            return Ok(false);
        }
        let mut messages = crate::core::session_control_db::take_runtime_mailbox_messages_for_delivery(
            control_db_path,
            &recipient,
            |message| is_runtime_mailbox_message_due(message, options),
            i64::MAX as usize,
        )
        .map_err(|cause| AgentSessionError::new(cause.message()))?;
        if options.include_next_model_call {
            messages.sort_by_key(runtime_mailbox_checkpoint_priority);
        }
        let refs = messages
            .iter()
            .map(crate::core::runtime_mailbox_acceptance::RuntimeMailboxAcceptanceRef::from)
            .collect::<Vec<_>>();
        let result = self
            .deliver_claimed_runtime_mailbox_messages(messages, &recipient.session_id, mode, guard, drain_guard)
            .await;
        crate::core::runtime_mailbox_acceptance::release_unaccepted_runtime_mailbox_messages(
            &lock(&self.inner.session_manager),
            control_db_path,
            &refs,
        )
        .map_err(|cause| error(cause.message()))?;
        result
    }

    async fn deliver_claimed_runtime_mailbox_messages(
        &self,
        messages: Vec<RuntimeMailboxMessage>,
        recipient_session_id: &JsString,
        mode: MailboxDeliveryMode,
        guard: Option<tokio::sync::MutexGuard<'_, ()>>,
        drain_guard: tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<bool, AgentSessionError> {
        let prompt_messages = self.accept_intercepted_mailbox_messages(messages).await?;
        if prompt_messages.is_empty() {
            return Ok(false);
        }
        let formatted = prompt_messages
            .iter()
            .map(|message| {
                crate::core::runtime_coordination_format::format_runtime_mailbox_prompt(message, recipient_session_id)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        let prompt = JsString::join(formatted, "\n\n");
        if mode == MailboxDeliveryMode::Steer {
            let message = runtime_mailbox_user_message(&prompt, &[], Some(InputSource::Extension));
            self.steer_agent(message.clone());
            self.accept_runtime_mailbox_messages(&prompt_messages, Some(&message))?;
            return Ok(true);
        }
        self.prompt_mailbox_messages(&prompt, &prompt_messages, guard, drain_guard)
            .await
    }

    async fn accept_intercepted_mailbox_messages(
        &self,
        messages: Vec<RuntimeMailboxMessage>,
    ) -> Result<Vec<RuntimeMailboxMessage>, AgentSessionError> {
        let mut prompt_messages = Vec::new();
        for message in messages {
            if self.intercept_runtime_mailbox_message(&message).await? {
                self.accept_runtime_mailbox_messages(std::slice::from_ref(&message), None)?;
            } else {
                prompt_messages.push(message);
            }
        }
        Ok(prompt_messages)
    }

    async fn prompt_mailbox_messages(
        &self,
        prompt: &JsString,
        messages: &[RuntimeMailboxMessage],
        guard: Option<tokio::sync::MutexGuard<'_, ()>>,
        drain_guard: tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<bool, AgentSessionError> {
        let guard = guard.expect("prompt delivery holds the turn start lock");
        // Frozen releases the mailbox drain before _promptTurn: its tool-result
        // checkpoint must be able to drain again while this prompt is running.
        drop(drain_guard);
        let result = Box::pin(self.prompt_turn(
            &JsString::from(prompt),
            PromptOptions {
                expand_prompt_templates: Some(false),
                source: Some(InputSource::Extension),
                ..Default::default()
            },
            guard,
            Some(messages),
        ))
        .await;
        self.complete_runtime_mailbox_steering_turn(&self.messages())?;
        result.map(|_| false)
    }

    fn record_detached_tool_call_completion(&self, message: &RuntimeMailboxMessage) {
        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return;
        };
        let Some(sender_id) = message.sender.agent_id.as_ref() else {
            return;
        };
        let Some(data) = build_detached_tool_call_completion_entry(store.get_agent(sender_id).as_ref()) else {
            return;
        };
        let already_recorded = lock(&self.inner.session_manager).get_entries().iter().any(|entry| {
            if entry.entry_type() != "custom" {
                return false;
            }
            let known = entry.known();
            let Some(crate::core::session_manager::KnownSessionEntry::Custom(custom)) = known.as_deref() else {
                return false;
            };
            if custom.custom_type != DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE {
                return false;
            }
            let Some(existing) = custom.data.as_ref() else {
                return false;
            };
            existing.get("agentId").and_then(JsValue::as_string) == Some(&JsString::from(&data.agent_id))
                && existing.get("toolCallId").and_then(JsValue::as_string) == Some(&JsString::from(&data.tool_call_id))
        });
        if already_recorded {
            return;
        }
        if let Err(error) = lock(&self.inner.session_manager).append_custom_entry(
            &DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE.into(),
            Some(pi_js_compat::js_json::to_value(&data).expect("mailbox delivery serializes")),
        ) {
            eprintln!("Failed to record detached tool call completion: {error}");
        }
    }

    async fn intercept_runtime_mailbox_message(
        &self,
        message: &RuntimeMailboxMessage,
    ) -> Result<bool, AgentSessionError> {
        let runner = self.runner();
        if !runner.has_handlers("runtime_mailbox") {
            return Ok(false);
        }
        match runner
            .emit_runtime_mailbox(crate::core::extensions::types::RuntimeMailboxEvent {
                message: message.clone(),
            })
            .await
        {
            Ok(result) => Ok(result.handled),
            Err(cause) => Err(error(cause.message)),
        }
    }

    fn accept_runtime_mailbox_messages(
        &self,
        messages: &[RuntimeMailboxMessage],
        accepted: Option<&AgentMessage>,
    ) -> Result<(), AgentSessionError> {
        let control_db_path = self
            .runtime_mailbox_control_db_path()
            .ok_or_else(|| error("Runtime mailbox control database is unavailable"))?;
        let refs = messages
            .iter()
            .map(crate::core::runtime_mailbox_acceptance::RuntimeMailboxAcceptanceRef::from)
            .collect::<Vec<_>>();
        {
            let mut manager = lock(&self.inner.session_manager);
            lock(&self.inner.runtime_mailbox_acceptance)
                .accept_messages(&mut manager, &control_db_path, &refs, accepted)
                .map_err(|cause| error(cause.message()))?;
        }
        for message in messages {
            self.mark_store_mailbox_message_delivered(message)?;
            self.record_detached_tool_call_completion(message);
        }
        Ok(())
    }

    fn mark_store_mailbox_message_delivered(&self, message: &RuntimeMailboxMessage) -> Result<(), AgentSessionError> {
        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return Ok(());
        };
        let Some(persistence) = store
            .get_persistence_target()
            .filter(|target| message.store_ref.session_path == target.session_path)
        else {
            return Ok(());
        };
        store
            .expire_mailbox_messages(now_millis() as f64)
            .map_err(|cause| error(cause.message))?;
        if message.kind == crate::core::session_control_db::RuntimeMailboxMessageKind::Steer {
            self.mark_store_steering_delivered(message, &persistence)?;
        }
        Ok(())
    }

    /// Claims already advanced steering lifecycle; acceptance refreshes its projection.
    fn mark_store_steering_delivered(
        &self,
        message: &RuntimeMailboxMessage,
        persistence: &crate::core::multi_agent_store::PersistenceTarget,
    ) -> Result<(), AgentSessionError> {
        let Some(agent_id) = message.recipient.agent_id.as_ref() else {
            return Ok(());
        };
        let Some(store) = &self.inner.multi_agent_store else {
            return Ok(());
        };
        let agent = crate::core::session_control_db::read_multi_agent_agent(
            &persistence.control_db_path,
            &persistence.session_path,
            agent_id,
        )
        .map_err(|cause| error(cause.message()))?;
        let Some(agent) = agent else {
            return Ok(());
        };
        store
            .publish_lifecycle_coordinator_snapshot(&agent)
            .map_err(|cause| error(cause.message))?;
        let mut steering = lock(&self.inner.runtime_mailbox_steering_agent_ids);
        if !steering.iter().any(|tracked| tracked == &agent.id) {
            steering.push(agent.id.clone());
        }
        Ok(())
    }

    /// `_completeRuntimeMailboxSteeringTurn`.
    fn complete_runtime_mailbox_steering_turn(&self, messages: &[AgentMessage]) -> Result<(), CallbackError> {
        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return Ok(());
        };
        let agent_ids = lock(&self.inner.runtime_mailbox_steering_agent_ids).clone();
        if agent_ids.is_empty() {
            return Ok(());
        }
        let lifecycle = runtime_mailbox_steering_lifecycle(messages);
        let summary = (lifecycle == AgentLifecycle::Completed)
            .then(|| last_runtime_mailbox_assistant_text(messages))
            .flatten();
        for agent_id in &agent_ids {
            if self.complete_runtime_mailbox_steered_agent(agent_id, lifecycle, summary.as_ref())? {
                continue;
            }
            if !is_terminal_multi_agent_lifecycle(lifecycle) {
                continue;
            }
            let has_active_descendants = store
                .list_descendants(agent_id)
                .iter()
                .any(|agent| !is_terminal_multi_agent_lifecycle(agent.lifecycle));
            if has_active_descendants {
                self.defer_runtime_mailbox_steered_terminal(agent_id, lifecycle, summary.clone());
            }
        }
        lock(&self.inner.runtime_mailbox_steering_agent_ids).clear();
        Ok(())
    }

    fn complete_runtime_mailbox_steered_agent(
        &self,
        agent_id: &JsString,
        lifecycle: AgentLifecycle,
        summary: Option<&JsString>,
    ) -> Result<bool, CallbackError> {
        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return Ok(false);
        };
        let Some(current) = store.get_agent(agent_id) else {
            return Ok(true);
        };
        if is_terminal_multi_agent_lifecycle(current.lifecycle) {
            return Ok(true);
        }
        if !is_terminal_multi_agent_lifecycle(lifecycle) {
            return Ok(true);
        }
        let result = summary.map(|summary| crate::core::multi_agent_store::AgentResult {
            summary: Some(summary.to_owned()),
            file_refs: None,
            duration_ms: None,
            tool_call_id: None,
        });
        let error = (lifecycle == AgentLifecycle::Failed).then(|| crate::core::multi_agent_store::AgentError {
            message: "Runtime mailbox steering turn failed".into(),
            code: None,
        });
        self.finalize_reserved_multi_agent(&current, lifecycle_name(lifecycle), error.as_ref(), result.as_ref())
    }

    fn defer_runtime_mailbox_steered_terminal(
        &self,
        agent_id: &JsString,
        lifecycle: AgentLifecycle,
        summary: Option<JsString>,
    ) {
        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return;
        };
        lock(&self.inner.runtime_mailbox_pending_terminal)
            .insert(agent_id.to_owned(), PendingSteeringTerminal { lifecycle, summary });
        if lock(&self.inner.runtime_mailbox_pending_terminal_unsubscribe).is_some() {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        let subscription = store.subscribe_agent_updates(move |_previous, _current| {
            if let Some(inner) = weak.upgrade() {
                AgentSession { inner }.retry_runtime_mailbox_steered_terminals()?;
            }
            Ok(())
        });
        *lock(&self.inner.runtime_mailbox_pending_terminal_unsubscribe) = Some(subscription);
    }

    fn retry_runtime_mailbox_steered_terminals(&self) -> Result<(), CallbackError> {
        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return Ok(());
        };
        let pending: Vec<(JsString, PendingSteeringTerminal)> = lock(&self.inner.runtime_mailbox_pending_terminal)
            .iter()
            .map(|(agent_id, pending)| (agent_id.clone(), pending.clone()))
            .collect();
        for (agent_id, pending) in pending {
            let has_active_descendants = store
                .list_descendants(&agent_id)
                .iter()
                .any(|agent| !is_terminal_multi_agent_lifecycle(agent.lifecycle));
            if has_active_descendants {
                continue;
            }
            lock(&self.inner.runtime_mailbox_pending_terminal).shift_remove(&agent_id);
            if !self.complete_runtime_mailbox_steered_agent(&agent_id, pending.lifecycle, pending.summary.as_ref())? {
                lock(&self.inner.runtime_mailbox_pending_terminal).insert(agent_id, pending);
            }
        }
        if !lock(&self.inner.runtime_mailbox_pending_terminal).is_empty() {
            return Ok(());
        }
        *lock(&self.inner.runtime_mailbox_pending_terminal_unsubscribe) = None;
        Ok(())
    }

    /// `_finalizeReservedMultiAgent`.
    fn finalize_reserved_multi_agent(
        &self,
        agent: &crate::core::multi_agent_store::AgentSnapshot,
        terminal_lifecycle: &str,
        error: Option<&crate::core::multi_agent_store::AgentError>,
        result: Option<&crate::core::multi_agent_store::AgentResult>,
    ) -> Result<bool, CallbackError> {
        use crate::core::lifecycle_coordinator::{FinalizeChildCommandInput, LifecycleCommandResult};

        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return Ok(false);
        };
        let Some(persistence) = store.get_persistence_target() else {
            return Ok(false);
        };
        let ownership = crate::core::session_control_db::read_multi_agent_runtime_ownership(
            &persistence.control_db_path,
            &persistence.session_path,
            &agent.id,
        )?;
        let Some(ownership) = ownership else {
            return Ok(false);
        };
        let coordinator = crate::core::lifecycle_coordinator::LifecycleCoordinator::new(
            crate::core::lifecycle_coordinator::LifecycleCoordinatorOptions {
                control_db_path: persistence.control_db_path.clone(),
                create_agent_id: Arc::new({
                    let store = Arc::clone(store);
                    move || {
                        store
                            .allocate_agent_id_for_lifecycle_coordinator("agent")
                            .map_err(|error| crate::core::sqlite::ControlDbError::new(error.message))
                    }
                }),
                now: Arc::new(crate::utils::iso_time::now_iso_string),
                process_identity: crate::core::runtime_process_identity::runtime_process_identity().clone(),
                session_path: persistence.session_path.clone(),
            },
        );
        let finalized = coordinator.finalize_child(&FinalizeChildCommandInput {
            agent: agent.clone(),
            ownership,
            error: error.cloned(),
            result: result.cloned(),
            terminal_lifecycle: terminal_lifecycle.to_owned(),
        })?;
        if !matches!(finalized, LifecycleCommandResult::Ok { .. }) {
            return Ok(false);
        }
        self.drain_terminal_outbox_projections()?;
        Ok(true)
    }

    fn start_runtime_mailbox_polling(&self) {
        if self.inner.disable_runtime_coordination_inbound {
            return;
        }
        let Some(control_db_path) = self.runtime_mailbox_control_db_path() else {
            return;
        };
        if lock(&self.inner.runtime_mailbox_poll_token).is_some()
            && lock(&self.inner.runtime_mailbox_control_db_path).as_ref() == Some(&control_db_path)
        {
            return;
        }
        self.stop_runtime_mailbox_polling();
        *lock(&self.inner.terminal_outbox_last_cleanup_at) = None;
        *lock(&self.inner.runtime_mailbox_control_db_path) = Some(control_db_path.clone());
        let release = crate::core::session_control_db::retain_control_db_connection(&control_db_path)
            .expect("control db connection must be retainable");
        *lock(&self.inner.runtime_mailbox_control_db_release) = Some(release);
        let controller_token = AbortController::new();
        let token = controller_token.signal();
        *lock(&self.inner.runtime_mailbox_poll_token) = Some(controller_token.clone());
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval_at(
                tokio::time::Instant::now() + RUNTIME_MAILBOX_POLL_INTERVAL,
                RUNTIME_MAILBOX_POLL_INTERVAL,
            );
            loop {
                tokio::select! {
                    _ = token.wait() => break,
                    _ = interval.tick() => {
                        let Some(inner) = weak.upgrade() else { break };
                        let session = AgentSession { inner };
                        if let Err(error) = session
                            .drain_runtime_coordination_messages(DrainCoordinationOptions {
                                checkpoint: None,
                                include_next_model_call: false,
                                trigger_if_idle: true,
                            })
                            .await
                        {
                            eprintln!("Failed to drain runtime coordination messages: {error}");
                        }
                    }
                }
            }
        });
    }

    fn stop_runtime_mailbox_polling(&self) {
        if let Some(token) = lock(&self.inner.runtime_mailbox_poll_token).take() {
            token.abort(None);
        }
        lock(&self.inner.runtime_mailbox_control_db_release).take();
        *lock(&self.inner.runtime_mailbox_control_db_path) = None;
    }

    fn start_runtime_mailbox_signal_wake(&self) {
        if self.inner.disable_runtime_coordination_inbound {
            return;
        }
        let Some(control_db_path) = self.runtime_mailbox_control_db_path() else {
            return;
        };
        let agent_id = self.inner.multi_agent_agent_id.clone();
        self.register_runtime_mailbox_listeners(&control_db_path, agent_id.as_ref());
        let recipient = self.runtime_mailbox_recipient();
        if let Err(error) =
            crate::core::session_control_db::initialize_shared_channel_cursor_at_tail(&control_db_path, &recipient)
        {
            eprintln!("Failed to initialize shared channel cursor: {error}");
        }
        self.start_runtime_mailbox_heartbeat();
        self.start_runtime_mailbox_signal_handler();
    }

    fn register_runtime_mailbox_listeners(&self, control_db_path: &JsString, agent_id: Option<&JsString>) {
        let recipient = self.runtime_mailbox_recipient();
        let session_file = if agent_id.is_none() { self.session_file() } else { None };
        if let Err(error) = crate::core::session_control_db::register_runtime_mailbox_listener(
            control_db_path,
            &recipient,
            std::process::id(),
            session_file.as_ref(),
            Default::default(),
        ) {
            eprintln!("Failed to refresh runtime mailbox listeners: {error}");
        }
    }

    fn start_runtime_mailbox_heartbeat(&self) {
        if lock(&self.inner.runtime_mailbox_heartbeat_token).is_some() {
            return;
        }
        let controller_token = AbortController::new();
        let token = controller_token.signal();
        *lock(&self.inner.runtime_mailbox_heartbeat_token) = Some(controller_token.clone());
        let agent_id = self.inner.multi_agent_agent_id.clone();
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval_at(
                tokio::time::Instant::now() + RUNTIME_MAILBOX_HEARTBEAT_INTERVAL,
                RUNTIME_MAILBOX_HEARTBEAT_INTERVAL,
            );
            loop {
                tokio::select! {
                    _ = token.wait() => break,
                    _ = interval.tick() => {
                        let Some(inner) = weak.upgrade() else { break };
                        let session = AgentSession { inner };
                        let Some(control_db_path) = session.runtime_mailbox_control_db_path() else {
                            continue;
                        };
                        session.register_runtime_mailbox_listeners(&control_db_path, agent_id.as_ref());
                    }
                }
            }
        });
    }

    fn stop_runtime_mailbox_signal_wake(&self) {
        if let Some(token) = lock(&self.inner.runtime_mailbox_heartbeat_token).take() {
            token.abort(None);
        }
        self.retire_runtime_mailbox_listeners();
        if let Some(token) = lock(&self.inner.runtime_mailbox_signal_token).take() {
            token.abort(None);
        }
    }

    fn retire_runtime_mailbox_listeners(&self) {
        if self.inner.no_supervisor {
            return;
        }
        let Some(control_db_path) = self.runtime_mailbox_control_db_path() else {
            return;
        };
        let recipient = self.runtime_mailbox_recipient();
        if let Err(error) = crate::core::session_control_db::retire_runtime_mailbox_listener(
            &control_db_path,
            &recipient,
            std::process::id(),
        ) {
            eprintln!("Failed to retire runtime mailbox listeners: {error}");
        }
    }

    fn start_runtime_mailbox_signal_handler(&self) {
        #[cfg(unix)]
        {
            if lock(&self.inner.runtime_mailbox_signal_token).is_some() {
                return;
            }
            let mut signal = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined2()) {
                Ok(signal) => signal,
                Err(error) => {
                    eprintln!("Failed to install runtime mailbox signal handler: {error}");
                    return;
                }
            };
            crate::core::session_control_db::mark_runtime_mailbox_signal_keepalive_installed();
            let controller_token = AbortController::new();
            let token = controller_token.signal();
            *lock(&self.inner.runtime_mailbox_signal_token) = Some(controller_token.clone());
            let weak = Arc::downgrade(&self.inner);
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = token.wait() => break,
                        received = signal.recv() => {
                            if received.is_none() {
                                break;
                            }
                            let Some(inner) = weak.upgrade() else { break };
                            let session = AgentSession { inner };
                            if let Err(error) = session
                                .drain_runtime_coordination_messages(DrainCoordinationOptions {
                                    checkpoint: None,
                                    include_next_model_call: false,
                                    trigger_if_idle: true,
                                })
                                .await
                            {
                                eprintln!("Failed to drain runtime coordination messages: {error}");
                            }
                        }
                    }
                }
            });
        }
    }

    async fn drain_shared_channel_messages(
        &self,
        options: DrainCoordinationOptions,
    ) -> Result<bool, AgentSessionError> {
        if self.inner.disable_runtime_coordination_inbound || self.inner.multi_agent_agent_id.is_some() {
            return Ok(false);
        }
        if self.inner.shared_channel_drain_in_progress.swap(true, Ordering::SeqCst) {
            return Ok(false);
        }
        let result = self.drain_shared_channel_messages_inner(options).await;
        self.inner
            .shared_channel_drain_in_progress
            .store(false, Ordering::SeqCst);
        result
    }

    async fn drain_shared_channel_messages_inner(
        &self,
        options: DrainCoordinationOptions,
    ) -> Result<bool, AgentSessionError> {
        if options.trigger_if_idle && self.is_streaming() {
            return Ok(false);
        }
        let Some(control_db_path) = self.runtime_mailbox_control_db_path() else {
            return Ok(false);
        };
        let recipient = self.runtime_mailbox_recipient();
        let cursor =
            crate::core::session_control_db::initialize_shared_channel_cursor_at_tail(&control_db_path, &recipient)
                .map_err(|cause| AgentSessionError::new(cause.message()))?;
        let last_message_id = crate::core::session_control_db::read_shared_channel_tail(&control_db_path)
            .map_err(|cause| AgentSessionError::new(cause.message()))?;
        let messages = crate::core::session_control_db::list_shared_channel_messages_after(
            &control_db_path,
            cursor,
            i64::MAX,
            last_message_id,
        )
        .map_err(|cause| AgentSessionError::new(cause.message()))?;
        let deliverable: Vec<_> = messages
            .into_iter()
            .filter(|message| !is_own_shared_channel_message(message, &recipient))
            .filter(|message| !is_subagent_shared_channel_message(message))
            .collect();
        if deliverable.is_empty() {
            crate::core::session_control_db::advance_shared_channel_cursor(
                &control_db_path,
                &recipient,
                last_message_id,
            )
            .map_err(|cause| AgentSessionError::new(cause.message()))?;
            return Ok(false);
        }
        let prompt =
            crate::core::runtime_coordination_format::format_shared_channel_prompt(&deliverable, &recipient.session_id);
        let queued = self.send_shared_channel_prompt(&prompt, options).await?;
        crate::core::session_control_db::advance_shared_channel_cursor(&control_db_path, &recipient, last_message_id)
            .map_err(|cause| AgentSessionError::new(cause.message()))?;
        if let Some(handler) = self.on_shared_channel_message_delivered() {
            handler(prompt);
        }
        Ok(queued)
    }

    async fn send_shared_channel_prompt(
        &self,
        prompt: &JsString,
        options: DrainCoordinationOptions,
    ) -> Result<bool, AgentSessionError> {
        if options.trigger_if_idle && !self.is_streaming() {
            Box::pin(self.send_custom_message(
                &"shared_channel".into(),
                CustomMessageContent::Text(prompt.to_owned().into()),
                true,
                None,
                true,
                Some(StreamingBehavior::FollowUp),
            ))
            .await?;
            return Ok(false);
        }
        lock(&self.inner.follow_up_messages).push(QueuedMessagePreview {
            text: prompt.clone(),
            input_source: None,
            custom_type: Some("shared_channel".into()),
        });
        self.emit_queue_update();
        self.inner.agent.follow_up(create_custom_message(
            &"shared_channel".into(),
            CustomMessageContent::Text(prompt.to_owned().into()),
            true,
            None,
            crate::utils::iso_time::now_iso_string(),
        ));
        Ok(true)
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
            steering: self.get_steering_messages(),
            follow_up: self.get_follow_up_messages(),
        }));
    }

    fn reset_duplicate_turn_guard(&self) {
        *lock(&self.inner.last_assistant_turn_fingerprint) = None;
        self.inner.duplicate_turn_guard_injected.store(false, Ordering::SeqCst);
        self.inner.duplicate_turn_loop_detected.store(false, Ordering::SeqCst);
    }

    fn mark_duplicate_assistant_message_for_presentation(&self, message: &AssistantMessage) -> bool {
        if message.stop_reason == StopReason::Aborted || message.stop_reason == StopReason::Error {
            return false;
        }
        if message
            .content
            .iter()
            .any(|content| matches!(content, AssistantContent::ToolCall(_)))
        {
            return false;
        }
        let fingerprint = duplicate_fingerprint(message);
        lock(&self.inner.last_assistant_turn_fingerprint).as_deref() == Some(fingerprint.as_str())
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
        // The presentation marker is computed from the fingerprint at emission
        // time (`mark_duplicate_assistant_message_for_presentation`).
        if self.inner.duplicate_turn_guard_injected.load(Ordering::SeqCst) {
            self.inner.duplicate_turn_loop_detected.store(true, Ordering::SeqCst);
            return;
        }
        self.inner.duplicate_turn_guard_injected.store(true, Ordering::SeqCst);
        let guard = AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(vec![pi_ai::types::UserContentBlock::Text(TextContent::new(
                DUPLICATE_TURN_GUARD_PROMPT,
            ))]),
            input_source: Some(InputSource::Extension),
            provider_native: None,
            timestamp: pi_js_compat::date::js_now_millis(),
        });
        self.steer_agent(guard);
    }

    fn steer_agent(&self, message: AgentMessage) {
        self.inner.agent.steer(message);
        if lock(&self.inner.multi_agent_active_tools).is_empty() {
            self.start_thinking_phase_deadline();
        }
    }

    async fn handle_agent_event(&self, mut event: AgentEvent) -> Result<(), AgentError> {
        let mailbox_delivery = if let AgentEvent::MessageEnd { message } = &event {
            crate::core::runtime_mailbox_acceptance::read_runtime_mailbox_tool_delivery(message)
                .map_err(|error| AgentError::new(error.message()))?
        } else {
            None
        };
        let original_tool_call_id = match &event {
            AgentEvent::MessageEnd {
                message: AgentMessage::ToolResult(result),
            } => Some(result.tool_call_id.clone()),
            _ => None,
        };
        if matches!(event, AgentEvent::ToolExecutionStart { .. }) {
            self.reset_duplicate_turn_guard();
        }
        let mut runtime_message_marker = None;
        if let AgentEvent::MessageEnd { message } = &event {
            match message {
                AgentMessage::Assistant(assistant)
                    if self.mark_duplicate_assistant_message_for_presentation(assistant) =>
                {
                    runtime_message_marker = Some(RuntimeMessageMarker::DuplicateTurnAssistant);
                }
                AgentMessage::User(_) if is_duplicate_turn_guard_message(message) => {
                    runtime_message_marker = Some(RuntimeMessageMarker::DuplicateTurnGuard);
                }
                _ => {}
            }
        }
        self.publish_current_agent_activity(&event)?;
        let session_continuation = match &event {
            AgentEvent::AgentEnd { .. } if lock(&self.inner.codex_restriction_validation).is_some() => {
                Some(crate::core::extensions::types::AgentEndSessionContinuation::CodexRestrictionValidation)
            }
            AgentEvent::AgentEnd { .. } if self.runner().has_prepared_tool_result_relocation() => {
                Some(crate::core::extensions::types::AgentEndSessionContinuation::CwdRelocation)
            }
            _ => None,
        };
        self.remove_started_message_from_queue(&event);
        let replacement = self.emit_extension_event(&mut event, session_continuation).await;
        if let (Some(replacement), AgentEvent::MessageEnd { message }) = (replacement, &mut event) {
            *message = replacement;
            runtime_message_marker = None;
        }
        if let AgentEvent::MessageStart { message } = &event
            && is_duplicate_turn_guard_message(message)
        {
            runtime_message_marker = Some(RuntimeMessageMarker::DuplicateTurnGuard);
        }
        let listener_event = match &event {
            AgentEvent::AgentEnd { messages } => AgentSessionEvent::AgentEnd(AgentEndEvent::new(
                messages.clone(),
                session_continuation
                    == Some(crate::core::extensions::types::AgentEndSessionContinuation::CwdRelocation)
                    || self
                        .will_retry_after_agent_end(messages)
                        .map_err(|failure| AgentError::new(failure.message()))?,
                session_continuation,
            )),
            AgentEvent::MessageStart { .. } | AgentEvent::MessageUpdate { .. } | AgentEvent::MessageEnd { .. } => {
                AgentSessionEvent::Message(AgentMessageEvent {
                    event: event.clone(),
                    runtime_message_marker,
                })
            }
            other => AgentSessionEvent::Agent(other.clone()),
        };
        self.emit(listener_event);
        if let AgentEvent::MessageEnd { message } = &event {
            self.persist_completed_message(message, original_tool_call_id.as_deref(), mailbox_delivery.as_ref());
            if let AgentMessage::Assistant(assistant) = message {
                self.start_background_compaction_during_tool_turn(assistant)
                    .map_err(|failure| AgentError::new(failure.message()))?;
            }
        }
        if matches!(event, AgentEvent::AgentEnd { .. })
            && session_continuation == Some(crate::core::extensions::types::AgentEndSessionContinuation::CwdRelocation)
        {
            self.runner()
                .activate_tool_result_relocation()
                .await
                .map_err(|error| AgentError::new(error.message))?;
        }
        Ok(())
    }

    fn current_agent_activity_owner(&self) -> Option<crate::core::multi_agent_store::AgentCurrentActivityOwner> {
        let owner_session_id = self.inner.multi_agent_parent_session_id.clone()?;
        // TS computes this identity during construction and lets a failure throw.
        let instance_id = crate::core::session_control_db::get_runtime_process_instance_id()
            .expect("runtime process identity must be readable");
        let process_identity: crate::core::runtime_process::ProcessIdentity =
            serde_json::from_str(instance_id).expect("runtime process identity must be valid JSON");
        Some(crate::core::multi_agent_store::AgentCurrentActivityOwner {
            owner_session_id,
            process_identity,
        })
    }

    fn publish_current_agent_activity(&self, event: &AgentEvent) -> Result<(), AgentError> {
        self.update_active_agent_tools(event);
        self.update_thinking_phase_deadline(event);
        self.publish_activity_for_event(event)
    }

    fn update_active_agent_tools(&self, event: &AgentEvent) {
        if matches!(event, AgentEvent::AgentStart | AgentEvent::AgentEnd { .. }) {
            lock(&self.inner.multi_agent_active_tools).clear();
        }
        if let AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            started_at,
            ..
        } = event
        {
            lock(&self.inner.multi_agent_active_tools).push((
                tool_call_id.clone(),
                ActiveTool {
                    started_at: crate::utils::iso_time::format_iso_millis(*started_at as i64),
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.clone(),
                },
            ));
        }
        if let AgentEvent::ToolExecutionEnd { tool_call_id, .. } = event {
            lock(&self.inner.multi_agent_active_tools).retain(|(id, _)| id != tool_call_id);
        }
    }

    fn publish_activity_for_event(&self, event: &AgentEvent) -> Result<(), AgentError> {
        use crate::core::multi_agent_store::AgentCurrentActivity;

        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return Ok(());
        };
        let Some(agent_id) = self.inner.multi_agent_agent_id.as_ref() else {
            return Ok(());
        };
        let ownership = self.current_agent_activity_owner();
        match event {
            AgentEvent::AgentStart => self.publish_agent_activity(
                store,
                agent_id,
                Some(AgentCurrentActivity::Thinking {
                    started_at: crate::utils::iso_time::now_iso_string().into(),
                }),
                ownership.as_ref(),
            ),
            AgentEvent::ToolExecutionStart { .. } => match self.sole_active_tool_activity() {
                Some(activity) => self.publish_agent_activity(store, agent_id, Some(activity), ownership.as_ref()),
                None => Ok(()),
            },
            AgentEvent::ToolExecutionEnd { finished_at, .. } => {
                let activity = self
                    .first_active_tool_activity()
                    .unwrap_or_else(|| AgentCurrentActivity::Thinking {
                        started_at: crate::utils::iso_time::format_iso_millis(*finished_at as i64).into(),
                    });
                self.publish_agent_activity(store, agent_id, Some(activity), ownership.as_ref())
            }
            AgentEvent::AgentEnd { .. } => self.publish_agent_activity(store, agent_id, None, ownership.as_ref()),
            _ => Ok(()),
        }
    }

    /// The active tool's activity when it is the only one running.
    fn sole_active_tool_activity(&self) -> Option<crate::core::multi_agent_store::AgentCurrentActivity> {
        let active_tools = lock(&self.inner.multi_agent_active_tools);
        if active_tools.len() != 1 {
            return None;
        }
        active_tool_activity(active_tools.first())
    }

    /// The first still-running tool, published when an earlier parallel tool ends.
    fn first_active_tool_activity(&self) -> Option<crate::core::multi_agent_store::AgentCurrentActivity> {
        let active_tools = lock(&self.inner.multi_agent_active_tools);
        active_tool_activity(active_tools.first())
    }

    fn start_thinking_phase_deadline(&self) {
        if self.inner.multi_agent_runtime_role == Some(MultiAgentRuntimeRole::Observer)
            || self.inner.thinking_phase_timeout_ms <= 0.0
        {
            return;
        }
        self.clear_thinking_phase_deadline();
        let weak = Arc::downgrade(&self.inner);
        let timeout = self.inner.thinking_phase_timeout_ms;
        let handle = tokio::spawn(async move {
            let delay = pi_js_compat::node_timers::normalize_timeout_delay(timeout).trunc() as u64;
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let session = AgentSession { inner };
            *lock(&session.inner.thinking_phase_timer) = None;
            let subject = if session
                .inner
                .multi_agent_agent_id
                .as_ref()
                .is_some_and(|id| !id.is_empty())
            {
                "Child agent"
            } else {
                "Main session"
            };
            *lock(&session.inner.thinking_phase_timeout_error) = Some(AgentSessionError::new(format!(
                "{subject} thinking phase exceeded 20 minutes"
            )));
            session.inner.agent.abort();
        });
        *lock(&self.inner.thinking_phase_timer) = Some(handle);
    }

    fn clear_thinking_phase_deadline(&self) {
        if let Some(handle) = lock(&self.inner.thinking_phase_timer).take() {
            handle.abort();
        }
    }

    fn consume_thinking_phase_timeout_error(&self) -> Option<AgentSessionError> {
        lock(&self.inner.thinking_phase_timeout_error).take()
    }

    fn update_thinking_phase_deadline(&self, event: &AgentEvent) {
        match event {
            AgentEvent::AgentStart => self.start_thinking_phase_deadline(),
            AgentEvent::ToolExecutionStart { .. } => {
                if lock(&self.inner.multi_agent_active_tools).len() == 1 {
                    self.clear_thinking_phase_deadline();
                }
            }
            AgentEvent::ToolExecutionEnd { .. } => {
                if lock(&self.inner.multi_agent_active_tools).is_empty() {
                    self.start_thinking_phase_deadline();
                }
            }
            AgentEvent::AgentEnd { .. } => self.clear_thinking_phase_deadline(),
            _ => {}
        }
    }

    fn publish_agent_activity(
        &self,
        store: &Arc<crate::core::multi_agent_store::MultiAgentStore>,
        agent_id: &JsString,
        activity: Option<crate::core::multi_agent_store::AgentCurrentActivity>,
        ownership: Option<&crate::core::multi_agent_store::AgentCurrentActivityOwner>,
    ) -> Result<(), AgentError> {
        store
            .publish_agent_current_activity(agent_id, activity, ownership, None)
            .map(|_| ())
            .map_err(|error| AgentError::new(error.message))
    }

    fn start_background_compaction_during_tool_turn(
        &self,
        message: &AssistantMessage,
    ) -> Result<(), AgentSessionError> {
        if message.stop_reason != StopReason::ToolUse {
            return Ok(());
        }
        let model = self.inner.agent.model();
        if message.provider != model.provider || message.model != model.id {
            return Ok(());
        }
        let context_window = model.context_window;
        let context_tokens = crate::core::compaction::calculate_context_tokens(&message.usage);
        if context_tokens <= 0.0 || context_window <= 0.0 {
            return Ok(());
        }
        let settings = compaction_settings(&self.inner.settings_manager)?;
        if crate::core::compaction::should_compact(
            context_tokens,
            context_window,
            &crate::core::compaction::CompactionSettings {
                enabled: true,
                ..settings
            },
            model.auto_compaction_threshold,
        ) {
            return Ok(());
        }
        self.start_background_compaction(context_tokens, context_window)
    }

    fn start_background_compaction(&self, context_tokens: f64, context_window: f64) -> Result<(), AgentSessionError> {
        if context_window <= 0.0 || context_tokens < context_window * BACKGROUND_COMPACTION_CONTEXT_RATIO {
            return Ok(());
        }
        if lock(&self.inner.background_compaction_cache).is_some()
            || lock(&self.inner.auto_compaction_abort_controller).is_some()
            || lock(&self.inner.compaction_abort_controller).is_some()
        {
            return Ok(());
        }
        let model = self.inner.agent.model();
        let settings = compaction_settings(&self.inner.settings_manager)?;
        if !settings.enabled && !self.runner().has_handlers("compaction") {
            return Ok(());
        }
        let branch_entries = lock(&self.inner.session_manager).get_branch(None);
        let Some(snapshot_leaf_id) = lock(&self.inner.session_manager).get_leaf_id().cloned() else {
            return Ok(());
        };
        let Some(preparation) = crate::core::compaction::prepare_compaction(&branch_entries, settings) else {
            return Ok(());
        };
        static NEXT_BACKGROUND_COMPACTION_TOKEN: AtomicU64 = AtomicU64::new(1);
        let token = NEXT_BACKGROUND_COMPACTION_TOKEN.fetch_add(1, Ordering::SeqCst);
        let cache = BackgroundCompactionCache {
            token,
            state: BackgroundCompactionState::Pending,
            session_id: lock(&self.inner.session_manager).get_session_id().to_owned(),
            snapshot_leaf_id,
            branch_entries,
            preparation,
            settings,
            model_provider: model.provider.clone(),
            model_id: model.id.clone(),
            model_api: model.api.clone(),
            thinking_level: self.thinking_level(),
            system_prompt: self.inner.agent.state().system_prompt,
            runner: self.runner(),
            abort: AbortController::new(),
            compaction: None,
            from_extension: false,
            generation_duration_ms: None,
        };
        *lock(&self.inner.background_compaction_cache) = Some(cache);
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            AgentSession { inner }.generate_background_compaction(token).await;
        });
        Ok(())
    }

    async fn generate_background_compaction(&self, token: u64) {
        let Some((preparation, branch_entries, abort, runner, model)) = lock(&self.inner.background_compaction_cache)
            .as_ref()
            .filter(|cache| cache.token == token)
            .map(|cache| {
                (
                    cache.preparation.clone(),
                    cache.branch_entries.clone(),
                    cache.abort.signal(),
                    cache.runner.clone(),
                    self.inner.agent.model(),
                )
            })
        else {
            return;
        };
        let started_at = now_millis();
        let extension_compaction =
            match emit_compaction_event(&runner, &preparation, &branch_entries, None, "threshold", false, &abort).await
            {
                CompactionExtensionDecision::Cancel => return,
                CompactionExtensionDecision::None => None,
                CompactionExtensionDecision::Compaction(compaction) => Some(*compaction),
            };
        let from_extension = extension_compaction.is_some();
        let generated = match extension_compaction {
            Some(compaction) => Ok(compaction),
            None => match self.get_compaction_request_auth(&model).await {
                Ok(auth) => self.run_local_compaction(&preparation, &auth, None, &abort).await,
                Err(error) => Err(error),
            },
        };
        if abort.aborted() {
            return;
        }
        let mut cache_guard = lock(&self.inner.background_compaction_cache);
        let Some(cache) = cache_guard.as_mut().filter(|cache| cache.token == token) else {
            return;
        };
        match generated {
            Ok(compaction) => {
                cache.compaction = Some(compaction);
                cache.from_extension = from_extension;
                cache.generation_duration_ms = Some(now_millis() - started_at);
                cache.state = BackgroundCompactionState::Ready;
                drop(cache_guard);
                self.schedule_background_compaction_install(token);
            }
            Err(error) => {
                cache.state = BackgroundCompactionState::Failed;
                eprintln!("Background compaction cache generation failed: {error}");
            }
        }
    }

    async fn emit_session_before_compact_event(
        &self,
        preparation: &crate::core::compaction::CompactionPreparation,
        branch_entries: &[SessionEntry],
        custom_instructions: Option<&JsString>,
        reason: &str,
        will_retry: bool,
        abort: &AbortSignal,
    ) -> CompactionExtensionDecision {
        let runner = self.runner();
        if !runner.has_handlers("session_before_compact") {
            return CompactionExtensionDecision::None;
        }
        let mut event = ExtensionEvent::SessionBeforeCompact(SessionBeforeCompactEvent {
            preparation: preparation.clone(),
            branch_entries: branch_entries.to_vec(),
            custom_instructions: custom_instructions.cloned(),
            reason: compaction_reason(reason),
            will_retry,
            signal: Some(abort.clone()),
        });
        match runner.emit(&mut event).await {
            EmitResult::SessionBeforeCompact(result) => compaction_extension_decision(result.cancel, result.compaction),
            _ => CompactionExtensionDecision::None,
        }
    }

    async fn run_local_compaction(
        &self,
        preparation: &crate::core::compaction::CompactionPreparation,
        auth: &CompactionRequestAuth,
        custom_instructions: Option<&JsString>,
        abort: &AbortSignal,
    ) -> Result<crate::core::compaction::CompactionResult, AgentSessionError> {
        let stream_fn = self.inner.agent.stream_fn();
        let summary_options = crate::core::compaction::CompactionSummaryOptions {
            model: &auth.model,
            api_key: auth.api_key.clone(),
            headers: auth.headers.clone(),
            custom_instructions,
            signal: Some(abort.clone()),
            thinking_level: Some(self.thinking_level()),
            stream_fn: Some(&stream_fn),
            env: auth.env.clone(),
        };
        crate::core::compaction::compact(preparation, &auth.model, &summary_options)
            .await
            .map_err(|compaction_error| error(compaction_error.message()))
    }

    fn schedule_background_compaction_install(&self, token: u64) {
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if let Err(failure) = (AgentSession { inner }).install_background_compaction(token).await {
                eprintln!("Failed to install background compaction cache: {failure}");
            }
        });
    }

    async fn install_background_compaction(&self, token: u64) -> Result<(), AgentSessionError> {
        let _guard = self.inner.turn_start_lock.lock().await;
        if !self.background_compaction_matches_current_session(token)? {
            if lock(&self.inner.background_compaction_cache)
                .as_ref()
                .is_some_and(|cache| cache.token == token)
            {
                *lock(&self.inner.background_compaction_cache) = None;
            }
            return Ok(());
        }
        let session_advanced = lock(&self.inner.session_manager).get_leaf_id().cloned()
            != lock(&self.inner.background_compaction_cache)
                .as_ref()
                .map(|cache| cache.snapshot_leaf_id.clone());
        let agent_run_needs_post_processing = self.is_streaming() || lock(&self.inner.last_assistant_message).is_some();
        let ready = lock(&self.inner.background_compaction_cache)
            .as_ref()
            .is_some_and(|cache| cache.token == token && cache.state == BackgroundCompactionState::Ready);
        if !ready || session_advanced || agent_run_needs_post_processing {
            return Ok(());
        }
        self.run_auto_compaction("threshold", false, false).await?;
        Ok(())
    }

    fn background_compaction_matches_current_session(&self, token: u64) -> Result<bool, AgentSessionError> {
        let model = self.inner.agent.model();
        let settings = compaction_settings(&self.inner.settings_manager)?;
        let Some(cache) = lock(&self.inner.background_compaction_cache).clone() else {
            return Ok(false);
        };
        if cache.token != token {
            return Ok(false);
        }
        let snapshot_is_ancestor = lock(&self.inner.session_manager)
            .get_branch(None)
            .iter()
            .any(|entry| entry.id() == Some(&cache.snapshot_leaf_id));
        Ok(&cache.session_id == lock(&self.inner.session_manager).get_session_id()
            && snapshot_is_ancestor
            && cache.model_provider == model.provider
            && cache.model_id == model.id
            && cache.model_api == model.api
            && cache.thinking_level == self.thinking_level()
            && cache.system_prompt == self.inner.agent.state().system_prompt
            && cache.settings.enabled == settings.enabled
            && cache.settings.reserve_tokens == settings.reserve_tokens
            && cache.settings.keep_recent_tokens == settings.keep_recent_tokens)
    }

    fn has_ready_background_compaction(&self) -> Result<bool, AgentSessionError> {
        let token = lock(&self.inner.background_compaction_cache)
            .as_ref()
            .filter(|cache| cache.state == BackgroundCompactionState::Ready)
            .map(|cache| cache.token);
        let Some(token) = token else {
            return Ok(false);
        };
        if self.background_compaction_matches_current_session(token)? {
            return Ok(true);
        }
        *lock(&self.inner.background_compaction_cache) = None;
        Ok(false)
    }

    async fn cancel_background_compaction(&self) {
        let abort = lock(&self.inner.background_compaction_cache)
            .as_ref()
            .map(|cache| cache.abort.clone());
        let Some(abort) = abort else {
            return;
        };
        abort.abort(None);
        *lock(&self.inner.background_compaction_cache) = None;
    }

    async fn take_background_compaction(&self) -> Result<Option<BackgroundCompactionCache>, AgentSessionError> {
        let (token, state, abort) = {
            let guard = lock(&self.inner.background_compaction_cache);
            let Some(cache) = guard.as_ref() else {
                return Ok(None);
            };
            (cache.token, cache.state, cache.abort.clone())
        };
        if state == BackgroundCompactionState::Pending {
            abort.abort(None);
            *lock(&self.inner.background_compaction_cache) = None;
            return Ok(None);
        }
        if state != BackgroundCompactionState::Ready || !self.background_compaction_matches_current_session(token)? {
            *lock(&self.inner.background_compaction_cache) = None;
            return Ok(None);
        }
        Ok(lock(&self.inner.background_compaction_cache).take())
    }

    fn remove_started_message_from_queue(&self, event: &AgentEvent) {
        let AgentEvent::MessageStart { message } = event else {
            return;
        };
        match message {
            AgentMessage::Custom(object) => {
                let is_shared_channel =
                    object.get("customType").and_then(JsValue::as_string) == Some(&JsString::from("shared_channel"));
                let Some(content) = object.get("content").and_then(JsValue::as_string) else {
                    return;
                };
                if !is_shared_channel {
                    return;
                }
                let mut follow_ups = lock(&self.inner.follow_up_messages);
                if let Some(index) = follow_ups.iter().position(|message| &message.text == content) {
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
                lock(&self.inner.quota_fallback_visited_providers).clear();
                let text = user_message_text(&user.content);
                if text.is_empty() {
                    return;
                }
                let mut steering = lock(&self.inner.steering_messages);
                if let Some(index) = steering.iter().position(|message| message.text == text) {
                    steering.remove(index);
                    drop(steering);
                    self.emit_queue_update();
                    return;
                }
                drop(steering);
                let mut follow_ups = lock(&self.inner.follow_up_messages);
                if let Some(index) = follow_ups.iter().position(|message| message.text == text) {
                    follow_ups.remove(index);
                    drop(follow_ups);
                    self.emit_queue_update();
                }
            }
            _ => {}
        }
    }

    fn track_completed_assistant_message(&self, message: &AssistantMessage) {
        let identity = self
            .inner
            .agent
            .current_message_end_identity()
            .expect("Agent message_end identity scope");
        *lock(&self.inner.last_assistant_message) = Some((message.clone(), identity));
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
            &encode_node_utf8(&content),
        );
    }

    fn persist_completed_message(
        &self,
        message: &AgentMessage,
        original_tool_call_id: Option<&str>,
        mailbox_delivery: Option<&crate::core::runtime_mailbox_acceptance::RuntimeMailboxToolDelivery>,
    ) {
        let is_internal = is_duplicate_turn_guard_message(message);
        let mut manager = lock(&self.inner.session_manager);
        let result = match message {
            AgentMessage::Custom(object) => append_custom_agent_message(&mut manager, object),
            AgentMessage::User(_) | AgentMessage::Assistant(_) | AgentMessage::ToolResult(_) => {
                if is_internal {
                    Ok(None)
                } else if let Some(delivery) = mailbox_delivery {
                    let accepted = lock(&self.inner.runtime_mailbox_acceptance).accept_messages(
                        &mut manager,
                        &delivery.control_db_path,
                        &delivery.messages,
                        Some(message),
                    );
                    let released = crate::core::runtime_mailbox_acceptance::release_unaccepted_runtime_mailbox_messages(
                        &manager,
                        &delivery.control_db_path,
                        &delivery.messages,
                    );
                    released
                        .and(accepted)
                        .map(|()| None)
                        .map_err(|error| crate::core::session_manager::SessionManagerError::new(error.message()))
                } else if lock(&self.inner.runtime_mailbox_acceptance).consume_persisted_message(message) {
                    Ok(None)
                } else {
                    manager.append_message(message.clone()).map(Some)
                }
            }
        };
        let result = result.and_then(|id| {
            if let Some(id) = id.as_ref()
                && manager
                    .get_entry(id)
                    .is_some_and(|entry| entry.entry_type() == "message")
            {
                let identity = self
                    .inner
                    .agent
                    .current_message_end_identity()
                    .map_err(|error| crate::core::session_manager::SessionManagerError::new(error.message()))?;
                manager.bind_message_identity(id, identity)?;
            }
            Ok(id)
        });
        drop(manager);
        if let Err(error) = result {
            crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message().clone());
        }
        let _ = original_tool_call_id;
        if let AgentMessage::Assistant(assistant) = message {
            self.track_completed_assistant_message(assistant);
        }
    }

    fn will_retry_after_agent_end(&self, messages: &[AgentMessage]) -> Result<bool, AgentSessionError> {
        if let Some(retry) = self.codex_restriction_retry_after_agent_end()? {
            return Ok(retry);
        }
        let recovery_available = lock(&self.inner.thinking_timeout_dispatch)
            .as_ref()
            .is_some_and(|dispatch| {
                let dispatch = lock(dispatch);
                !dispatch.recovery_used && !dispatch.cancelled
            });
        if lock(&self.inner.thinking_phase_timeout_error).is_some() && recovery_available {
            return Ok(true);
        }
        for message in messages.iter().rev() {
            let AgentMessage::Assistant(assistant) = message else {
                continue;
            };
            if self.find_quota_fallback_model(assistant)?.is_some() {
                return Ok(true);
            }
            let settings = lock(&self.inner.settings_manager)
                .get_retry_settings()
                .map_err(|failure| error(failure.message().clone()))?;
            return Ok(settings.enabled
                && self.inner.retry_attempt.load(Ordering::SeqCst) < settings.max_retries as u64
                && self.is_retryable_error(assistant));
        }
        Ok(false)
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
        let messages = self.inner.agent.messages();
        let Some(AgentMessage::Assistant(assistant)) = messages.last() else {
            return;
        };
        let stop = assistant.stop_reason;
        if stop == StopReason::Aborted
            || stop == StopReason::Error
            || (remove_tool_use_assistant && stop == StopReason::ToolUse)
        {
            self.inner.agent.truncate_messages(messages.len() - 1);
        }
    }

    fn is_thinking_timeout_dispatch_active(&self, dispatch: &Arc<Mutex<ThinkingTimeoutDispatch>>) -> bool {
        let owns_dispatch = lock(&self.inner.thinking_timeout_dispatch)
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, dispatch));
        owns_dispatch && !lock(dispatch).cancelled && !self.inner.disposed.load(Ordering::SeqCst)
    }

    fn continue_agent_with_thinking_timeout<'a>(
        &'a self,
        dispatch: &'a Arc<Mutex<ThinkingTimeoutDispatch>>,
        process_queued_messages_first: bool,
    ) -> pi_ai::request::BoxFuture<'a, Result<(), AgentSessionError>> {
        Box::pin(self.run_agent_with_thinking_timeout(
            self.inner.agent.continue_run(pi_agent_core::agent::ContinueOptions {
                process_queued_messages_first,
            }),
            dispatch,
        ))
    }

    async fn run_agent_with_thinking_timeout(
        &self,
        run: impl std::future::Future<Output = Result<(), pi_agent_core::agent::types::AgentError>> + Send,
        dispatch: &Arc<Mutex<ThinkingTimeoutDispatch>>,
    ) -> Result<(), AgentSessionError> {
        let result = run.await;
        let Some(timeout_error) = self.consume_thinking_phase_timeout_error() else {
            return result.map_err(Into::into);
        };
        self.recover_thinking_timeout(dispatch, timeout_error).await
    }

    async fn recover_thinking_timeout(
        &self,
        dispatch: &Arc<Mutex<ThinkingTimeoutDispatch>>,
        timeout_error: AgentSessionError,
    ) -> Result<(), AgentSessionError> {
        if !self.is_thinking_timeout_dispatch_active(dispatch) {
            return Ok(());
        }
        {
            let mut state = lock(dispatch);
            if state.recovery_used {
                return Err(timeout_error);
            }
            state.recovery_used = true;
        }
        self.emit(AgentSessionEvent::AutoRetryStart(AutoRetryStartEvent {
            event_type: "auto_retry_start",
            attempt: 1,
            max_attempts: 1,
            delay_ms: 0.0,
            error_message: timeout_error.message().clone(),
        }));
        let guard = self.inner.turn_start_lock.lock().await;
        if !self.is_thinking_timeout_dispatch_active(dispatch) {
            return Ok(());
        }
        self.remove_trailing_interrupted_assistant(false);
        let continuation = self.continue_agent_with_thinking_timeout(dispatch, true);
        drop(guard);
        continuation.await
    }

    async fn run_agent_prompt(&self, messages: Vec<AgentMessage>) -> Result<(), AgentSessionError> {
        self.run_thinking_timeout_dispatch(self.inner.agent.prompt(messages))
            .await
    }

    async fn run_agent_continuation(&self) -> Result<(), AgentSessionError> {
        self.run_thinking_timeout_dispatch(self.inner.agent.continue_run(Default::default()))
            .await
    }

    async fn run_thinking_timeout_dispatch(
        &self,
        run: impl std::future::Future<Output = Result<(), pi_agent_core::agent::types::AgentError>> + Send,
    ) -> Result<(), AgentSessionError> {
        let dispatch = Arc::new(Mutex::new(ThinkingTimeoutDispatch::default()));
        *lock(&self.inner.thinking_timeout_dispatch) = Some(Arc::clone(&dispatch));
        *lock(&self.inner.thinking_phase_timeout_error) = None;
        let result = match self.run_agent_with_thinking_timeout(run, &dispatch).await {
            Ok(()) => self.continue_post_agent_runs(&dispatch).await,
            Err(error) => Err(error),
        };
        self.finish_thinking_timeout_dispatch(&dispatch, result.as_ref().err());
        result
    }

    fn finish_thinking_timeout_dispatch(
        &self,
        dispatch: &Arc<Mutex<ThinkingTimeoutDispatch>>,
        final_error: Option<&AgentSessionError>,
    ) {
        let (recovery_used, cancelled) = {
            let state = lock(dispatch);
            (state.recovery_used, state.cancelled)
        };
        if recovery_used {
            let reported_error = if cancelled {
                Some(JsString::from("Retry cancelled"))
            } else {
                final_error.map(|error| error.message().clone()).or_else(|| {
                    self.find_last_assistant_message()
                        .and_then(|message| message.error_message)
                })
            };
            self.emit(AgentSessionEvent::AutoRetryEnd(AutoRetryEndEvent {
                event_type: "auto_retry_end",
                success: reported_error.is_none(),
                attempt: 1,
                final_error: reported_error,
            }));
        }
        {
            let mut active = lock(&self.inner.thinking_timeout_dispatch);
            if !active.as_ref().is_some_and(|active| Arc::ptr_eq(active, dispatch)) {
                return;
            }
            *active = None;
        }
        self.cancel_codex_restriction_validation();
        self.inner
            .codex_restriction_attempt_active
            .store(false, Ordering::SeqCst);
        self.clear_thinking_phase_deadline();
        *lock(&self.inner.system_prompt_override) = None;
        self.flush_pending_bash_messages();
        self.schedule_idle_compaction();
    }

    async fn continue_post_agent_runs(
        &self,
        dispatch: &Arc<Mutex<ThinkingTimeoutDispatch>>,
    ) -> Result<(), AgentSessionError> {
        if self.inner.disposed.load(Ordering::SeqCst) || lock(dispatch).cancelled {
            return Ok(());
        }
        loop {
            let guard = self.inner.turn_start_lock.lock().await;
            if self.is_streaming() || !self.is_thinking_timeout_dispatch_active(dispatch) {
                return Ok(());
            }
            let Some(kind) = self.handle_post_agent_run().await? else {
                return Ok(());
            };
            if !self.is_thinking_timeout_dispatch_active(dispatch) {
                return Ok(());
            }
            let continuation = self.continue_agent_with_thinking_timeout(dispatch, kind == "queued");
            drop(guard);
            continuation.await?;
        }
    }

    /// Validates that the active model has configured auth.
    pub async fn validate_model_authentication(&self) -> Result<(), AgentSessionError> {
        let model = self.inner.agent.model();
        let has_configured_auth = self.inner.model_registry.has_configured_auth(&model)?;
        if has_configured_auth {
            return Ok(());
        }
        if self.inner.model_registry.is_using_oauth(&model) {
            return Err(error(format!(
                "Authentication failed for \"{}\". Credentials may have expired or network is unavailable. Run '/login {}' to re-authenticate.",
                model.provider, model.provider
            )));
        }
        Err(error(crate::core::auth_guidance::format_no_api_key_found_message(
            &model.provider,
        )?))
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
        if let Some(replacement) = self.recover_working_directory_before_run().await? {
            return Box::pin(replacement.continue_run()).await;
        }
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
            self.check_compaction(&last_assistant, false).await?;
        }
        if self.is_streaming() {
            return Err(error(
                "Agent is already processing. Wait for completion before continuing.",
            ));
        }
        drop(guard);
        let result = self.run_agent_continuation().await;
        self.complete_runtime_mailbox_steering_turn(&self.messages())?;
        result
    }

    /// Sends a prompt to the agent.
    pub async fn prompt(&self, text: impl Into<JsString>, options: PromptOptions) -> Result<(), AgentSessionError> {
        let text = text.into();
        let text = &text;
        let expand = options.expand_prompt_templates.unwrap_or(true);
        if expand && text.starts_with("/") && self.try_execute_extension_command(text, &options).await? {
            return Ok(());
        }
        let guard = self.inner.turn_start_lock.lock().await;
        if let Some(replacement) = self.recover_working_directory_before_run().await? {
            drop(guard);
            return Box::pin(replacement.prompt(text.clone(), options)).await;
        }
        if expand && text.starts_with("/") && !self.is_known_slash_command(text) {
            return Err(error(
                JsString::from("Unknown slash command: ") + slash_command_name(text),
            ));
        }
        let result = self.prompt_turn(text, options, guard, None).await;
        self.complete_runtime_mailbox_steering_turn(&self.messages())?;
        if result.is_ok() {
            self.drain_runtime_coordination_messages(DrainCoordinationOptions {
                checkpoint: None,
                include_next_model_call: false,
                trigger_if_idle: true,
            })
            .await?;
        }
        result
    }

    async fn prompt_turn(
        &self,
        text: &JsString,
        options: PromptOptions,
        guard: tokio::sync::MutexGuard<'_, ()>,
        mailbox_messages: Option<&[RuntimeMailboxMessage]>,
    ) -> Result<(), AgentSessionError> {
        let preflight = options.preflight_result.clone();
        let result = self.prompt_turn_inner(text, &options, mailbox_messages).await;
        match result {
            Ok(Some(messages)) => {
                if let Some(batch) = mailbox_messages {
                    self.accept_runtime_mailbox_messages(
                        batch,
                        messages.iter().find(|message| matches!(message, AgentMessage::User(_))),
                    )?;
                }
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
        text: &JsString,
        options: &PromptOptions,
        mailbox_messages: Option<&[RuntimeMailboxMessage]>,
    ) -> Result<Option<Vec<AgentMessage>>, AgentSessionError> {
        if lock(&self.inner.compaction_abort_controller).is_some() {
            return Err(error(
                "Cannot submit a prompt while compaction is in progress. Wait for compaction to finish and retry.",
            ));
        }
        let input = self.prepare_prompt_input(text, options).await?;
        let Some((text, images)) = input else {
            if let Some(batch) = mailbox_messages {
                self.accept_runtime_mailbox_messages(batch, None)?;
            }
            return Ok(None);
        };
        if self
            .queue_prompt_if_streaming(&text, &images, options, mailbox_messages)
            .await?
        {
            return Ok(None);
        }
        self.prepare_prompt_turn().await?;
        if self
            .queue_prompt_if_streaming(&text, &images, options, mailbox_messages)
            .await?
        {
            return Ok(None);
        }
        let messages = self.build_prompt_messages(&text, &images, options).await;
        Ok(Some(messages))
    }

    async fn prepare_prompt_input(
        &self,
        text: &JsString,
        options: &PromptOptions,
    ) -> Result<Option<(JsString, Vec<ImageContent>)>, AgentSessionError> {
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
            InputEventResult::Handled => return Ok(None),
            InputEventResult::Transform { text, images } => {
                current_text = text;
                if let Some(images) = images {
                    current_images = images.into_vec();
                }
            }
            InputEventResult::Continue => {}
        }
        self.record_codex_restriction_user_request(text, options.source)?;
        if options.expand_prompt_templates.unwrap_or(true) {
            current_text = self.expand_skill_command(&current_text);
            current_text = expand_prompt_template(&current_text, &self.prompt_templates());
        }
        Ok(Some((current_text, current_images)))
    }

    fn is_supervised_main_session(&self) -> bool {
        !self.inner.no_supervisor
            && self.inner.multi_agent_agent_id.as_ref().is_none_or(JsString::is_empty)
            && !self.inner.multi_agent_requires_agent_id
    }

    fn record_codex_restriction_user_request(
        &self,
        text: &JsString,
        source: Option<InputSource>,
    ) -> Result<(), AgentSessionError> {
        if source == Some(InputSource::Extension) || !self.is_supervised_main_session() {
            return Ok(());
        }
        self.save_codex_restriction_user_request(text)
    }

    async fn queue_prompt_if_streaming(
        &self,
        text: &JsString,
        images: &[ImageContent],
        options: &PromptOptions,
        mailbox_messages: Option<&[RuntimeMailboxMessage]>,
    ) -> Result<bool, AgentSessionError> {
        if !self.is_streaming() {
            return Ok(false);
        }
        if let Some(batch) = mailbox_messages {
            let message = runtime_mailbox_user_message(text, images, options.source.or(Some(InputSource::Extension)));
            self.steer_agent(message.clone());
            self.accept_runtime_mailbox_messages(batch, Some(&message))?;
            return Ok(true);
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
            self.check_compaction(&last_assistant, false).await?;
        }
        Ok(())
    }

    async fn build_prompt_messages(
        &self,
        text: &JsString,
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
            timestamp: pi_js_compat::date::js_now_millis(),
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
                    crate::utils::iso_time::now_iso_string(),
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

    fn is_known_slash_command(&self, text: &JsString) -> bool {
        let name = slash_command_name(text).strip_prefix("/").expect("slash prefix");
        if builtin_slash_commands().iter().any(|command| name == command.name) {
            return true;
        }
        if let Some(skill_name) = name.strip_prefix("skill:") {
            return self
                .inner
                .resource_loader
                .get_skills()
                .skills
                .iter()
                .any(|skill| skill_name == skill.name);
        }
        self.prompt_templates().iter().any(|template| name == template.name)
    }

    fn expand_skill_command(&self, text: &JsString) -> JsString {
        let Some(rest) = text.strip_prefix("/skill:") else {
            return text.to_owned();
        };
        let (skill_name, args) = split_at_first_space(&rest);
        let args = args.trim();
        let skills = self.inner.resource_loader.get_skills().skills;
        let Some(skill) = skills.iter().find(|skill| skill_name == skill.name) else {
            return text.to_owned();
        };
        let result = std::fs::read(encode_node_utf8(&skill.file_path))
            .map_err(|error| JsString::from(error.to_string()))
            .and_then(|bytes| {
                strip_frontmatter(JsString::from(String::from_utf8_lossy(&bytes).as_ref()))
                    .map_err(|error| error.message().clone())
            });
        match result {
            Ok(body) => {
                let block = pi_js_compat::js_format!(
                    "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
                    &skill.name,
                    &skill.file_path,
                    &skill.base_dir,
                    body.trim()
                );
                if args.is_empty() { block } else { block + "\n\n" + args }
            }
            Err(error) => {
                self.runner()
                    .emit_error(crate::core::extensions::types::ExtensionError {
                        extension_path: skill.file_path.clone(),
                        event: "skill_expansion".to_owned(),
                        error,
                        stack: None,
                    });
                text.clone()
            }
        }
    }

    /// Queues a steering message while the agent is running.
    pub async fn steer(&self, text: impl Into<JsString>, images: &[ImageContent]) -> Result<(), AgentSessionError> {
        let text = text.into();
        let text = &text;
        if text.starts_with("/") {
            self.throw_if_extension_command(text)?;
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
    pub async fn follow_up(&self, text: impl Into<JsString>, images: &[ImageContent]) -> Result<(), AgentSessionError> {
        let text = text.into();
        let text = &text;
        if text.starts_with("/") {
            self.throw_if_extension_command(text)?;
        }
        let mut expanded = self.expand_skill_command(text);
        expanded = expand_prompt_template(&expanded, &self.prompt_templates());
        self.queue_follow_up(&expanded, images, None).await;
        Ok(())
    }

    fn throw_if_extension_command(&self, text: &JsString) -> Result<(), AgentSessionError> {
        let (command_name, _) = split_prompt_command(text);
        if self.runner().get_command(&command_name).is_some() {
            return Err(AgentSessionError::new(
                JsString::from("Extension command \"/")
                    + &command_name
                    + "\" cannot be queued. Use prompt() or execute the command when not streaming.",
            ));
        }
        Ok(())
    }

    async fn queue_steer(&self, text: &JsString, images: &[ImageContent], source: Option<InputSource>) {
        lock(&self.inner.steering_messages).push(QueuedMessagePreview {
            text: text.clone(),
            input_source: source,
            custom_type: None,
        });
        self.emit_queue_update();
        let mut content = vec![TextOrImageContent::Text(TextContent::new(text))];
        content.extend(images.iter().cloned().map(TextOrImageContent::Image));
        self.steer_agent(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(content),
            input_source: source,
            provider_native: None,
            timestamp: pi_js_compat::date::js_now_millis(),
        }));
        self.emit(AgentSessionEvent::SteeringMessageQueued(SteeringMessageQueuedEvent {
            event_type: "steering_message_queued",
        }));
    }

    async fn queue_follow_up(&self, text: &JsString, images: &[ImageContent], source: Option<InputSource>) {
        lock(&self.inner.follow_up_messages).push(QueuedMessagePreview {
            text: text.clone(),
            input_source: source,
            custom_type: None,
        });
        self.emit_queue_update();
        let mut content = vec![TextOrImageContent::Text(TextContent::new(text))];
        content.extend(images.iter().cloned().map(TextOrImageContent::Image));
        self.inner.agent.follow_up(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(content),
            input_source: source,
            provider_native: None,
            timestamp: pi_js_compat::date::js_now_millis(),
        }));
    }

    /// Sends a custom message to the session.
    pub async fn send_custom_message(
        &self,
        custom_type: &JsString,
        content: CustomMessageContent,
        display: bool,
        details: Option<JsValue>,
        trigger_turn: bool,
        deliver_as: Option<StreamingBehavior>,
    ) -> Result<(), AgentSessionError> {
        let app_message = create_custom_message(
            custom_type,
            content,
            display,
            details,
            crate::utils::iso_time::now_iso_string(),
        );
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
        custom_type: &JsString,
        content: CustomMessageContent,
        display: bool,
        details: Option<JsValue>,
    ) {
        lock(&self.inner.pending_next_turn_messages).push(create_custom_message(
            custom_type,
            content,
            display,
            details,
            crate::utils::iso_time::now_iso_string(),
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
        self.inner.agent.append_message(app_message.clone());
        if let AgentMessage::Custom(object) = app_message {
            let mut manager = lock(&self.inner.session_manager);
            if let Err(error) = append_custom_agent_message(&mut manager, object) {
                crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message().clone());
            }
        }
        self.emit(AgentSessionEvent::Message(AgentMessageEvent {
            event: AgentEvent::MessageStart {
                message: app_message.clone(),
            },
            runtime_message_marker: None,
        }));
        self.emit(AgentSessionEvent::Message(AgentMessageEvent {
            event: AgentEvent::MessageEnd {
                message: app_message.clone(),
            },
            runtime_message_marker: None,
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
                (JsString::join(text_parts, "\n"), images)
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
    pub fn clear_queue(&self) -> (Vec<JsString>, Vec<JsString>) {
        let steering = std::mem::take(&mut *lock(&self.inner.steering_messages));
        let follow_up = std::mem::take(&mut *lock(&self.inner.follow_up_messages));
        self.inner.agent.clear_all_queues();
        self.emit_queue_update();
        (
            steering.into_iter().map(|message| message.text).collect(),
            follow_up.into_iter().map(|message| message.text).collect(),
        )
    }

    pub fn pending_message_count(&self) -> usize {
        lock(&self.inner.steering_messages).len() + lock(&self.inner.follow_up_messages).len()
    }

    pub fn has_pending_messages(&self) -> bool {
        self.pending_message_count() > 0 || self.inner.external_user_input_reservations.load(Ordering::SeqCst) > 0
    }

    pub fn reserve_external_user_input(&self) -> ExternalUserInputReservation {
        self.abort_codex_restriction_advice();
        self.cancel_codex_restriction_validation();
        self.inner
            .external_user_input_reservations
            .fetch_add(1, Ordering::SeqCst);
        ExternalUserInputReservation {
            inner: Arc::downgrade(&self.inner),
            released: AtomicBool::new(false),
        }
    }

    pub fn get_steering_messages(&self) -> Vec<JsString> {
        lock(&self.inner.steering_messages)
            .iter()
            .map(|message| message.text.clone())
            .collect()
    }

    pub fn get_follow_up_messages(&self) -> Vec<JsString> {
        lock(&self.inner.follow_up_messages)
            .iter()
            .map(|message| message.text.clone())
            .collect()
    }

    pub fn get_queued_message_previews(&self) -> QueuedMessagePreviews {
        QueuedMessagePreviews {
            steering: lock(&self.inner.steering_messages).clone(),
            follow_up: lock(&self.inner.follow_up_messages).clone(),
        }
    }

    /// Aborts the current operation, then submits preserved user input.
    pub async fn interrupt(&self, message: &JsString, source: InputSource) -> Result<(), AgentSessionError> {
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
        self.abort_codex_restriction_advice();
        self.cancel_codex_restriction_validation();
        if let Some(dispatch) = lock(&self.inner.thinking_timeout_dispatch).clone() {
            lock(&dispatch).cancelled = true;
        }
        self.abort_retry();
        self.clear_thinking_phase_deadline();
        *lock(&self.inner.thinking_phase_timeout_error) = None;
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
        self.set_model_with_thinking_level(model, None).await
    }

    /// Source model selection with an optional explicit effort, published before mutation.
    pub async fn set_model_with_thinking_level(
        &self,
        model: Model,
        thinking_level: Option<ThinkingLevel>,
    ) -> Result<(), AgentSessionError> {
        let thinking_level = self.get_thinking_level_for_model_switch(thinking_level);
        self.select_model(model, ModelSelectSource::Set, thinking_level).await
    }

    async fn select_model(
        &self,
        model: Model,
        source: ModelSelectSource,
        thinking_level: ThinkingLevel,
    ) -> Result<(), AgentSessionError> {
        if !self.inner.model_registry.has_configured_auth(&model)? {
            return Err(error(format!("No API key for {}/{}", model.provider, model.id)));
        }
        let effective = thinking_level_from_model(pi_ai::models::clamp_thinking_level(
            &model,
            model_thinking_level(thinking_level),
        ));
        self.publish_child_agent_model(&model, effective)?;
        let previous_model = self.inner.agent.model();
        self.inner.agent.set_model(model.clone());
        lock(&self.inner.session_manager)
            .set_session_model(&JsString::from(&model.provider), &JsString::from(&model.id))
            .map_err(AgentSessionError::from)?;
        if self.inner.multi_agent_agent_id.is_none() {
            lock_settings_manager(&self.inner.settings_manager)
                .set_default_model_and_provider(&model.provider, &model.id);
        }
        self.set_thinking_level(thinking_level)?;
        self.emit_model_select(&model, Some(&previous_model), source).await;
        Ok(())
    }

    /// Cycles to the next/previous scoped model.
    pub async fn cycle_model(&self, direction: CycleDirection) -> Result<Option<ModelCycleResult>, AgentSessionError> {
        let scoped_models = self.get_scoped_models_for_cycle().await?;
        if scoped_models.is_empty() {
            return Ok(None);
        }
        self.cycle_scoped_model(direction, &scoped_models).await
    }

    async fn get_scoped_models_for_cycle(&self) -> Result<Vec<ScopedModel>, AgentSessionError> {
        let configured = lock(&self.inner.scoped_models).clone();
        if !configured.is_empty() {
            return Ok(configured);
        }
        let enabled_models = lock(&self.inner.settings_manager).get_enabled_models();
        let Some(enabled_models) = enabled_models.filter(|models| !models.is_empty()) else {
            return Ok(Vec::new());
        };
        let resolved = resolve_model_scope(&enabled_models, &self.inner.model_registry).await?;
        self.filter_narrow_model_scope(resolved).await
    }

    async fn filter_narrow_model_scope(&self, models: Vec<ScopedModel>) -> Result<Vec<ScopedModel>, AgentSessionError> {
        let available_models = self.inner.model_registry.get_available()?;
        let covers_every_model = available_models.iter().all(|model| {
            models
                .iter()
                .any(|scoped| pi_ai::models::models_are_equal(Some(&scoped.model), Some(model)))
        });
        if covers_every_model {
            return Ok(Vec::new());
        }
        Ok(models)
    }

    async fn cycle_scoped_model(
        &self,
        direction: CycleDirection,
        models: &[ScopedModel],
    ) -> Result<Option<ModelCycleResult>, AgentSessionError> {
        let mut authed = Vec::new();
        for scoped in models {
            if self.inner.model_registry.has_configured_auth(&scoped.model)? {
                authed.push(scoped.clone());
            }
        }
        if authed.len() <= 1 {
            return Ok(None);
        }
        let current_model = self.inner.agent.model();
        let mut current_index = authed
            .iter()
            .position(|scoped| pi_ai::models::models_are_equal(Some(&scoped.model), Some(&current_model)))
            .unwrap_or(0);
        let length = authed.len();
        current_index = match direction {
            CycleDirection::Forward => (current_index + 1) % length,
            CycleDirection::Backward => (current_index + length - 1) % length,
        };
        let next = authed[current_index].clone();
        let thinking_level = self.get_thinking_level_for_model_switch(next.thinking_level);
        self.select_model(next.model.clone(), ModelSelectSource::Cycle, thinking_level)
            .await?;
        Ok(Some(ModelCycleResult {
            model: next.model,
            thinking_level: self.thinking_level(),
            is_scoped: true,
        }))
    }

    // =========================================================================
    // Thinking level management
    // =========================================================================

    fn publish_child_agent_model(&self, model: &Model, level: ThinkingLevel) -> Result<(), AgentSessionError> {
        let Some(store) = self.inner.multi_agent_store.as_ref() else {
            return Ok(());
        };
        let Some(agent_id) = self.inner.multi_agent_agent_id.as_ref() else {
            return Ok(());
        };
        store
            .publish_agent_model(
                agent_id,
                &crate::core::multi_agent_store::AgentModel {
                    provider_id: model.provider.clone(),
                    model_id: model.id.clone(),
                    thinking_level: Some(level.as_str().to_owned()),
                },
                self.current_agent_activity_owner().as_ref(),
            )
            .map_err(AgentSessionError::from)?;
        Ok(())
    }

    /// Sets the thinking level, clamped to the model capabilities.
    pub fn set_thinking_level(&self, level: ThinkingLevel) -> Result<(), AgentSessionError> {
        let available = self.get_available_thinking_levels();
        let effective = if available.contains(&level) {
            level
        } else {
            self.clamp_thinking_level(level)
        };
        let previous = self.inner.agent.thinking_level();
        let is_changing = effective != previous;
        self.publish_child_agent_model(&self.inner.agent.model(), effective)?;
        self.inner.agent.set_thinking_level(effective);
        if is_changing {
            lock(&self.inner.session_manager)
                .set_session_thinking_level(&effective.as_str().into())
                .map_err(AgentSessionError::from)?;
            if self.inner.multi_agent_agent_id.is_none()
                && (self.supports_thinking() || effective != ThinkingLevel::Off)
            {
                lock_settings_manager(&self.inner.settings_manager).set_default_thinking_level(effective);
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
        Ok(())
    }

    /// Cycles to the next thinking level.
    pub fn cycle_thinking_level(&self) -> Result<Option<ThinkingLevel>, AgentSessionError> {
        if !self.supports_thinking() {
            return Ok(None);
        }
        let levels = self.get_available_thinking_levels();
        let current = self.thinking_level();
        let index = levels.iter().position(|level| *level == current).unwrap_or(0);
        let next = levels[(index + 1) % levels.len()];
        self.set_thinking_level(next)?;
        Ok(Some(next))
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
        lock_settings_manager(&self.inner.settings_manager).set_steering_mode(mode);
    }

    pub fn set_follow_up_mode(&self, mode: pi_agent_core::harness::QueueMode) {
        self.inner.agent.set_follow_up_mode(mode);
        lock_settings_manager(&self.inner.settings_manager).set_follow_up_mode(mode);
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

    fn find_quota_fallback_model(
        &self,
        message: &AssistantMessage,
    ) -> Result<Option<Model>, pi_js_compat::node_error::NativeNodeError> {
        use crate::core::codex_quota_fallback::{CODEX_FALLBACK_PROVIDERS, is_quota_exhaustion_error};

        if !is_quota_exhaustion_error(message) {
            return Ok(None);
        }
        let current_model = self.inner.agent.model();
        if message.provider != current_model.provider || message.model != current_model.id {
            return Ok(None);
        }
        let Some((_, providers)) = CODEX_FALLBACK_PROVIDERS
            .iter()
            .find(|(provider, _)| *provider == message.provider)
        else {
            return Ok(None);
        };
        for provider in providers {
            if lock(&self.inner.quota_fallback_visited_providers).contains(*provider) {
                continue;
            }
            if let Some(fallback) = self.inner.model_registry.find(provider, &message.model)
                && self.inner.model_registry.has_configured_auth(&fallback)?
            {
                return Ok(Some(fallback));
            }
        }
        Ok(None)
    }

    async fn prepare_quota_fallback(&self, message: &AssistantMessage) -> Result<bool, AgentSessionError> {
        let Some(fallback_model) = self.find_quota_fallback_model(message)? else {
            return Ok(false);
        };
        lock(&self.inner.quota_fallback_visited_providers).insert(message.provider.clone());
        let messages = self.inner.agent.messages();
        if matches!(messages.last(), Some(AgentMessage::Assistant(_))) {
            self.inner.agent.truncate_messages(messages.len() - 1);
        }
        let previous_model = self.inner.agent.model();
        self.inner.agent.set_model(fallback_model.clone());
        lock(&self.inner.session_manager)
            .set_session_model(
                &JsString::from(&fallback_model.provider),
                &JsString::from(&fallback_model.id),
            )
            .map_err(AgentSessionError::from)?;
        self.emit_model_select(&fallback_model, Some(&previous_model), ModelSelectSource::Fallback)
            .await;
        Ok(true)
    }

    async fn prepare_retry(&self, message: &AssistantMessage) -> Result<bool, AgentSessionError> {
        let settings = lock(&self.inner.settings_manager)
            .get_retry_settings()
            .map_err(|failure| error(failure.message().clone()))?;
        if !settings.enabled {
            return Ok(false);
        }
        let attempt = self.inner.retry_attempt.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt > settings.max_retries as u64 {
            self.inner.retry_attempt.store(attempt - 1, Ordering::SeqCst);
            return Ok(false);
        }
        let delay_ms = crate::core::retry_delay::calculate_retry_delay_ms(
            attempt as f64,
            settings.base_delay_ms,
            settings.max_backoff_ms,
            crate::utils::random::random_f64(),
        );
        self.emit(AgentSessionEvent::AutoRetryStart(AutoRetryStartEvent {
            event_type: "auto_retry_start",
            attempt,
            max_attempts: settings.max_retries as u64,
            delay_ms,
            error_message: message
                .error_message
                .clone()
                .unwrap_or_else(|| "Unknown error".into())
                .into(),
        }));
        let messages = self.inner.agent.messages();
        if matches!(messages.last(), Some(AgentMessage::Assistant(_))) {
            self.inner.agent.truncate_messages(messages.len() - 1);
        }
        let controller_token = AbortController::new();
        let token = controller_token.signal();
        *lock(&self.inner.retry_abort_controller) = Some(controller_token.clone());
        let cancelled = tokio::select! {
            () = tokio::time::sleep(std::time::Duration::from_millis(delay_ms.max(0.0) as u64)) => false,
            _ = token.wait() => true,
        };
        *lock(&self.inner.retry_abort_controller) = None;
        if cancelled {
            let attempt = self.inner.retry_attempt.swap(0, Ordering::SeqCst);
            self.emit(AgentSessionEvent::AutoRetryEnd(AutoRetryEndEvent {
                event_type: "auto_retry_end",
                success: false,
                attempt,
                final_error: Some("Retry cancelled".into()),
            }));
            return Ok(false);
        }
        Ok(true)
    }

    /// Cancels an in-progress retry.
    pub fn abort_retry(&self) {
        if let Some(token) = lock(&self.inner.retry_abort_controller).as_ref() {
            token.abort(None);
        }
        let dispatch = lock(&self.inner.thinking_timeout_dispatch).clone();
        let Some(dispatch) = dispatch else {
            return;
        };
        {
            let mut dispatch = lock(&dispatch);
            if !dispatch.recovery_used {
                return;
            }
            dispatch.cancelled = true;
        }
        self.clear_thinking_phase_deadline();
        *lock(&self.inner.thinking_phase_timeout_error) = None;
        self.inner.agent.abort();
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
            || lock(&self.inner.thinking_timeout_dispatch)
                .as_ref()
                .is_some_and(|dispatch| {
                    let dispatch = lock(dispatch);
                    dispatch.recovery_used && !dispatch.cancelled
                })
    }

    pub fn is_retrying(&self) -> bool {
        lock(&self.inner.retry_abort_controller).is_some()
    }

    pub fn auto_retry_enabled(&self) -> bool {
        lock(&self.inner.settings_manager).get_retry_enabled()
    }

    pub fn set_auto_retry_enabled(&self, enabled: bool) {
        lock_settings_manager(&self.inner.settings_manager).set_retry_enabled(enabled);
    }
}

pub use pi_coding_agent_tools::core::agent_session::CycleDirection;

/// Options for [`AgentSession::execute_bash`].
#[derive(Clone, Default)]
pub struct ExecuteBashOptions {
    pub exclude_from_context: Option<bool>,
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

fn compaction_settings(
    settings_manager: &Arc<Mutex<SettingsManager>>,
) -> Result<crate::core::compaction::CompactionSettings, AgentSessionError> {
    let view = lock(settings_manager)
        .get_compaction_settings()
        .map_err(|failure| error(failure.message().clone()))?;
    Ok(crate::core::compaction::CompactionSettings {
        enabled: view.enabled,
        reserve_tokens: view.reserve_tokens,
        keep_recent_tokens: view.keep_recent_tokens,
    })
}

struct CompactedContextEstimate {
    estimated_tokens_after: f64,
    kept_from_previous_context_tokens: f64,
}

fn estimate_compacted_context_tokens(
    messages: &[AgentMessage],
    summary: &JsString,
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

fn create_compaction_summary_probe(summary: &JsString, tokens_before: f64) -> AgentMessage {
    pi_agent_core::harness::messages::create_compaction_summary_message(
        summary,
        tokens_before,
        "1970-01-01T00:00:00.000Z",
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

fn normalize_prompt_snippet(text: Option<&JsString>) -> Option<JsString> {
    let text = text?.trim();
    let mut one_line = JsString::default();
    let mut pending_space = false;
    for unit in text.units() {
        if pi_js_compat::string::is_js_whitespace_unit(*unit) {
            pending_space = true;
            continue;
        }
        if pending_space && !one_line.is_empty() {
            one_line.push_unit(b' ' as u16);
        }
        pending_space = false;
        one_line.push_unit(*unit);
    }
    (!one_line.is_empty()).then_some(one_line)
}

fn normalize_prompt_guidelines(guidelines: &[JsString]) -> Vec<JsString> {
    let mut seen = std::collections::HashSet::new();
    guidelines
        .iter()
        .map(JsString::trim)
        .filter(|text| !text.is_empty() && seen.insert(text.clone()))
        .collect()
}

fn split_at_first_space(text: &JsString) -> (JsString, JsString) {
    let units = text.units();
    let end = units.iter().position(|unit| *unit == 0x20).unwrap_or(units.len());
    let rest_start = (end + 1).min(units.len());
    (
        JsString::from_utf16(units[..end].to_vec()),
        JsString::from_utf16(units[rest_start..].to_vec()),
    )
}

fn split_prompt_command(text: &JsString) -> (JsString, JsString) {
    let (name, args) = split_at_first_space(text);
    (name.strip_prefix("/").unwrap_or(name), args)
}

fn slash_command_name(text: &JsString) -> JsString {
    let units = text.units();
    if units.first() != Some(&0x2f) {
        return JsString::from("/");
    }
    let end = units
        .iter()
        .skip(1)
        .position(|unit| pi_js_compat::string::is_js_whitespace_unit(*unit))
        .map_or(units.len(), |index| index + 1);
    JsString::from_utf16(units[..end].to_vec())
}

fn duplicate_fingerprint(message: &AssistantMessage) -> String {
    let text = get_assistant_message_text(message);
    pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json!({"text":text,"stopReason":message.stop_reason}))
}

fn write_back_tool_input(args: &Arc<Mutex<JsValue>>, input: &JsObject) {
    *args.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = JsValue::Object(input.clone());
}

fn is_permission_prompt_protocol_tool(name: &str, parameters: &Value) -> bool {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern =
        PATTERN.get_or_init(|| regex::Regex::new(r"^mcp__[^_]+(?:_[^_]+)*__[^_]+(?:_[^_]+)*$").expect("valid pattern"));
    if !pattern.is_match(name) {
        return false;
    }
    let Some(tool_name) = name.split("__").nth(2) else {
        return false;
    };
    if tool_name != "approval_prompt" {
        return false;
    }
    let Some(properties) = parameters.get("properties").and_then(Value::as_object) else {
        return false;
    };
    ["tool_name", "input", "tool_use_id", "cwd"]
        .iter()
        .all(|field| properties.contains_key(*field))
}

/// Owner-side stored-marker lookup for the internally created guard message.
///
/// The source stores the marker in a `WeakMap` keyed by message identity; Rust
/// messages are owned values, so the session recognizes its own guard message
/// by the exact content and input source it wrote.
fn stored_runtime_message_marker(message: &AgentMessage) -> Option<RuntimeMessageMarker> {
    match message {
        AgentMessage::User(user)
            if user.input_source == Some(InputSource::Extension)
                && user_message_text(&user.content) == DUPLICATE_TURN_GUARD_PROMPT =>
        {
            Some(RuntimeMessageMarker::DuplicateTurnGuard)
        }
        _ => None,
    }
}

fn is_duplicate_turn_guard_message(message: &AgentMessage) -> bool {
    is_duplicate_turn_guard_message_marker(stored_runtime_message_marker(message), None)
}

fn append_custom_agent_message(
    manager: &mut SessionManager,
    object: &JsObject,
) -> Result<Option<JsString>, SessionManagerError> {
    let custom_type = object
        .get("customType")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_default();
    let content = object
        .get("content")
        .map(pi_js_compat::js_json::from_value::<CustomMessageContent>)
        .transpose()
        .map_err(|error| SessionManagerError::new(error.to_string()))?
        .unwrap_or(CustomMessageContent::Blocks(Vec::new()));
    let display = object.get("display").and_then(JsValue::as_bool).unwrap_or(false);
    let details = object.get("details").cloned();
    manager
        .append_custom_message_entry(&custom_type, content, display, details)
        .map(Some)
}

/// Borrowed projection of the fields read by frozen Partial<AgentSessionConfig> validation.
#[derive(Default)]
pub struct StandaloneWorkerOptions<'a> {
    pub no_supervisor: bool,
    pub session_is_persisted: bool,
    pub has_multi_agent_store: bool,
    pub multi_agent_runtime_role: Option<MultiAgentRuntimeRole>,
    pub has_execution_capability: bool,
    pub multi_agent_agent_id: Option<&'a JsString>,
    pub multi_agent_parent_session_id: Option<&'a JsString>,
    pub multi_agent_requires_agent_id: bool,
}

pub fn assert_standalone_worker_options(config: &StandaloneWorkerOptions<'_>) -> Result<(), AgentSessionError> {
    if !config.no_supervisor {
        return Ok(());
    }
    if config.session_is_persisted {
        return Err(error("Standalone worker requires an in-memory session"));
    }
    let has_orchestration = config.has_multi_agent_store
        || config
            .multi_agent_runtime_role
            .is_some_and(|role| role != MultiAgentRuntimeRole::Standalone)
        || config.has_execution_capability
        || config.multi_agent_agent_id.is_some_and(|id| !id.is_empty())
        || config.multi_agent_parent_session_id.is_some_and(|id| !id.is_empty())
        || config.multi_agent_requires_agent_id;
    if has_orchestration {
        return Err(error("Standalone worker cannot receive multi-agent orchestration"));
    }
    Ok(())
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

    pub fn settings_manager(&self) -> SettingsManagerGuard<'_> {
        lock_settings_manager(&self.inner.settings_manager)
    }

    /// The shared settings handle owned by this session.
    pub fn settings_manager_handle(&self) -> Arc<Mutex<SettingsManager>> {
        Arc::clone(&self.inner.settings_manager)
    }

    /// `ExtensionRunner.setSandboxProfile`.
    pub fn set_sandbox_profile(
        &self,
        profile: Option<SandboxProfileName>,
        scope: SandboxProfileScope,
    ) -> Result<(), AgentSessionError> {
        if scope == SandboxProfileScope::Session {
            let persisted = self.session_manager().set_session_sandbox_profile(profile)?;
            if !persisted {
                return Err(error("Session sandbox profiles require a persisted session"));
            }
            self.settings_manager().set_session_sandbox_profile(profile);
        } else {
            let Some(profile) = profile else {
                return Err(error("Only session sandbox profiles can inherit configured settings"));
            };
            let settings_scope = match scope {
                SandboxProfileScope::Global => SettingsScope::Global,
                SandboxProfileScope::Project => SettingsScope::Project,
                SandboxProfileScope::Session => unreachable!(),
            };
            self.settings_manager()
                .set_sandbox_profile(profile, settings_scope)
                .map_err(|failure| error(failure.message().clone()))?;
        }

        let effective_profile = self.settings_manager().get_explicit_sandbox_profile();
        let full_access = effective_profile.is_none() || effective_profile == Some(SandboxProfileName::FullAccess);
        let status_text = if full_access {
            "bwrap: full access".to_owned()
        } else {
            format!("bwrap: {}", effective_profile.expect("checked above").as_str())
        };
        let ui = lock(&self.inner.extension_ui_context)
            .clone()
            .unwrap_or_else(|| Arc::new(NoOpUiContext));
        let color = if full_access {
            ThemeColor::Muted
        } else {
            ThemeColor::Accent
        };
        let theme = crate::modes::interactive::theme::try_current_theme()
            .map_err(|theme_error| error(theme_error.message()))?;
        let styled = theme.fg(color, &status_text);
        ui.set_status(
            &"bwrap".into(),
            Some(crate::core::footer_data_provider::ExtensionStatus::Text(styled)),
        )?;
        Ok(())
    }

    pub fn auth_storage(&self) -> &crate::core::auth_storage::AuthStorage {
        self.inner.model_registry.auth_storage()
    }

    pub fn model_registry(&self) -> &ModelRegistry {
        &self.inner.model_registry
    }

    pub fn cwd(&self) -> &JsString {
        &self.inner.cwd
    }

    pub fn agent_dir(&self) -> &JsString {
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

    /// `session.retryAttempt`.
    pub fn retry_attempt(&self) -> u64 {
        self.inner.retry_attempt.load(Ordering::SeqCst)
    }

    pub fn system_prompt(&self) -> JsString {
        self.inner.agent.system_prompt()
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

    pub fn set_active_tools_by_name(&self, tool_names: &[String]) -> Result<(), AgentSessionError> {
        self.apply_active_tools(tool_names)
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

    pub fn session_file(&self) -> Option<JsString> {
        lock(&self.inner.session_manager).get_session_file().cloned()
    }

    pub fn no_supervisor(&self) -> bool {
        self.inner.no_supervisor
    }

    pub fn session_id(&self) -> JsString {
        lock(&self.inner.session_manager).get_session_id().to_owned()
    }

    pub fn control_db_path(&self) -> Option<JsString> {
        self.inner.control_db_path.clone()
    }

    pub fn session_name(&self) -> Option<JsString> {
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
        if let Some(dispatch) = lock(&self.inner.thinking_timeout_dispatch).as_ref() {
            lock(dispatch).cancelled = true;
        }
        self.abort_codex_restriction_advice();
        self.cancel_codex_restriction_validation();
        self.abort_retry();
        self.abort_compaction();
        self.abort_compaction_summary_materialization();
        self.abort_branch_summary();
        self.abort_bash();
        self.inner.agent.abort();
        self.clear_thinking_phase_deadline();
        if let Some(timer) = lock(&self.inner.idle_compaction_timer).take() {
            timer.abort();
        }
        *lock(&self.inner.thinking_phase_timeout_error) = None;
        self.runner().invalidate(None);
        self.stop_runtime_mailbox_polling();
        self.stop_runtime_mailbox_signal_wake();
        if let Some(subscription) = lock(&self.inner.agent_subscription).take() {
            self.inner.agent.unsubscribe(subscription);
        }
        lock(&self.inner.event_listeners).clear();
        lock(&self.inner.runtime_mailbox_acceptance).clear();
        if let Err(error) = pi_ai::session_resources::cleanup_session_resources(Some(&self.session_id())) {
            crate::utils::uncaught::throw_uncaught("AggregateError", error.to_string());
        }
    }

    // =========================================================================
    // Compaction
    // =========================================================================

    /// Manually compacts the session context.
    pub async fn compact(
        &self,
        custom_instructions: Option<&JsString>,
    ) -> Result<crate::core::compaction::CompactionResult, AgentSessionError> {
        let guard = self.inner.turn_start_lock.lock().await;
        let result = self.compact_inner(custom_instructions).await;
        drop(guard);
        result
    }

    async fn compact_inner(
        &self,
        custom_instructions: Option<&JsString>,
    ) -> Result<crate::core::compaction::CompactionResult, AgentSessionError> {
        let was_running_agent_turn = self.is_streaming();
        let compaction_started_during_tool_execution =
            was_running_agent_turn && !self.inner.agent.pending_tool_calls().is_empty();
        self.disconnect_from_agent();
        self.abort().await?;
        self.cancel_background_compaction().await;
        let controller_abort = AbortController::new();
        let abort = controller_abort.signal();
        *lock(&self.inner.compaction_abort_controller) = Some(controller_abort.clone());
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
                self.emit(AgentSessionEvent::CompactionEnd(Box::new(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: "manual".to_owned(),
                    result: Some(compaction_result.clone()),
                    aborted: false,
                    will_retry: should_continue,
                    error_message: None,
                })));
                self.reconnect_to_agent();
                if should_continue {
                    self.continue_after_manual_compaction(was_running_agent_turn).await?;
                }
                Ok(compaction_result)
            }
            Err(error) => {
                let aborted = error.message() == "Compaction cancelled";
                *lock(&self.inner.compaction_abort_controller) = None;
                self.emit(AgentSessionEvent::CompactionEnd(Box::new(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: "manual".to_owned(),
                    result: None,
                    aborted,
                    will_retry: false,
                    error_message: (!aborted).then(|| JsString::from("Compaction failed: ") + error.message()),
                })));
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
        self.run_agent_continuation().await
    }

    /// Runs the `session_before_compact` preflight and the `compaction` provider
    /// event; the latter overrides the former when both supply a result.
    async fn resolve_extension_compaction(
        &self,
        preparation: &crate::core::compaction::CompactionPreparation,
        path_entries: &[SessionEntry],
        custom_instructions: Option<&JsString>,
        reason: &str,
        abort: &AbortSignal,
    ) -> Result<Option<crate::core::compaction::CompactionResult>, AgentSessionError> {
        let mut extension_compaction = match self
            .emit_session_before_compact_event(preparation, path_entries, custom_instructions, reason, false, abort)
            .await
        {
            CompactionExtensionDecision::Cancel => return Err(error("Compaction cancelled")),
            CompactionExtensionDecision::None => None,
            CompactionExtensionDecision::Compaction(compaction) => Some(*compaction),
        };
        match emit_compaction_event(
            &self.runner(),
            preparation,
            path_entries,
            custom_instructions,
            reason,
            false,
            abort,
        )
        .await
        {
            CompactionExtensionDecision::Cancel => return Err(error("Compaction cancelled")),
            CompactionExtensionDecision::None => {}
            CompactionExtensionDecision::Compaction(compaction) => extension_compaction = Some(*compaction),
        }
        Ok(extension_compaction)
    }

    #[allow(clippy::too_many_arguments)]
    async fn compact_with_settings(
        &self,
        custom_instructions: Option<&JsString>,
        abort: &AbortSignal,
        was_running_agent_turn: bool,
        compaction_started_during_tool_execution: bool,
        started_at: i64,
        reason: &str,
    ) -> Result<(crate::core::compaction::CompactionResult, bool), AgentSessionError> {
        let model = self.inner.agent.model();
        let settings = compaction_settings(&self.inner.settings_manager)?;
        let auth = if settings.enabled {
            Some(self.get_compaction_request_auth(&model).await?)
        } else {
            None
        };
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
        let extension_compaction = self
            .resolve_extension_compaction(&preparation, &path_entries, custom_instructions, reason, abort)
            .await?;
        let from_extension = extension_compaction.is_some();
        let compact_result = match extension_compaction {
            Some(compaction) => compaction,
            None => {
                if !settings.enabled {
                    return Err(error(BUILT_IN_COMPACTION_DISABLED_MESSAGE));
                }
                let Some(auth) = auth else {
                    return Err(error(BUILT_IN_COMPACTION_DISABLED_MESSAGE));
                };
                self.run_local_compaction(&preparation, &auth, custom_instructions, abort)
                    .await?
            }
        };
        if abort.aborted() {
            return Err(error("Compaction cancelled"));
        }
        let duration_ms = (now_millis() - started_at) as f64;
        let compact_result = self.append_compaction_result(compact_result, Some(from_extension), duration_ms)?;
        let will_retry = reason == "manual"
            && !compaction_started_during_tool_execution
            && (was_running_agent_turn || should_continue_after_manual_compaction(&self.inner.agent.messages()));
        Ok((compact_result, will_retry))
    }

    fn append_compaction_result(
        &self,
        mut compact_result: crate::core::compaction::CompactionResult,
        from_hook: Option<bool>,
        duration_ms: f64,
    ) -> Result<crate::core::compaction::CompactionResult, AgentSessionError> {
        let source = compact_result.source.clone();
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
                        from_hook,
                        duration_ms: Some(duration_ms),
                        provider_native,
                    },
                )
                .map_err(|manager_error| error(manager_error.message()))?;
        }
        let (session_context, identities) = lock(&self.inner.session_manager).build_session_context_with_identities();
        self.inner
            .agent
            .set_messages_with_identities(session_context.messages.clone(), identities)?;
        let estimate = estimate_compacted_context_tokens(
            &session_context.messages,
            &compact_result.summary,
            compact_result.tokens_before,
            compacted_result_tokens,
        );
        compact_result.duration_ms = Some(duration_ms);
        compact_result.estimated_tokens_after = Some(estimate.estimated_tokens_after);
        compact_result.kept_from_previous_context_tokens = Some(estimate.kept_from_previous_context_tokens);
        compact_result.compacted_result_tokens = compacted_result_tokens;
        compact_result.source = source;
        Ok(compact_result)
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

    async fn get_compaction_source_hint(
        &self,
        reason: &str,
        will_retry: bool,
    ) -> Option<crate::core::compaction::CompactionSourceInfo> {
        let runner = self.runner();
        if runner.has_handlers("session_compaction_source") {
            let mut event =
                ExtensionEvent::SessionCompactionSource(crate::core::extensions::types::SessionCompactionSourceEvent {
                    reason: match reason {
                        "threshold" => crate::core::extensions::types::CompactionReason::Threshold,
                        "overflow" => crate::core::extensions::types::CompactionReason::Overflow,
                        "idle" => crate::core::extensions::types::CompactionReason::Idle,
                        _ => crate::core::extensions::types::CompactionReason::Manual,
                    },
                    will_retry,
                });
            let emit_result = runner.emit(&mut event).await;
            if let Some(source) = emit_result.session_compaction_source()
                && let Some(source) = &source.source
            {
                return Some(source.clone());
            }
        }
        if runner.has_handlers("compaction") {
            return None;
        }
        if !lock(&self.inner.settings_manager).get_compaction_enabled() {
            return None;
        }
        let model = self.inner.agent.model();
        Some(crate::core::compaction::CompactionSourceInfo {
            r#type: crate::core::compaction::CompactionSourceType::Local,
            provider: model.provider,
            model: model.id,
            endpoint: None,
        })
    }

    async fn get_compaction_request_auth(&self, model: &Model) -> Result<CompactionRequestAuth, AgentSessionError> {
        let auth = self
            .inner
            .model_registry
            .get_api_key_and_headers(model)
            .await
            .map_err(|error| AgentSessionError::new(error.message))?;
        Ok(match auth {
            ResolvedRequestAuth::Ok { api_key, headers, env } => CompactionRequestAuth {
                model: model.clone(),
                api_key,
                headers: without_deleted_headers(headers.as_ref()),
                env,
            },
            ResolvedRequestAuth::Err { .. } => CompactionRequestAuth {
                model: model.clone(),
                api_key: None,
                headers: None,
                env: None,
            },
        })
    }

    /// Cancels in-progress compaction (manual or auto).
    pub fn abort_compaction(&self) {
        if let Some(token) = lock(&self.inner.compaction_abort_controller).as_ref() {
            token.abort(None);
        }
        if let Some(token) = lock(&self.inner.background_compaction_cache)
            .as_ref()
            .map(|cache| cache.abort.clone())
        {
            token.abort(None);
        }
        if let Some(token) = lock(&self.inner.auto_compaction_abort_controller).as_ref() {
            token.abort(None);
        }
    }

    fn schedule_idle_compaction(&self) {
        if let Some(timer) = lock(&self.inner.idle_compaction_timer).take() {
            timer.abort();
        }
        let Some(target) = self.find_idle_compaction_target() else {
            return;
        };
        let remaining = target.due_at - now_millis() as f64;
        // Math.max propagates NaN; f64::max would incorrectly select zero.
        let delay = if remaining.is_nan() {
            remaining
        } else {
            remaining.max(0.0)
        };
        let delay = pi_js_compat::node_timers::normalize_timeout_delay(delay).trunc() as u64;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(delay);
        let weak = Arc::downgrade(&self.inner);
        let timer = tokio::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            let Some(inner) = weak.upgrade() else {
                return;
            };
            let session = Self { inner };
            if let Err(error) = session.run_idle_compaction(target.identity).await {
                crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message().clone());
            }
        });
        *lock(&self.inner.idle_compaction_timer) = Some(timer);
    }

    fn find_idle_compaction_target(&self) -> Option<IdleCompactionTarget> {
        if self.inner.disposed.load(Ordering::SeqCst)
            || self.inner.multi_agent_runtime_role == Some(MultiAgentRuntimeRole::Observer)
        {
            return None;
        }
        let settings_allow_idle = {
            let settings = lock(&self.inner.settings_manager);
            settings.get_compaction_enabled() && settings.get_idle_compaction_enabled()
        };
        if !settings_allow_idle {
            return None;
        }
        let (message, identity) = self
            .inner
            .agent
            .find_last_message_with_identity(|message| matches!(message, AgentMessage::Assistant(_)))?;
        let AgentMessage::Assistant(message) = message else {
            unreachable!("assistant predicate");
        };
        if message.stop_reason == StopReason::Error {
            return None;
        }
        let model = self.inner.agent.model();
        if message.provider != model.provider || message.model != model.id {
            return None;
        }
        if self.idle_message_precedes_compaction(&message, &identity) {
            return None;
        }
        let due_at = crate::core::idle_compaction::compute_idle_compaction_due_at(&message)?;
        Some(IdleCompactionTarget { identity, due_at })
    }

    fn idle_message_precedes_compaction(
        &self,
        message: &AssistantMessage,
        identity: &pi_agent_core::agent::AgentMessageIdentity,
    ) -> bool {
        let manager = lock(&self.inner.session_manager);
        let branch = manager.get_branch(None);
        let Some(compaction) = get_latest_compaction_entry(&branch) else {
            return false;
        };
        if let Some(message_id) = manager.message_entry_id(identity)
            && let Some(assistant_index) = branch.iter().position(|entry| entry.id() == Some(message_id))
        {
            let compaction_index = branch
                .iter()
                .rposition(|entry| entry.id() == compaction.id())
                .expect("branch compaction");
            return assistant_index < compaction_index;
        }
        compaction
            .timestamp()
            .and_then(parse_timestamp_millis)
            .is_some_and(|timestamp| message.timestamp <= timestamp as f64)
    }

    async fn run_idle_compaction(
        &self,
        identity: pi_agent_core::agent::AgentMessageIdentity,
    ) -> Result<(), AgentSessionError> {
        *lock(&self.inner.idle_compaction_timer) = None;
        let _guard = self.inner.turn_start_lock.lock().await;
        let has_queued_input = self.has_pending_messages() || self.inner.agent.has_queued_messages();
        let session_is_busy = self.is_streaming() || self.is_compacting() || has_queued_input;
        let target = self.find_idle_compaction_target();
        if session_is_busy || target.is_none_or(|target| target.identity != identity) {
            return Ok(());
        }
        self.run_auto_compaction("idle", false, false).await?;
        Ok(())
    }

    /// Cancels in-progress compaction-summary materialization.
    pub fn abort_compaction_summary_materialization(&self) {
        if let Some(token) = lock(&self.inner.compaction_summary_materialization_abort_controller).as_ref() {
            token.abort(None);
        }
    }

    /// Cancels in-progress branch summarization.
    pub fn abort_branch_summary(&self) {
        if let Some(token) = lock(&self.inner.branch_summary_abort_controller).as_ref() {
            token.abort(None);
        }
    }

    async fn check_compaction(
        &self,
        assistant_message: &AssistantMessage,
        post_run_check: bool,
    ) -> Result<bool, AgentSessionError> {
        let settings = compaction_settings(&self.inner.settings_manager)?;
        if post_run_check && assistant_message.stop_reason == StopReason::Aborted {
            return Ok(false);
        }
        let model = self.inner.agent.model();
        let context_window = model.context_window;
        let same_model = assistant_message.provider == model.provider && assistant_message.model == model.id;
        let branch = lock(&self.inner.session_manager).get_branch(None);
        let compaction_entry = get_latest_compaction_entry(&branch).cloned();
        let assistant_is_before_compaction = compaction_entry.as_ref().is_some_and(|entry| {
            let compaction_timestamp = entry.timestamp().and_then(parse_timestamp_millis);
            compaction_timestamp.is_some_and(|timestamp| assistant_message.timestamp <= timestamp as f64)
        });
        if assistant_is_before_compaction {
            return Ok(false);
        }
        if same_model && pi_ai::utils::overflow::is_context_overflow(assistant_message, Some(context_window)) {
            let should_retry_after_compaction = assistant_message.stop_reason != StopReason::Stop;
            let should_continue_after_compaction = post_run_check && should_retry_after_compaction;
            if !should_retry_after_compaction {
                return self.run_auto_compaction("overflow", false, false).await;
            }
            if self.inner.overflow_recovery_attempted.load(Ordering::SeqCst) {
                self.emit(AgentSessionEvent::CompactionEnd(Box::new(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: "overflow".to_owned(),
                    result: None,
                    aborted: false,
                    will_retry: false,
                    error_message: Some(
                        "Context overflow recovery failed after one compact-and-retry attempt. Try reducing context or switching to a larger-context model.".into(),
                    ),
                })));
                return Ok(false);
            }
            if post_run_check {
                self.inner.overflow_recovery_attempted.store(true, Ordering::SeqCst);
                let messages = self.inner.agent.messages();
                if matches!(messages.last(), Some(AgentMessage::Assistant(_))) {
                    self.inner.agent.truncate_messages(messages.len() - 1);
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
                return Ok(false);
            };
            let index = last_usage_index as usize;
            if let Some(compaction_entry) = &compaction_entry
                && let Some(AgentMessage::Assistant(usage_message)) = messages.get(index)
                && let Some(timestamp) = compaction_entry.timestamp().and_then(parse_timestamp_millis)
                && usage_message.timestamp <= timestamp as f64
            {
                return Ok(false);
            }
            context_tokens = estimate.tokens;
        }
        if crate::core::compaction::should_compact(
            context_tokens,
            context_window,
            &crate::core::compaction::CompactionSettings {
                enabled: true,
                ..settings
            },
            None,
        ) {
            let turn_was_truncated = assistant_message.stop_reason == StopReason::Length;
            let will_retry =
                post_run_check && turn_was_truncated && !self.inner.length_recovery_attempted.load(Ordering::SeqCst);
            if will_retry {
                self.inner.length_recovery_attempted.store(true, Ordering::SeqCst);
            }
            return self.run_auto_compaction("threshold", will_retry, will_retry).await;
        }
        if self.has_ready_background_compaction()? {
            return self.run_auto_compaction("threshold", false, false).await;
        }
        self.start_background_compaction(context_tokens, context_window)?;
        Ok(false)
    }

    async fn run_auto_compaction(
        &self,
        reason: &str,
        will_retry: bool,
        continue_after_compaction: bool,
    ) -> Result<bool, AgentSessionError> {
        let settings = compaction_settings(&self.inner.settings_manager)?;
        let cached = self.take_background_compaction().await?;
        let path_entries = cached
            .as_ref()
            .map(|cache| cache.branch_entries.clone())
            .unwrap_or_else(|| lock(&self.inner.session_manager).get_branch(None));
        let preparation = cached
            .as_ref()
            .map(|cache| cache.preparation.clone())
            .or_else(|| crate::core::compaction::prepare_compaction(&path_entries, settings));
        let Some(preparation) = preparation else {
            return Ok(false);
        };
        let source_hint = self.get_compaction_source_hint(reason, will_retry).await;
        self.emit(AgentSessionEvent::CompactionStart(CompactionStartEvent {
            event_type: "compaction_start",
            reason: reason.to_owned(),
            source_hint,
        }));
        let controller_abort = AbortController::new();
        let abort = controller_abort.signal();
        *lock(&self.inner.auto_compaction_abort_controller) = Some(controller_abort.clone());
        let started_at = now_millis();
        let mut extension_compaction = cached.as_ref().and_then(|cache| cache.compaction.clone());
        let mut from_extension = cached.as_ref().is_some_and(|cache| cache.from_extension);
        match self
            .emit_session_before_compact_event(&preparation, &path_entries, None, reason, will_retry, &abort)
            .await
        {
            CompactionExtensionDecision::Cancel => {
                *lock(&self.inner.auto_compaction_abort_controller) = None;
                self.emit(AgentSessionEvent::CompactionEnd(Box::new(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: reason.to_owned(),
                    result: None,
                    aborted: true,
                    will_retry: false,
                    error_message: None,
                })));
                return Ok(false);
            }
            CompactionExtensionDecision::None => {}
            CompactionExtensionDecision::Compaction(compaction) => {
                extension_compaction = Some(*compaction);
                from_extension = true;
            }
        }
        if cached.is_none() {
            match emit_compaction_event(
                &self.runner(),
                &preparation,
                &path_entries,
                None,
                reason,
                will_retry,
                &abort,
            )
            .await
            {
                CompactionExtensionDecision::Cancel => {
                    *lock(&self.inner.auto_compaction_abort_controller) = None;
                    self.emit(AgentSessionEvent::CompactionEnd(Box::new(CompactionEndEvent {
                        event_type: "compaction_end",
                        reason: reason.to_owned(),
                        result: None,
                        aborted: true,
                        will_retry: false,
                        error_message: None,
                    })));
                    return Ok(false);
                }
                CompactionExtensionDecision::None => {}
                CompactionExtensionDecision::Compaction(compaction) => {
                    extension_compaction = Some(*compaction);
                    from_extension = true;
                }
            }
        }
        let result = match extension_compaction {
            Some(compaction) => {
                let duration_ms = cached
                    .as_ref()
                    .map(|cache| cache.generation_duration_ms.unwrap_or_default() as f64)
                    .unwrap_or_default();
                self.append_compaction_result(compaction, Some(from_extension), duration_ms)
                    .map(|result| (result, false))
            }
            None => {
                if !settings.enabled {
                    Err(error(BUILT_IN_COMPACTION_DISABLED_MESSAGE))
                } else {
                    let model = self.inner.agent.model();
                    let generated = match self.get_compaction_request_auth(&model).await {
                        Ok(auth) => self.run_local_compaction(&preparation, &auth, None, &abort).await,
                        Err(error) => Err(error),
                    };
                    if abort.aborted() {
                        Err(error("Compaction cancelled"))
                    } else {
                        generated.and_then(|compaction| {
                            let duration_ms = (now_millis() - started_at) as f64;
                            self.append_compaction_result(compaction, Some(false), duration_ms)
                                .map(|result| (result, false))
                        })
                    }
                }
            }
        };
        *lock(&self.inner.auto_compaction_abort_controller) = None;
        match result {
            Ok((compaction_result, _)) => {
                self.emit(AgentSessionEvent::CompactionEnd(Box::new(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: reason.to_owned(),
                    result: Some(compaction_result),
                    aborted: false,
                    will_retry,
                    error_message: None,
                })));
                if continue_after_compaction {
                    let messages = self.inner.agent.messages();
                    if let Some(AgentMessage::Assistant(assistant)) = messages.last()
                        && (assistant.stop_reason == StopReason::Error || assistant.stop_reason == StopReason::Length)
                    {
                        self.inner.agent.truncate_messages(messages.len() - 1);
                    }
                    return Ok(true);
                }
                Ok(self.inner.agent.has_queued_messages())
            }
            Err(error) => {
                let aborted = abort.aborted() || error.message() == "Compaction cancelled";
                self.emit(AgentSessionEvent::CompactionEnd(Box::new(CompactionEndEvent {
                    event_type: "compaction_end",
                    reason: reason.to_owned(),
                    result: None,
                    aborted,
                    will_retry: false,
                    error_message: (!aborted).then(|| {
                        if reason == "overflow" {
                            JsString::from("Context overflow recovery failed: ") + error.message()
                        } else {
                            JsString::from("Auto-compaction failed: ") + error.message()
                        }
                    }),
                })));
                Ok(false)
            }
        }
    }

    pub fn set_auto_compaction_enabled(&self, enabled: bool) {
        lock_settings_manager(&self.inner.settings_manager).set_compaction_enabled(enabled);
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
        command: &JsString,
        on_chunk: Option<crate::core::bash_executor::BashChunkCallback>,
        options: ExecuteBashOptions,
    ) -> Result<crate::core::bash_executor::BashResult, AgentSessionError> {
        use crate::core::bash_executor::{BashExecutorOptions, execute_bash_with_operations};

        let controller_abort = AbortController::new();
        let abort = controller_abort.signal();
        lock(&self.inner.bash_abort_controllers).push(controller_abort.clone());
        let (prefix, shell_path) = {
            let settings = lock(&self.inner.settings_manager);
            (settings.get_shell_command_prefix().cloned(), settings.get_shell_path())
        };
        let resolved_command = match prefix {
            Some(prefix) => JsString::from(prefix) + "\n" + command,
            None => command.clone(),
        };
        let operations = options.operations.clone().unwrap_or_else(|| {
            Arc::new(crate::core::tools::bash::create_local_bash_operations(
                shell_path.as_ref(),
            ))
        });
        let cwd = lock(&self.inner.session_manager).get_cwd().to_owned();
        let result = execute_bash_with_operations(
            &resolved_command,
            &cwd,
            operations,
            &BashExecutorOptions {
                on_chunk,
                signal: Some(abort.clone()),
            },
        )
        .await;
        lock(&self.inner.bash_abort_controllers).retain(|token| token != &controller_abort);
        let result = result.map_err(|bash_error| error(bash_error.message))?;
        self.record_bash_result(command, &result, options.exclude_from_context);
        Ok(result)
    }

    /// Records a bash execution result in session history.
    pub fn record_bash_result(
        &self,
        command: &JsString,
        result: &crate::core::bash_executor::BashResult,
        exclude_from_context: Option<bool>,
    ) {
        let bash_message = BashExecutionMessage {
            role: BashExecutionRole::BashExecution,
            command: command.clone(),
            output: result.output.clone(),
            exit_code: result.exit_code,
            cancelled: result.cancelled,
            truncated: result.truncated,
            full_output_path: result.full_output_path.clone().map(JsString::from),
            timestamp: pi_js_compat::date::js_now_millis() as f64,
            exclude_from_context,
        };
        if self.is_streaming() {
            lock(&self.inner.pending_bash_messages).push(bash_message);
            return;
        }
        let message = bash_message_to_agent_message(&bash_message);
        let identity = self.inner.agent.append_message(message.clone());
        let mut manager = lock(&self.inner.session_manager);
        if let Err(error) = manager
            .append_message(message.clone())
            .and_then(|id| manager.bind_message_identity(&id, identity))
        {
            crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message().clone());
        }
        drop(manager);
        self.emit(AgentSessionEvent::BashMessagesCommitted(BashMessagesCommittedEvent {
            event_type: "bash_messages_committed",
            messages: vec![message],
        }));
    }

    /// Cancels a running bash command.
    pub fn abort_bash(&self) {
        let tokens: Vec<AbortController> = lock(&self.inner.bash_abort_controllers).clone();
        for token in tokens {
            token.abort(None);
        }
    }

    pub fn is_bash_running(&self) -> bool {
        !lock(&self.inner.bash_abort_controllers).is_empty()
    }

    // Frozen agent-session.ts:5875-5884 forwards detach throws without a catch.
    pub fn detach_running_tool(&self) -> Result<bool, CallbackError> {
        self.inner
            .tool_detach_registry
            .as_ref()
            .map_or(Ok(false), |registry| registry.detach_running())
    }

    pub fn detach_all_running_tools(&self) -> Result<(), CallbackError> {
        if let Some(registry) = &self.inner.tool_detach_registry {
            registry.detach_all()?;
        }
        Ok(())
    }

    pub fn detach_bash_tool(&self) -> Result<bool, CallbackError> {
        self.detach_running_tool()
    }

    pub fn has_detachable_tool(&self) -> bool {
        self.inner
            .tool_detach_registry
            .as_ref()
            .is_some_and(|registry| registry.has_running())
    }

    pub fn has_detachable_bash_tool(&self) -> bool {
        self.has_detachable_tool()
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
        let mut committed = Vec::new();
        for bash_message in messages {
            let message = bash_message_to_agent_message(&bash_message);
            let identity = self.inner.agent.append_message(message.clone());
            let mut manager = lock(&self.inner.session_manager);
            if let Err(error) = manager
                .append_message(message.clone())
                .and_then(|id| manager.bind_message_identity(&id, identity))
            {
                crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message().clone());
            }
            committed.push(message);
        }
        self.emit(AgentSessionEvent::BashMessagesCommitted(BashMessagesCommittedEvent {
            event_type: "bash_messages_committed",
            messages: committed,
        }));
    }

    // =========================================================================
    // Session management
    // =========================================================================

    /// Sets a display name for the current session.
    pub fn set_session_name(&self, name: &JsString) -> Result<(), AgentSessionError> {
        lock(&self.inner.session_manager)
            .set_session_name(Some(name))
            .map_err(|failure| error(failure.message().clone()))?;
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
        Ok(())
    }

    pub fn clear_session_name(&self) -> Result<(), AgentSessionError> {
        lock(&self.inner.session_manager)
            .set_session_name(None)
            .map_err(|failure| error(failure.message().clone()))?;
        self.emit(AgentSessionEvent::SessionInfoChanged(SessionInfoChangedEvent {
            event_type: "session_info_changed",
            name: None,
        }));
        let session = self.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                session.emit_session_info_changed(None).await;
            });
        }
        Ok(())
    }

    pub fn update_compaction_summary(&self, entry_id: &JsString, summary: &JsString) -> Result<(), AgentSessionError> {
        let messages = {
            let mut manager = lock(&self.inner.session_manager);
            manager
                .update_compaction_summary(entry_id, summary)
                .map_err(|failure| error(failure.message().clone()))?;
            manager.build_session_context_with_identities()
        };
        self.inner
            .agent
            .set_messages_with_identities(messages.0.messages, messages.1)?;
        Ok(())
    }

    /// Materializes a provider-native compaction summary.
    pub async fn materialize_compaction_summary(
        &self,
        entry_id: &JsString,
    ) -> Result<crate::core::compaction::MaterializeCompactionSummaryResult, AgentSessionError> {
        let (entry, model) = self.require_compaction_summary_materialization_target(entry_id)?;
        if lock(&self.inner.compaction_summary_materialization_abort_controller).is_some() {
            return Err(error("Compaction summary materialization is already running"));
        }
        let controller_abort = AbortController::new();
        let abort = controller_abort.signal();
        *lock(&self.inner.compaction_summary_materialization_abort_controller) = Some(controller_abort.clone());
        let auth = self.get_required_request_auth(&model).await?;
        let settings = compaction_settings(&self.inner.settings_manager)?;
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
        entry_id: &JsString,
    ) -> Result<(crate::core::session_manager::CompactionEntry, Model), AgentSessionError> {
        let entry = lock(&self.inner.session_manager)
            .get_entry(entry_id)
            .cloned()
            .ok_or_else(|| error(pi_js_compat::js_format!("Entry {} not found", entry_id)))?;
        let Some(compaction) = entry.as_compaction().map(std::borrow::Cow::into_owned) else {
            return Err(error(pi_js_compat::js_format!(
                "Entry {} is not a compaction",
                entry_id
            )));
        };
        let Some(checkpoint) = &compaction.provider_native else {
            return Err(error(pi_js_compat::js_format!(
                "Compaction entry {} has no provider-native checkpoint",
                entry_id
            )));
        };
        let model = self.inner.agent.model();
        let provider = &checkpoint["provider"];
        let api = &checkpoint["api"];
        let matching_provider = provider.as_string() == Some(&JsString::from(&model.provider));
        let matching_api = api.as_string() == Some(&JsString::from(&model.api));
        let supported_format = checkpoint["format"].as_string() == Some(&JsString::from("openai.responses.input"))
            && checkpoint["value"].as_array().is_some();
        if !matching_provider || !matching_api || !supported_format {
            return Err(error(pi_js_compat::js_format!(
                "Compaction summary materialization requires an active {}/{} model",
                pi_js_compat::js_json::to_js_string(provider).map_err(error)?,
                pi_js_compat::js_json::to_js_string(api).map_err(error)?
            )));
        }
        Ok((compaction, model))
    }

    async fn get_required_request_auth(&self, model: &Model) -> Result<CompactionRequestAuth, AgentSessionError> {
        match self
            .inner
            .model_registry
            .get_api_key_and_headers(model)
            .await
            .map_err(|error| AgentSessionError::new(error.message))?
        {
            ResolvedRequestAuth::Err { error } => {
                return Err(normalize_required_request_auth_error(error, &model.provider));
            }
            ResolvedRequestAuth::Ok {
                api_key: Some(api_key),
                headers,
                env,
            } if !api_key.is_empty() => {
                return Ok(CompactionRequestAuth {
                    model: model.clone(),
                    api_key: Some(api_key),
                    headers: without_deleted_headers(headers.as_ref()),
                    env,
                });
            }
            ResolvedRequestAuth::Ok { .. } => {}
        }
        if self.inner.model_registry.is_using_oauth(&model) {
            return Err(error(format!(
                "Authentication failed for \"{}\". Credentials may have expired or network is unavailable. Run '/login {}' to re-authenticate.",
                model.provider, model.provider
            )));
        }
        Err(error(crate::core::auth_guidance::format_no_api_key_found_message(
            &model.provider,
        )?))
    }

    // =========================================================================
    // Tree navigation
    // =========================================================================

    /// Navigates to a different node in the session tree.
    pub async fn navigate_tree(
        &self,
        target_id: &JsString,
        options: NavigateTreeOptions,
    ) -> Result<NavigateTreeResult, AgentSessionError> {
        if self.is_streaming() {
            return Err(error(
                "Wait for the current response to finish before navigating the session tree.",
            ));
        }
        let (old_leaf_id, target_entry, entries_to_summarize, common_ancestor_id) = {
            let manager = lock(&self.inner.session_manager);
            let old_leaf_id = manager.get_leaf_id().cloned();
            let collected = crate::core::compaction::branch_summarization::collect_entries_for_branch_summary(
                &manager,
                old_leaf_id.as_ref(),
                target_id,
            );
            (
                old_leaf_id,
                manager.get_entry(target_id).cloned(),
                collected.entries,
                collected.common_ancestor_id,
            )
        };
        if old_leaf_id.as_ref() == Some(target_id) {
            return Ok(NavigateTreeResult {
                editor_text: None,
                cancelled: false,
                aborted: None,
                summary_entry: None,
            });
        }
        let Some(target_entry) = target_entry else {
            return Err(error(pi_js_compat::js_format!("Entry {} not found", target_id)));
        };

        let mut custom_instructions = options.custom_instructions.clone();
        let mut replace_instructions = options.replace_instructions;
        let mut label = options.label.clone();
        let controller_abort = AbortController::new();
        let abort = controller_abort.signal();
        *lock(&self.inner.branch_summary_abort_controller) = Some(controller_abort.clone());

        let runner = self.runner();
        let mut extension_summary = None;
        if runner.has_handlers("session_before_tree") {
            let mut event = crate::core::extensions::types::ExtensionEvent::SessionBeforeTree(
                crate::core::extensions::types::SessionBeforeTreeEvent {
                    preparation: crate::core::extensions::types::TreePreparation {
                        target_id: target_id.to_owned(),
                        old_leaf_id: old_leaf_id.clone(),
                        common_ancestor_id: common_ancestor_id.clone(),
                        entries_to_summarize: entries_to_summarize.clone(),
                        user_wants_summary: options.summarize.unwrap_or(false),
                        custom_instructions: custom_instructions.clone(),
                        replace_instructions,
                        label: label.clone(),
                    },
                    signal: Some(abort.clone()),
                },
            );
            if let crate::core::extensions::runner::EmitResult::SessionBeforeTree(result) =
                runner.emit(&mut event).await
            {
                if result.cancel == Some(true) {
                    *lock(&self.inner.branch_summary_abort_controller) = None;
                    return Ok(NavigateTreeResult {
                        editor_text: None,
                        cancelled: true,
                        aborted: None,
                        summary_entry: None,
                    });
                }
                if let Some(summary) = result.summary
                    && options.summarize == Some(true)
                {
                    extension_summary = Some(summary);
                }
                if let Some(instructions) = result.custom_instructions {
                    custom_instructions = Some(instructions);
                }
                if let Some(replace) = result.replace_instructions {
                    replace_instructions = Some(replace);
                }
                if let Some(next_label) = result.label {
                    label = Some(next_label);
                }
            }
        }

        let mut summary_text: Option<JsString> = None;
        let mut summary_details = None;
        let mut from_extension = false;
        if options.summarize == Some(true) && !entries_to_summarize.is_empty() && extension_summary.is_none() {
            let model = self.inner.agent.model();
            let auth = self.get_compaction_request_auth(&model).await.map_err(|error| {
                *lock(&self.inner.branch_summary_abort_controller) = None;
                error
            })?;
            let reserve_tokens = lock(&self.inner.settings_manager)
                .get_branch_summary_settings()
                .reserve_tokens;
            let stream_fn = self.inner.agent.stream_fn();
            let result = crate::core::compaction::branch_summarization::generate_branch_summary(
                &entries_to_summarize,
                &crate::core::compaction::branch_summarization::GenerateBranchSummaryOptions {
                    model: &auth.model,
                    api_key: auth.api_key.clone(),
                    headers: auth.headers.clone(),
                    env: auth.env.clone(),
                    signal: Some(abort.clone()),
                    custom_instructions: custom_instructions.as_ref(),
                    replace_instructions: replace_instructions.unwrap_or(false),
                    reserve_tokens: Some(reserve_tokens),
                    stream_fn: Some(&stream_fn),
                },
            )
            .await
            .map_err(|failure| {
                *lock(&self.inner.branch_summary_abort_controller) = None;
                error(failure.message().clone())
            })?;
            if result.aborted == Some(true) {
                *lock(&self.inner.branch_summary_abort_controller) = None;
                return Ok(NavigateTreeResult {
                    editor_text: None,
                    cancelled: true,
                    aborted: Some(true),
                    summary_entry: None,
                });
            }
            if let Some(summary_error) = result.error {
                *lock(&self.inner.branch_summary_abort_controller) = None;
                return Err(error(summary_error));
            }
            summary_text = result.summary;
            summary_details = Some(pi_js_compat::js_json!({
                "readFiles": result.read_files.unwrap_or_default(),
                "modifiedFiles": result.modified_files.unwrap_or_default(),
            }));
        } else if let Some(summary) = extension_summary {
            summary_text = Some(summary.summary);
            summary_details = summary.details;
            from_extension = true;
        }

        let (new_leaf_id, editor_text) = match target_entry.known().as_deref() {
            Some(crate::core::session_manager::KnownSessionEntry::Message(message))
                if message.message.role() == "user" =>
            {
                let text = match message.message.as_ref() {
                    AgentMessage::User(user) => user_message_text(&user.content),
                    _ => JsString::default(),
                };
                (message.parent_id.clone(), Some(text))
            }
            Some(crate::core::session_manager::KnownSessionEntry::CustomMessage(message)) => {
                let text = match &message.content {
                    CustomMessageContent::Text(text) => text.clone(),
                    CustomMessageContent::Blocks(blocks) => JsString::join(
                        blocks.iter().filter_map(|block| match block {
                            TextOrImageContent::Text(text) => Some(&text.text),
                            TextOrImageContent::Image(_) => None,
                        }),
                        "",
                    ),
                };
                (message.parent_id.clone(), Some(text))
            }
            _ => (Some(target_id.to_owned()), None),
        };

        let mut summary_entry = None;
        let had_summary = summary_text.is_some();
        {
            let mut manager = lock(&self.inner.session_manager);
            if let Some(text) = summary_text {
                let summary_id =
                    manager.branch_with_summary(new_leaf_id.as_ref(), &text, summary_details, Some(from_extension))?;
                summary_entry = manager.get_entry(&summary_id).cloned();
                if let Some(label) = &label {
                    manager.append_label_change(&summary_id, Some(label))?;
                }
            } else {
                if let Some(new_leaf_id) = &new_leaf_id {
                    manager.branch(new_leaf_id)?;
                } else {
                    manager.reset_leaf();
                }
                if let Some(label) = &label {
                    manager.append_label_change(target_id, Some(label))?;
                }
            }
        }
        let (session_context, identities) = lock(&self.inner.session_manager).build_session_context_with_identities();
        self.inner
            .agent
            .set_messages_with_identities(session_context.messages, identities)?;
        let mut event = crate::core::extensions::types::ExtensionEvent::SessionTree(
            crate::core::extensions::types::SessionTreeEvent {
                new_leaf_id: lock(&self.inner.session_manager).get_leaf_id().cloned(),
                old_leaf_id,
                summary_entry: summary_entry.clone().and_then(|entry| match entry.known().as_deref() {
                    Some(crate::core::session_manager::KnownSessionEntry::BranchSummary(summary)) => {
                        Some(summary.clone())
                    }
                    _ => None,
                }),
                from_extension: had_summary.then_some(from_extension),
            },
        );
        runner.emit(&mut event).await;
        *lock(&self.inner.branch_summary_abort_controller) = None;
        Ok(NavigateTreeResult {
            editor_text,
            cancelled: false,
            aborted: None,
            summary_entry,
        })
    }

    pub fn set_working_directory_recovery_handlers(&self, handlers: WorkingDirectoryRecoveryHandlers) {
        *lock(&self.inner.working_directory_recovery) = Some(handlers);
    }

    async fn recover_working_directory_before_run(&self) -> Result<Option<Self>, AgentSessionError> {
        if self.is_streaming() {
            return Ok(None);
        }
        let before_run = lock(&self.inner.working_directory_recovery)
            .as_ref()
            .map(|handlers| handlers.before_run.clone());
        let Some(before_run) = before_run else {
            return Ok(None);
        };
        let replacement = before_run().await.map_err(AgentSessionError::from)?;
        Ok(replacement.filter(|session| !Arc::ptr_eq(&self.inner, &session.inner)))
    }

    /// Binds the runtime-owned working-directory relocation handlers on the extension runner.
    ///
    /// TypeScript captures `this` inside `AgentSessionRuntime`; Rust callers supply the
    /// closures because the runtime owns the session and cannot reference itself during
    /// construction.
    pub fn bind_session_runtime_relocation(
        &self,
        relocate: crate::core::extensions::types::RelocateCallback,
        prepare_tool_result: crate::core::extensions::types::RelocateCallback,
        activate_tool_result: crate::core::extensions::types::ActivateToolResultRelocationHandler,
    ) {
        let mut runner = self.runner();
        runner.set_relocate_handler(relocate);
        runner.set_tool_result_relocation_handlers(prepare_tool_result, activate_tool_result);
    }

    /// All user messages from the session, for the fork selector.
    pub fn get_user_messages_for_forking(&self) -> Vec<UserMessageForForking> {
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
                result.push(UserMessageForForking {
                    entry_id: message.id.clone(),
                    text,
                });
            }
        }
        result
    }

    // =========================================================================
    // Statistics / export
    // =========================================================================

    pub fn get_session_stats(&self) -> SessionStats {
        let messages = self.messages();
        let totals = messages.iter().fold(SessionStatsTotals::default(), add_session_message);
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
    pub fn export_to_jsonl(&self, output_path: Option<&JsString>) -> Result<JsString, AgentSessionError> {
        let default_name: JsString = format!(
            "session-{}.jsonl",
            crate::utils::iso_time::now_iso_string().replace([':', '.'], "-")
        )
        .into();
        let file_path =
            crate::utils::paths::resolve_path(output_path.unwrap_or(&default_name), "", &Default::default())
                .map_err(|path_error| error(path_error.message()))?;
        let parent = crate::utils::paths::node_dirname(&file_path);
        std::fs::create_dir_all(encode_node_utf8(&parent))
            .map_err(|io_error| error(format!("Failed to create export directory: {io_error}")))?;
        let (session_id, cwd, branch_entries) = {
            let manager = lock(&self.inner.session_manager);
            (
                manager.get_session_id().to_owned(),
                manager.get_cwd().to_owned(),
                manager.get_branch(None),
            )
        };
        let mut header = JsObject::new();
        header.insert("type".into(), JsValue::String("session".into()));
        header.insert(
            "version".into(),
            JsValue::Number(crate::core::session_manager::CURRENT_SESSION_VERSION as f64),
        );
        header.insert("id".into(), JsValue::String(session_id));
        header.insert(
            "timestamp".into(),
            JsValue::String(crate::utils::iso_time::now_iso_string().into()),
        );
        header.insert("cwd".into(), JsValue::String(cwd));
        let mut lines = vec![pi_js_compat::js_json::stringify_lossless(&JsValue::Object(header))];
        let mut previous_id: Option<JsString> = None;
        for entry in branch_entries {
            let mut value =
                pi_js_compat::js_json::to_value(&entry).map_err(|json_error| error(json_error.to_string()))?;
            if let JsValue::Object(object) = &mut value {
                object.insert(
                    "parentId".into(),
                    previous_id.clone().map(JsValue::String).unwrap_or(JsValue::Null),
                );
            }
            previous_id = entry.id().cloned();
            lines.push(pi_js_compat::js_json::stringify_lossless(&value));
        }
        std::fs::write(encode_node_utf8(&file_path), format!("{}\n", lines.join("\n")))
            .map_err(|io_error| error(format!("Failed to write session export: {io_error}")))?;
        Ok(file_path)
    }

    /// Exports the current session branch to an HTML file.
    pub async fn export_to_html(&self, output_path: Option<&JsString>) -> Result<JsString, AgentSessionError> {
        use crate::core::export_html::{
            ExportOptions, ToolHtmlRendererDeps, create_tool_html_renderer, export_session_to_html,
        };
        use crate::modes::interactive::theme::{current_theme_if_initialized, get_theme_by_name};

        let configured_theme_name = lock(&self.inner.settings_manager).get_theme().cloned();
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
            completion_executor: None,
        });
        let state = self.state();
        let session_manager = self.session_manager().clone();
        export_session_to_html(
            &session_manager,
            Some(&state),
            Some(ExportOptions {
                output_path: output_path.cloned(),
                theme_name,
                tool_renderer: Some(tool_renderer),
            }),
        )
        .await
        .map_err(|export_error| error(export_error.message()))
    }

    /// Text content of the last assistant message.
    pub fn get_last_assistant_text(&self) -> Option<JsString> {
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
    pub fn has_extension_handlers(&self, event_type: &str) -> bool {
        self.runner().has_handlers(event_type)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BashExecutionWire<'a> {
    role: &'static str,
    command: &'a JsString,
    output: &'a JsString,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    exit_code: Option<f64>,
    cancelled: bool,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    full_output_path: Option<&'a JsString>,
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
        label: definition.name.into(),
        description: definition.description,
        prompt_snippet: definition.prompt_snippet,
        prompt_guidelines: Some(definition.prompt_guidelines),
        approval_required: Some(definition.approval_required),
        approval_kind: definition.approval_kind,
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
        full_output_path: message.full_output_path.as_ref(),
        timestamp: message.timestamp,
        exclude_from_context: message.exclude_from_context,
    };
    AgentMessage::Custom(
        pi_js_compat::js_json::to_value(&wire)
            .expect("bash message serializes")
            .as_object()
            .expect("bash message object")
            .clone(),
    )
}

/// Re-export used by `sdk` and `agent-session-services`.
pub use crate::core::resource_loader::{
    ExtendedResourcePath, LoadedAgentsFiles, LoadedSkills, ResourceExtensionPaths, StaticResourceLoader,
};

// =============================================================================
// Extension integration
// =============================================================================

use crate::core::extensions::runner::{
    BeforeAgentStartCombinedResult, EmitResult, ErrorSubscription, ExtensionErrorListener, ExtensionRunner,
    emit_session_shutdown_event,
};
use crate::core::extensions::types::ToolCallEvent;
use crate::core::extensions::types::{
    AgentEndEvent as ExtensionAgentEndEvent, AgentStartEvent, CompactionEvent, CompactionReason, ExtensionActions,
    ExtensionCommandContext, ExtensionCommandContextActions, ExtensionContextActions, ExtensionEvent, ExtensionMode,
    ExtensionUiContext, HandlerError, InputEventResult, InputSource as ExtensionInputSource, MessageDelivery,
    MessageEndEvent, MessageStartEvent, MessageUpdateEvent, ModelSelectEvent, ModelSelectSource, ProviderActions,
    SessionBeforeCompactEvent, SessionShutdownEvent, SessionStartEvent, SetThinkingLevelHandler,
    StreamingBehavior as ExtensionStreamingBehavior, ThinkingLevelSelectEvent, ToolExecutionEndEvent,
    ToolExecutionStartEvent, ToolExecutionUpdateEvent, ToolInfo as ExtensionToolInfo, TurnEndEvent, TurnStartEvent,
    UserMessageContent, UserMessageDelivery,
};
use crate::core::extensions::wrapper::{wrap_definition_tool, wrap_registered_tool};
use crate::core::footer_data_provider::FooterDataProvider;
use crate::core::slash_commands::SlashCommandInfo;

/// `ExtensionBindings` passed to [`AgentSession::bind_extensions`].
#[derive(Default)]
pub struct ExtensionBindings {
    pub ui_context: Option<Arc<dyn ExtensionUiContext>>,
    pub footer_data: Option<Arc<FooterDataProvider>>,
    pub mode: Option<ExtensionMode>,
    pub control_db_path: Option<JsString>,
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
    pub async fn bind_extensions(&self, bindings: ExtensionBindings) -> Result<(), AgentSessionError> {
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
        self.start_runtime_mailbox_signal_wake();
        self.start_runtime_mailbox_polling();
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
        self.extend_resources_from_extensions().await?;
        self.schedule_idle_compaction();
        Ok(())
    }

    /// Reloads settings, resources, and extensions (`AgentSession.reload`).
    pub async fn reload(
        &self,
        before_session_start: Option<Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>>,
    ) -> Result<(), AgentSessionError> {
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
        lock(&self.inner.settings_manager).reload();
        let (steering_mode, follow_up_mode) = {
            let settings = lock(&self.inner.settings_manager);
            (settings.get_steering_mode(), settings.get_follow_up_mode())
        };
        self.inner.agent.set_steering_mode(steering_mode);
        self.inner.agent.set_follow_up_mode(follow_up_mode);
        pi_ai::compat::reset_api_providers();
        self.inner
            .resource_loader
            .reload()
            .await
            .map_err(AgentSessionError::new)?;
        self.build_runtime(
            Some(self.get_active_tool_names()),
            Some(flag_values_map(&previous_flag_values)),
            true,
        )?;

        let has_bindings = lock(&self.inner.extension_ui_context).is_some()
            || lock(&self.inner.extension_command_context_actions).is_some()
            || lock(&self.inner.extension_shutdown_handler).is_some()
            || lock(&self.inner.extension_error_listener).is_some();
        if !has_bindings {
            return Ok(());
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
            .await?;
        self.schedule_idle_compaction();
        Ok(())
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

    async fn extend_resources_from_extensions(&self) -> Result<(), AgentSessionError> {
        let reason =
            if self.inner.session_start_event.reason == crate::core::extensions::types::SessionStartReason::Reload {
                crate::core::extensions::types::ResourcesDiscoverReason::Reload
            } else {
                crate::core::extensions::types::ResourcesDiscoverReason::Startup
            };
        self.extend_resources_with_reason(reason).await
    }

    async fn extend_resources_with_reason(
        &self,
        reason: crate::core::extensions::types::ResourcesDiscoverReason,
    ) -> Result<(), AgentSessionError> {
        let runner = self.runner();
        if !runner.has_handlers("resources_discover") {
            return Ok(());
        }
        let paths = runner.emit_resources_discover(&self.inner.cwd, reason).await;
        if paths.skill_paths.is_empty() && paths.prompt_paths.is_empty() && paths.theme_paths.is_empty() {
            return Ok(());
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
                            .then(|| crate::utils::paths::node_dirname(&entry.extension_path)),
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
            })
            .map_err(AgentSessionError::new)?;
        let prompt = self.rebuild_system_prompt(&self.get_active_tool_names())?;
        *lock(&self.inner.base_system_prompt) = prompt.clone();
        self.inner.agent.set_system_prompt(prompt);
        Ok(())
    }

    fn create_internal_commands(
        &self,
    ) -> indexmap::IndexMap<String, crate::core::extensions::types::InternalCommandHandler> {
        let handler: crate::core::extensions::types::InternalCommandHandler =
            Arc::new(move |args: JsString, ctx: ExtensionCommandContext| {
                let model_reference = args.trim();
                Box::pin(async move {
                    if model_reference.is_empty() {
                        return Ok(());
                    }
                    let scoped = ctx.get_scoped_models()?;
                    let models = if scoped.is_empty() {
                        ctx.model_registry()?.get_available()?
                    } else {
                        scoped.into_iter().map(|scoped| scoped.model).collect()
                    };
                    let Some(model) = find_exact_model_reference_match(&model_reference, &models) else {
                        return Err(crate::core::extensions::types::HandlerError::new(
                            JsString::from("Model not found or not authenticated: ") + model_reference,
                        ));
                    };
                    let provider = model.provider.clone();
                    let model_id = model.id.clone();
                    if !ctx.set_model(model).await? {
                        return Err(crate::core::extensions::types::HandlerError::new(format!(
                            "No API key for {provider}/{model_id}"
                        )));
                    }
                    Ok(())
                })
            });
        let mut commands = indexmap::IndexMap::new();
        commands.insert("model".to_owned(), handler);
        commands
    }

    fn bind_extension_core(&self, runner: &mut ExtensionRunner) -> Result<(), CallbackError> {
        let weak = Arc::downgrade(&self.inner);
        let actions = extension_actions(weak.clone());
        let context_actions = extension_context_actions(weak.clone());
        let provider_actions = extension_provider_actions(weak);
        runner.bind_core(actions, context_actions, Some(provider_actions))
    }

    async fn emit_extension_event(
        &self,
        event: &mut AgentEvent,
        session_continuation: Option<crate::core::extensions::types::AgentEndSessionContinuation>,
    ) -> Option<AgentMessage> {
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
                    session_continuation,
                });
                runner.emit(&mut extension_event).await;
                None
            }
            AgentEvent::TurnStart => {
                let mut extension_event = ExtensionEvent::TurnStart(TurnStartEvent {
                    turn_index: self.inner.turn_index.load(Ordering::SeqCst) as f64,
                    timestamp: pi_js_compat::date::js_now_millis() as f64,
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
                    partial_result: pi_js_compat::js_json::to_value(partial_result).expect("partial result serializes"),
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
            AgentEvent::ModelRequestStart => {
                runner
                    .emit(&mut ExtensionEvent::ModelRequestStart(
                        crate::core::extensions::types::ModelRequestStartEvent {},
                    ))
                    .await;
                None
            }
            AgentEvent::ModelRequestEnd => {
                runner
                    .emit(&mut ExtensionEvent::ModelRequestEnd(
                        crate::core::extensions::types::ModelRequestEndEvent {},
                    ))
                    .await;
                None
            }
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

    async fn emit_session_info_changed(&self, name: Option<JsString>) {
        let mut event =
            ExtensionEvent::SessionInfoChanged(crate::core::extensions::types::SessionInfoChangedEvent { name });
        self.runner().emit(&mut event).await;
    }

    async fn try_execute_extension_command(
        &self,
        text: &JsString,
        options: &PromptOptions,
    ) -> Result<bool, AgentSessionError> {
        let (command_name, args) = split_prompt_command(text);
        let runner = self.runner();
        let Some(command) = runner.get_prompt_command(&command_name) else {
            return Ok(false);
        };
        if let Some(replacement) = self.recover_working_directory_before_run().await? {
            Box::pin(replacement.prompt(text.clone(), options.clone())).await?;
            return Ok(true);
        }
        let context = self.create_command_context_for(&command.invocation_name);
        if let Err(error) = (command.handler)(args, context).await {
            runner.emit_error(crate::core::extensions::types::ExtensionError {
                extension_path: pi_js_compat::js_format!("command:{}", command.invocation_name),
                event: "command".to_owned(),
                error: error.message,
                stack: None,
            });
        }
        self.record_handled_slash_command_history(text);
        if let Some(preflight) = &options.preflight_result {
            preflight(true);
        }
        Ok(true)
    }

    fn record_handled_slash_command_history(&self, text: &JsString) {
        let Some(control_db_path) = &self.inner.control_db_path else {
            return;
        };
        if let Err(error) =
            crate::core::session_control_db::record_prompt_history_entry(control_db_path, &encode_node_utf8(text))
        {
            eprintln!("Failed to persist prompt history for handled slash command: {error}");
        }
    }

    async fn emit_extension_input(
        &self,
        text: &JsString,
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
                (!images.is_empty()).then(|| crate::core::extensions::types::InputImages::new(images.to_vec())),
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
        text: &JsString,
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
        reason: result.reason.map(JsString::from),
    }
}

fn extension_source_label(extension_path: &JsString) -> JsString {
    if extension_path.starts_with('<') {
        return JsString::from("extension:") + extension_path.replace('<', "").replace('>', "");
    }
    let base = crate::utils::paths::node_basename(extension_path);
    let name = base
        .strip_suffix(".ts")
        .or_else(|| base.strip_suffix(".js"))
        .unwrap_or(base);
    JsString::from("extension:") + name
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

fn send_custom_message_from_extension(
    session: &AgentSession,
    message: crate::core::extensions::types::CustomMessagePayload,
    options: Option<crate::core::extensions::types::SendMessageOptions>,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    let content = match message.content {
        crate::core::extensions::types::CustomMessageContent::Text(text) => CustomMessageContent::Text(text),
        crate::core::extensions::types::CustomMessageContent::Blocks(blocks) => CustomMessageContent::Blocks(blocks),
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
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return Ok(());
    };
    let session = session.clone();
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
    Ok(())
}

fn send_user_message_from_extension(
    session: &AgentSession,
    content: UserMessageContent,
    options: Option<crate::core::extensions::types::SendUserMessageOptions>,
) -> Result<(), crate::core::extensions::types::HandlerError> {
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
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return Ok(());
    };
    let session = session.clone();
    handle.spawn(async move {
        let _ = session.send_user_message(content, deliver_as, expand).await;
    });
    Ok(())
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
            send_custom_message_from_extension(&session, message, options)
        },
    );
    let send_user_weak = weak.clone();
    let send_user_message: crate::core::extensions::types::SendUserMessageHandler = Arc::new(
        move |content: UserMessageContent, options: Option<crate::core::extensions::types::SendUserMessageOptions>| {
            let Some(inner) = send_user_weak.upgrade() else {
                return Ok(());
            };
            let session = AgentSession { inner };
            send_user_message_from_extension(&session, content, options)
        },
    );
    let append_weak = weak.clone();
    let append_entry: crate::core::extensions::types::AppendEntryHandler =
        Arc::new(move |custom_type: &JsString, data: Option<JsValue>| {
            let Some(inner) = append_weak.upgrade() else {
                return Ok(());
            };
            let session = AgentSession { inner };
            let entry_id = session
                .session_manager()
                .append_custom_entry(custom_type, data)
                .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message()))?;
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
    let set_session_name: crate::core::extensions::types::SetSessionNameHandler = Arc::new(move |name: &JsString| {
        let Some(inner) = name_weak.upgrade() else {
            return Ok(());
        };
        AgentSession { inner }
            .set_session_name(name)
            .map_err(|failure| crate::core::extensions::types::HandlerError::new(failure.message()))
    });
    let get_name_weak = weak.clone();
    let get_session_name: crate::core::extensions::types::GetSessionNameHandler =
        Arc::new(move || match get_name_weak.upgrade() {
            Some(inner) => Ok(AgentSession { inner }.session_name()),
            None => Ok(None),
        });
    let label_weak = weak.clone();
    let set_label: crate::core::extensions::types::SetLabelHandler =
        Arc::new(move |entry_id: &JsString, label: Option<&JsString>| {
            let Some(inner) = label_weak.upgrade() else {
                return Ok(());
            };
            AgentSession { inner }
                .session_manager()
                .append_label_change(entry_id, label)
                .map(|_| ())
                .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message()))
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
                AgentSession { inner }
                    .set_active_tools_by_name(tool_names)
                    .map_err(crate::core::extensions::types::HandlerError::from)?;
            }
            Ok(())
        });
    let call_tool_weak = weak.clone();
    let call_tool: crate::core::extensions::types::CallToolHandler = Arc::new(
        move |name: String, params: JsValue, signal: Option<AbortSignal>, tool_call_id: Option<String>| {
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
        Arc::new(move |name: JsString, args: Option<JsString>| {
            let Some(inner) = command_weak.upgrade() else {
                return Box::pin(async {
                    Err(crate::core::extensions::types::HandlerError::new("Session is disposed"))
                });
            };
            let session = AgentSession { inner };
            Box::pin(async move {
                let runner = session.runner();
                let Some(command) = runner.get_prompt_command(&name) else {
                    return Err(crate::core::extensions::types::HandlerError::new(
                        pi_js_compat::js_format!("Command not found: {}", name),
                    ));
                };
                let context = session.create_command_context_for(&name);
                (command.handler)(args.unwrap_or_default(), context)
                    .await
                    .map(|()| JsValue::Null)
            })
        });
    let refresh_weak = weak.clone();
    let refresh_tools: Arc<dyn Fn() -> Result<(), crate::core::extensions::types::HandlerError> + Send + Sync> =
        Arc::new(move || {
            if let Some(inner) = refresh_weak.upgrade() {
                AgentSession { inner }
                    .refresh_tool_registry(None, false)
                    .map_err(crate::core::extensions::types::HandlerError::from)?;
            }
            Ok(())
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
            if !session.model_registry().has_configured_auth(&model)? {
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
                AgentSession { inner }
                    .set_thinking_level(level)
                    .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message()))?;
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

// Frozen agent-session.ts:5432-5438: completion is inside the catch, error callback is not.
fn complete_extension_compaction(
    options: Option<&crate::core::extensions::types::CompactOptions>,
    result: Result<crate::core::compaction::CompactionResult, pi_js_compat::callback_error::CallbackError>,
) -> Result<(), pi_js_compat::callback_error::CallbackError> {
    let completed = result.and_then(|result| {
        if let Some(callback) = options.and_then(|options| options.on_complete.as_ref()) {
            callback(&result)
        } else {
            Ok(())
        }
    });
    let Err(error) = completed else {
        return Ok(());
    };
    let error = match error.thrown_value() {
        pi_js_compat::abort::AbortReason::Value(_) => {
            pi_js_compat::callback_error::CallbackError::new(error.error_or_string_message()?)
        }
        _ => error,
    };
    if let Some(callback) = options.and_then(|options| options.on_error.as_ref()) {
        callback(&error)?;
    }
    Ok(())
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
    let set_thinking_weak = weak.clone();
    let set_thinking_level: Option<SetThinkingLevelHandler> = Some(Arc::new(move |level| {
        let inner = set_thinking_weak
            .upgrade()
            .ok_or_else(|| HandlerError::new("Session is disposed"))?;
        AgentSession { inner }
            .set_thinking_level(level)
            .map_err(|error| HandlerError::new(error.message()))
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
    let get_signal: Arc<dyn Fn() -> Option<AbortSignal> + Send + Sync> =
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
    let restart: crate::core::extensions::types::RestartHandler = Arc::new(move |options| {
        let Some(inner) = restart_weak.upgrade() else {
            return Box::pin(async {
                Err(crate::core::extensions::types::HandlerError::new(
                    "Restart is not available in this session mode",
                ))
            }) as BoxFuture<'static, Result<(), crate::core::extensions::types::HandlerError>>;
        };
        let session = AgentSession { inner };
        if session.inner.multi_agent_runtime_role == Some(MultiAgentRuntimeRole::Child) {
            return Box::pin(async {
                Err(crate::core::extensions::types::HandlerError::new(
                    "Child agent runtimes cannot restart Pi",
                ))
            });
        }
        let actions = lock(&session.inner.extension_command_context_actions).clone();
        Box::pin(async move {
            match actions {
                Some(actions) => (actions.restart)(options).await,
                None => Err(crate::core::extensions::types::HandlerError::new(
                    "Restart is not available in this session mode",
                )),
            }
        })
    });
    let control_weak = weak.clone();
    let get_control_db_path: Option<Arc<dyn Fn() -> Option<JsString> + Send + Sync>> = Some(Arc::new(move || {
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
                    let result = session
                        .compact(custom.as_ref())
                        .await
                        .map_err(pi_js_compat::callback_error::CallbackError::from);
                    if let Err(error) = complete_extension_compaction(options.as_ref(), result) {
                        crate::utils::uncaught::throw_callback_uncaught(error);
                    }
                });
            }
        });
    let prompt_weak = weak.clone();
    let get_system_prompt: Arc<dyn Fn() -> JsString + Send + Sync> = Arc::new(move || {
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
        set_thinking_level,
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
        get_multi_agent_agent_id: Some(Arc::new({
            let weak = weak.clone();
            move || weak.upgrade().and_then(|inner| inner.multi_agent_agent_id.clone())
        })),
        get_multi_agent_parent_session_id: Some(Arc::new({
            let weak = weak.clone();
            move || {
                weak.upgrade()
                    .and_then(|inner| inner.multi_agent_parent_session_id.clone())
            }
        })),
        get_no_supervisor: Some(Arc::new({
            let weak = weak.clone();
            move || weak.upgrade().is_some_and(|inner| inner.no_supervisor)
        })),
        get_multi_agent_requires_agent_id: Some(Arc::new({
            let weak = weak.clone();
            move || weak.upgrade().is_some_and(|inner| inner.multi_agent_requires_agent_id)
        })),
        get_multi_agent_store: None,
        get_tool_detach_registry: Some(Arc::new({
            let weak = weak.clone();
            move || weak.upgrade().and_then(|inner| inner.tool_detach_registry.clone())
        })),
        compact,
        get_system_prompt,
        get_system_prompt_options,
    }
}

fn extension_provider_actions(weak: Weak<Inner>) -> ProviderActions {
    let register_weak = weak.clone();
    let register_provider: Option<RegisterProviderAction> = Some(Arc::new(
        move |name: &str, config: &crate::core::model_registry::ProviderConfigInput| {
            let Some(inner) = register_weak.upgrade() else {
                return Err("Session is disposed".into());
            };
            // Frozen agent-session.ts:5445–5451 propagates registration before
            // refreshing the selected model, including raw callback rejections.
            inner.model_registry.register_provider(name, config.clone())?;
            refresh_current_model_from_registry(&inner);
            Ok(())
        },
    ));
    let unregister_weak = weak;
    let unregister_provider: Option<UnregisterProviderAction> = Some(Arc::new(move |name: &str| {
        if let Some(inner) = unregister_weak.upgrade() {
            inner.model_registry.unregister_provider(name)?;
            refresh_current_model_from_registry(&inner);
        }
        Ok(())
    }));
    ProviderActions {
        register_provider,
        unregister_provider,
    }
}

fn refresh_current_model_from_registry(inner: &Arc<Inner>) {
    let current = inner.agent.model();
    if let Some(refreshed) = inner.model_registry.find(&current.provider, &current.id) {
        inner.agent.set_model(refreshed);
    }
}

impl AgentSession {
    fn create_command_context(&self) -> ExtensionCommandContext {
        self.runner().create_command_context()
    }

    /// `_createCommandContext`: `model` and `effort` commands from a main
    /// session act on the selected child session when one is being viewed.
    fn create_command_context_for(&self, command_name: &JsString) -> ExtensionCommandContext {
        let context = self.create_command_context();
        if self.inner.multi_agent_agent_id.is_some() || (command_name != "model" && command_name != "effort") {
            return context;
        }
        let resolver = lock(&self.inner.session_mutation_target_resolver).clone();
        let Some(resolver) = resolver else {
            return context;
        };
        ExtensionCommandContext {
            context: context.context.with_viewed_session_mutation_target(resolver),
        }
    }

    /// `createReplacedSessionContext`.
    pub fn create_replaced_session_context(&self) -> crate::core::extensions::types::ReplacedSessionContext {
        let command = self.runner().create_command_context();
        let send_session = self.clone();
        let send_message: crate::core::extensions::types::ReplacedSendMessageCallback =
            Arc::new(move |message, options| {
                let session = send_session.clone();
                Box::pin(async move { send_custom_message_from_extension(&session, message, options) })
            });
        let send_user_session = self.clone();
        let send_user_message: crate::core::extensions::types::ReplacedSendUserMessageCallback =
            Arc::new(move |content, options| {
                let session = send_user_session.clone();
                Box::pin(async move { send_user_message_from_extension(&session, content, options) })
            });
        crate::core::extensions::types::ReplacedSessionContext {
            command,
            send_message,
            send_user_message,
        }
    }

    fn slash_commands(&self) -> Vec<SlashCommandInfo> {
        let runner = self.runner();
        let mut commands: Vec<SlashCommandInfo> = runner
            .get_registered_commands()
            .into_iter()
            .map(|command| SlashCommandInfo {
                name: command.invocation_name.into(),
                description: command.description.map(Into::into),
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
                name: JsString::from("skill:") + skill.name,
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
        params: JsValue,
        signal: Option<AbortSignal>,
        active_tool_call_id: Option<String>,
    ) -> Result<pi_agent_core::agent::types::AgentToolResult, crate::core::extensions::types::HandlerError> {
        if self.inner.no_supervisor
            && crate::core::tool_capabilities::STANDALONE_DISABLED_TOOL_NAMES.contains(&tool_name)
        {
            return Err(crate::core::extensions::types::HandlerError::new(js_format!(
                "Tool {} is unavailable in standalone worker mode",
                tool_name
            )));
        }
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
            let mut object = JsObject::new();
            object.insert("value".into(), params.clone());
            object
        });
        let tool_call = AgentToolCall {
            r#type: pi_ai::types::ToolCallTag::Tag,
            id: tool_call_id.clone(),
            name: tool_name.to_owned(),
            arguments,
            index: None,
            partial_json: None,
            partial_args: None,
            stream_index: None,
            execution: None,
            thought_signature: None,
            namespace: None,
        };
        let tool_spec = pi_ai::types::Tool::new(tool_name, tool.description(), tool.parameters().clone());
        let validated = pi_ai::utils::validation::validate_tool_arguments(&tool_spec, &tool_call)
            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message()))?;
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
            .await
            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message()))?;
        if before.as_ref().and_then(|result| result.block) == Some(true) {
            let reason = before
                .and_then(|result| result.reason)
                .unwrap_or_else(|| "Tool execution was blocked".to_owned().into());
            return Ok(pi_agent_core::agent::types::AgentToolResult::text(reason));
        }
        let result = tool
            .execute(tool_call_id, validated.clone(), signal, Arc::new(|_| Ok(())), None)
            .await
            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.message))?;
        Ok(result)
    }
}

fn synthetic_tool_call_assistant_message(model: &Model, tool_call: &AgentToolCall) -> AssistantMessage {
    let mut message = AssistantMessage::empty(model.api.clone(), model.provider.clone(), model.id.clone(), 0.0);
    message.content = vec![AssistantContent::ToolCall(tool_call.clone())];
    message.stop_reason = StopReason::ToolUse;
    message.timestamp = pi_js_compat::date::js_now_millis();
    message
}

fn parse_timestamp_millis(text: &JsString) -> Option<i64> {
    let millis = pi_js_compat::date::date_parse_utf16(text.units());
    millis.is_finite().then_some(millis as i64)
}

#[cfg(test)]
mod supervisor_project_failure_tests {
    mod scripted_registry {
        use crate as pi_coding_agent;
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/scripted_registry.rs"
        ));
    }
    use super::*;

    #[test]
    fn compaction_summary_probe_uses_epoch_zero_date_string() {
        // Frozen coding-agent/src/core/agent-session.ts:701–709 passes new Date(0).toISOString().
        let summary = JsString::from_utf16(vec![0xd800]);
        let AgentMessage::Custom(message) = create_compaction_summary_probe(&summary, 12.5) else {
            unreachable!()
        };
        assert_eq!(message.get("timestamp").unwrap().as_f64(), Some(0.0));
        assert_eq!(message.get("summary").unwrap().as_string(), Some(&summary));
        assert_eq!(message.get("tokensBefore").unwrap().as_f64(), Some(12.5));
    }

    #[test]
    fn human_tool_prompt_and_resource_labels_keep_native_units() {
        let raw = JsString::from_utf16(vec![0xd800]);
        let snippet = JsString::from(" \n") + &raw + "\u{feff}  text\n";
        assert_eq!(normalize_prompt_snippet(Some(&snippet)), Some(raw.clone() + " text"));
        assert_eq!(
            normalize_prompt_guidelines(&[raw.clone(), raw.clone() + " "]),
            vec![raw.clone()]
        );
        assert_eq!(
            extension_source_label(&(JsString::from("/extensions/") + &raw + ".ts")),
            JsString::from("extension:") + raw
        );
    }

    #[test]
    fn project_resolution_error_propagates_before_supervisor_request() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("memory/supervisor");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("projects.json"), r#"{"\ud800":0}"#).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "core::agent_session::supervisor_project_failure_tests::project_resolution_error_child",
                "--ignored",
                "--nocapture",
            ])
            .env("PI_KB_DIR", temp.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }

    #[tokio::test]
    #[ignore = "isolated process environment fixture"]
    async fn project_resolution_error_child() {
        use crate::core::sdk::{CreateAgentSessionOptions, NoTools, create_agent_session};
        use pi_ai::providers::faux::{FauxModelDefinition, RegisterFauxProviderOptions, faux_provider};
        use std::sync::atomic::{AtomicBool, Ordering};
        let directory = tempfile::tempdir().unwrap();
        let cwd = JsString::from(directory.path().to_str().unwrap());
        let runtime = crate::core::model_registry::ModelRegistry::in_memory(
            crate::core::auth_storage::AuthStorage::in_memory(Vec::<(String, serde_json::Value)>::new()),
        )
        .unwrap();
        let faux = faux_provider(RegisterFauxProviderOptions {
            provider: Some("supervisor-project-test".to_owned()),
            models: Some(vec![FauxModelDefinition {
                id: "test".to_owned(),
                name: None,
                reasoning: None,
                input: None,
                cost: None,
                context_window: None,
                max_tokens: None,
            }]),
            ..Default::default()
        });
        scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
        let mut settings =
            crate::core::settings_manager::SettingsManager::in_memory(Default::default(), Default::default());
        settings
            .set_approval_preset(ApprovalPresetName::LlmApprovedDeny, SettingsScope::Global)
            .expect("approval preset");
        let called = Arc::new(AtomicBool::new(false));
        let request_called = Arc::clone(&called);
        let session = create_agent_session(CreateAgentSessionOptions {
            cwd: Some(cwd.clone()),
            agent_dir: Some(cwd.clone()),
            model: Some(faux.get_model(None).unwrap()),
            model_registry: Some(runtime),
            no_tools: Some(NoTools::All),
            resource_loader: Some(crate::core::resource_loader::StaticResourceLoader::new()),
            session_manager: Some(crate::core::session_manager::SessionManager::in_memory(Some(&cwd), None).unwrap()),
            settings_manager: Some(Arc::new(Mutex::new(settings))),
            no_supervisor: None,
            supervisor_decision_requester: Some(Arc::new(move |_| {
                request_called.store(true, Ordering::SeqCst);
                Box::pin(async {
                    Ok(crate::core::session_control_db::SupervisorResponse::Approve {
                        reason: "unexpected".into(),
                    })
                })
            })),
            ..Default::default()
        })
        .await
        .unwrap()
        .session;
        let event = ToolCallEvent {
            tool_name: crate::core::extensions::types::ToolName::Custom("fixture-write".to_owned()),
            tool_call_id: "call".to_owned(),
            bypass_permissions: false,
            input: JsObject::new(),
        };
        let reviewer = session.create_tool_approval_llm_reviewer(&event, None).unwrap();
        let error = reviewer().await.unwrap_err();
        assert_eq!(
            error.message,
            JsString::from("Supervisor project mapping must be an object: ") + JsString::from_utf16(vec![0xd800])
        );
        assert!(!called.load(Ordering::SeqCst));
        session.dispose();
    }
}
