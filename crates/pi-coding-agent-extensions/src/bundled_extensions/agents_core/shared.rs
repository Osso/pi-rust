//! Shared agents-core types (`extensions/agents-core/src/shared.ts` plus the
//! session factory contracts those modules exchange).

use std::sync::Arc;

use pi_agent_core::agent::types::AgentMessage;
use pi_ai::request::BoxFuture;
use pi_js_compat::js_json::JsString;
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
    pub agent_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordination_prompt: Option<pi_js_compat::js_json::JsString>,
    pub kind: WaitAgentsWakeKind,
}

impl WaitAgentsWakeUp {
    pub fn steering(agent_id: Option<JsString>) -> Self {
        Self {
            agent_id,
            coordination_prompt: None,
            kind: WaitAgentsWakeKind::Steering,
        }
    }

    pub fn coordination(prompt: pi_js_compat::js_json::JsString) -> Self {
        Self {
            agent_id: None,
            coordination_prompt: Some(prompt),
            kind: WaitAgentsWakeKind::Coordination,
        }
    }
}

pub type WaitWakeListener = Arc<dyn Fn(&WaitAgentsWakeUp) + Send + Sync>;

/// `Set<(wakeUp) => void>` for wait wake listeners.
#[derive(Clone, Default)]
pub struct WaitWakeListeners {
    inner: Arc<std::sync::Mutex<Vec<(u64, WaitWakeListener)>>>,
    next_id: Arc<std::sync::Mutex<u64>>,
}

pub struct WaitWakeUnsubscribe {
    listeners: WaitWakeListeners,
    id: u64,
}

impl Drop for WaitWakeUnsubscribe {
    fn drop(&mut self) {
        self.listeners
            .inner
            .lock()
            .expect("wait wake listeners")
            .retain(|(id, _)| *id != self.id);
    }
}

impl WaitWakeListeners {
    pub fn add(&self, listener: WaitWakeListener) -> WaitWakeUnsubscribe {
        let mut next_id = self.next_id.lock().expect("wait wake next id");
        let id = *next_id;
        *next_id += 1;
        drop(next_id);
        self.inner.lock().expect("wait wake listeners").push((id, listener));
        WaitWakeUnsubscribe {
            listeners: self.clone(),
            id,
        }
    }

    pub fn emit(&self, wake_up: &WaitAgentsWakeUp) {
        let listeners: Vec<WaitWakeListener> = self
            .inner
            .lock()
            .expect("wait wake listeners")
            .iter()
            .map(|(_, listener)| Arc::clone(listener))
            .collect();
        for listener in listeners {
            listener(wake_up);
        }
    }
}

pub fn empty_agent(agent_id: impl Into<JsString>) -> AgentSnapshot {
    let agent_id = agent_id.into();
    let timestamp = "1970-01-01T00:00:00.000Z".to_owned();
    AgentSnapshot {
        key_order: Vec::new(),
        id: agent_id.clone(),
        parent_id: None,
        display_name: agent_id,
        agent_type: "unknown".into(),
        origin: None,
        lifecycle: AgentLifecycle::Failed,
        revision: 0,
        created_at: timestamp.clone().into(),
        updated_at: timestamp.into(),
        cwd: JsString::default(),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: "on-request".into(),
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
    pub prompt: pi_js_compat::js_json::JsString,
    pub signal: Option<pi_js_compat::abort::AbortSignal>,
}

#[derive(Clone)]
pub struct AttachedSessionDispatchInput {
    pub active_tool_call_id: Option<String>,
    pub agent: AgentSnapshot,
    pub ctx: ExtensionContext,
    pub prompt: pi_js_compat::js_json::JsString,
    pub signal: Option<pi_js_compat::abort::AbortSignal>,
    pub session_path: JsString,
}

pub trait ChildAgentSession: Send + Sync {
    fn abort(&self) {}
    fn dispose(&self) {}
    fn drain_runtime_coordination(&self) -> Option<BoxFuture<'static, Result<(), pi_js_compat::js_json::JsString>>> {
        None
    }
    fn messages(&self) -> Vec<AgentMessage>;
    fn prompt(
        &self,
        text: pi_js_compat::js_json::JsString,
    ) -> BoxFuture<'static, Result<(), pi_js_compat::js_json::JsString>>;
    fn transcript(&self) -> Option<AgentTranscriptMetadata>;
    /// Live mutation surface when this session supports the `model`/`effort` commands.
    fn session_mutation_target(&self) -> Option<Arc<dyn crate::core::extensions::types::ViewedSessionMutationTarget>> {
        None
    }
}

type ChildFactoryFn = dyn Fn(
        ChildAgentDispatchInput,
    ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, pi_js_compat::js_json::JsString>>
    + Send
    + Sync;

#[derive(Clone)]
pub struct ChildAgentSessionFactory {
    create: Arc<ChildFactoryFn>,
    production: bool,
}

impl ChildAgentSessionFactory {
    pub fn new(
        create: impl Fn(
            ChildAgentDispatchInput,
        ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, pi_js_compat::js_json::JsString>>
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
        create: impl Fn(
            ChildAgentDispatchInput,
        ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, pi_js_compat::js_json::JsString>>
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
    ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, pi_js_compat::js_json::JsString>> {
        (self.create)(input)
    }
}

type AttachedFactoryFn = dyn Fn(
        AttachedSessionDispatchInput,
    ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, pi_js_compat::js_json::JsString>>
    + Send
    + Sync;

#[derive(Clone)]
pub struct AttachedSessionFactory {
    create: Arc<AttachedFactoryFn>,
}

impl AttachedSessionFactory {
    pub fn new(
        create: impl Fn(
            AttachedSessionDispatchInput,
        ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, pi_js_compat::js_json::JsString>>
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
    ) -> BoxFuture<'static, Result<Arc<dyn ChildAgentSession>, pi_js_compat::js_json::JsString>> {
        (self.create)(input)
    }
}
