//! Port of `packages/agent/src/agent.ts`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use pi_ai::request::{BoxFuture, SimpleStreamOptions};
use pi_ai::types::{
    AssistantMessage, ImageContent, Message, Model, ModelCost, ModelCostRates, StopReason, TextContent,
    TextOrImageContent, ThinkingBudgets, Transport, UserContent, UserMessage, UserRole,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::agent::agent_loop::{AgentEventSink, run_agent_loop, run_agent_loop_continue};
use crate::agent::stream_fn::get_default_stream_fn;
use crate::agent::types::{
    AfterToolCallHook, AgentContext, AgentError, AgentEvent, AgentLoopConfig, AgentLoopTurnUpdate, AgentMessage,
    AgentState, AgentTool, BeforeToolCallHook, ConvertToLlm, GetApiKey, MessageQueueHook, PrepareNextTurnContext,
    PrepareNextTurnHook, ShouldStopAfterTurnContext, ShouldStopAfterTurnHook, StreamFn, TransformContext,
    reasoning_for,
};
use crate::harness::types::{QueueMode, ThinkingLevel, ToolExecutionMode};

/// `prepareNextTurn(signal?)`: legacy signal-only hook.
pub type PrepareNextTurnSignalHook = Arc<
    dyn Fn(Option<CancellationToken>) -> BoxFuture<'static, Result<Option<AgentLoopTurnUpdate>, AgentError>>
        + Send
        + Sync,
>;

/// `prepareNextTurnWithContext(context, signal?)` hook exposed by `AgentOptions`.
pub type PrepareNextTurnContextHook = Arc<
    dyn Fn(
            PrepareNextTurnContext,
            Option<CancellationToken>,
        ) -> BoxFuture<'static, Result<Option<AgentLoopTurnUpdate>, AgentError>>
        + Send
        + Sync,
>;

/// Event listener registered through [`Agent::subscribe`].
pub type AgentListener = Arc<dyn Fn(AgentEvent, CancellationToken) -> BoxFuture<'static, ()> + Send + Sync>;

/// Prompt input accepted by [`Agent::prompt`].
#[derive(Debug, Clone)]
pub enum PromptInput {
    Text(String),
    Message(Box<AgentMessage>),
    Messages(Vec<AgentMessage>),
}

impl From<&str> for PromptInput {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for PromptInput {
    fn from(value: String) -> Self {
        Self::Text(value)
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
    pub system_prompt: Option<String>,
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
    pub session_id: Option<String>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub transport: Option<Transport>,
    pub max_retry_delay_ms: Option<f64>,
    pub tool_execution: Option<ToolExecutionMode>,
    /// Provider options forwarded to each request (callbacks, headers, temperature, ...).
    pub stream_options: Option<SimpleStreamOptions>,
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
    token: CancellationToken,
    handle: Arc<RunHandle>,
}

struct AgentInner {
    state: Mutex<AgentState>,
    listeners: Mutex<Vec<(u64, AgentListener)>>,
    next_listener_id: AtomicU64,
    steering_queue: Arc<Mutex<PendingMessageQueue>>,
    follow_up_queue: Arc<Mutex<PendingMessageQueue>>,
    active_run: Mutex<Option<ActiveRun>>,
    convert_to_llm: ConvertToLlm,
    transform_context: Option<TransformContext>,
    stream_function: StreamFn,
    get_api_key: Option<GetApiKey>,
    before_tool_call: Option<BeforeToolCallHook>,
    after_tool_call: Option<AfterToolCallHook>,
    should_stop_after_turn: Option<ShouldStopAfterTurnHook>,
    prepare_next_turn: Option<PrepareNextTurnSignalHook>,
    prepare_next_turn_with_context: Option<PrepareNextTurnContextHook>,
    session_id: Mutex<Option<String>>,
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
        let stream_function = match options.stream_fn {
            Some(stream_function) => stream_function,
            None => get_default_stream_fn()?,
        };
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
            pending_tool_calls: std::collections::BTreeSet::new(),
            error_message: None,
        };
        Ok(Self {
            inner: Arc::new(AgentInner {
                state: Mutex::new(state),
                listeners: Mutex::new(Vec::new()),
                next_listener_id: AtomicU64::new(1),
                steering_queue: Arc::new(Mutex::new(PendingMessageQueue::new(
                    options.steering_mode.unwrap_or(QueueMode::OneAtATime),
                ))),
                follow_up_queue: Arc::new(Mutex::new(PendingMessageQueue::new(
                    options.follow_up_mode.unwrap_or(QueueMode::OneAtATime),
                ))),
                active_run: Mutex::new(None),
                convert_to_llm: options
                    .convert_to_llm
                    .unwrap_or_else(|| Arc::new(default_convert_to_llm) as ConvertToLlm),
                transform_context: options.transform_context,
                stream_function,
                get_api_key: options.get_api_key,
                before_tool_call: options.before_tool_call,
                after_tool_call: options.after_tool_call,
                should_stop_after_turn: options.should_stop_after_turn,
                prepare_next_turn: options.prepare_next_turn,
                prepare_next_turn_with_context: options.prepare_next_turn_with_context,
                session_id: Mutex::new(options.session_id),
                thinking_budgets: Mutex::new(options.thinking_budgets),
                transport: Mutex::new(options.transport.unwrap_or(Transport::Auto)),
                max_retry_delay_ms: Mutex::new(options.max_retry_delay_ms),
                tool_execution: Mutex::new(options.tool_execution.unwrap_or(ToolExecutionMode::Parallel)),
                stream_options: Mutex::new(options.stream_options.unwrap_or_default()),
            }),
        })
    }

    /// Subscribes to lifecycle events; returns the subscription id.
    pub fn subscribe(&self, listener: AgentListener) -> u64 {
        let id = self.inner.next_listener_id.fetch_add(1, Ordering::SeqCst);
        self.inner
            .listeners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((id, listener));
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        self.inner
            .listeners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|(listener_id, _)| *listener_id != id);
    }

    /// Current state snapshot.
    pub fn state(&self) -> AgentState {
        self.state_lock().clone()
    }

    pub fn system_prompt(&self) -> String {
        self.state_lock().system_prompt.clone()
    }

    pub fn set_system_prompt(&self, system_prompt: impl Into<String>) {
        self.state_lock().system_prompt = system_prompt.into();
    }

    pub fn model(&self) -> Model {
        self.state_lock().model.clone()
    }

    pub fn set_model(&self, model: Model) {
        self.state_lock().model = model;
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
        self.state_lock().messages = messages;
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

    pub fn pending_tool_calls(&self) -> std::collections::BTreeSet<String> {
        self.state_lock().pending_tool_calls.clone()
    }

    pub fn error_message(&self) -> Option<String> {
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

    pub fn session_id(&self) -> Option<String> {
        self.inner
            .session_id
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_session_id(&self, session_id: Option<String>) {
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
            run.token.cancel();
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
    pub fn signal(&self) -> Option<CancellationToken> {
        self.active_run_lock().as_ref().map(|run| run.token.clone())
    }

    pub fn abort(&self) {
        if let Some(run) = self.active_run_lock().as_ref() {
            run.token.cancel();
        }
    }

    /// Resolves after the current run and its awaited `agent_end` listeners finish.
    pub async fn wait_for_idle(&self) {
        let handle = self.active_run_lock().as_ref().map(|run| Arc::clone(&run.handle));
        if let Some(handle) = handle {
            handle.wait().await;
        }
    }

    pub fn reset(&self) -> Result<(), AgentError> {
        if self.active_run_lock().is_some() {
            return Err(AgentError::already_processing_reset());
        }
        {
            let mut state = self.state_lock();
            state.messages.clear();
            state.is_streaming = false;
            state.is_model_request_active = false;
            state.streaming_message = None;
            state.pending_tool_calls.clear();
            state.error_message = None;
        }
        self.clear_follow_up_queue();
        self.clear_steering_queue();
        Ok(())
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
            Some(self.inner.stream_function.clone()),
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
            Some(self.inner.stream_function.clone()),
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
            token: CancellationToken::new(),
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
        signal: &CancellationToken,
    ) -> Result<(), AgentError> {
        let mut listener_error = None;
        if let Err(error) = result {
            let failure = self.build_failure_message(&error, signal.is_cancelled());
            let events = [
                AgentEvent::MessageStart {
                    message: failure.clone(),
                },
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
            ];
            for event in events {
                if let Err(error) = self.process_events(event).await {
                    listener_error = Some(error);
                    break;
                }
            }
        }
        self.finish_run();
        match listener_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn build_failure_message(&self, error: &AgentError, aborted: bool) -> AgentMessage {
        let state = self.state_lock();
        let mut message = AssistantMessage::empty(
            state.model.api.clone(),
            state.model.provider.clone(),
            state.model.id.clone(),
            pi_ai::request::now_millis(),
        );
        message.content = vec![pi_ai::types::AssistantContent::Text(TextContent::new(""))];
        message.stop_reason = if aborted {
            StopReason::Aborted
        } else {
            StopReason::Error
        };
        message.error_message = Some(error.to_string());
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
        let run_signal = self.signal();
        let (get_steering_messages, get_follow_up_messages) = self.build_queue_hooks(skip_initial_steering_poll);
        let state = self.state_lock();

        AgentLoopConfig {
            model: state.model.clone(),
            stream_options: self.build_stream_options(state.thinking_level),
            convert_to_llm: Arc::clone(&self.inner.convert_to_llm),
            transform_context: self.inner.transform_context.clone(),
            get_api_key: self.inner.get_api_key.clone(),
            should_stop_after_turn: self.build_should_stop_hook(run_signal.clone()),
            prepare_next_turn: self.build_prepare_next_turn_hook(run_signal),
            get_steering_messages,
            get_follow_up_messages,
            tool_execution: *self
                .inner
                .tool_execution
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            before_tool_call: self.inner.before_tool_call.clone(),
            after_tool_call: self.inner.after_tool_call.clone(),
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

    fn build_should_stop_hook(&self, run_signal: Option<CancellationToken>) -> Option<ShouldStopAfterTurnHook> {
        self.inner.should_stop_after_turn.clone().map(|hook| {
            Arc::new(
                move |context: ShouldStopAfterTurnContext, _: Option<CancellationToken>| {
                    hook(context, run_signal.clone())
                },
            ) as ShouldStopAfterTurnHook
        })
    }

    fn build_prepare_next_turn_hook(&self, run_signal: Option<CancellationToken>) -> Option<PrepareNextTurnHook> {
        match (
            &self.inner.prepare_next_turn_with_context,
            &self.inner.prepare_next_turn,
        ) {
            (Some(with_context), _) => {
                let with_context = Arc::clone(with_context);
                Some(
                    Arc::new(move |context: PrepareNextTurnContext| with_context(context, run_signal.clone()))
                        as PrepareNextTurnHook,
                )
            }
            (None, Some(signal_only)) => {
                let signal_only = Arc::clone(signal_only);
                Some(Arc::new(move |_: PrepareNextTurnContext| signal_only(run_signal.clone())) as PrepareNextTurnHook)
            }
            (None, None) => None,
        }
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
                    return Vec::new();
                }
                queue_drain(&steering_queue)
            }) as pi_ai::request::BoxFuture<'static, Vec<AgentMessage>>
        }) as MessageQueueHook);
        let get_follow_up_messages = Some(Arc::new(move || {
            let follow_up_queue = Arc::clone(&follow_up_queue);
            Box::pin(async move { queue_drain(&follow_up_queue) })
                as pi_ai::request::BoxFuture<'static, Vec<AgentMessage>>
        }) as MessageQueueHook);

        (get_steering_messages, get_follow_up_messages)
    }

    fn event_sink(&self) -> AgentEventSink {
        let agent = self.clone();
        Arc::new(move |event| {
            let agent = agent.clone();
            Box::pin(async move {
                let _ = agent.process_events(event).await;
            })
        })
    }

    async fn process_events(&self, event: AgentEvent) -> Result<(), AgentError> {
        {
            let mut state = self.state_lock();
            match &event {
                AgentEvent::ModelRequestStart => state.is_model_request_active = true,
                AgentEvent::ModelRequestEnd => state.is_model_request_active = false,
                AgentEvent::MessageStart { message } | AgentEvent::MessageUpdate { message, .. } => {
                    state.streaming_message = Some(message.clone());
                }
                AgentEvent::MessageEnd { message } => {
                    state.streaming_message = None;
                    state.messages.push(message.clone());
                }
                AgentEvent::ToolExecutionStart { tool_call_id, .. } => {
                    state.pending_tool_calls.insert(tool_call_id.clone());
                }
                AgentEvent::ToolExecutionEnd { tool_call_id, .. } => {
                    state.pending_tool_calls.remove(tool_call_id);
                }
                AgentEvent::TurnEnd { message, .. } => {
                    if let Some(assistant) = message.as_assistant()
                        && let Some(error_message) = &assistant.error_message
                    {
                        state.error_message = Some(error_message.clone());
                    }
                }
                AgentEvent::AgentEnd { .. } => state.streaming_message = None,
                _ => {}
            }
        }

        let signal = self.signal().ok_or_else(AgentError::listener_outside_run)?;
        let listeners: Vec<AgentListener> = self
            .inner
            .listeners
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .map(|(_, listener)| Arc::clone(listener))
            .collect();
        for listener in listeners {
            listener(event.clone(), signal.clone()).await;
        }
        Ok(())
    }

    fn state_lock(&self) -> MutexGuard<'_, AgentState> {
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

    fn current_signal(&self) -> CancellationToken {
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
                timestamp: pi_ai::request::now_millis(),
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
        id: "unknown".to_owned(),
        name: "unknown".to_owned(),
        api: "unknown".to_owned(),
        provider: "unknown".to_owned(),
        base_url: String::new(),
        reasoning: false,
        thinking_level_map: None,
        input: Vec::new(),
        cost: ModelCost {
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 0.0,
        auto_compaction_threshold: None,
        max_tokens: 0.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}
