//! Native contracts for frozen harness/types.ts:493–839.
use super::compaction::CompactionPreparation;
use super::session::{Session, SessionTreeEntry};
use super::{
    AgentHarnessResources, AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, ExecutionEnv, QueueMode,
    ThinkingLevel,
};
use crate::agent::{AfterToolCallResult, AgentError, AgentEvent, AgentMessage, AgentTool};
use pi_ai::models::Models;
use pi_ai::request::BoxFuture;
use pi_ai::types::{ImageContent, Model, TextOrImageContent};
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentHarnessPhase {
    Idle,
    Turn,
    Compaction,
    BranchSummary,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentHarnessErrorCode {
    Busy,
    InvalidState,
    InvalidArgument,
    Session,
    Compaction,
    BranchSummary,
    Hook,
    Unknown,
}
#[derive(Debug, Clone)]
pub struct AgentHarnessError {
    pub code: AgentHarnessErrorCode,
    pub message: JsString,
    pub causes: Vec<JsString>,
}
impl AgentHarnessError {
    pub fn new(code: AgentHarnessErrorCode, message: impl Into<JsString>) -> Self {
        Self {
            code,
            message: message.into(),
            causes: Vec::new(),
        }
    }
}
impl std::fmt::Display for AgentHarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}
impl std::error::Error for AgentHarnessError {}
impl From<super::session::SessionError> for AgentHarnessError {
    fn from(error: super::session::SessionError) -> Self {
        Self::new(AgentHarnessErrorCode::Session, error.message)
    }
}
impl From<super::CompactionError> for AgentHarnessError {
    fn from(error: super::CompactionError) -> Self {
        Self::new(AgentHarnessErrorCode::Compaction, error.message)
    }
}
impl From<super::BranchSummaryError> for AgentHarnessError {
    fn from(error: super::BranchSummaryError) -> Self {
        Self::new(AgentHarnessErrorCode::BranchSummary, error.message)
    }
}
impl From<AgentHarnessError> for AgentError {
    fn from(error: AgentHarnessError) -> Self {
        Self::new(error.message)
    }
}

#[derive(Clone)]
pub struct SystemPromptContext {
    pub env: Arc<dyn ExecutionEnv>,
    pub session: Session,
    pub model: Model,
    pub thinking_level: ThinkingLevel,
    pub active_tools: Vec<Arc<dyn AgentTool>>,
    pub resources: AgentHarnessResources,
}
pub type SystemPromptCallback =
    Arc<dyn Fn(SystemPromptContext) -> BoxFuture<'static, Result<JsString, AgentHarnessError>> + Send + Sync>;
#[derive(Clone)]
pub enum HarnessSystemPrompt {
    Text(JsString),
    Callback(SystemPromptCallback),
}
pub struct AgentHarnessOptions {
    pub env: Arc<dyn ExecutionEnv>,
    pub session: Session,
    pub models: Arc<dyn Models>,
    pub model: Model,
    pub tools: Vec<Arc<dyn AgentTool>>,
    pub resources: AgentHarnessResources,
    pub system_prompt: Option<HarnessSystemPrompt>,
    pub stream_options: AgentHarnessStreamOptions,
    pub thinking_level: ThinkingLevel,
    pub active_tool_names: Option<Vec<String>>,
    pub steering_mode: QueueMode,
    pub follow_up_mode: QueueMode,
}
impl AgentHarnessOptions {
    pub fn new(env: Arc<dyn ExecutionEnv>, session: Session, models: Arc<dyn Models>, model: Model) -> Self {
        Self {
            env,
            session,
            models,
            model,
            tools: Vec::new(),
            resources: AgentHarnessResources {
                skills: None,
                prompt_templates: None,
            },
            system_prompt: None,
            stream_options: Default::default(),
            thinking_level: ThinkingLevel::Off,
            active_tool_names: None,
            steering_mode: QueueMode::OneAtATime,
            follow_up_mode: QueueMode::OneAtATime,
        }
    }
}
#[derive(Clone, Default)]
pub struct AgentHarnessPromptOptions {
    pub images: Option<Vec<ImageContent>>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AbortResult {
    pub cleared_steer: Vec<AgentMessage>,
    pub cleared_follow_up: Vec<AgentMessage>,
}
#[derive(Debug, Clone, Default)]
pub struct NavigateTreeOptions {
    pub summarize: bool,
    pub custom_instructions: Option<JsString>,
    pub replace_instructions: bool,
    pub label: Option<JsString>,
}
#[derive(Debug, Clone)]
pub struct NavigateTreeResult {
    pub cancelled: bool,
    pub editor_text: Option<JsString>,
    pub summary_entry: Option<SessionTreeEntry>,
}
#[derive(Debug, Clone)]
pub struct TreePreparation {
    pub target_id: JsString,
    pub old_leaf_id: Option<JsString>,
    pub common_ancestor_id: Option<JsString>,
    pub entries_to_summarize: Vec<SessionTreeEntry>,
    pub user_wants_summary: bool,
    pub custom_instructions: Option<JsString>,
    pub replace_instructions: bool,
    pub label: Option<JsString>,
}
#[derive(Debug, Clone)]
pub struct HarnessCompactResult {
    pub summary: JsString,
    pub first_kept_entry_id: JsString,
    pub tokens_before: f64,
    pub details: Option<JsValue>,
    pub duration_ms: Option<f64>,
}
#[derive(Debug, Clone, Default)]
pub struct SessionBeforeTreeResult {
    pub cancel: bool,
    pub summary: Option<(JsString, Option<JsValue>)>,
    pub custom_instructions: Option<JsString>,
    pub replace_instructions: Option<bool>,
    pub label: Option<JsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HarnessHookKind {
    BeforeAgentStart,
    Context,
    BeforeProviderRequest,
    BeforeProviderPayload,
    AfterProviderResponse,
    ToolCall,
    ToolResult,
    SessionBeforeCompact,
    SessionCompact,
    SessionBeforeTree,
    SessionTree,
    ModelUpdate,
    ThinkingLevelUpdate,
    ResourcesUpdate,
    ToolsUpdate,
    QueueUpdate,
    SavePoint,
    Abort,
    Settled,
}
#[derive(Debug, Clone)]
pub enum HarnessEvent {
    Agent(AgentEvent),
    QueueUpdate {
        steer: Vec<AgentMessage>,
        follow_up: Vec<AgentMessage>,
        next_turn: Vec<AgentMessage>,
    },
    SavePoint {
        had_pending_mutations: bool,
    },
    Abort(AbortResult),
    Settled {
        next_turn_count: f64,
    },
    BeforeAgentStart {
        prompt: JsString,
        images: Option<Vec<ImageContent>>,
        system_prompt: JsString,
        resources: AgentHarnessResources,
    },
    Context {
        messages: Vec<AgentMessage>,
    },
    BeforeProviderRequest {
        model: Model,
        session_id: JsString,
        stream_options: AgentHarnessStreamOptions,
    },
    BeforeProviderPayload {
        model: Model,
        payload: JsValue,
    },
    AfterProviderResponse {
        status: f64,
        headers: indexmap::IndexMap<String, String>,
    },
    ToolCall {
        tool_call_id: String,
        tool_name: String,
        input: JsValue,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        input: JsValue,
        content: Vec<TextOrImageContent>,
        details: Option<JsValue>,
        is_error: bool,
    },
    SessionBeforeCompact {
        preparation: CompactionPreparation,
        branch_entries: Vec<SessionTreeEntry>,
        custom_instructions: Option<JsString>,
        signal: CancellationToken,
    },
    SessionCompact {
        compaction_entry: SessionTreeEntry,
        from_hook: bool,
    },
    SessionBeforeTree {
        preparation: TreePreparation,
        signal: CancellationToken,
    },
    SessionTree {
        new_leaf_id: Option<JsString>,
        old_leaf_id: Option<JsString>,
        summary_entry: Option<SessionTreeEntry>,
        from_hook: bool,
    },
    ModelUpdate {
        model: Model,
        previous_model: Model,
    },
    ThinkingLevelUpdate {
        level: ThinkingLevel,
        previous_level: ThinkingLevel,
    },
    ToolsUpdate {
        tool_names: Vec<String>,
        previous_tool_names: Vec<String>,
        active_tool_names: Vec<String>,
        previous_active_tool_names: Vec<String>,
    },
    ResourcesUpdate {
        resources: AgentHarnessResources,
        previous_resources: AgentHarnessResources,
    },
}
impl HarnessEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Agent(event) => event.kind(),
            Self::QueueUpdate { .. } => "queue_update",
            Self::SavePoint { .. } => "save_point",
            Self::Abort(_) => "abort",
            Self::Settled { .. } => "settled",
            Self::BeforeAgentStart { .. } => "before_agent_start",
            Self::Context { .. } => "context",
            Self::BeforeProviderRequest { .. } => "before_provider_request",
            Self::BeforeProviderPayload { .. } => "before_provider_payload",
            Self::AfterProviderResponse { .. } => "after_provider_response",
            Self::ToolCall { .. } => "tool_call",
            Self::ToolResult { .. } => "tool_result",
            Self::SessionBeforeCompact { .. } => "session_before_compact",
            Self::SessionCompact { .. } => "session_compact",
            Self::SessionBeforeTree { .. } => "session_before_tree",
            Self::SessionTree { .. } => "session_tree",
            Self::ModelUpdate { .. } => "model_update",
            Self::ThinkingLevelUpdate { .. } => "thinking_level_update",
            Self::ToolsUpdate { .. } => "tools_update",
            Self::ResourcesUpdate { .. } => "resources_update",
        }
    }
}
#[derive(Clone)]
pub enum HarnessHookResult {
    BeforeAgentStart {
        messages: Option<Vec<AgentMessage>>,
        system_prompt: Option<JsString>,
    },
    Context(Vec<AgentMessage>),
    BeforeProviderRequest(AgentHarnessStreamOptionsPatch),
    BeforeProviderPayload(JsValue),
    ToolCall(super::BeforeToolCallResult),
    ToolResult(AfterToolCallResult),
    SessionBeforeCompact {
        cancel: bool,
        compaction: Option<HarnessCompactResult>,
    },
    SessionBeforeTree(SessionBeforeTreeResult),
}
pub type HarnessHook =
    Arc<dyn Fn(HarnessEvent) -> BoxFuture<'static, Result<Option<HarnessHookResult>, AgentError>> + Send + Sync>;
pub type HarnessListener =
    Arc<dyn Fn(HarnessEvent, Option<CancellationToken>) -> BoxFuture<'static, Result<(), AgentError>> + Send + Sync>;
pub type Unsubscribe = Box<dyn Fn() + Send + Sync>;
