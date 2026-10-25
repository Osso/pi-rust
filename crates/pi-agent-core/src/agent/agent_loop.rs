//! Port of `packages/agent-core/src/agent-loop.ts`.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::request::{CallbackError, SimpleStreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, Message, StopReason, TextContent,
    TextOrImageContent, Tool, ToolCall, ToolResultMessage, UserMessage, UserRole,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, EventStream, event_stream};
use pi_ai::utils::validation::validate_tool_arguments;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use tokio::task::JoinSet;

use crate::agent::stream_fn::default_stream_fn;
use crate::agent::types::{
    AfterToolCallContext, AfterToolCallHook, AgentContext, AgentError, AgentEvent, AgentLoopConfig, AgentMessage,
    AgentTool, AgentToolResult, AgentToolUpdateCallback, BeforeToolCallContext, PrepareNextTurnContext,
    ShouldStopAfterTurnContext, StreamFn,
};

/// Event sink used by the low-level loop.
pub type AgentEventSink =
    Arc<dyn Fn(AgentEvent) -> pi_ai::request::BoxFuture<'static, Result<(), AgentError>> + Send + Sync>;

const END_TURN_CONTINUATION_INSTRUCTION: &str = "Your previous response was already delivered. Do not continue, repeat, or infer a new user request. Call `end_turn` now with a concise reason.";

const MAX_TOOL_RESULT_TEXT_BYTES: usize = 10 * 1024;
const MAX_TOOL_RESULT_TEXT_LINES: usize = 2000;

/// Starts an agent loop with a new prompt message, streaming events.
pub fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<AbortSignal>,
    stream_fn: Option<StreamFn>,
) -> EventStream<AgentEvent, Vec<AgentMessage>> {
    let (sender, stream) = create_agent_stream();
    tokio::spawn(async move {
        let result = run_agent_loop(prompts, context, config, event_sink(sender.clone()), signal, stream_fn).await;
        match result {
            Ok(messages) => sender.end(Some(messages)),
            Err(error) => sender.fail(CallbackError::from_error(error.message().clone(), error).into()),
        }
    });
    stream
}

/// Continues an agent loop from the current context without adding a prompt.
pub fn agent_loop_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<AbortSignal>,
    stream_fn: Option<StreamFn>,
) -> Result<EventStream<AgentEvent, Vec<AgentMessage>>, AgentError> {
    if context.messages.is_empty() {
        return Err(AgentError::cannot_continue_empty_context());
    }
    let (sender, stream) = create_agent_stream();
    tokio::spawn(async move {
        let result = run_agent_loop_continue(context, config, event_sink(sender.clone()), signal, stream_fn).await;
        match result {
            Ok(messages) => sender.end(Some(messages)),
            Err(error) => sender.fail(CallbackError::from_error(error.message().clone(), error).into()),
        }
    });
    Ok(stream)
}

fn event_sink(sender: pi_ai::utils::event_stream::EventSender<AgentEvent, Vec<AgentMessage>>) -> AgentEventSink {
    Arc::new(move |event| {
        let sender = sender.clone();
        Box::pin(async move {
            sender.push(event);
            Ok(())
        })
    })
}

fn create_agent_stream() -> (
    pi_ai::utils::event_stream::EventSender<AgentEvent, Vec<AgentMessage>>,
    EventStream<AgentEvent, Vec<AgentMessage>>,
) {
    event_stream(
        |event: &AgentEvent| matches!(event, AgentEvent::AgentEnd { .. }),
        |event: &AgentEvent| match event {
            AgentEvent::AgentEnd { messages } => messages.clone(),
            _ => Vec::new(),
        },
    )
}

/// Runs the loop for a new prompt batch and returns the messages produced.
pub async fn run_agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: AgentLoopConfig,
    emit: AgentEventSink,
    signal: Option<AbortSignal>,
    stream_fn: Option<StreamFn>,
) -> Result<Vec<AgentMessage>, AgentError> {
    let new_messages = prompts.clone();
    let mut current_context = AgentContext {
        system_prompt: context.system_prompt,
        messages: context.messages,
        tools: context.tools,
    };
    current_context.messages.extend(prompts.iter().cloned());

    emit_event(&emit, AgentEvent::AgentStart).await?;
    emit_event(&emit, AgentEvent::TurnStart).await?;
    for prompt in &prompts {
        emit_event(
            &emit,
            AgentEvent::MessageStart {
                message: prompt.clone(),
            },
        )
        .await?;
        emit_event(
            &emit,
            AgentEvent::MessageEnd {
                message: prompt.clone(),
            },
        )
        .await?;
    }

    let stream_fn = resolve_stream_fn(stream_fn);
    run_loop(current_context, new_messages, config, signal, &emit, stream_fn).await
}

/// Continues from an existing context and returns only newly produced messages.
pub async fn run_agent_loop_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    emit: AgentEventSink,
    signal: Option<AbortSignal>,
    stream_fn: Option<StreamFn>,
) -> Result<Vec<AgentMessage>, AgentError> {
    if context.messages.is_empty() {
        return Err(AgentError::cannot_continue_empty_context());
    }

    let mut new_messages: Vec<AgentMessage> = Vec::new();
    let mut current_context = context;
    let mut continuation_config = config;

    emit_event(&emit, AgentEvent::AgentStart).await?;
    emit_event(&emit, AgentEvent::TurnStart).await?;

    if restored_pending_tool_message(&current_context.messages).is_some()
        && let ResumeOutcome::Stop(messages) = resume_pending_tool_batch(
            &mut current_context,
            &mut new_messages,
            &mut continuation_config,
            &signal,
            &emit,
        )
        .await?
    {
        return Ok(messages);
    }

    let stream_fn = resolve_stream_fn(stream_fn);
    run_loop(
        current_context,
        new_messages,
        continuation_config,
        signal,
        &emit,
        stream_fn,
    )
    .await
}

enum ResumeOutcome {
    Continue,
    Stop(Vec<AgentMessage>),
}

async fn resume_pending_tool_batch(
    current_context: &mut AgentContext,
    new_messages: &mut Vec<AgentMessage>,
    config: &mut AgentLoopConfig,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
) -> Result<ResumeOutcome, AgentError> {
    let Some(pending_tool_message) = restored_pending_tool_message(&current_context.messages) else {
        return Ok(ResumeOutcome::Continue);
    };
    let batch = execute_tool_calls(current_context, &pending_tool_message, config, signal, emit).await?;
    for result in &batch.messages {
        current_context.messages.push(result.clone().into());
        new_messages.push(result.clone().into());
    }
    emit_event(
        emit,
        AgentEvent::TurnEnd {
            message: pending_tool_message.clone().into(),
            tool_results: batch.messages.clone(),
        },
    )
    .await?;
    throw_if_aborted(signal)?;
    if batch.terminate {
        let messages = finish_agent(new_messages.clone(), emit).await?;
        return Ok(ResumeOutcome::Stop(messages));
    }

    let next_turn_context = ShouldStopAfterTurnContext {
        message: pending_tool_message,
        tool_results: batch.messages,
        context: current_context.clone(),
        new_messages: new_messages.clone(),
    };
    if let Some(update) = prepare_next_turn(config, next_turn_context.clone()).await? {
        if let Some(context) = update.context.clone() {
            *current_context = context;
        }
        apply_config_update(config, &update);
    }
    let stop_context = ShouldStopAfterTurnContext {
        context: current_context.clone(),
        ..next_turn_context
    };
    if should_stop_after_turn(config, stop_context).await? {
        let messages = finish_agent(new_messages.clone(), emit).await?;
        return Ok(ResumeOutcome::Stop(messages));
    }
    Ok(ResumeOutcome::Continue)
}

fn resolve_stream_fn(stream_fn: Option<StreamFn>) -> StreamFn {
    stream_fn.unwrap_or_else(default_stream_fn)
}

/// Returns the trailing assistant batch reduced to calls without a tool result.
pub fn restored_pending_tool_message(messages: &[AgentMessage]) -> Option<AssistantMessage> {
    let mut completed_tool_call_ids = std::collections::BTreeSet::new();
    for message in messages.iter().rev() {
        match message {
            AgentMessage::ToolResult(result) => {
                completed_tool_call_ids.insert(result.tool_call_id.clone());
                continue;
            }
            AgentMessage::Assistant(assistant) => {
                if matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error) {
                    return None;
                }
                let pending_ids: std::collections::BTreeSet<&str> = assistant
                    .content
                    .iter()
                    .filter_map(|content| match content {
                        AssistantContent::ToolCall(call) if !completed_tool_call_ids.contains(&call.id) => {
                            Some(call.id.as_str())
                        }
                        _ => None,
                    })
                    .collect();
                if pending_ids.is_empty() {
                    return None;
                }
                let mut message = assistant.as_ref().clone();
                message.content.retain(|content| match content {
                    AssistantContent::ToolCall(call) => pending_ids.contains(call.id.as_str()),
                    _ => true,
                });
                return Some(message);
            }
            _ => return None,
        }
    }
    None
}

struct LoopState {
    current_context: AgentContext,
    config: AgentLoopConfig,
    first_turn: bool,
    continuation_instruction_pending: bool,
    new_messages: Vec<AgentMessage>,
}

struct TurnToolOutcome {
    results: Vec<ToolResultMessage>,
    has_more: bool,
    continuation_pending: bool,
}

async fn run_loop(
    initial_context: AgentContext,
    new_messages: Vec<AgentMessage>,
    initial_config: AgentLoopConfig,
    signal: Option<AbortSignal>,
    emit: &AgentEventSink,
    stream_fn: StreamFn,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut state = LoopState {
        current_context: initial_context,
        config: initial_config,
        first_turn: true,
        continuation_instruction_pending: false,
        new_messages,
    };
    let mut pending_messages = poll_steering(&state.config).await?;

    loop {
        let mut has_more_tool_calls = true;

        while has_more_tool_calls || !pending_messages.is_empty() {
            throw_if_aborted(&signal)?;
            start_turn(&mut state, emit).await?;
            inject_pending_messages(&mut state, &mut pending_messages, emit).await?;

            let message = request_assistant_message(&mut state, &signal, emit, &stream_fn).await?;
            state.new_messages.push(message.clone().into());

            if is_terminal_stop(&message) {
                emit_event(
                    emit,
                    AgentEvent::TurnEnd {
                        message: message.into(),
                        tool_results: Vec::new(),
                    },
                )
                .await?;
                return finish_agent(state.new_messages, emit).await;
            }

            let turn_tools = run_turn_tools(&state.current_context, &message, &state.config, &signal, emit).await?;
            has_more_tool_calls = turn_tools.has_more;
            state.continuation_instruction_pending = turn_tools.continuation_pending;
            append_tool_results(&mut state, &turn_tools.results);

            if let Some(messages) = finish_turn(&mut state, &message, &turn_tools.results, &signal, emit).await? {
                return Ok(messages);
            }

            pending_messages = poll_steering(&state.config).await?;
        }

        let follow_up_messages = poll_follow_up(&state.config).await?;
        if follow_up_messages.is_empty() {
            break;
        }
        pending_messages = follow_up_messages;
    }

    finish_agent(state.new_messages, emit).await
}

async fn finish_agent(messages: Vec<AgentMessage>, emit: &AgentEventSink) -> Result<Vec<AgentMessage>, AgentError> {
    emit_event(
        emit,
        AgentEvent::AgentEnd {
            messages: messages.clone(),
        },
    )
    .await?;
    Ok(messages)
}

async fn start_turn(state: &mut LoopState, emit: &AgentEventSink) -> Result<(), AgentError> {
    if state.first_turn {
        state.first_turn = false;
        return Ok(());
    }
    emit_event(emit, AgentEvent::TurnStart).await
}

async fn inject_pending_messages(
    state: &mut LoopState,
    pending_messages: &mut Vec<AgentMessage>,
    emit: &AgentEventSink,
) -> Result<(), AgentError> {
    if !pending_messages.is_empty() {
        state.continuation_instruction_pending = false;
    }
    for message in pending_messages.drain(..) {
        emit_event(
            emit,
            AgentEvent::MessageStart {
                message: message.clone(),
            },
        )
        .await?;
        emit_event(
            emit,
            AgentEvent::MessageEnd {
                message: message.clone(),
            },
        )
        .await?;
        state.current_context.messages.push(message.clone());
        state.new_messages.push(message);
    }
    Ok(())
}

async fn request_assistant_message(
    state: &mut LoopState,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
    stream_fn: &StreamFn,
) -> Result<AssistantMessage, AgentError> {
    let runtime_continuation_message = state.continuation_instruction_pending.then(|| UserMessage {
        role: UserRole::Tag,
        content: pi_ai::types::UserContent::Text(END_TURN_CONTINUATION_INSTRUCTION.to_owned().into()),
        input_source: None,
        provider_native: None,
        timestamp: pi_ai::request::now_millis() as f64,
    });
    state.continuation_instruction_pending = false;

    emit_event(emit, AgentEvent::ModelRequestStart).await?;
    let streamed = stream_assistant_response(
        &mut state.current_context,
        &state.config,
        signal,
        emit,
        stream_fn,
        runtime_continuation_message,
    )
    .await;
    emit_event(emit, AgentEvent::ModelRequestEnd).await?;
    streamed
}

async fn run_turn_tools(
    context: &AgentContext,
    message: &AssistantMessage,
    config: &AgentLoopConfig,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
) -> Result<TurnToolOutcome, AgentError> {
    if tool_calls_of(message).is_empty() {
        let has_end_turn = has_end_turn_tool(context);
        return Ok(TurnToolOutcome {
            results: Vec::new(),
            has_more: has_end_turn,
            continuation_pending: has_end_turn,
        });
    }

    let batch = execute_tool_calls(context, message, config, signal, emit).await?;
    Ok(TurnToolOutcome {
        results: batch.messages,
        has_more: !batch.terminate,
        continuation_pending: false,
    })
}

fn append_tool_results(state: &mut LoopState, results: &[ToolResultMessage]) {
    for result in results {
        state.current_context.messages.push(result.clone().into());
        state.new_messages.push(result.clone().into());
    }
}

async fn finish_turn(
    state: &mut LoopState,
    message: &AssistantMessage,
    tool_results: &[ToolResultMessage],
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
) -> Result<Option<Vec<AgentMessage>>, AgentError> {
    emit_event(
        emit,
        AgentEvent::TurnEnd {
            message: message.clone().into(),
            tool_results: tool_results.to_vec(),
        },
    )
    .await?;
    throw_if_aborted(signal)?;

    let next_turn_context = ShouldStopAfterTurnContext {
        message: message.clone(),
        tool_results: tool_results.to_vec(),
        context: state.current_context.clone(),
        new_messages: state.new_messages.clone(),
    };
    apply_next_turn(state, next_turn_context.clone()).await?;
    let stop_context = ShouldStopAfterTurnContext {
        context: state.current_context.clone(),
        ..next_turn_context
    };
    if should_stop_after_turn(&state.config, stop_context).await? {
        return Ok(Some(finish_agent(state.new_messages.clone(), emit).await?));
    }
    Ok(None)
}

async fn apply_next_turn(state: &mut LoopState, context: ShouldStopAfterTurnContext) -> Result<(), AgentError> {
    let Some(update) = prepare_next_turn(&state.config, context).await? else {
        return Ok(());
    };
    if let Some(context) = update.context.clone() {
        state.current_context = context;
    }
    apply_config_update(&mut state.config, &update);
    Ok(())
}

fn is_terminal_stop(message: &AssistantMessage) -> bool {
    matches!(
        message.stop_reason,
        StopReason::Error | StopReason::Aborted | StopReason::Length
    )
}

fn apply_config_update(config: &mut AgentLoopConfig, update: &crate::agent::types::AgentLoopTurnUpdate) {
    if let Some(model) = &update.model {
        config.model = model.clone();
    }
    if let Some(thinking_level) = update.thinking_level {
        config.stream_options.reasoning = crate::agent::types::reasoning_for(thinking_level);
    }
}

fn has_end_turn_tool(context: &AgentContext) -> bool {
    context
        .tools
        .as_ref()
        .is_some_and(|tools| tools.iter().any(|tool| tool.name() == "end_turn"))
}

async fn poll_steering(config: &AgentLoopConfig) -> Result<Vec<AgentMessage>, AgentError> {
    match &config.get_steering_messages {
        Some(hook) => hook().await,
        None => Ok(Vec::new()),
    }
}

async fn poll_follow_up(config: &AgentLoopConfig) -> Result<Vec<AgentMessage>, AgentError> {
    match &config.get_follow_up_messages {
        Some(hook) => hook().await,
        None => Ok(Vec::new()),
    }
}

async fn prepare_next_turn(
    config: &AgentLoopConfig,
    context: PrepareNextTurnContext,
) -> Result<Option<crate::agent::types::AgentLoopTurnUpdate>, AgentError> {
    match &config.prepare_next_turn {
        Some(hook) => hook(context).await,
        None => Ok(None),
    }
}

async fn should_stop_after_turn(
    config: &AgentLoopConfig,
    context: ShouldStopAfterTurnContext,
) -> Result<bool, AgentError> {
    match &config.should_stop_after_turn {
        Some(hook) => hook(context).await,
        None => Ok(false),
    }
}

async fn stream_assistant_response(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
    stream_fn: &StreamFn,
    runtime_continuation_message: Option<UserMessage>,
) -> Result<AssistantMessage, AgentError> {
    let messages = match &config.transform_context {
        Some(transform) => transform(context.messages.clone(), signal.clone()).await?,
        None => context.messages.clone(),
    };
    let mut request_messages = (config.convert_to_llm)(messages).await?;
    if let Some(continuation) = runtime_continuation_message {
        request_messages.push(Message::User(continuation));
    }
    let llm_context = Context {
        system_prompt: Some(context.system_prompt.clone()),
        messages: request_messages,
        tools: context.tools.as_ref().map(|tools| tools.iter().map(llm_tool).collect()),
    };

    let resolved_api_key = match &config.get_api_key {
        Some(get_api_key) => get_api_key(config.model.provider.clone())
            .await
            .filter(|key| !key.is_empty()),
        None => None,
    };
    let mut options: SimpleStreamOptions = config.stream_options.clone();
    options.stream.request.api_key = resolved_api_key.or_else(|| options.stream.request.api_key.clone());
    options.stream.request.signal = signal.clone();

    let mut response = await_with_abort(stream_fn(config.model.clone(), llm_context, options), signal).await??;
    let mut should_close = true;
    let streamed = consume_assistant_stream(&mut response, context, signal, emit, &mut should_close).await;
    let result = match streamed {
        Ok(streamed) => finish_assistant_message(context, signal, emit, streamed.message, streamed.added_partial).await,
        Err(error) => Err(error),
    };
    if should_close {
        close_assistant_iterator(&response, signal).await?;
    }
    result
}

async fn close_assistant_iterator(
    response: &AssistantMessageEventStream,
    signal: &Option<AbortSignal>,
) -> Result<(), AgentError> {
    let Some(close) = response
        .return_iterator()
        .map_err(|error| AgentError::new(error.message()))?
    else {
        return Ok(());
    };
    // JS promises keep running when awaitWithAbort stops waiting for them.
    let close = tokio::spawn(close);
    if signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Ok(());
    }
    let result = await_with_abort(close, signal)
        .await?
        .map_err(|error| AgentError::new(error.to_string()))?;
    result.map_err(|error| AgentError::new(error.message()))
}

struct StreamedAssistant {
    message: AssistantMessage,
    added_partial: bool,
}

async fn handle_stream_start(
    context: &mut AgentContext,
    partial: &AssistantMessage,
    emit: &AgentEventSink,
) -> Result<(), AgentError> {
    context.messages.push(partial.clone().into());
    emit_event(
        emit,
        AgentEvent::MessageStart {
            message: partial.clone().into(),
        },
    )
    .await
}

async fn handle_stream_update(
    context: &mut AgentContext,
    partial: &AssistantMessage,
    event: &AssistantMessageEvent,
    emit: &AgentEventSink,
) -> Result<(), AgentError> {
    if let Some(last) = context.messages.last_mut() {
        *last = partial.clone().into();
    }
    emit_event(
        emit,
        AgentEvent::MessageUpdate {
            message: partial.clone().into(),
            assistant_message_event: Box::new(event.clone()),
        },
    )
    .await
}

fn assistant_event_partial(event: &AssistantMessageEvent) -> Option<&AssistantMessage> {
    match event {
        AssistantMessageEvent::TextStart { partial, .. }
        | AssistantMessageEvent::TextDelta { partial, .. }
        | AssistantMessageEvent::TextEnd { partial, .. }
        | AssistantMessageEvent::ThinkingStart { partial, .. }
        | AssistantMessageEvent::ThinkingDelta { partial, .. }
        | AssistantMessageEvent::ThinkingEnd { partial, .. }
        | AssistantMessageEvent::ToolCallStart { partial, .. }
        | AssistantMessageEvent::ToolCallDelta { partial, .. }
        | AssistantMessageEvent::ToolCallEnd { partial, .. } => Some(partial),
        _ => None,
    }
}

async fn consume_assistant_stream(
    response: &mut AssistantMessageEventStream,
    context: &mut AgentContext,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
    should_close: &mut bool,
) -> Result<StreamedAssistant, AgentError> {
    let mut partial_message: Option<AssistantMessage> = None;
    let mut added_partial = false;

    loop {
        let next = await_with_abort(response.next(), signal).await;
        let next = match next {
            Ok(Some(Ok(event))) => Some(event),
            Ok(None) => {
                *should_close = false;
                None
            }
            failed => {
                *should_close = signal.as_ref().is_some_and(AbortSignal::aborted);
                return Err(match failed {
                    Err(error) => error,
                    Ok(Some(Err(error))) => AgentError::new(error.message()),
                    _ => unreachable!("handled successful next result"),
                });
            }
        };
        let Some(event) = next else {
            break;
        };
        match &event {
            AssistantMessageEvent::Start { partial } => {
                partial_message = Some(partial.clone());
                added_partial = true;
                handle_stream_start(context, partial, emit).await?;
            }
            AssistantMessageEvent::Done { .. } | AssistantMessageEvent::Error { .. } => {
                let message = await_with_abort(response.result(), signal)
                    .await?
                    .map_err(|failure| AgentError::new(failure.message()))?;
                return Ok(StreamedAssistant { message, added_partial });
            }
            _ => {
                let Some(partial) = assistant_event_partial(&event) else {
                    continue;
                };
                if partial_message.is_none() {
                    continue;
                }
                partial_message = Some(partial.clone());
                handle_stream_update(context, partial, &event, emit).await?;
            }
        }
    }

    let message = await_with_abort(response.result(), signal)
        .await?
        .map_err(|failure| AgentError::new(failure.message()))?;
    Ok(StreamedAssistant { message, added_partial })
}

async fn finish_assistant_message(
    context: &mut AgentContext,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
    provider_message: AssistantMessage,
    added_partial: bool,
) -> Result<AssistantMessage, AgentError> {
    let final_message = prepare_assistant_tool_executions(provider_message, context.tools.as_deref(), signal).await?;
    if added_partial {
        if let Some(last) = context.messages.last_mut() {
            *last = final_message.clone().into();
        }
    } else {
        context.messages.push(final_message.clone().into());
        emit_event(
            emit,
            AgentEvent::MessageStart {
                message: final_message.clone().into(),
            },
        )
        .await?;
    }
    emit_event(
        emit,
        AgentEvent::MessageEnd {
            message: final_message.clone().into(),
        },
    )
    .await?;
    Ok(final_message)
}

async fn prepare_assistant_tool_executions(
    message: AssistantMessage,
    tools: Option<&[Arc<dyn AgentTool>]>,
    signal: &Option<AbortSignal>,
) -> Result<AssistantMessage, AgentError> {
    let Some(tools) = tools else {
        return Ok(message);
    };
    if tools.is_empty() {
        return Ok(message);
    }
    let mut changed = false;
    let mut content = Vec::with_capacity(message.content.len());
    for block in &message.content {
        let AssistantContent::ToolCall(call) = block else {
            content.push(block.clone());
            continue;
        };
        if call.execution.is_some() {
            content.push(block.clone());
            continue;
        }
        let Some(tool) = tools.iter().rev().find(|tool| tool.name() == call.name) else {
            content.push(block.clone());
            continue;
        };
        let Some(prepare_execution) = tool.prepare_execution() else {
            content.push(block.clone());
            continue;
        };
        let execution = await_with_abort(prepare_execution(call.id.clone(), signal.clone()), signal).await?;
        match execution? {
            Some(execution) => {
                changed = true;
                let mut call = call.clone();
                call.execution = Some(execution);
                content.push(AssistantContent::ToolCall(call));
            }
            None => content.push(block.clone()),
        }
    }
    if changed {
        Ok(AssistantMessage { content, ..message })
    } else {
        Ok(message)
    }
}

fn llm_tool(tool: &Arc<dyn AgentTool>) -> Tool {
    let mut llm_tool = Tool::typebox(
        tool.name(),
        tool.description(),
        pi_ai::types::TypeBoxSchema::new(tool.parameters().clone()),
    );
    llm_tool.label = Some(tool.label());
    llm_tool.execution_mode = tool.execution_mode().map(|mode| mode.as_str().into());
    llm_tool
}

fn tool_calls_of(message: &AssistantMessage) -> Vec<ToolCall> {
    message
        .content
        .iter()
        .filter_map(|content| match content {
            AssistantContent::ToolCall(call) => Some(call.clone()),
            _ => None,
        })
        .collect()
}

struct ExecutedToolCallBatch {
    messages: Vec<ToolResultMessage>,
    terminate: bool,
}

struct PreparedToolCall {
    tool_call: ToolCall,
    tool: Arc<dyn AgentTool>,
    args: Value,
}

impl Clone for PreparedToolCall {
    fn clone(&self) -> Self {
        Self {
            tool_call: self.tool_call.clone(),
            tool: Arc::clone(&self.tool),
            args: self.args.clone(),
        }
    }
}

enum Preparation {
    Prepared(PreparedToolCall),
    Immediate {
        result: AgentToolResult,
        is_error: bool,
        args: Value,
    },
}

impl Preparation {
    fn args(&self) -> Value {
        match self {
            Self::Prepared(prepared) => prepared.args.clone(),
            Self::Immediate { args, .. } => args.clone(),
        }
    }
}

struct ExecutedToolCallOutcome {
    result: AgentToolResult,
    is_error: bool,
}

struct FinalizedToolCallOutcome {
    tool_call: ToolCall,
    result: AgentToolResult,
    is_error: bool,
}

enum FinalizedEntry {
    Done(Box<FinalizedToolCallOutcome>),
    Pending(pi_ai::request::BoxFuture<'static, Result<FinalizedToolCallOutcome, AgentError>>),
}

async fn execute_tool_calls(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    config: &AgentLoopConfig,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
) -> Result<ExecutedToolCallBatch, AgentError> {
    let tool_calls = tool_calls_of(assistant_message);
    let has_sequential_tool_call = tool_calls.iter().any(|tool_call| {
        current_context
            .find_tool(&tool_call.name)
            .is_some_and(|tool| tool.execution_mode() == Some(crate::harness::types::ToolExecutionMode::Sequential))
    });
    if config.tool_execution == crate::harness::types::ToolExecutionMode::Sequential || has_sequential_tool_call {
        execute_tool_calls_sequential(current_context, assistant_message, &tool_calls, config, signal, emit).await
    } else {
        execute_tool_calls_parallel(current_context, assistant_message, &tool_calls, config, signal, emit).await
    }
}

async fn execute_tool_calls_sequential(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_calls: &[ToolCall],
    config: &AgentLoopConfig,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
) -> Result<ExecutedToolCallBatch, AgentError> {
    let mut finalized_calls: Vec<FinalizedToolCallOutcome> = Vec::new();
    let mut messages: Vec<ToolResultMessage> = Vec::new();

    for tool_call in tool_calls {
        let preparation = prepare_tool_call(current_context, assistant_message, tool_call, config, signal).await;
        let started_at = now_ms_f64();
        emit_tool_execution_start(tool_call, &preparation, started_at, emit).await?;
        let finalized = match preparation {
            Preparation::Immediate { result, is_error, .. } => FinalizedToolCallOutcome {
                tool_call: tool_call.clone(),
                result,
                is_error,
            },
            Preparation::Prepared(prepared) => {
                let executed = execute_prepared_tool_call(prepared.clone(), signal, emit, started_at).await?;
                finalize_executed_tool_call(current_context, assistant_message, prepared, executed, config, signal)
                    .await
            }
        };
        let finalized = cap_finalized_tool_call_outcome(finalized);
        emit_tool_execution_end(&finalized, started_at, emit).await?;
        let tool_result_message = create_tool_result_message(&finalized);
        emit_tool_result_message(&tool_result_message, emit).await?;
        finalized_calls.push(finalized);
        messages.push(tool_result_message);
        if is_aborted(signal) {
            break;
        }
    }

    Ok(ExecutedToolCallBatch {
        messages,
        terminate: should_terminate_tool_batch(&finalized_calls),
    })
}

async fn execute_tool_calls_parallel(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_calls: &[ToolCall],
    config: &AgentLoopConfig,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
) -> Result<ExecutedToolCallBatch, AgentError> {
    let mut entries: Vec<FinalizedEntry> = Vec::new();

    for tool_call in tool_calls {
        entries.push(build_parallel_entry(current_context, assistant_message, tool_call, config, signal, emit).await?);
        if is_aborted(signal) {
            break;
        }
    }

    let ordered_finalized_calls = join_finalized_entries(entries).await?;
    let mut messages: Vec<ToolResultMessage> = Vec::new();
    for finalized in &ordered_finalized_calls {
        let tool_result_message = create_tool_result_message(finalized);
        emit_tool_result_message(&tool_result_message, emit).await?;
        messages.push(tool_result_message);
    }

    Ok(ExecutedToolCallBatch {
        messages,
        terminate: should_terminate_tool_batch(&ordered_finalized_calls),
    })
}

async fn build_parallel_entry(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_call: &ToolCall,
    config: &AgentLoopConfig,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
) -> Result<FinalizedEntry, AgentError> {
    let preparation = prepare_tool_call(current_context, assistant_message, tool_call, config, signal).await;
    let started_at = now_ms_f64();
    emit_tool_execution_start(tool_call, &preparation, started_at, emit).await?;

    match preparation {
        Preparation::Immediate { result, is_error, .. } => {
            let finalized = cap_finalized_tool_call_outcome(FinalizedToolCallOutcome {
                tool_call: tool_call.clone(),
                result,
                is_error,
            });
            emit_tool_execution_end(&finalized, started_at, emit).await?;
            Ok(FinalizedEntry::Done(Box::new(finalized)))
        }
        Preparation::Prepared(prepared) => {
            let current_context = current_context.clone();
            let assistant_message = assistant_message.clone();
            let config = config.clone();
            let signal = signal.clone();
            let emit = Arc::clone(emit);
            Ok(FinalizedEntry::Pending(Box::pin(async move {
                let executed = execute_prepared_tool_call(prepared.clone(), &signal, &emit, started_at).await?;
                let finalized = cap_finalized_tool_call_outcome(
                    finalize_executed_tool_call(
                        &current_context,
                        &assistant_message,
                        prepared,
                        executed,
                        &config,
                        &signal,
                    )
                    .await,
                );
                emit_tool_execution_end(&finalized, started_at, &emit).await?;
                Ok(finalized)
            })))
        }
    }
}

async fn join_finalized_entries(entries: Vec<FinalizedEntry>) -> Result<Vec<FinalizedToolCallOutcome>, AgentError> {
    let mut slots: Vec<Option<FinalizedToolCallOutcome>> = entries.iter().map(|_| None).collect();
    let mut pending = JoinSet::new();
    for (index, entry) in entries.into_iter().enumerate() {
        match entry {
            FinalizedEntry::Done(finalized) => slots[index] = Some(*finalized),
            FinalizedEntry::Pending(future) => {
                pending.spawn(async move { (index, future.await) });
            }
        }
    }
    while let Some(joined) = pending.join_next().await {
        let outcome = joined
            .map_err(|error| AgentError::new(error.to_string()))
            .and_then(|(index, result)| result.map(|finalized| (index, finalized)));
        match outcome {
            Ok((index, finalized)) => slots[index] = Some(finalized),
            Err(error) => {
                // Promise.all rejects without cancelling sibling tool promises.
                pending.detach_all();
                return Err(error);
            }
        }
    }
    slots
        .into_iter()
        .map(|slot| slot.ok_or_else(|| AgentError::new("Missing finalized tool call")))
        .collect()
}

fn should_terminate_tool_batch(finalized_calls: &[FinalizedToolCallOutcome]) -> bool {
    !finalized_calls.is_empty()
        && finalized_calls
            .iter()
            .all(|finalized| finalized.result.terminate == Some(true))
}

fn prepare_tool_call_arguments(tool: &Arc<dyn AgentTool>, tool_call: &ToolCall) -> Result<ToolCall, AgentError> {
    let original = Value::Object(tool_call.arguments.clone());
    let prepared = tool.prepare_arguments(original.clone())?;
    if prepared == original {
        return Ok(tool_call.clone());
    }
    let object = prepared
        .as_object()
        .cloned()
        .ok_or_else(|| AgentError::new("prepareArguments must return an object"))?;
    let mut tool_call = tool_call.clone();
    tool_call.arguments = object;
    Ok(tool_call)
}

fn immediate_error(message: impl Into<JsString>, args: Value) -> Preparation {
    Preparation::Immediate {
        result: create_error_tool_result(message),
        is_error: true,
        args,
    }
}

fn original_arguments(tool_call: &ToolCall) -> Value {
    Value::Object(tool_call.arguments.clone())
}

async fn prepare_tool_call(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_call: &ToolCall,
    config: &AgentLoopConfig,
    signal: &Option<AbortSignal>,
) -> Preparation {
    let Some(tool) = current_context.find_tool(&tool_call.name) else {
        return immediate_error(
            format!("Tool {} not found", tool_call.name),
            original_arguments(tool_call),
        );
    };

    let prepared_tool_call = match prepare_tool_call_arguments(&tool, tool_call) {
        Ok(prepared) => prepared,
        Err(error) => return immediate_error(error.message().clone(), original_arguments(tool_call)),
    };
    let validated_args = match validate_tool_arguments(&llm_tool(&tool), &prepared_tool_call) {
        Ok(args) => args,
        Err(error) => return immediate_error(error.message().clone(), original_arguments(tool_call)),
    };
    let validated_args = match apply_before_tool_call(
        config,
        assistant_message,
        tool_call,
        current_context,
        validated_args,
        signal,
    )
    .await
    {
        Ok(args) => args,
        Err(immediate) => return *immediate,
    };

    if is_aborted(signal) {
        return immediate_error("Operation aborted", validated_args);
    }
    Preparation::Prepared(PreparedToolCall {
        tool_call: tool_call.clone(),
        tool,
        args: validated_args,
    })
}

async fn apply_before_tool_call(
    config: &AgentLoopConfig,
    assistant_message: &AssistantMessage,
    tool_call: &ToolCall,
    current_context: &AgentContext,
    validated_args: Value,
    signal: &Option<AbortSignal>,
) -> Result<Value, Box<Preparation>> {
    let Some(before_tool_call) = &config.before_tool_call else {
        return Ok(validated_args);
    };

    let args_cell = Arc::new(Mutex::new(validated_args));
    let before_context = BeforeToolCallContext {
        assistant_message: assistant_message.clone(),
        tool_call: tool_call.clone(),
        args: Arc::clone(&args_cell),
        context: current_context.clone(),
    };
    let before_result = before_tool_call(before_context, signal.clone())
        .await
        .map_err(|error| Box::new(immediate_error(error.message().clone(), original_arguments(tool_call))))?;
    let validated_args = args_cell
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    if is_aborted(signal) {
        return Err(Box::new(immediate_error("Operation aborted", validated_args)));
    }
    let Some(before_result) = before_result else {
        return Ok(validated_args);
    };
    if before_result.block != Some(true) {
        return Ok(validated_args);
    }
    let tool_result = create_error_tool_result(
        before_result
            .reason
            .filter(|reason| !reason.is_empty())
            .unwrap_or_else(|| "Tool execution was blocked".to_owned().into()),
    );
    Err(Box::new(Preparation::Immediate {
        result: tool_result,
        is_error: true,
        args: validated_args,
    }))
}

type UpdateTasks = Arc<Mutex<Vec<tokio::task::JoinHandle<Result<(), AgentError>>>>>;

async fn execute_prepared_tool_call(
    prepared: PreparedToolCall,
    signal: &Option<AbortSignal>,
    emit: &AgentEventSink,
    started_at: f64,
) -> Result<ExecutedToolCallOutcome, AgentError> {
    let accepting_updates = Arc::new(AtomicBool::new(true));
    let update_tasks: UpdateTasks = Arc::new(Mutex::new(Vec::new()));
    let on_update = tool_update_callback(&prepared, emit, &accepting_updates, &update_tasks);

    let outcome = run_prepared_tool(prepared, signal, on_update, started_at).await;
    accepting_updates.store(false, Ordering::SeqCst);
    await_update_tasks(&update_tasks).await?;
    Ok(outcome)
}

fn tool_update_callback(
    prepared: &PreparedToolCall,
    emit: &AgentEventSink,
    accepting_updates: &Arc<AtomicBool>,
    update_tasks: &UpdateTasks,
) -> AgentToolUpdateCallback {
    let accepting_updates = Arc::clone(accepting_updates);
    let update_tasks = Arc::clone(update_tasks);
    let emit = Arc::clone(emit);
    let tool_call_id = prepared.tool_call.id.clone();
    let tool_name = prepared.tool_call.name.clone();
    let args = prepared.args.clone();
    Arc::new(move |partial_result: AgentToolResult| {
        if !accepting_updates.load(Ordering::SeqCst) {
            return Ok(());
        }
        let event = AgentEvent::ToolExecutionUpdate {
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            args: args.clone(),
            partial_result: cap_tool_result(partial_result),
        };
        let emit = Arc::clone(&emit);
        let task = tokio::spawn(async move { emit(event).await });
        update_tasks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(task);
        Ok(())
    })
}

async fn run_prepared_tool(
    prepared: PreparedToolCall,
    signal: &Option<AbortSignal>,
    on_update: AgentToolUpdateCallback,
    started_at: f64,
) -> ExecutedToolCallOutcome {
    let execution = crate::harness::types::AgentToolExecutionContext {
        started_at,
        agent_id: prepared
            .tool_call
            .execution
            .as_ref()
            .map(|execution| execution.agent_id.clone()),
    };
    let execution_future = prepared.tool.execute(
        prepared.tool_call.id.clone(),
        prepared.args.clone(),
        signal.clone(),
        on_update,
        Some(execution),
    );
    match await_tool_execution(execution_future, signal).await {
        Ok(Ok(result)) => ExecutedToolCallOutcome {
            is_error: result.is_error.unwrap_or(false),
            result,
        },
        Ok(Err(error)) => ExecutedToolCallOutcome {
            result: create_error_tool_result(error.message),
            is_error: true,
        },
        Err(error) => ExecutedToolCallOutcome {
            result: create_error_tool_result(error.to_string()),
            is_error: true,
        },
    }
}

/// Like an uncancelled JS promise, a tool ignoring its signal outlives the loop's
/// abort race. It runs until completion or Tokio runtime shutdown; late results
/// are discarded and the update callback is closed by the caller. A detached
/// observer consumes task panics rather than leaving a failed JoinHandle unread.
async fn await_tool_execution<T: Send + 'static>(
    execution: impl Future<Output = T> + Send + 'static,
    signal: &Option<AbortSignal>,
) -> Result<T, AgentError> {
    let mut task = tokio::spawn(execution);
    match await_with_abort(&mut task, signal).await {
        Ok(joined) => joined.map_err(|error| AgentError::new(error.to_string())),
        Err(error) => {
            tokio::spawn(async move {
                if let Err(error) = task.await {
                    eprintln!("Detached tool execution failed: {error}");
                }
            });
            Err(error)
        }
    }
}

async fn await_update_tasks(update_tasks: &UpdateTasks) -> Result<(), AgentError> {
    let mut tasks: Vec<_> = update_tasks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .drain(..)
        .collect();
    // Poll all sinks, as Promise.all rejects without waiting for pending siblings.
    // Dropping their JoinHandles detaches (does not cancel) the remaining sinks.
    std::future::poll_fn(move |cx| {
        let mut index = 0;
        while index < tasks.len() {
            match std::pin::Pin::new(&mut tasks[index]).poll(cx) {
                std::task::Poll::Ready(joined) => {
                    drop(tasks.swap_remove(index));
                    joined.map_err(|error| AgentError::new(error.to_string()))??;
                }
                std::task::Poll::Pending => index += 1,
            }
        }
        if tasks.is_empty() {
            std::task::Poll::Ready(Ok(()))
        } else {
            std::task::Poll::Pending
        }
    })
    .await
}

async fn finalize_executed_tool_call(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    prepared: PreparedToolCall,
    executed: ExecutedToolCallOutcome,
    config: &AgentLoopConfig,
    signal: &Option<AbortSignal>,
) -> FinalizedToolCallOutcome {
    let (result, is_error) = match &config.after_tool_call {
        Some(after_tool_call) => {
            apply_after_tool_call(
                after_tool_call,
                current_context,
                assistant_message,
                &prepared,
                executed,
                signal,
            )
            .await
        }
        None => (executed.result, executed.is_error),
    };

    FinalizedToolCallOutcome {
        tool_call: prepared.tool_call,
        result,
        is_error,
    }
}

async fn apply_after_tool_call(
    after_tool_call: &AfterToolCallHook,
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    prepared: &PreparedToolCall,
    executed: ExecutedToolCallOutcome,
    signal: &Option<AbortSignal>,
) -> (AgentToolResult, bool) {
    let after_context = AfterToolCallContext {
        assistant_message: assistant_message.clone(),
        tool_call: prepared.tool_call.clone(),
        args: prepared.args.clone(),
        result: executed.result.clone(),
        is_error: executed.is_error,
        context: current_context.clone(),
    };
    match after_tool_call(after_context, signal.clone()).await {
        Ok(Some(after_result)) => apply_after_tool_call_result(executed, after_result),
        Ok(None) => (executed.result, executed.is_error),
        Err(error) => (create_error_tool_result(error.message().clone()), true),
    }
}

fn apply_after_tool_call_result(
    executed: ExecutedToolCallOutcome,
    after_result: crate::agent::types::AfterToolCallResult,
) -> (AgentToolResult, bool) {
    let mut result = executed.result;
    result.is_error = None;
    let mut is_error = executed.is_error;
    if let Some(content) = after_result.content {
        result.content = content;
    }
    if let Some(details) = after_result.details.filter(|details| !details.is_null()) {
        result.details = Some(details);
    }
    if let Some(terminate) = after_result.terminate {
        result.terminate = Some(terminate);
    }
    if let Some(after_is_error) = after_result.is_error {
        is_error = after_is_error;
    }
    (result, is_error)
}

fn create_error_tool_result(message: impl Into<JsString>) -> AgentToolResult {
    AgentToolResult::text(message)
}

fn cap_finalized_tool_call_outcome(finalized: FinalizedToolCallOutcome) -> FinalizedToolCallOutcome {
    FinalizedToolCallOutcome {
        tool_call: finalized.tool_call,
        result: cap_tool_result(finalized.result),
        is_error: finalized.is_error,
    }
}

fn cap_tool_result(result: AgentToolResult) -> AgentToolResult {
    let content = cap_tool_result_content(result.content);
    let details = cap_tool_result_details(result.details);
    AgentToolResult {
        content,
        details,
        ..result
    }
}

fn cap_tool_result_content(content: Vec<TextOrImageContent>) -> Vec<TextOrImageContent> {
    content
        .into_iter()
        .map(|part| match part {
            TextOrImageContent::Text(text) => {
                let capped = cap_tool_result_text(&text.text);
                if capped == text.text {
                    TextOrImageContent::Text(text)
                } else {
                    TextOrImageContent::Text(TextContent { text: capped, ..text })
                }
            }
            other => other,
        })
        .collect()
}

fn cap_tool_result_details(details: Option<pi_js_compat::js_json::JsValue>) -> Option<pi_js_compat::js_json::JsValue> {
    use pi_js_compat::js_json::{JsString, JsValue, stringify_lossless};
    let details = details?;
    let text = JsString::from(stringify_lossless(&details));
    if !is_tool_result_text_too_large(&text) {
        return Some(details);
    }
    Some(JsValue::object([
        ("truncated", JsValue::Bool(true)),
        (
            "reason",
            JsValue::String("tool result details exceeded storage cap".into()),
        ),
        ("maxBytes", JsValue::Number(MAX_TOOL_RESULT_TEXT_BYTES as f64)),
        ("maxLines", JsValue::Number(MAX_TOOL_RESULT_TEXT_LINES as f64)),
        ("preview", JsValue::String(cap_tool_result_text(&text))),
    ]))
}

fn cap_tool_result_text(text: &pi_js_compat::js_json::JsString) -> pi_js_compat::js_json::JsString {
    use pi_js_compat::js_json::JsString;
    if !is_tool_result_text_too_large(text) {
        return text.clone();
    }
    let lines: Vec<_> = text.units().split(|unit| *unit == u16::from(b'\n')).collect();
    let first = lines.len().saturating_sub(MAX_TOOL_RESULT_TEXT_LINES);
    let line_capped = JsString::join(
        lines[first..].iter().map(|line| JsString::from_utf16(line.to_vec())),
        "\n",
    );
    let mut units = line_capped.units();
    // Buffer.byteLength measures Node UTF-8 encoding; never store its replacement text.
    while !units.is_empty() && String::from_utf16_lossy(units).len() > MAX_TOOL_RESULT_TEXT_BYTES {
        let start = std::cmp::max(1, units.len() / 10);
        units = &units[start..];
    }
    let mut capped = JsString::from(format!(
        "[tool output truncated: max {} bytes / {} lines]\n",
        MAX_TOOL_RESULT_TEXT_BYTES, MAX_TOOL_RESULT_TEXT_LINES
    ));
    capped.push_str(JsString::from_utf16(units.to_vec()));
    capped
}

fn is_tool_result_text_too_large(text: &pi_js_compat::js_json::JsString) -> bool {
    text.units().split(|unit| *unit == u16::from(b'\n')).count() > MAX_TOOL_RESULT_TEXT_LINES
        || text.to_utf8_lossy().len() > MAX_TOOL_RESULT_TEXT_BYTES
}

async fn emit_tool_execution_start(
    tool_call: &ToolCall,
    preparation: &Preparation,
    started_at: f64,
    emit: &AgentEventSink,
) -> Result<(), AgentError> {
    emit_event(
        emit,
        AgentEvent::ToolExecutionStart {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            args: preparation.args(),
            started_at,
        },
    )
    .await
}

async fn emit_tool_execution_end(
    finalized: &FinalizedToolCallOutcome,
    started_at: f64,
    emit: &AgentEventSink,
) -> Result<(), AgentError> {
    emit_event(
        emit,
        AgentEvent::ToolExecutionEnd {
            tool_call_id: finalized.tool_call.id.clone(),
            tool_name: finalized.tool_call.name.clone(),
            result: finalized.result.clone(),
            is_error: finalized.is_error,
            started_at,
            finished_at: now_ms_f64(),
        },
    )
    .await
}

fn create_tool_result_message(finalized: &FinalizedToolCallOutcome) -> ToolResultMessage {
    ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: finalized.tool_call.id.clone(),
        tool_name: finalized.tool_call.name.clone(),
        content: finalized.result.content.clone(),
        details: finalized.result.details.clone(),
        usage: None,
        added_tool_names: None,
        is_error: finalized.is_error,
        timestamp: pi_ai::request::now_millis() as f64,
    }
}

async fn emit_tool_result_message(
    tool_result_message: &ToolResultMessage,
    emit: &AgentEventSink,
) -> Result<(), AgentError> {
    emit_event(
        emit,
        AgentEvent::MessageStart {
            message: tool_result_message.clone().into(),
        },
    )
    .await?;
    emit_event(
        emit,
        AgentEvent::MessageEnd {
            message: tool_result_message.clone().into(),
        },
    )
    .await
}

async fn emit_event(emit: &AgentEventSink, event: AgentEvent) -> Result<(), AgentError> {
    emit(event).await
}

fn is_aborted(signal: &Option<AbortSignal>) -> bool {
    signal.as_ref().is_some_and(AbortSignal::aborted)
}

fn throw_if_aborted(signal: &Option<AbortSignal>) -> Result<(), AgentError> {
    if is_aborted(signal) {
        Err(AgentError::run_aborted())
    } else {
        Ok(())
    }
}

async fn await_with_abort<T>(future: impl Future<Output = T>, signal: &Option<AbortSignal>) -> Result<T, AgentError> {
    let Some(signal) = signal else {
        return Ok(future.await);
    };
    if signal.aborted() {
        return Err(AgentError::run_aborted());
    }
    tokio::select! {
        biased;
        _ = signal.wait() => Err(AgentError::run_aborted()),
        value = future => Ok(value),
    }
}

fn now_ms_f64() -> f64 {
    pi_ai::request::now_millis() as f64
}

/// Event stream type produced by [`agent_loop`] and [`agent_loop_continue`].
pub type AgentEventStream = EventStream<AgentEvent, Vec<AgentMessage>>;
