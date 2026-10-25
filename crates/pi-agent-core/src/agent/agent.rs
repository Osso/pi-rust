//! Port of `packages/agent-core/src/agent.ts`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use pi_ai::request::{BoxFuture, CallbackError, OnPayload, OnResponse, OnRetry, SimpleStreamOptions};
use pi_ai::types::{
    AssistantMessage, ImageContent, Message, Model, ModelCost, ModelCostRates, StopReason, TextContent,
    TextOrImageContent, ThinkingBudgets, Transport, UserContent, UserMessage, UserRole,
};
use pi_js_compat::abort::{AbortController, AbortSignal};
use pi_js_compat::js_json::JsString;
use tokio::sync::Notify;

use crate::agent::agent_loop::{AgentEventSink, run_agent_loop, run_agent_loop_continue};
use crate::agent::stream_fn::default_stream_fn;
use crate::agent::types::{
    AfterToolCallHook, AgentContext, AgentError, AgentEvent, AgentLoopConfig, AgentLoopTurnUpdate, AgentMessage,
    AgentState, AgentTool, BeforeToolCallHook, ConvertToLlm, GetApiKey, MessageQueueHook, PrepareNextTurnContext,
    PrepareNextTurnHook, ShouldStopAfterTurnHook, StreamFn, TransformContext, reasoning_for,
};
use crate::harness::types::{QueueMode, ThinkingLevel, ToolExecutionMode};

/// `prepareNextTurn(signal?)`: legacy signal-only hook.
pub type PrepareNextTurnSignalHook = Arc<
    dyn Fn(Option<AbortSignal>) -> BoxFuture<'static, Result<Option<AgentLoopTurnUpdate>, AgentError>> + Send + Sync,
>;

/// `prepareNextTurnWithContext(context, signal?)` hook exposed by `AgentOptions`.
pub type PrepareNextTurnContextHook = Arc<
    dyn Fn(
            PrepareNextTurnContext,
            Option<AbortSignal>,
        ) -> BoxFuture<'static, Result<Option<AgentLoopTurnUpdate>, AgentError>>
        + Send
        + Sync,
>;

/// Event listener registered through [`Agent::subscribe`].
pub type AgentListener =
    Arc<dyn Fn(AgentEvent, AbortSignal) -> BoxFuture<'static, Result<(), AgentError>> + Send + Sync>;

/// Prompt input accepted by [`Agent::prompt`].
#[derive(Debug, Clone)]
pub enum PromptInput {
    Text(JsString),
    Message(Box<AgentMessage>),
    Messages(Vec<AgentMessage>),
}

impl From<&str> for PromptInput {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

impl From<String> for PromptInput {
    fn from(value: String) -> Self {
        Self::Text(value.into())
    }
}
impl From<JsString> for PromptInput {
    fn from(value: JsString) -> Self {
        Self::Text(value)
    }
}
impl From<&JsString> for PromptInput {
    fn from(value: &JsString) -> Self {
        Self::Text(value.clone())
    }
}

impl From<AgentMessage> for PromptInput {
    fn from(value: AgentMessage) -> Self {
        Self::Message(Box::new(value))
    }
}

impl From<Vec<AgentMessage>> for PromptInput {
    fn from(value: Vec<AgentMessage>) -> Self {
        Self::Messages(value)
    }
}

/// Initial agent state overrides.
#[derive(Clone, Default)]
pub struct AgentInitialState {
    pub system_prompt: Option<JsString>,
    pub model: Option<Model>,
    pub thinking_level: Option<ThinkingLevel>,
    pub tools: Option<Vec<Arc<dyn AgentTool>>>,
    pub messages: Option<Vec<AgentMessage>>,
}

/// Options for constructing an [`Agent`].
#[derive(Clone, Default)]
pub struct AgentOptions {
    pub initial_state: Option<AgentInitialState>,
    pub convert_to_llm: Option<ConvertToLlm>,
    pub transform_context: Option<TransformContext>,
    pub stream_fn: Option<StreamFn>,
    pub get_api_key: Option<GetApiKey>,
    pub before_tool_call: Option<BeforeToolCallHook>,
    pub after_tool_call: Option<AfterToolCallHook>,
    pub should_stop_after_turn: Option<ShouldStopAfterTurnHook>,
    pub prepare_next_turn: Option<PrepareNextTurnSignalHook>,
    pub prepare_next_turn_with_context: Option<PrepareNextTurnContextHook>,
    pub steering_mode: Option<QueueMode>,
    pub follow_up_mode: Option<QueueMode>,
    pub session_id: Option<JsString>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub transport: Option<Transport>,
    pub max_retry_delay_ms: Option<f64>,
    pub tool_execution: Option<ToolExecutionMode>,
    pub on_payload: Option<OnPayload>,
    pub on_response: Option<OnResponse>,
    pub on_retry: Option<OnRetry>,
}

/// Options for [`Agent::continue_run`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ContinueOptions {
    pub process_queued_messages_first: bool,
}

struct PendingMessageQueue {
    messages: Vec<AgentMessage>,
    mode: QueueMode,
}

impl PendingMessageQueue {
    fn new(mode: QueueMode) -> Self {
        Self {
            messages: Vec::new(),
            mode,
        }
    }

    fn enqueue(&mut self, message: AgentMessage) {
        self.messages.push(message);
    }

    fn has_items(&self) -> bool {
        !self.messages.is_empty()
    }

    fn drain(&mut self) -> Vec<AgentMessage> {
        match self.mode {
            QueueMode::All => std::mem::take(&mut self.messages),
            QueueMode::OneAtATime => {
                if self.messages.is_empty() {
                    Vec::new()
                } else {
                    vec![self.messages.remove(0)]
                }
            }
        }
    }

    fn clear(&mut self) {
        self.messages.clear();
    }
}

struct RunHandle {
    done: Mutex<bool>,
    notify: Notify,
}

impl RunHandle {
    fn new() -> Self {
        Self {
            done: Mutex::new(false),
            notify: Notify::new(),
        }
    }

    fn finish(&self) {
        *self.done.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        loop {
            if *self.done.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) {
                return;
            }
            let notified = self.notify.notified();
            if *self.done.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) {
                return;
            }
            notified.await;
        }
    }
}

struct ActiveRun {
    token: AbortController,
    handle: Arc<RunHandle>,
}

/// Opaque native carrier of a stored message's frozen JavaScript reference identity.
#[derive(Debug, Clone)]
pub struct AgentMessageIdentity(Arc<()>);
impl AgentMessageIdentity {
    pub fn new() -> Self {
        Self(Arc::new(()))
    }
}
impl Default for AgentMessageIdentity {
    fn default() -> Self {
        Self::new()
    }
}
impl PartialEq for AgentMessageIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for AgentMessageIdentity {}

#[derive(Debug, Clone)]
pub struct AgentStateIdentity {
    owner: Weak<AgentInner>,
    model_generation: u64,
    messages: Vec<AgentMessageIdentity>,
}
impl AgentStateIdentity {
    pub fn has_same_model(&self, other: &Self) -> bool {
        self.owner.ptr_eq(&other.owner) && self.model_generation == other.model_generation
    }
    pub fn message_tokens(&self) -> &[AgentMessageIdentity] {
        &self.messages
    }

    /// Refreshes only the message array snapshot; the captured model reference stays unchanged.
    pub fn refresh_message_references(&mut self, current: &Self) -> Result<(), AgentError> {
        if !self.owner.ptr_eq(&current.owner) {
            return Err(AgentError::new("Cannot refresh message references from another Agent"));
        }
        self.messages = current.messages.clone();
        Ok(())
    }
}

struct AgentStorage {
    state: AgentState,
    model_generation: u64,
    message_tokens: Vec<AgentMessageIdentity>,
}
impl AgentStorage {
    fn new(state: AgentState) -> Self {
        let message_tokens = state
            .messages
            .iter()
            .map(|_| AgentMessageIdentity(Arc::new(())))
            .collect();
        Self {
            state,
            model_generation: 0,
            message_tokens,
        }
    }
    fn append_message(&mut self, message: AgentMessage) -> AgentMessageIdentity {
        let token = AgentMessageIdentity(Arc::new(()));
        self.state.messages.push(message);
        self.message_tokens.push(token.clone());
        token
    }
    fn replace_messages(&mut self, messages: Vec<AgentMessage>) {
        self.message_tokens = messages.iter().map(|_| AgentMessageIdentity(Arc::new(()))).collect();
        self.state.messages = messages;
    }
    fn truncate_messages(&mut self, length: usize) {
        self.state.messages.truncate(length);
        self.message_tokens.truncate(length);
    }
}
impl std::ops::Deref for AgentStorage {
    type Target = AgentState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}
impl std::ops::DerefMut for AgentStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

struct AgentEventIdentity {
    agent: Weak<AgentInner>,
    message: Option<AgentMessageIdentity>,
}
tokio::task_local! {
    static AGENT_EVENT_IDENTITY:AgentEventIdentity;
}

struct AgentInner {
    state: Mutex<AgentStorage>,
    listeners: Mutex<Vec<(u64, AgentListener)>>,
    next_listener_id: AtomicU64,
    steering_queue: Arc<Mutex<PendingMessageQueue>>,
    follow_up_queue: Arc<Mutex<PendingMessageQueue>>,
    active_run: Mutex<Option<ActiveRun>>,
    convert_to_llm: Mutex<ConvertToLlm>,
    transform_context: Mutex<Option<TransformContext>>,
    stream_function: Mutex<StreamFn>,
    get_api_key: Mutex<Option<GetApiKey>>,
    before_tool_call: Mutex<Option<BeforeToolCallHook>>,
    after_tool_call: Mutex<Option<AfterToolCallHook>>,
    should_stop_after_turn: Mutex<Option<ShouldStopAfterTurnHook>>,
    prepare_next_turn: Mutex<Option<PrepareNextTurnSignalHook>>,
    prepare_next_turn_with_context: Mutex<Option<PrepareNextTurnContextHook>>,
    session_id: Mutex<Option<JsString>>,
    thinking_budgets: Mutex<Option<ThinkingBudgets>>,
    transport: Mutex<Transport>,
    max_retry_delay_ms: Mutex<Option<f64>>,
    tool_execution: Mutex<ToolExecutionMode>,
    stream_options: Mutex<SimpleStreamOptions>,
}

/// Stateful wrapper around the low-level agent loop.
#[derive(Clone)]
pub struct Agent {
    inner: Arc<AgentInner>,
}

impl Agent {
    pub fn new(options: AgentOptions) -> Result<Self, AgentError> {
        let mut stream_options = SimpleStreamOptions::default();
        stream_options.stream.request.on_payload = options.on_payload;
        stream_options.stream.request.on_response = options.on_response;
        stream_options.stream.on_retry = options.on_retry;
        let stream_function = options.stream_fn.unwrap_or_else(default_stream_fn);
        let initial = options.initial_state.unwrap_or_default();
        let state = AgentState {
            system_prompt: initial.system_prompt.unwrap_or_default(),
            model: initial.model.unwrap_or_else(default_model),
            thinking_level: initial.thinking_level.unwrap_or(ThinkingLevel::Off),
            tools: initial.tools.unwrap_or_default(),
            messages: initial.messages.unwrap_or_default(),
            is_streaming: false,
            is_model_request_active: false,
            streaming_message: None,
            pending_tool_calls: indexmap::IndexSet::new(),
            error_message: None,
        };
        Ok(Self {
            inner: Arc::new(AgentInner {
                state: Mutex::new(AgentStorage::new(state)),
                listeners: Mutex::new(Vec::new()),
                next_listener_id: AtomicU64::new(1),
                steering_queue: Arc::new(Mutex::new(PendingMessageQueue::new(
                    options.steering_mode.unwrap_or(QueueMode::OneAtATime),
                ))),
                follow_up_queue: Arc::new(Mutex::new(PendingMessageQueue::new(
                    options.follow_up_mode.unwrap_or(QueueMode::OneAtATime),
                ))),
                active_run: Mutex::new(None),
                convert_to_llm: Mutex::new(
                    options
                        .convert_to_llm
                        .unwrap_or_else(|| Arc::new(default_convert_to_llm) as ConvertToLlm),
                ),
                transform_context: Mutex::new(options.transform_context),
                stream_function: Mutex::new(stream_function),
                get_api_key: Mutex::new(options.get_api_key),
                before_tool_call: Mutex::new(options.before_tool_call),
                after_tool_call: Mutex::new(options.after_tool_call),
                should_stop_after_turn: Mutex::new(options.should_stop_after_turn),
                prepare_next_turn: Mutex::new(options.prepare_next_turn),
                prepare_next_turn_with_context: Mutex::new(options.prepare_next_turn_with_context),
                session_id: Mutex::new(options.session_id),
                thinking_budgets: Mutex::new(options.thinking_budgets),
                transport: Mutex::new(options.transport.unwrap_or(Transport::Auto)),
                max_retry_delay_ms: Mutex::new(options.max_retry_delay_ms),
                tool_execution: Mutex::new(options.tool_execution.unwrap_or(ToolExecutionMode::Parallel)),
                stream_options: Mutex::new(stream_options),
            }),
        })
    }

    /// Subscribes to lifecycle events; returns the subscription id.
    pub fn subscribe(&self, listener: AgentListener) -> u64 {
        let mut listeners = self
            .inner
            .listeners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((id, _)) = listeners.iter().find(|(_, existing)| Arc::ptr_eq(existing, &listener)) {
            return *id;
        }
        let id = self.inner.next_listener_id.fetch_add(1, Ordering::SeqCst);
        listeners.push((id, listener));
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        self.inner
            .listeners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|(listener_id, _)| *listener_id != id);
    }

    fn hook<T: Clone>(&self, slot: &Mutex<Option<T>>) -> Option<T> {
        slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    /// Replaces `beforeToolCall` (TypeScript's public mutable field).
    pub fn set_before_tool_call(&self, hook: Option<BeforeToolCallHook>) {
        *self
            .inner
            .before_tool_call
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
    }

    /// Replaces `afterToolCall`.
    pub fn set_after_tool_call(&self, hook: Option<AfterToolCallHook>) {
        *self
            .inner
            .after_tool_call
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
    }

    /// Replaces `shouldStopAfterTurn`.
    pub fn set_should_stop_after_turn(&self, hook: Option<ShouldStopAfterTurnHook>) {
        *self
            .inner
            .should_stop_after_turn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
    }

    /// Replaces `prepareNextTurnWithContext`.
    pub fn set_prepare_next_turn_with_context(&self, hook: Option<PrepareNextTurnContextHook>) {
        *self
            .inner
            .prepare_next_turn_with_context
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
    }

    /// Returns the currently installed `shouldStopAfterTurn` hook.
    pub fn should_stop_after_turn_hook(&self) -> Option<ShouldStopAfterTurnHook> {
        self.hook(&self.inner.should_stop_after_turn)
    }

    /// Returns the currently installed `prepareNextTurnWithContext` hook.
    pub fn prepare_next_turn_with_context_hook(&self) -> Option<PrepareNextTurnContextHook> {
        self.hook(&self.inner.prepare_next_turn_with_context)
    }

    /// Returns the currently installed legacy `prepareNextTurn` hook.
    pub fn prepare_next_turn_hook(&self) -> Option<PrepareNextTurnSignalHook> {
        self.hook(&self.inner.prepare_next_turn)
    }

    /// The configured stream function (`streamFn`).
    pub fn stream_fn(&self) -> StreamFn {
        self.inner
            .stream_function
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_stream_fn(&self, stream_fn: StreamFn) {
        *self
            .inner
            .stream_function
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = stream_fn;
    }

    pub fn set_convert_to_llm(&self, convert: ConvertToLlm) {
        *self
            .inner
            .convert_to_llm
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = convert;
    }

    pub fn set_transform_context(&self, transform: Option<TransformContext>) {
        *self
            .inner
            .transform_context
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = transform;
    }

    pub fn set_get_api_key(&self, get_api_key: Option<GetApiKey>) {
        *self
            .inner
            .get_api_key
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = get_api_key;
    }

    pub fn set_prepare_next_turn(&self, hook: Option<PrepareNextTurnSignalHook>) {
        *self
            .inner
            .prepare_next_turn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
    }

    pub fn set_on_payload(&self, callback: Option<OnPayload>) {
        self.inner
            .stream_options
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stream
            .request
            .on_payload = callback;
    }

    pub fn set_on_response(&self, callback: Option<OnResponse>) {
        self.inner
            .stream_options
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stream
            .request
            .on_response = callback;
    }

    pub fn set_on_retry(&self, callback: Option<OnRetry>) {
        self.inner
            .stream_options
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stream
            .on_retry = callback;
    }

    pub fn set_thinking_budgets(&self, budgets: Option<ThinkingBudgets>) {
        *self
            .inner
            .thinking_budgets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = budgets;
    }

    pub fn set_max_retry_delay_ms(&self, delay: Option<f64>) {
        *self
            .inner
            .max_retry_delay_ms
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = delay;
    }

    /// Current state snapshot.
    pub fn state(&self) -> AgentState {
        self.state_lock().state.clone()
    }

    pub fn system_prompt(&self) -> JsString {
        self.state_lock().system_prompt.clone()
    }

    pub fn set_system_prompt(&self, system_prompt: impl Into<JsString>) {
        self.state_lock().system_prompt = system_prompt.into();
    }

    pub fn model(&self) -> Model {
        self.state_lock().model.clone()
    }

    pub fn set_model(&self, model: Model) {
        let mut storage = self.state_lock();
        storage.model_generation = storage
            .model_generation
            .checked_add(1)
            .expect("model reference generation overflow");
        storage.state.model = model;
    }

    /// Preferred transport forwarded to the stream function.
    pub fn transport(&self) -> Transport {
        *self
            .inner
            .transport
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set_transport(&self, transport: Transport) {
        *self
            .inner
            .transport
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = transport;
    }

    pub fn thinking_level(&self) -> ThinkingLevel {
        self.state_lock().thinking_level
    }

    pub fn set_thinking_level(&self, level: ThinkingLevel) {
        self.state_lock().thinking_level = level;
    }

    pub fn tools(&self) -> Vec<Arc<dyn AgentTool>> {
        self.state_lock().tools.clone()
    }

    /// Replaces the tool list; the vector is copied.
    pub fn set_tools(&self, tools: Vec<Arc<dyn AgentTool>>) {
        self.state_lock().tools = tools;
    }

    pub fn messages(&self) -> Vec<AgentMessage> {
        self.state_lock().messages.clone()
    }

    /// Replaces the transcript; the vector is copied.
    pub fn set_messages(&self, messages: Vec<AgentMessage>) {
        self.state_lock().replace_messages(messages);
    }

    /// Identity-only snapshot; the predicate reads each authoritative stored message under one lock.
    pub fn identity_snapshot(&self, include: impl Fn(&AgentMessage) -> bool) -> AgentStateIdentity {
        let storage = self.state_lock();
        let messages = storage
            .messages
            .iter()
            .zip(&storage.message_tokens)
            .filter(|&(message, _)| include(message))
            .map(|(_, token)| token.clone())
            .collect();
        AgentStateIdentity {
            owner: Arc::downgrade(&self.inner),
            model_generation: storage.model_generation,
            messages,
        }
    }

    /// Captures one stored message and its reference under the same state lock.
    pub fn find_last_message_with_identity(
        &self,
        include: impl Fn(&AgentMessage) -> bool,
    ) -> Option<(AgentMessage, AgentMessageIdentity)> {
        let storage = self.state_lock();
        storage
            .messages
            .iter()
            .zip(&storage.message_tokens)
            .rev()
            .find(|(message, _)| include(message))
            .map(|(message, identity)| (message.clone(), identity.clone()))
    }

    /// Source `state.messages.push(message)`: existing object references survive.
    pub fn append_message(&self, message: AgentMessage) -> AgentMessageIdentity {
        self.state_lock().append_message(message)
    }

    /// Source prefix slice: retain the identities of every surviving object.
    pub fn truncate_messages(&self, length: usize) {
        self.state_lock().truncate_messages(length);
    }

    /// Restores references from the session entries that own these message objects.
    pub fn set_messages_with_identities(
        &self,
        messages: Vec<AgentMessage>,
        identities: Vec<AgentMessageIdentity>,
    ) -> Result<(), AgentError> {
        if messages.len() != identities.len() {
            return Err(AgentError::new(
                "Message identity count does not match restored messages",
            ));
        }
        let mut storage = self.state_lock();
        storage.state.messages = messages;
        storage.message_tokens = identities;
        Ok(())
    }

    /// Available only inside this Agent's awaited message-end listener dispatch.
    pub fn current_message_end_identity(&self) -> Result<AgentMessageIdentity, AgentError> {
        let identity = AGENT_EVENT_IDENTITY.try_with(|scope| {
            if scope.agent.ptr_eq(&Arc::downgrade(&self.inner)) {
                scope.message.clone()
            } else {
                None
            }
        });
        match identity {
            Ok(Some(identity)) => Ok(identity),
            Ok(None) | Err(_) => Err(AgentError::new(
                "No message-end identity is active for this listener dispatch",
            )),
        }
    }

    pub fn is_streaming(&self) -> bool {
        self.state_lock().is_streaming
    }

    pub fn is_model_request_active(&self) -> bool {
        self.state_lock().is_model_request_active
    }

    pub fn streaming_message(&self) -> Option<AgentMessage> {
        self.state_lock().streaming_message.clone()
    }

    pub fn pending_tool_calls(&self) -> indexmap::IndexSet<String> {
        self.state_lock().pending_tool_calls.clone()
    }

    pub fn error_message(&self) -> Option<JsString> {
        self.state_lock().error_message.clone()
    }

    pub fn steering_mode(&self) -> QueueMode {
        self.queue_lock(&self.inner.steering_queue).mode
    }

    pub fn set_steering_mode(&self, mode: QueueMode) {
        self.queue_lock(&self.inner.steering_queue).mode = mode;
    }

    pub fn follow_up_mode(&self) -> QueueMode {
        self.queue_lock(&self.inner.follow_up_queue).mode
    }

    pub fn set_follow_up_mode(&self, mode: QueueMode) {
        self.queue_lock(&self.inner.follow_up_queue).mode = mode;
    }

    pub fn tool_execution(&self) -> ToolExecutionMode {
        *self
            .inner
            .tool_execution
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set_tool_execution(&self, mode: ToolExecutionMode) {
        *self
            .inner
            .tool_execution
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = mode;
    }

    pub fn session_id(&self) -> Option<JsString> {
        self.inner
            .session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_session_id(&self, session_id: Option<JsString>) {
        *self
            .inner
            .session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = session_id;
    }

    /// Queues a steering message and interrupts the active model request.
    pub fn steer(&self, message: AgentMessage) {
        self.queue_lock(&self.inner.steering_queue).enqueue(message);
        if self.state_lock().is_model_request_active
            && let Some(run) = self.active_run_lock().as_ref()
        {
            run.token.abort(None);
        }
    }

    /// Queues a message to run only after the agent would otherwise stop.
    pub fn follow_up(&self, message: AgentMessage) {
        self.queue_lock(&self.inner.follow_up_queue).enqueue(message);
    }

    pub fn clear_steering_queue(&self) {
        self.queue_lock(&self.inner.steering_queue).clear();
    }

    pub fn clear_follow_up_queue(&self) {
        self.queue_lock(&self.inner.follow_up_queue).clear();
    }

    pub fn clear_all_queues(&self) {
        self.clear_steering_queue();
        self.clear_follow_up_queue();
    }

    pub fn has_queued_messages(&self) -> bool {
        self.queue_lock(&self.inner.steering_queue).has_items()
            || self.queue_lock(&self.inner.follow_up_queue).has_items()
    }

    /// Active abort signal for the current run.
    pub fn signal(&self) -> Option<AbortSignal> {
        self.active_run_lock().as_ref().map(|run| run.token.signal())
    }

    pub fn abort(&self) {
        if let Some(run) = self.active_run_lock().as_ref() {
            run.token.abort(None);
        }
    }

    /// Resolves after the current run and its awaited `agent_end` listeners finish.
    pub async fn wait_for_idle(&self) {
        let handle = self.active_run_lock().as_ref().map(|run| Arc::clone(&run.handle));
        if let Some(handle) = handle {
            handle.wait().await;
        }
    }

    pub fn reset(&self) {
        {
            let mut state = self.state_lock();
            state.truncate_messages(0);
            state.is_streaming = false;
            state.is_model_request_active = false;
            state.streaming_message = None;
            state.pending_tool_calls.clear();
            state.error_message = None;
        }
        self.clear_follow_up_queue();
        self.clear_steering_queue();
    }

    pub async fn prompt(&self, input: impl Into<PromptInput>) -> Result<(), AgentError> {
        self.prompt_with_images(input, Vec::new()).await
    }

    pub async fn prompt_with_images(
        &self,
        input: impl Into<PromptInput>,
        images: Vec<ImageContent>,
    ) -> Result<(), AgentError> {
        if self.active_run_lock().is_some() {
            return Err(AgentError::already_processing_prompt());
        }
        let messages = normalize_prompt_input(input.into(), images);
        self.run_prompt_messages(messages, false).await
    }

    pub async fn continue_run(&self, options: ContinueOptions) -> Result<(), AgentError> {
        if self.active_run_lock().is_some() {
            return Err(AgentError::already_processing_continue());
        }
        let last_message = self.state_lock().messages.last().cloned();
        let Some(last_message) = last_message else {
            return Err(AgentError::no_messages_to_continue());
        };

        if options.process_queued_messages_first || last_message.role() == "assistant" {
            let queued_steering = self.queue_lock(&self.inner.steering_queue).drain();
            if !queued_steering.is_empty() {
                return self.run_prompt_messages(queued_steering, true).await;
            }
            let queued_follow_ups = self.queue_lock(&self.inner.follow_up_queue).drain();
            if !queued_follow_ups.is_empty() {
                return self.run_prompt_messages(queued_follow_ups, false).await;
            }
        }

        self.run_continuation().await
    }

    async fn run_prompt_messages(
        &self,
        messages: Vec<AgentMessage>,
        skip_initial_steering_poll: bool,
    ) -> Result<(), AgentError> {
        self.begin_run()?;
        let config = self.create_loop_config(skip_initial_steering_poll);
        let context = self.create_context_snapshot();
        let signal = self.current_signal();
        let result = run_agent_loop(
            messages,
            context,
            config,
            self.event_sink(),
            Some(signal.clone()),
            Some(self.stream_fn()),
        )
        .await;
        self.finish_run_with(result, &signal).await
    }

    async fn run_continuation(&self) -> Result<(), AgentError> {
        self.begin_run()?;
        let config = self.create_loop_config(false);
        let context = self.create_context_snapshot();
        let signal = self.current_signal();
        let result = run_agent_loop_continue(
            context,
            config,
            self.event_sink(),
            Some(signal.clone()),
            Some(self.stream_fn()),
        )
        .await;
        self.finish_run_with(result, &signal).await
    }

    fn begin_run(&self) -> Result<(), AgentError> {
        let mut active_run = self.active_run_lock();
        if active_run.is_some() {
            return Err(AgentError::already_processing());
        }
        *active_run = Some(ActiveRun {
            token: AbortController::new(),
            handle: Arc::new(RunHandle::new()),
        });
        let mut state = self.state_lock();
        state.is_streaming = true;
        state.is_model_request_active = false;
        state.streaming_message = None;
        state.error_message = None;
        Ok(())
    }

    async fn finish_run_with(
        &self,
        result: Result<Vec<AgentMessage>, AgentError>,
        signal: &AbortSignal,
    ) -> Result<(), AgentError> {
        let failure_result = match result {
            Ok(_) => Ok(()),
            Err(error) => self.emit_run_failure(error, signal.aborted()).await,
        };
        self.finish_run();
        failure_result
    }

    async fn emit_run_failure(&self, error: AgentError, aborted: bool) -> Result<(), AgentError> {
        let error_message = CallbackError::from_error(error.message().clone(), error)
            .error_or_string_message()
            .map_err(AgentError::from)?;
        let partial = self
            .state_lock()
            .streaming_message
            .clone()
            .filter(|message| message.as_assistant().is_some());
        let failure = self.build_failure_message(error_message, aborted, partial.as_ref());
        if partial.is_none() {
            self.process_events(AgentEvent::MessageStart {
                message: failure.clone(),
            })
            .await?;
        }
        for event in [
            AgentEvent::MessageEnd {
                message: failure.clone(),
            },
            AgentEvent::TurnEnd {
                message: failure.clone(),
                tool_results: Vec::new(),
            },
            AgentEvent::AgentEnd {
                messages: vec![failure],
            },
        ] {
            self.process_events(event).await?;
        }
        Ok(())
    }

    fn build_failure_message(
        &self,
        error_message: JsString,
        aborted: bool,
        partial: Option<&AgentMessage>,
    ) -> AgentMessage {
        let state = self.state_lock();
        let mut message = AssistantMessage::empty(
            state.model.api.clone(),
            state.model.provider.clone(),
            state.model.id.clone(),
            pi_ai::request::now_millis() as f64,
        );
        message.content = vec![pi_ai::types::AssistantContent::Text(TextContent::new(""))];
        if let Some(partial) = partial.and_then(AgentMessage::as_assistant) {
            message = partial.clone();
        }
        message.stop_reason = if aborted {
            StopReason::Aborted
        } else {
            StopReason::Error
        };
        message.error_message = Some(error_message);
        AgentMessage::Assistant(Box::new(message))
    }

    fn finish_run(&self) {
        let run = self.active_run_lock().take();
        {
            let mut state = self.state_lock();
            state.is_streaming = false;
            state.is_model_request_active = false;
            state.streaming_message = None;
            state.pending_tool_calls.clear();
        }
        if let Some(run) = run {
            run.handle.finish();
        }
    }

    fn create_context_snapshot(&self) -> AgentContext {
        let state = self.state_lock();
        AgentContext {
            system_prompt: state.system_prompt.clone(),
            messages: state.messages.clone(),
            tools: Some(state.tools.clone()),
        }
    }

    fn create_loop_config(&self, skip_initial_steering_poll: bool) -> AgentLoopConfig {
        let (get_steering_messages, get_follow_up_messages) = self.build_queue_hooks(skip_initial_steering_poll);
        let state = self.state_lock();

        AgentLoopConfig {
            model: state.model.clone(),
            stream_options: self.build_stream_options(state.thinking_level),
            convert_to_llm: self
                .inner
                .convert_to_llm
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
            transform_context: self.hook(&self.inner.transform_context),
            get_api_key: self.hook(&self.inner.get_api_key),
            should_stop_after_turn: self.hook(&self.inner.should_stop_after_turn),
            prepare_next_turn: self.build_prepare_next_turn_hook(),
            get_steering_messages,
            get_follow_up_messages,
            tool_execution: *self
                .inner
                .tool_execution
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            before_tool_call: self.hook(&self.inner.before_tool_call),
            after_tool_call: self.hook(&self.inner.after_tool_call),
        }
    }

    fn build_stream_options(&self, thinking_level: ThinkingLevel) -> SimpleStreamOptions {
        let mut stream_options = self
            .inner
            .stream_options
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        stream_options.reasoning = reasoning_for(thinking_level);
        stream_options.stream.session_id = self.session_id();
        stream_options.stream.transport = Some(
            *self
                .inner
                .transport
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        stream_options.thinking_budgets = self
            .inner
            .thinking_budgets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        stream_options.stream.request.max_retry_delay_ms = *self
            .inner
            .max_retry_delay_ms
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        stream_options
    }

    fn build_prepare_next_turn_hook(&self) -> Option<PrepareNextTurnHook> {
        if self.hook(&self.inner.prepare_next_turn_with_context).is_none()
            && self.hook(&self.inner.prepare_next_turn).is_none()
        {
            return None;
        }
        let agent = self.clone();
        Some(Arc::new(move |context: PrepareNextTurnContext| {
            let with_context = agent.hook(&agent.inner.prepare_next_turn_with_context);
            let signal_only = agent.hook(&agent.inner.prepare_next_turn);
            let signal = agent.signal();
            Box::pin(async move {
                if let Some(hook) = with_context {
                    return hook(context, signal).await;
                }
                if let Some(hook) = signal_only {
                    return hook(signal).await;
                }
                Ok(None)
            })
        }))
    }

    fn build_queue_hooks(
        &self,
        skip_initial_steering_poll: bool,
    ) -> (Option<MessageQueueHook>, Option<MessageQueueHook>) {
        let skip_initial_steering_poll = Arc::new(AtomicBool::new(skip_initial_steering_poll));
        let steering_queue = Arc::clone(&self.inner.steering_queue);
        let follow_up_queue = Arc::clone(&self.inner.follow_up_queue);

        let get_steering_messages = Some(Arc::new(move || {
            let steering_queue = Arc::clone(&steering_queue);
            let skip_initial_steering_poll = Arc::clone(&skip_initial_steering_poll);
            Box::pin(async move {
                if skip_initial_steering_poll.swap(false, Ordering::SeqCst) {
                    return Ok(Vec::new());
                }
                Ok(queue_drain(&steering_queue))
            }) as pi_ai::request::BoxFuture<'static, Result<Vec<AgentMessage>, AgentError>>
        }) as MessageQueueHook);
        let get_follow_up_messages = Some(Arc::new(move || {
            let follow_up_queue = Arc::clone(&follow_up_queue);
            Box::pin(async move { Ok(queue_drain(&follow_up_queue)) })
                as pi_ai::request::BoxFuture<'static, Result<Vec<AgentMessage>, AgentError>>
        }) as MessageQueueHook);

        (get_steering_messages, get_follow_up_messages)
    }

    fn event_sink(&self) -> AgentEventSink {
        let agent = self.clone();
        Arc::new(move |event| {
            let agent = agent.clone();
            Box::pin(async move { agent.process_events(event).await })
        })
    }

    async fn process_events(&self, event: AgentEvent) -> Result<(), AgentError> {
        let message_identity = {
            let mut state = self.state_lock();
            let mut message_identity = None;
            match &event {
                AgentEvent::ModelRequestStart => state.is_model_request_active = true,
                AgentEvent::ModelRequestEnd => state.is_model_request_active = false,
                AgentEvent::MessageStart { message } | AgentEvent::MessageUpdate { message, .. } => {
                    state.streaming_message = Some(message.clone());
                }
                AgentEvent::MessageEnd { message } => {
                    state.streaming_message = None;
                    message_identity = Some(state.append_message(message.clone()));
                }
                AgentEvent::ToolExecutionStart { tool_call_id, .. } => {
                    state.pending_tool_calls.insert(tool_call_id.clone());
                }
                AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
                    state.pending_tool_calls.shift_remove(tool_call_id);
                }
                AgentEvent::TurnEnd { message, .. } => {
                    if let Some(assistant) = message.as_assistant()
                        && let Some(error_message) = &assistant.error_message
                        && !error_message.is_empty()
                    {
                        state.error_message = Some(error_message.clone());
                    }
                }
                AgentEvent::AgentEnd { .. } => state.streaming_message = None,
                _ => {}
            }
            message_identity
        };

        let signal = self.signal().ok_or_else(AgentError::listener_outside_run)?;
        let mut last_id = 0;
        loop {
            let next = self
                .inner
                .listeners
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .iter()
                .find(|(id, _)| *id > last_id)
                .cloned();
            let Some((id, listener)) = next else {
                break;
            };
            last_id = id;
            let scope = AgentEventIdentity {
                agent: Arc::downgrade(&self.inner),
                message: message_identity.clone(),
            };
            if let Err(error) = AGENT_EVENT_IDENTITY
                .scope(scope, async { listener(event.clone(), signal.clone()).await })
                .await
            {
                eprintln!("Agent event listener error ({}): {error}", event.kind());
            }
        }
        Ok(())
    }

    fn state_lock(&self) -> MutexGuard<'_, AgentStorage> {
        self.inner.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn active_run_lock(&self) -> MutexGuard<'_, Option<ActiveRun>> {
        self.inner
            .active_run
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn queue_lock<'a>(&self, queue: &'a Mutex<PendingMessageQueue>) -> MutexGuard<'a, PendingMessageQueue> {
        queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn current_signal(&self) -> AbortSignal {
        self.signal().expect("run has an active abort signal")
    }
}

fn queue_drain(queue: &Arc<Mutex<PendingMessageQueue>>) -> Vec<AgentMessage> {
    queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).drain()
}

fn normalize_prompt_input(input: PromptInput, images: Vec<ImageContent>) -> Vec<AgentMessage> {
    match input {
        PromptInput::Messages(messages) => messages,
        PromptInput::Message(message) => vec![*message],
        PromptInput::Text(text) => {
            let mut content = vec![TextOrImageContent::Text(TextContent::new(text))];
            content.extend(images.into_iter().map(TextOrImageContent::Image));
            vec![AgentMessage::User(UserMessage {
                role: UserRole::Tag,
                content: UserContent::Blocks(content),
                input_source: None,
                provider_native: None,
                timestamp: pi_ai::request::now_millis() as f64,
            })]
        }
    }
}

fn default_convert_to_llm(messages: Vec<AgentMessage>) -> BoxFuture<'static, Result<Vec<Message>, AgentError>> {
    Box::pin(async move {
        Ok(messages
            .into_iter()
            .filter_map(|message| match message {
                AgentMessage::User(message) => Some(Message::User(message)),
                AgentMessage::Assistant(message) => Some(Message::Assistant(message)),
                AgentMessage::ToolResult(message) => Some(Message::ToolResult(message)),
                AgentMessage::Custom(_) => None,
            })
            .collect())
    })
}

fn default_model() -> Model {
    Model {
        properties: Default::default(),
        id: "unknown".to_owned(),
        name: "unknown".into(),
        api: "unknown".to_owned(),
        provider: "unknown".to_owned(),
        base_url: JsString::default(),
        reasoning: false,
        thinking_level_map: None,
        input: Vec::new(),
        cost: ModelCost {
            properties: Default::default(),
            rates: ModelCostRates::default(),
        },
        context_window: 0.0,
        auto_compaction_threshold: None,
        max_tokens: 0.0,
        headers: None,
        compat: None,
    }
}
