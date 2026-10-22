use super::super::convert_to_llm;
use super::super::prompt_templates::format_prompt_template_invocation;
use super::super::skills::format_skill_invocation;
use super::*;
use crate::agent::types::reasoning_for;
use crate::agent::{
    AgentContext, AgentError, AgentEvent, AgentLoopConfig, AgentLoopTurnUpdate, StreamFn, run_agent_loop,
};
use pi_ai::request::{CallbackError, SimpleStreamOptions};
use pi_ai::types::{
    AssistantContent, ImageContent, StopReason, TextContent, TextOrImageContent, UserContent, UserMessage, UserRole,
};
use pi_js_compat::{js_format, js_json::JsValue};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct TurnState {
    messages: Vec<AgentMessage>,
    resources: AgentHarnessResources,
    stream_options: AgentHarnessStreamOptions,
    session_id: JsString,
    system_prompt: JsString,
    model: Model,
    thinking_level: ThinkingLevel,
    active_tools: Vec<Arc<dyn AgentTool>>,
}
impl TurnState {
    fn context(&self, system_prompt: Option<JsString>) -> AgentContext {
        AgentContext {
            system_prompt: system_prompt.unwrap_or_else(|| self.system_prompt.clone()),
            messages: self.messages.clone(),
            tools: Some(self.active_tools.clone()),
        }
    }
}
impl<M: SessionMetadataValue> AgentHarness<M> {
    async fn create_turn_state(&self) -> Result<TurnState, AgentHarnessError> {
        let context = self.inner.session.build_context().await?;
        let resources = self.get_resources();
        let metadata = self.inner.session.get_metadata().await?;
        let active_tools = self.get_active_tools();
        let (model, thinking_level, system_prompt) = {
            let state = self.state();
            (state.model.clone(), state.thinking_level, state.system_prompt.clone())
        };
        let system_prompt = match system_prompt {
            None => "You are a helpful assistant.".into(),
            Some(HarnessSystemPrompt::Text(text)) => text,
            Some(HarnessSystemPrompt::Callback(callback)) => {
                callback(SystemPromptContext {
                    env: self.env(),
                    session: self.session(),
                    model: model.clone(),
                    thinking_level,
                    active_tools: active_tools.clone(),
                    resources: resources.clone(),
                })
                .await?
            }
        };
        // Frozen reads model/options again after the asynchronous system prompt callback.
        let state = self.state();
        Ok(TurnState {
            messages: context.messages,
            resources,
            stream_options: state.stream_options.clone(),
            session_id: metadata.id().clone(),
            system_prompt,
            model: state.model.clone(),
            thinking_level: state.thinking_level,
            active_tools,
        })
    }
    pub(super) async fn run_prompt(
        &self,
        invocation: PromptInvocation,
        options: Option<AgentHarnessPromptOptions>,
    ) -> Result<AssistantMessage, AgentHarnessError> {
        let completion = self.start_run()?;
        let result = self.prepare_and_execute(invocation, options).await;
        if result.is_err() {
            self.state().phase = AgentHarnessPhase::Idle;
        }
        self.state().completion = None;
        completion.send_replace(true);
        result
    }
    async fn prepare_and_execute(
        &self,
        invocation: PromptInvocation,
        options: Option<AgentHarnessPromptOptions>,
    ) -> Result<AssistantMessage, AgentHarnessError> {
        let turn = self.create_turn_state().await?;
        let text = match invocation {
            PromptInvocation::Text(text) => text,
            PromptInvocation::Skill(name, instructions) => {
                let skill = turn
                    .resources
                    .skills
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .find(|skill| skill.name == name)
                    .ok_or_else(|| {
                        AgentHarnessError::new(
                            AgentHarnessErrorCode::InvalidArgument,
                            js_format!("Unknown skill: {}", name),
                        )
                    })?;
                format_skill_invocation(skill, instructions.as_ref())
            }
            PromptInvocation::Template(name, args) => {
                let template = turn
                    .resources
                    .prompt_templates
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .find(|template| template.name == name)
                    .ok_or_else(|| {
                        AgentHarnessError::new(
                            AgentHarnessErrorCode::InvalidArgument,
                            js_format!("Unknown prompt template: {}", name),
                        )
                    })?;
                format_prompt_template_invocation(template, &args)
            }
        };
        self.execute_turn(turn, text, options).await
    }
    async fn execute_turn(
        &self,
        turn: TurnState,
        text: JsString,
        options: Option<AgentHarnessPromptOptions>,
    ) -> Result<AssistantMessage, AgentHarnessError> {
        let images = options.and_then(|options| options.images);
        let mut messages = self.drain_next_turn().await?;
        messages.push(create_user_message(text.clone(), images.clone()));
        let before = self
            .hook(
                HarnessHookKind::BeforeAgentStart,
                HarnessEvent::BeforeAgentStart {
                    prompt: text,
                    images,
                    system_prompt: turn.system_prompt.clone(),
                    resources: turn.resources.clone(),
                },
            )
            .await?;
        let system_prompt = if let Some(HarnessHookResult::BeforeAgentStart {
            messages: extra,
            system_prompt,
        }) = before
        {
            messages.extend(extra.unwrap_or_default());
            system_prompt
        } else {
            None
        };
        let abort = CancellationToken::new();
        self.state().abort = Some(abort.clone());
        let active = Arc::new(Mutex::new(turn.clone()));
        let harness = self.clone();
        let signal = abort.clone();
        let emit = Arc::new(move |event| {
            let harness = harness.clone();
            let signal = signal.clone();
            Box::pin(async move {
                harness
                    .handle_agent_event(event, Some(signal))
                    .await
                    .map_err(Into::into)
            }) as pi_ai::request::BoxFuture<'static, Result<(), AgentError>>
        });
        let result = run_agent_loop(
            messages,
            turn.context(system_prompt),
            self.loop_config(active.clone()),
            emit,
            Some(abort.clone()),
            Some(self.stream_fn(active.clone())),
        )
        .await;
        let result = match result {
            Ok(messages) => Ok(messages),
            Err(error) => {
                let model = active.lock().expect("turn state").model.clone();
                self.emit_failure(model, error, abort.clone()).await
            }
        };
        let result = result.and_then(|messages| {
            messages
                .into_iter()
                .rev()
                .find_map(|message| match message {
                    AgentMessage::Assistant(message) => Some(*message),
                    _ => None,
                })
                .ok_or_else(|| {
                    AgentHarnessError::new(
                        AgentHarnessErrorCode::InvalidState,
                        "AgentHarness prompt completed without an assistant message",
                    )
                })
        });
        let flushed = self.flush_pending().await;
        self.state().abort = None;
        flushed?;
        result
    }
    async fn emit_failure(
        &self,
        model: Model,
        error: AgentError,
        signal: CancellationToken,
    ) -> Result<Vec<AgentMessage>, AgentHarnessError> {
        let message: AgentMessage =
            create_failure_message(&model, error.message().clone(), signal.is_cancelled()).into();
        let events = [
            AgentEvent::MessageStart {
                message: message.clone(),
            },
            AgentEvent::MessageEnd {
                message: message.clone(),
            },
            AgentEvent::TurnEnd {
                message: message.clone(),
                tool_results: Vec::new(),
            },
            AgentEvent::AgentEnd {
                messages: vec![message.clone()],
            },
        ];
        for event in events {
            if let Err(failure) = self.handle_agent_event(event, Some(signal.clone())).await {
                return Err(AgentHarnessError::aggregate(
                    AgentHarnessErrorCode::Unknown,
                    "Agent run failed and failure reporting failed",
                    vec![Arc::new(error), Arc::new(failure)],
                ));
            }
        }
        Ok(vec![message])
    }
    async fn handle_agent_event(
        &self,
        event: AgentEvent,
        signal: Option<CancellationToken>,
    ) -> Result<(), AgentHarnessError> {
        match &event {
            AgentEvent::MessageEnd { message } => {
                self.inner.session.append_message(message.clone()).await?;
                self.emit(HarnessEvent::Agent(event), signal).await
            }
            AgentEvent::TurnEnd { .. } => {
                let emitted = self.emit(HarnessEvent::Agent(event), signal).await;
                let had_pending_mutations = !self.state().pending.is_empty();
                self.flush_pending().await?;
                emitted?;
                self.emit(HarnessEvent::SavePoint { had_pending_mutations }, None).await
            }
            AgentEvent::AgentEnd { .. } => {
                self.flush_pending().await?;
                self.state().phase = AgentHarnessPhase::Idle;
                self.emit(HarnessEvent::Agent(event), signal.clone()).await?;
                let next_turn_count = self.state().next_turn.len() as f64;
                self.emit(HarnessEvent::Settled { next_turn_count }, signal).await
            }
            _ => self.emit(HarnessEvent::Agent(event), signal).await,
        }
    }
    async fn request_options(
        &self,
        model: Model,
        turn: &TurnState,
    ) -> Result<AgentHarnessStreamOptions, AgentHarnessError> {
        let mut options = turn.stream_options.clone();
        let mut cursor = None;
        while let Some((id, hook)) = self.next_hook(HarnessHookKind::BeforeProviderRequest, cursor) {
            cursor = Some(id);
            let event = HarnessEvent::BeforeProviderRequest {
                model: model.clone(),
                session_id: turn.session_id.clone(),
                stream_options: options.clone(),
            };
            if let Some(HarnessHookResult::BeforeProviderRequest(patch)) = hook(event).await? {
                options = options.patched(Some(&patch));
            }
        }
        Ok(options)
    }
    fn stream_fn(&self, active: Arc<Mutex<TurnState>>) -> StreamFn {
        let harness = self.clone();
        Arc::new(move |model, context, stream_options| {
            let harness = harness.clone();
            let active = active.clone();
            Box::pin(async move {
                let turn = active.lock().expect("turn state").clone();
                let options = harness.request_options(model.clone(), &turn).await?;
                let options = harness.forward_options(options, stream_options, turn.session_id)?;
                Ok(harness.inner.models.stream_simple(model, context, options))
            })
        })
    }
    fn forward_options(
        &self,
        request: AgentHarnessStreamOptions,
        loop_options: SimpleStreamOptions,
        session_id: JsString,
    ) -> Result<SimpleStreamOptions, AgentError> {
        let mut options = SimpleStreamOptions {
            reasoning: loop_options.reasoning,
            ..Default::default()
        };
        let stream = &mut options.stream;
        stream.cache_retention = request.cache_retention;
        stream.transport = request.transport;
        stream.session_id = Some(session_id);
        stream.metadata = request.metadata;
        stream.request.timeout_ms = request.timeout_ms;
        stream.request.max_retries = request.max_retries;
        stream.request.max_retry_delay_ms = request.max_retry_delay_ms;
        stream.request.signal = loop_options.stream.request.signal.clone();
        stream.request.headers = request
            .headers
            .map(|headers| {
                headers
                    .into_iter()
                    .map(|(key, value)| {
                        key.to_utf8()
                            .map(|key| (key, Some(value)))
                            .map_err(|error| AgentError::new(error.to_string()))
                    })
                    .collect()
            })
            .transpose()?;
        let harness = self.clone();
        stream.request.on_payload = Some(Arc::new(move |payload, model| {
            let harness = harness.clone();
            Box::pin(async move {
                harness
                    .provider_payload(model, payload)
                    .await
                    .map_err(|error| CallbackError::new(error.message))
            })
        }));
        let harness = self.clone();
        let signal = loop_options.stream.request.signal;
        stream.request.on_response = Some(Arc::new(move |response, _| {
            let harness = harness.clone();
            let signal = signal.clone();
            Box::pin(async move {
                harness
                    .emit(
                        HarnessEvent::AfterProviderResponse {
                            status: response.status,
                            headers: response.headers,
                        },
                        signal,
                    )
                    .await
                    .map_err(|error| CallbackError::new(error.message))
            })
        }));
        Ok(options)
    }
    async fn provider_payload(&self, model: Model, payload: JsValue) -> Result<Option<JsValue>, AgentHarnessError> {
        let mut payload = Some(payload);
        let mut cursor = None;
        while let Some((id, hook)) = self.next_hook(HarnessHookKind::BeforeProviderPayload, cursor) {
            cursor = Some(id);
            if let Some(HarnessHookResult::BeforeProviderPayload(next)) = hook(HarnessEvent::BeforeProviderPayload {
                model: model.clone(),
                payload: payload.clone(),
            })
            .await?
            {
                payload = next;
            }
        }
        Ok(payload)
    }
    fn loop_config(&self, active: Arc<Mutex<TurnState>>) -> AgentLoopConfig {
        let turn = active.lock().expect("turn state").clone();
        let mut config = AgentLoopConfig::new(
            turn.model,
            Arc::new(|messages| Box::pin(async move { convert_to_llm(&messages) })),
        );
        config.stream_options.reasoning = reasoning_for(turn.thinking_level);
        let harness = self.clone();
        config.transform_context = Some(Arc::new(move |messages, _| {
            let harness = harness.clone();
            Box::pin(async move {
                match harness
                    .hook(
                        HarnessHookKind::Context,
                        HarnessEvent::Context {
                            messages: messages.clone(),
                        },
                    )
                    .await?
                {
                    Some(HarnessHookResult::Context(messages)) => Ok(messages),
                    _ => Ok(messages),
                }
            })
        }));
        let harness = self.clone();
        config.before_tool_call = Some(Arc::new(move |ctx, _| {
            let harness = harness.clone();
            Box::pin(async move {
                match harness
                    .hook(
                        HarnessHookKind::ToolCall,
                        HarnessEvent::ToolCall {
                            input: ctx.args_snapshot(),
                            tool_call_id: ctx.tool_call.id,
                            tool_name: ctx.tool_call.name,
                        },
                    )
                    .await?
                {
                    Some(HarnessHookResult::ToolCall(result)) => Ok(Some(result)),
                    _ => Ok(None),
                }
            })
        }));
        let harness = self.clone();
        config.after_tool_call = Some(Arc::new(move |ctx, _| {
            let harness = harness.clone();
            Box::pin(async move {
                match harness
                    .hook(
                        HarnessHookKind::ToolResult,
                        HarnessEvent::ToolResult {
                            tool_call_id: ctx.tool_call.id,
                            tool_name: ctx.tool_call.name,
                            input: ctx.args,
                            content: ctx.result.content,
                            details: ctx.result.details,
                            is_error: ctx.is_error,
                        },
                    )
                    .await?
                {
                    Some(HarnessHookResult::ToolResult(result)) => Ok(Some(result)),
                    _ => Ok(None),
                }
            })
        }));
        let harness = self.clone();
        config.prepare_next_turn = Some(Arc::new(move |_| {
            let harness = harness.clone();
            let active = active.clone();
            Box::pin(async move {
                harness.flush_pending().await?;
                let turn = harness.create_turn_state().await?;
                let result = AgentLoopTurnUpdate {
                    context: Some(turn.context(None)),
                    model: Some(turn.model.clone()),
                    thinking_level: Some(turn.thinking_level),
                };
                *active.lock().expect("turn state") = turn;
                Ok(Some(result))
            })
        }));
        let harness = self.clone();
        config.get_steering_messages = Some(Arc::new(move || {
            let harness = harness.clone();
            Box::pin(async move { harness.drain_queue(true).await.map_err(Into::into) })
        }));
        let harness = self.clone();
        config.get_follow_up_messages = Some(Arc::new(move || {
            let harness = harness.clone();
            Box::pin(async move { harness.drain_queue(false).await.map_err(Into::into) })
        }));
        config
    }
    async fn queue_update(&self) -> Result<(), AgentHarnessError> {
        let event = {
            let state = self.state();
            HarnessEvent::QueueUpdate {
                steer: state.steer.clone(),
                follow_up: state.follow_up.clone(),
                next_turn: state.next_turn.clone(),
            }
        };
        self.emit(event, None).await
    }
    async fn drain_next_turn(&self) -> Result<Vec<AgentMessage>, AgentHarnessError> {
        let queued = std::mem::take(&mut self.state().next_turn);
        if queued.is_empty() {
            return Ok(queued);
        }
        if let Err(error) = self.queue_update().await {
            self.state().next_turn.splice(0..0, queued);
            return Err(error);
        }
        Ok(queued)
    }
    async fn drain_queue(&self, steering: bool) -> Result<Vec<AgentMessage>, AgentHarnessError> {
        let messages = {
            let mut state = self.state();
            let mode = if steering {
                state.steering_mode
            } else {
                state.follow_up_mode
            };
            let queue = if steering {
                &mut state.steer
            } else {
                &mut state.follow_up
            };
            let count = if mode == QueueMode::All {
                queue.len()
            } else {
                queue.len().min(1)
            };
            queue.drain(..count).collect::<Vec<_>>()
        };
        if messages.is_empty() {
            return Ok(messages);
        }
        if let Err(error) = self.queue_update().await {
            let mut state = self.state();
            let queue = if steering {
                &mut state.steer
            } else {
                &mut state.follow_up
            };
            queue.splice(0..0, messages);
            return Err(error);
        }
        Ok(messages)
    }
    pub async fn steer(
        &self,
        text: JsString,
        options: Option<AgentHarnessPromptOptions>,
    ) -> Result<(), AgentHarnessError> {
        {
            let mut state = self.state();
            if state.phase == AgentHarnessPhase::Idle {
                return Err(AgentHarnessError::new(
                    AgentHarnessErrorCode::InvalidState,
                    "Cannot steer while idle",
                ));
            }
            state
                .steer
                .push(create_user_message(text, options.and_then(|options| options.images)));
        }
        self.queue_update().await
    }
    pub async fn follow_up(
        &self,
        text: JsString,
        options: Option<AgentHarnessPromptOptions>,
    ) -> Result<(), AgentHarnessError> {
        {
            let mut state = self.state();
            if state.phase == AgentHarnessPhase::Idle {
                return Err(AgentHarnessError::new(
                    AgentHarnessErrorCode::InvalidState,
                    "Cannot follow up while idle",
                ));
            }
            state
                .follow_up
                .push(create_user_message(text, options.and_then(|options| options.images)));
        }
        self.queue_update().await
    }
    pub async fn next_turn(
        &self,
        text: JsString,
        options: Option<AgentHarnessPromptOptions>,
    ) -> Result<(), AgentHarnessError> {
        self.state()
            .next_turn
            .push(create_user_message(text, options.and_then(|options| options.images)));
        self.queue_update().await
    }
    pub async fn abort(&self) -> Result<AbortResult, AgentHarnessError> {
        let result = {
            let mut state = self.state();
            let result = AbortResult {
                cleared_steer: std::mem::take(&mut state.steer),
                cleared_follow_up: std::mem::take(&mut state.follow_up),
            };
            if let Some(abort) = &state.abort {
                abort.cancel();
            }
            result
        };
        let mut errors = Vec::new();
        if let Err(error) = self.queue_update().await {
            errors.push(error);
        }
        self.wait_for_idle().await;
        if let Err(error) = self.emit(HarnessEvent::Abort(result.clone()), None).await {
            errors.push(error);
        }
        match errors.len() {
            0 => Ok(result),
            1 => Err(errors.remove(0)),
            _ => Err(AgentHarnessError::aggregate(
                AgentHarnessErrorCode::Hook,
                "Abort completed with errors",
                errors
                    .into_iter()
                    .map(|error| Arc::new(error) as Arc<dyn std::error::Error + Send + Sync>)
                    .collect(),
            )),
        }
    }
}
fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_secs_f64()
        * 1000.0
}
fn create_user_message(text: JsString, images: Option<Vec<ImageContent>>) -> AgentMessage {
    let mut content = vec![TextOrImageContent::Text(TextContent::new(text))];
    content.extend(images.unwrap_or_default().into_iter().map(TextOrImageContent::Image));
    UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(content),
        timestamp: now_ms().floor(),
        input_source: None,
        provider_native: None,
    }
    .into()
}
fn create_failure_message(model: &Model, error: JsString, aborted: bool) -> AssistantMessage {
    let mut message = AssistantMessage::empty(
        model.api.clone(),
        model.provider.clone(),
        model.id.clone(),
        now_ms().floor(),
    );
    message.content = vec![AssistantContent::Text(TextContent::new(""))];
    message.error_message = Some(error);
    message.stop_reason = if aborted {
        StopReason::Aborted
    } else {
        StopReason::Error
    };
    message
}
