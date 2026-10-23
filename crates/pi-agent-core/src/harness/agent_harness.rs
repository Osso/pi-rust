//! Working harness from the common-pin agent-harness.ts.
mod navigation;
mod runtime;
pub use super::agent_harness_types::*;

use super::session::{Session, SessionMetadata, SessionMetadataValue};
use super::{AgentHarnessResources, AgentHarnessStreamOptions, ExecutionEnv, QueueMode, ThinkingLevel};
use crate::agent::{AgentMessage, AgentTool};
use indexmap::{IndexMap, IndexSet};
use pi_ai::models::Models;
use pi_ai::types::{AssistantMessage, Model};
use pi_js_compat::abort::{AbortController, AbortSignal};
use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::watch;

#[derive(Clone)]
pub struct AgentHarness<M: SessionMetadataValue = SessionMetadata> {
    inner: Arc<Inner<M>>,
}
struct Inner<M: SessionMetadataValue> {
    env: Arc<dyn ExecutionEnv>,
    session: Session<M>,
    models: Arc<dyn Models>,
    state: Mutex<State<M>>,
    handlers: Mutex<Handlers>,
}
#[derive(Default)]
struct Handlers {
    next_id: usize,
    listeners: IndexMap<usize, HarnessListener>,
    hooks: HashMap<HarnessHookKind, IndexMap<usize, HarnessHook>>,
}
struct State<M: SessionMetadataValue> {
    phase: AgentHarnessPhase,
    completion: Option<watch::Receiver<bool>>,
    abort: Option<AbortController>,
    pending: Vec<PendingWrite>,
    model: Model,
    thinking_level: ThinkingLevel,
    system_prompt: Option<HarnessSystemPrompt<M>>,
    stream_options: AgentHarnessStreamOptions,
    resources: AgentHarnessResources,
    tools: IndexMap<String, Arc<dyn AgentTool>>,
    active_tool_names: Vec<String>,
    steer: Vec<AgentMessage>,
    follow_up: Vec<AgentMessage>,
    next_turn: Vec<AgentMessage>,
    steering_mode: QueueMode,
    follow_up_mode: QueueMode,
}
#[derive(Clone)]
enum PendingWrite {
    Message(AgentMessage),
    Model(Model),
    Thinking(ThinkingLevel),
    Tools(Vec<String>),
}
impl<M: SessionMetadataValue> AgentHarness<M> {
    pub fn new(options: AgentHarnessOptions<M>) -> Result<Self, AgentHarnessError> {
        validate_unique(
            &options
                .tools
                .iter()
                .map(|tool| tool.name().to_owned())
                .collect::<Vec<_>>(),
            "Duplicate tool name(s)",
        )?;
        let tools: IndexMap<_, _> = options
            .tools
            .into_iter()
            .map(|tool| (tool.name().to_owned(), tool))
            .collect();
        let active_tool_names = options
            .active_tool_names
            .unwrap_or_else(|| tools.keys().cloned().collect());
        validate_tools(&active_tool_names, &tools)?;
        let state = State {
            phase: AgentHarnessPhase::Idle,
            completion: None,
            abort: None,
            pending: Vec::new(),
            model: options.model,
            thinking_level: options.thinking_level,
            system_prompt: options.system_prompt,
            stream_options: options.stream_options,
            resources: options.resources,
            tools,
            active_tool_names,
            steer: Vec::new(),
            follow_up: Vec::new(),
            next_turn: Vec::new(),
            steering_mode: options.steering_mode,
            follow_up_mode: options.follow_up_mode,
        };
        Ok(Self {
            inner: Arc::new(Inner {
                env: options.env,
                session: options.session,
                models: options.models,
                state: Mutex::new(state),
                handlers: Mutex::new(Handlers::default()),
            }),
        })
    }
    fn state(&self) -> MutexGuard<'_, State<M>> {
        self.inner.state.lock().expect("harness state")
    }
    pub fn env(&self) -> Arc<dyn ExecutionEnv> {
        self.inner.env.clone()
    }
    pub fn session(&self) -> Session<M> {
        self.inner.session.clone()
    }
    pub fn models(&self) -> Arc<dyn Models> {
        self.inner.models.clone()
    }
    pub fn phase(&self) -> AgentHarnessPhase {
        self.state().phase
    }
    pub fn get_model(&self) -> Model {
        self.state().model.clone()
    }
    pub fn get_thinking_level(&self) -> ThinkingLevel {
        self.state().thinking_level
    }
    pub fn get_tools(&self) -> Vec<Arc<dyn AgentTool>> {
        self.state().tools.values().cloned().collect()
    }
    pub fn get_active_tools(&self) -> Vec<Arc<dyn AgentTool>> {
        let state = self.state();
        state
            .active_tool_names
            .iter()
            .map(|name| state.tools[name].clone())
            .collect()
    }
    pub fn get_resources(&self) -> AgentHarnessResources {
        self.state().resources.clone()
    }
    pub fn get_stream_options(&self) -> AgentHarnessStreamOptions {
        self.state().stream_options.clone()
    }
    pub async fn set_stream_options(&self, options: AgentHarnessStreamOptions) {
        self.state().stream_options = options;
    }
    pub fn get_steering_mode(&self) -> QueueMode {
        self.state().steering_mode
    }
    pub async fn set_steering_mode(&self, mode: QueueMode) {
        self.state().steering_mode = mode;
    }
    pub fn get_follow_up_mode(&self) -> QueueMode {
        self.state().follow_up_mode
    }
    pub async fn set_follow_up_mode(&self, mode: QueueMode) {
        self.state().follow_up_mode = mode;
    }
    pub async fn set_resources(&self, resources: AgentHarnessResources) -> Result<(), AgentHarnessError> {
        let previous_resources = std::mem::replace(&mut self.state().resources, resources.clone());
        self.emit(
            HarnessEvent::ResourcesUpdate {
                resources,
                previous_resources,
            },
            None,
        )
        .await
    }
    async fn persist_or_queue(&self, write: PendingWrite) -> Result<(), AgentHarnessError> {
        {
            let mut state = self.state();
            if state.phase != AgentHarnessPhase::Idle {
                state.pending.push(write);
                return Ok(());
            }
        }
        self.persist(write).await
    }
    async fn persist(&self, write: PendingWrite) -> Result<(), AgentHarnessError> {
        let session = &self.inner.session;
        match write {
            PendingWrite::Message(message) => {
                session.append_message(message).await?;
            }
            PendingWrite::Model(model) => {
                session
                    .append_model_change(model.provider.into(), model.id.into())
                    .await?;
            }
            PendingWrite::Thinking(level) => {
                session.append_thinking_level_change(level.as_str().into()).await?;
            }
            PendingWrite::Tools(names) => {
                session
                    .append_active_tools_change(names.into_iter().map(Into::into).collect())
                    .await?;
            }
        }
        Ok(())
    }
    async fn flush_pending(&self) -> Result<(), AgentHarnessError> {
        loop {
            let Some(write) = self.state().pending.first().cloned() else {
                return Ok(());
            };
            self.persist(write).await?;
            self.state().pending.remove(0);
        }
    }
    pub async fn append_message(&self, message: AgentMessage) -> Result<(), AgentHarnessError> {
        self.persist_or_queue(PendingWrite::Message(message)).await
    }
    pub async fn set_model(&self, model: Model) -> Result<(), AgentHarnessError> {
        let previous_model = self.get_model();
        self.persist_or_queue(PendingWrite::Model(model.clone())).await?;
        self.state().model = model.clone();
        self.emit(HarnessEvent::ModelUpdate { model, previous_model }, None)
            .await
    }
    pub async fn set_thinking_level(&self, level: ThinkingLevel) -> Result<(), AgentHarnessError> {
        let previous_level = self.get_thinking_level();
        self.persist_or_queue(PendingWrite::Thinking(level)).await?;
        self.state().thinking_level = level;
        self.emit(HarnessEvent::ThinkingLevelUpdate { level, previous_level }, None)
            .await
    }
    pub async fn set_tools(
        &self,
        tools: Vec<Arc<dyn AgentTool>>,
        active: Option<Vec<String>>,
    ) -> Result<(), AgentHarnessError> {
        let names = tools.iter().map(|tool| tool.name().to_owned()).collect::<Vec<_>>();
        validate_unique(&names, "Duplicate tool name(s)")?;
        let tools = tools.into_iter().map(|tool| (tool.name().to_owned(), tool)).collect();
        let active = active.unwrap_or_else(|| self.state().active_tool_names.clone());
        validate_tools(&active, &tools)?;
        self.update_tools(tools, active).await
    }
    pub async fn set_active_tools(&self, names: Vec<String>) -> Result<(), AgentHarnessError> {
        let tools = self.state().tools.clone();
        validate_tools(&names, &tools)?;
        self.update_tools(tools, names).await
    }
    async fn update_tools(
        &self,
        tools: IndexMap<String, Arc<dyn AgentTool>>,
        active: Vec<String>,
    ) -> Result<(), AgentHarnessError> {
        let (previous_tool_names, previous_active_tool_names) = {
            let state = self.state();
            (state.tools.keys().cloned().collect(), state.active_tool_names.clone())
        };
        self.persist_or_queue(PendingWrite::Tools(active.clone())).await?;
        let tool_names = tools.keys().cloned().collect();
        {
            let mut state = self.state();
            state.tools = tools;
            state.active_tool_names = active.clone();
        }
        self.emit(
            HarnessEvent::ToolsUpdate {
                tool_names,
                previous_tool_names,
                active_tool_names: active,
                previous_active_tool_names,
            },
            None,
        )
        .await
    }
    pub fn subscribe(&self, listener: HarnessListener) -> Unsubscribe {
        let mut handlers = self.inner.handlers.lock().expect("harness handlers");
        let existing = handlers
            .listeners
            .iter()
            .find_map(|(id, registered)| Arc::ptr_eq(registered, &listener).then_some(*id));
        let id = existing.unwrap_or(handlers.next_id);
        if existing.is_none() {
            handlers.next_id += 1;
            handlers.listeners.insert(id, listener);
        }
        let inner = Arc::downgrade(&self.inner);
        Box::new(move || {
            if let Some(inner) = inner.upgrade() {
                inner
                    .handlers
                    .lock()
                    .expect("harness handlers")
                    .listeners
                    .shift_remove(&id);
            }
        })
    }
    pub fn on(&self, kind: HarnessHookKind, hook: HarnessHook) -> Unsubscribe {
        let mut handlers = self.inner.handlers.lock().expect("harness handlers");
        let existing = handlers.hooks.get(&kind).and_then(|hooks| {
            hooks
                .iter()
                .find_map(|(id, registered)| Arc::ptr_eq(registered, &hook).then_some(*id))
        });
        let id = existing.unwrap_or(handlers.next_id);
        if existing.is_none() {
            handlers.next_id += 1;
            handlers.hooks.entry(kind).or_default().insert(id, hook);
        }
        let inner = Arc::downgrade(&self.inner);
        Box::new(move || {
            if let Some(inner) = inner.upgrade() {
                if let Some(hooks) = inner.handlers.lock().expect("harness handlers").hooks.get_mut(&kind) {
                    hooks.shift_remove(&id);
                }
            }
        })
    }
    fn next_hook(&self, kind: HarnessHookKind, after: Option<usize>) -> Option<(usize, HarnessHook)> {
        let handlers = self.inner.handlers.lock().expect("harness handlers");
        handlers
            .hooks
            .get(&kind)?
            .iter()
            .find(|(id, _)| after.is_none_or(|after| **id > after))
            .map(|(id, hook)| (*id, hook.clone()))
    }
    fn next_listener(&self, after: Option<usize>) -> Option<(usize, HarnessListener)> {
        let handlers = self.inner.handlers.lock().expect("harness handlers");
        handlers
            .listeners
            .iter()
            .find(|(id, _)| after.is_none_or(|after| **id > after))
            .map(|(id, listener)| (*id, listener.clone()))
    }
    async fn emit(&self, event: HarnessEvent, signal: Option<AbortSignal>) -> Result<(), AgentHarnessError> {
        let mut cursor = None;
        while let Some((id, listener)) = self.next_listener(cursor) {
            cursor = Some(id);
            listener(event.clone(), signal.clone()).await?;
        }
        Ok(())
    }
    async fn hook(
        &self,
        kind: HarnessHookKind,
        event: HarnessEvent,
    ) -> Result<Option<HarnessHookResult>, AgentHarnessError> {
        let mut last = None;
        let mut cursor = None;
        while let Some((id, hook)) = self.next_hook(kind, cursor) {
            cursor = Some(id);
            if let Some(result) = hook(event.clone()).await? {
                last = Some(result);
            }
        }
        Ok(last)
    }
    fn start_run(&self) -> Result<watch::Sender<bool>, AgentHarnessError> {
        let mut state = self.state();
        if state.phase != AgentHarnessPhase::Idle {
            return Err(AgentHarnessError::new(
                AgentHarnessErrorCode::Busy,
                "AgentHarness is busy",
            ));
        }
        let (sender, receiver) = watch::channel(false);
        state.phase = AgentHarnessPhase::Turn;
        state.completion = Some(receiver);
        Ok(sender)
    }
    pub async fn wait_for_idle(&self) {
        let completion = self.state().completion.clone();
        if let Some(mut completion) = completion {
            let _ = completion.wait_for(|finished| *finished).await;
        }
    }
    pub async fn prompt(
        &self,
        text: JsString,
        options: Option<AgentHarnessPromptOptions>,
    ) -> Result<AssistantMessage, AgentHarnessError> {
        self.run_prompt(PromptInvocation::Text(text), options).await
    }
    pub async fn skill(
        &self,
        name: JsString,
        additional_instructions: Option<JsString>,
    ) -> Result<AssistantMessage, AgentHarnessError> {
        self.run_prompt(PromptInvocation::Skill(name, additional_instructions), None)
            .await
    }
    pub async fn prompt_from_template(
        &self,
        name: JsString,
        args: Vec<JsString>,
    ) -> Result<AssistantMessage, AgentHarnessError> {
        self.run_prompt(PromptInvocation::Template(name, args), None).await
    }
}
enum PromptInvocation {
    Text(JsString),
    Skill(JsString, Option<JsString>),
    Template(JsString, Vec<JsString>),
}
fn validate_unique(names: &[String], prefix: &str) -> Result<(), AgentHarnessError> {
    let mut seen = IndexSet::new();
    let mut duplicates = IndexSet::new();
    for name in names {
        if !seen.insert(name) {
            duplicates.insert(name.as_str());
        }
    }
    if duplicates.is_empty() {
        return Ok(());
    }
    Err(AgentHarnessError::new(
        AgentHarnessErrorCode::InvalidArgument,
        format!("{prefix}: {}", duplicates.into_iter().collect::<Vec<_>>().join(", ")),
    ))
}
fn validate_tools(names: &[String], tools: &IndexMap<String, Arc<dyn AgentTool>>) -> Result<(), AgentHarnessError> {
    validate_unique(names, "Duplicate active tool name(s)")?;
    let missing = names
        .iter()
        .filter(|name| !tools.contains_key(*name))
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    Err(AgentHarnessError::new(
        AgentHarnessErrorCode::InvalidArgument,
        format!("Unknown tool(s): {}", missing.join(", ")),
    ))
}
