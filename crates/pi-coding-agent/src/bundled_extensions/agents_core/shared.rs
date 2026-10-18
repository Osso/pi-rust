//! Shared agents-core types (`extensions/agents-core/src/shared.ts` plus the
//! session factory contracts those modules exchange).

use std::sync::Arc;

use pi_agent_core::agent::types::AgentMessage;
use pi_ai::request::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::extensions::types::{ExtensionContext, HandlerError};
use crate::core::multi_agent_store::{
    AgentLifecycle, AgentPermission, AgentSnapshot, AgentTranscriptMetadata, is_active_lifecycle,
};

pub const CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE: &str =
    "Agent orchestration is unavailable from child agent runtimes.";

pub fn file_reference_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {"type": "string"},
            "label": {"type": "string"}
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaitAgentsWakeKind {
    Steering,
    Coordination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitAgentsWakeUp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordination_prompt: Option<String>,
    pub kind: WaitAgentsWakeKind,
}

impl WaitAgentsWakeUp {
    pub fn steering(agent_id: Option<String>) -> Self {
        Self {
            agent_id,
            coordination_prompt: None,
            kind: WaitAgentsWakeKind::Steering,
        }
    }

    pub fn coordination(prompt: String) -> Self {
        Self {
            agent_id: None,
            coordination_prompt: Some(prompt),
            kind: WaitAgentsWakeKind::Coordination,
        }
    }
}

pub type WaitWakeListener = Arc<dyn Fn(&WaitAgentsWakeUp) + Send + Sync>;

pub fn empty_agent(agent_id: &str) -> AgentSnapshot {
    let timestamp = "1970-01-01T00:00:00.000Z".to_owned();
    AgentSnapshot {
        id: agent_id.to_owned(),
        parent_id: None,
        display_name: agent_id.to_owned(),
        agent_type: "unknown".to_owned(),
        origin: None,
        lifecycle: AgentLifecycle::Failed,
        revision: 0,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        cwd: String::new(),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: "on-request".to_owned(),
            inherited_from: None,
            narrowed: true,
        },
        slot: None,
        transcript: None,
        event_stream: None,
        worker: None,
        detached: None,
        current_activity: None,
        last_activity: None,
        result: None,
        error: None,
        extra: Default::default(),
    }
}

pub fn is_child_agent_runtime(ctx: &ExtensionContext) -> Result<bool, HandlerError> {
    Ok(ctx.multi_agent_agent_id()?.is_some() || ctx.multi_agent_requires_agent_id()?)
}

pub fn is_in_flight_lifecycle(lifecycle: AgentLifecycle) -> bool {
    is_active_lifecycle(lifecycle) && lifecycle != AgentLifecycle::WaitingForInput
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnContext {
    Fresh,
    Inherit,
}

#[derive(Clone)]
pub struct ChildAgentDispatchInput {
    pub active_tool_call_id: Option<String>,
    pub agent: AgentSnapshot,
    pub context: SpawnContext,
    pub ctx: ExtensionContext,
    pub prompt: String,
    pub signal: Option<tokio_util::sync::CancellationToken>,
}

#[derive(Clone)]
pub struct AttachedSessionDispatchInput {
    pub active_tool_call_id: Option<String>,
    pub agent: AgentSnapshot,
    pub ctx: ExtensionContext,
    pub prompt: String,
    pub signal: Option<tokio_util::sync::CancellationToken>,
    pub session_path: String,
}

pub trait ChildAgentSession: Send + Sync {
    fn abort(&self) {}
    fn dispose(&self) {}
    fn drain_runtime_coordination(&self) -> Option<BoxFuture<'static, Result<(), String>>> {
        None
    }
    fn messages(&self) -> Vec<AgentMessage>;
    fn prompt(&self, text: String) -> BoxFuture<'static, Result<(), String>>;
    fn transcript(&self) -> Option<AgentTranscriptMetadata>;
}

type ChildFactoryFn =
    dyn Fn(ChildAgentDispatchInput) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, String>> + Send + Sync;

#[derive(Clone)]
pub struct ChildAgentSessionFactory {
    create: Arc<ChildFactoryFn>,
    production: bool,
}

impl ChildAgentSessionFactory {
    pub fn new(
        create: impl Fn(ChildAgentDispatchInput) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, String>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            create: Arc::new(create),
            production: false,
        }
    }

    pub fn production(
        create: impl Fn(ChildAgentDispatchInput) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, String>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            create: Arc::new(create),
            production: true,
        }
    }

    pub fn is_production(&self) -> bool {
        self.production
    }

    pub fn call(
        &self,
        input: ChildAgentDispatchInput,
    ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, String>> {
        (self.create)(input)
    }
}

type AttachedFactoryFn = dyn Fn(AttachedSessionDispatchInput) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, String>>
    + Send
    + Sync;

#[derive(Clone)]
pub struct AttachedSessionFactory {
    create: Arc<AttachedFactoryFn>,
}

impl AttachedSessionFactory {
    pub fn new(
        create: impl Fn(AttachedSessionDispatchInput) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, String>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            create: Arc::new(create),
        }
    }

    pub fn call(
        &self,
        input: AttachedSessionDispatchInput,
    ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, String>> {
        (self.create)(input)
    }
}
