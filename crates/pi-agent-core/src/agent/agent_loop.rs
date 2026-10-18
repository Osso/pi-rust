//! Port of `packages/agent/src/agent-loop.ts`.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, Message, StopReason, TextContent,
    TextOrImageContent, Tool, ToolCall, ToolResultMessage, UserMessage, UserRole,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, EventStream, StreamFailure, event_stream};
use pi_ai::utils::validation::validate_tool_arguments;
use serde_json::Value;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::agent::stream_fn::get_default_stream_fn;
use crate::agent::types::{
    AfterToolCallContext, AgentContext, AgentError, AgentEvent, AgentLoopConfig, AgentMessage, AgentTool,
    AgentToolResult, AgentToolUpdateCallback, BeforeToolCallContext, PrepareNextTurnContext,
    ShouldStopAfterTurnContext, StreamFn,
};

/// Event sink used by the low-level loop.
pub type AgentEventSink = Arc<dyn Fn(AgentEvent) -> pi_ai::request::BoxFuture<'static, ()> + Send + Sync>;

const END_TURN_CONTINUATION_INSTRUCTION: &str = "Your previous response was already delivered. Do not continue, repeat, or infer a new user request. Call `end_turn` now with a concise reason.";

const MAX_TOOL_RESULT_TEXT_BYTES: usize = 10 * 1024;
const MAX_TOOL_RESULT_TEXT_LINES: usize = 2000;

/// Starts an agent loop with a new prompt message, streaming events.
pub fn agent_loop(
    prompts: Vec<AgentMessage>,
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
    stream_fn: Option<StreamFn>,
) -> EventStream<AgentEvent, Vec<AgentMessage>> {
    let (sender, stream) = create_agent_stream();
    tokio::spawn(async move {
        let result = run_agent_loop(prompts, context, config, event_sink(sender.clone()), signal, stream_fn).await;
        match result {
            Ok(messages) => sender.end(Some(messages)),
            Err(error) => sender.fail(StreamFailure::new(error.to_string())),
        }
    });
    stream
}

/// Continues an agent loop from the current context without adding a prompt.
pub fn agent_loop_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
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
            Err(error) => sender.fail(StreamFailure::new(error.to_string())),
        }
    });
    Ok(stream)
}

fn event_sink(sender: pi_ai::utils::event_stream::EventSender<AgentEvent, Vec<AgentMessage>>) -> AgentEventSink {
    Arc::new(move |event| {
        let sender = sender.clone();
        Box::pin(async move {
            sender.push(event);
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
    signal: Option<CancellationToken>,
    stream_fn: Option<StreamFn>,
) -> Result<Vec<AgentMessage>, AgentError> {
    let new_messages = prompts.clone();
    let mut current_context = AgentContext {
        system_prompt: context.system_prompt,
        messages: context.messages,
        tools: context.tools,
    };
    current_context.messages.extend(prompts.iter().cloned());

    emit_event(&emit, AgentEvent::AgentStart).await;
    emit_event(&emit, AgentEvent::TurnStart).await;
    for prompt in &prompts {
        emit_event(
            &emit,
            AgentEvent::MessageStart {
                message: prompt.clone(),
            },
        )
        .await;
        emit_event(
            &emit,
            AgentEvent::MessageEnd {
                message: prompt.clone(),
            },
        )
        .await;
    }

    let stream_fn = resolve_stream_fn(stream_fn)?;
    run_loop(current_context, new_messages, config, signal, &emit, stream_fn).await
}

/// Continues from an existing context and returns only newly produced messages.
pub async fn run_agent_loop_continue(
    context: AgentContext,
    config: AgentLoopConfig,
    emit: AgentEventSink,
    signal: Option<CancellationToken>,
    stream_fn: Option<StreamFn>,
) -> Result<Vec<AgentMessage>, AgentError> {
    if context.messages.is_empty() {
        return Err(AgentError::cannot_continue_empty_context());
    }

    let mut new_messages: Vec<AgentMessage> = Vec::new();
    let mut current_context = context;
    let mut continuation_config = config;

    emit_event(&emit, AgentEvent::AgentStart).await;
    emit_event(&emit, AgentEvent::TurnStart).await;

    if let Some(pending_tool_message) = restored_pending_tool_message(&current_context.messages) {
        let batch = execute_tool_calls(
            &current_context,
            &pending_tool_message,
            &continuation_config,
            &signal,
            &emit,
        )
        .await?;
        for result in &batch.messages {
            current_context.messages.push(result.clone().into());
            new_messages.push(result.clone().into());
        }
        emit_event(
            &emit,
            AgentEvent::TurnEnd {
                message: pending_tool_message.clone().into(),
                tool_results: batch.messages.clone(),
            },
        )
        .await;
        throw_if_aborted(&signal)?;
        if batch.terminate {
            emit_event(
                &emit,
                AgentEvent::AgentEnd {
                    messages: new_messages.clone(),
                },
            )
            .await;
            return Ok(new_messages);
        }
        let next_turn_context = ShouldStopAfterTurnContext {
            message: pending_tool_message.clone(),
            tool_results: batch.messages.clone(),
            context: current_context.clone(),
            new_messages: new_messages.clone(),
        };
        if let Some(update) = prepare_next_turn(&continuation_config, next_turn_context.clone()).await? {
            current_context = update.context.clone().unwrap_or(current_context);
            apply_config_update(&mut continuation_config, &update);
        }
        if should_stop_after_turn(&continuation_config, next_turn_context).await? {
            emit_event(
                &emit,
                AgentEvent::AgentEnd {
                    messages: new_messages.clone(),
                },
            )
            .await;
            return Ok(new_messages);
        }
    }

    let stream_fn = resolve_stream_fn(stream_fn)?;
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

fn resolve_stream_fn(stream_fn: Option<StreamFn>) -> Result<StreamFn, AgentError> {
    match stream_fn {
        Some(stream_fn) => Ok(stream_fn),
        None => get_default_stream_fn(),
    }
}

fn restored_pending_tool_message(messages: &[AgentMessage]) -> Option<AssistantMessage> {
    let mut completed_tool_call_ids = std::collections::BTreeSet::new();
    for message in messages.iter().rev() {
        match message {
            AgentMessage::ToolResult(result) => {
                completed_tool_call_ids.insert(result.tool_call_id.clone());
                continue;
            }
            AgentMessage::Assistant(assistant) => {
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

async fn run_loop(
    initial_context: AgentContext,
    mut new_messages: Vec<AgentMessage>,
    initial_config: AgentLoopConfig,
    signal: Option<CancellationToken>,
    emit: &AgentEventSink,
    stream_fn: StreamFn,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut current_context = initial_context;
    let mut config = initial_config;
    let mut first_turn = true;
    let mut continuation_instruction_pending = false;
    let mut pending_messages = poll_steering(&config).await;

    loop {
        let mut has_more_tool_calls = true;

        while has_more_tool_calls || !pending_messages.is_empty() {
            throw_if_aborted(&signal)?;
            if first_turn {
                first_turn = false;
            } else {
                emit_event(emit, AgentEvent::TurnStart).await;
            }

            for message in pending_messages.drain(..) {
                emit_event(
                    emit,
                    AgentEvent::MessageStart {
                        message: message.clone(),
                    },
                )
                .await;
                emit_event(
                    emit,
                    AgentEvent::MessageEnd {
                        message: message.clone(),
                    },
                )
                .await;
                current_context.messages.push(message.clone());
                new_messages.push(message);
            }

            let runtime_continuation_message = if continuation_instruction_pending {
                Some(UserMessage {
                    role: UserRole::Tag,
                    content: pi_ai::types::UserContent::Text(END_TURN_CONTINUATION_INSTRUCTION.to_owned()),
                    input_source: None,
                    provider_native: None,
                    timestamp: pi_ai::request::now_millis(),
                })
            } else {
                None
            };
            continuation_instruction_pending = false;

            emit_event(emit, AgentEvent::ModelRequestStart).await;
            let streamed = stream_assistant_response(
                &mut current_context,
                &config,
                &signal,
                emit,
                &stream_fn,
                runtime_continuation_message,
            )
            .await;
            emit_event(emit, AgentEvent::ModelRequestEnd).await;
            let message = streamed?;
            new_messages.push(message.clone().into());

            if matches!(
                message.stop_reason,
                StopReason::Error | StopReason::Aborted | StopReason::Length
            ) {
                emit_event(
                    emit,
                    AgentEvent::TurnEnd {
                        message: message.into(),
                        tool_results: Vec::new(),
                    },
                )
                .await;
                emit_event(
                    emit,
                    AgentEvent::AgentEnd {
                        messages: new_messages.clone(),
                    },
                )
                .await;
                return Ok(new_messages);
            }

            let tool_calls = tool_calls_of(&message);
            let mut tool_results: Vec<ToolResultMessage> = Vec::new();
            has_more_tool_calls = false;
            if !tool_calls.is_empty() {
                let batch = execute_tool_calls(&current_context, &message, &config, &signal, emit).await?;
                tool_results.extend(batch.messages.iter().cloned());
                has_more_tool_calls = !batch.terminate;
                for result in &tool_results {
                    current_context.messages.push(result.clone().into());
                    new_messages.push(result.clone().into());
                }
            } else if has_end_turn_tool(&current_context) {
                has_more_tool_calls = true;
                continuation_instruction_pending = true;
            }

            emit_event(
                emit,
                AgentEvent::TurnEnd {
                    message: message.clone().into(),
                    tool_results: tool_results.clone(),
                },
            )
            .await;
            throw_if_aborted(&signal)?;

            let next_turn_context = ShouldStopAfterTurnContext {
                message: message.clone(),
                tool_results: tool_results.clone(),
                context: current_context.clone(),
                new_messages: new_messages.clone(),
            };
            if let Some(update) = prepare_next_turn(&config, next_turn_context.clone()).await? {
                current_context = update.context.clone().unwrap_or(current_context);
                apply_config_update(&mut config, &update);
            }
            if should_stop_after_turn(&config, next_turn_context).await? {
                emit_event(
                    emit,
                    AgentEvent::AgentEnd {
                        messages: new_messages.clone(),
                    },
                )
                .await;
                return Ok(new_messages);
            }

            pending_messages = poll_steering(&config).await;
        }

        let follow_up_messages = poll_follow_up(&config).await;
        if follow_up_messages.is_empty() {
            break;
        }
        pending_messages = follow_up_messages;
    }

    emit_event(
        emit,
        AgentEvent::AgentEnd {
            messages: new_messages.clone(),
        },
    )
    .await;
    Ok(new_messages)
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

async fn poll_steering(config: &AgentLoopConfig) -> Vec<AgentMessage> {
    match &config.get_steering_messages {
        Some(hook) => hook().await,
        None => Vec::new(),
    }
}

async fn poll_follow_up(config: &AgentLoopConfig) -> Vec<AgentMessage> {
    match &config.get_follow_up_messages {
        Some(hook) => hook().await,
        None => Vec::new(),
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
        Some(hook) => hook(context, None).await,
        None => Ok(false),
    }
}

async fn stream_assistant_response(
    context: &mut AgentContext,
    config: &AgentLoopConfig,
    signal: &Option<CancellationToken>,
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

    let mut response = await_with_abort(stream_fn(config.model.clone(), llm_context, options), signal).await?;

    let mut partial_message: Option<AssistantMessage> = None;
    let mut added_partial = false;

    loop {
        let next = await_with_abort(response.next(), signal).await?;
        let Some(event) = next else {
            break;
        };
        let event = event.map_err(|failure| AgentError::new(failure.message()))?;
        match &event {
            AssistantMessageEvent::Start { partial } => {
                partial_message = Some(partial.clone());
                context.messages.push(partial.clone().into());
                added_partial = true;
                emit_event(
                    emit,
                    AgentEvent::MessageStart {
                        message: partial.clone().into(),
                    },
                )
                .await;
            }
            AssistantMessageEvent::TextStart { partial, .. }
            | AssistantMessageEvent::TextDelta { partial, .. }
            | AssistantMessageEvent::TextEnd { partial, .. }
            | AssistantMessageEvent::ThinkingStart { partial, .. }
            | AssistantMessageEvent::ThinkingDelta { partial, .. }
            | AssistantMessageEvent::ThinkingEnd { partial, .. }
            | AssistantMessageEvent::ToolCallStart { partial, .. }
            | AssistantMessageEvent::ToolCallDelta { partial, .. }
            | AssistantMessageEvent::ToolCallEnd { partial, .. } => {
                if partial_message.is_some() {
                    partial_message = Some(partial.clone());
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
                    .await;
                }
            }
            AssistantMessageEvent::Done { .. } | AssistantMessageEvent::Error { .. } => {
                let provider_message = await_with_abort(response.result(), signal)
                    .await?
                    .map_err(|failure| AgentError::new(failure.message()))?;
                return finish_assistant_message(context, signal, emit, provider_message, added_partial).await;
            }
        }
    }

    let provider_message = await_with_abort(response.result(), signal)
        .await?
        .map_err(|failure| AgentError::new(failure.message()))?;
    finish_assistant_message(context, signal, emit, provider_message, added_partial).await
}

async fn finish_assistant_message(
    context: &mut AgentContext,
    signal: &Option<CancellationToken>,
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
        .await;
    }
    emit_event(
        emit,
        AgentEvent::MessageEnd {
            message: final_message.clone().into(),
        },
    )
    .await;
    Ok(final_message)
}

async fn prepare_assistant_tool_executions(
    message: AssistantMessage,
    tools: Option<&[Arc<dyn AgentTool>]>,
    signal: &Option<CancellationToken>,
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
        let Some(tool) = tools.iter().find(|tool| tool.name() == call.name) else {
            content.push(block.clone());
            continue;
        };
        let execution = await_with_abort(tool.prepare_execution(call.id.clone(), signal.clone()), signal).await?;
        match execution {
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
    llm_tool.constrained_sampling = tool.constrained_sampling();
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
    Pending(pi_ai::request::BoxFuture<'static, FinalizedToolCallOutcome>),
}

async fn execute_tool_calls(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    config: &AgentLoopConfig,
    signal: &Option<CancellationToken>,
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
    signal: &Option<CancellationToken>,
    emit: &AgentEventSink,
) -> Result<ExecutedToolCallBatch, AgentError> {
    let mut finalized_calls: Vec<FinalizedToolCallOutcome> = Vec::new();
    let mut messages: Vec<ToolResultMessage> = Vec::new();

    for tool_call in tool_calls {
        let preparation = prepare_tool_call(current_context, assistant_message, tool_call, config, signal).await;
        let started_at = now_ms_f64();
        emit_tool_execution_start(tool_call, &preparation, started_at, emit).await;
        let finalized = match preparation {
            Preparation::Immediate { result, is_error, .. } => FinalizedToolCallOutcome {
                tool_call: tool_call.clone(),
                result,
                is_error,
            },
            Preparation::Prepared(prepared) => {
                let executed = execute_prepared_tool_call(prepared.clone(), signal, emit, started_at).await;
                finalize_executed_tool_call(current_context, assistant_message, prepared, executed, config, signal)
                    .await
            }
        };
        let finalized = cap_finalized_tool_call_outcome(finalized);
        emit_tool_execution_end(&finalized, started_at, emit).await;
        let tool_result_message = create_tool_result_message(&finalized);
        emit_tool_result_message(&tool_result_message, emit).await;
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
    signal: &Option<CancellationToken>,
    emit: &AgentEventSink,
) -> Result<ExecutedToolCallBatch, AgentError> {
    let mut entries: Vec<FinalizedEntry> = Vec::new();

    for tool_call in tool_calls {
        let preparation = prepare_tool_call(current_context, assistant_message, tool_call, config, signal).await;
        let started_at = now_ms_f64();
        emit_tool_execution_start(tool_call, &preparation, started_at, emit).await;
        match preparation {
            Preparation::Immediate { result, is_error, .. } => {
                let finalized = cap_finalized_tool_call_outcome(FinalizedToolCallOutcome {
                    tool_call: tool_call.clone(),
                    result,
                    is_error,
                });
                emit_tool_execution_end(&finalized, started_at, emit).await;
                entries.push(FinalizedEntry::Done(Box::new(finalized)));
            }
            Preparation::Prepared(prepared) => {
                let current_context = current_context.clone();
                let assistant_message = assistant_message.clone();
                let config = config.clone();
                let signal = signal.clone();
                let emit = Arc::clone(emit);
                entries.push(FinalizedEntry::Pending(Box::pin(async move {
                    let executed = execute_prepared_tool_call(prepared.clone(), &signal, &emit, started_at).await;
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
                    emit_tool_execution_end(&finalized, started_at, &emit).await;
                    finalized
                })));
            }
        }
        if is_aborted(signal) {
            break;
        }
    }

    let ordered_finalized_calls = join_finalized_entries(entries).await?;
    let mut messages: Vec<ToolResultMessage> = Vec::new();
    for finalized in &ordered_finalized_calls {
        let tool_result_message = create_tool_result_message(finalized);
        emit_tool_result_message(&tool_result_message, emit).await;
        messages.push(tool_result_message);
    }

    Ok(ExecutedToolCallBatch {
        messages,
        terminate: should_terminate_tool_batch(&ordered_finalized_calls),
    })
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
        let (index, finalized) = joined.map_err(|error| AgentError::new(error.to_string()))?;
        slots[index] = Some(finalized);
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
    let prepared = tool.prepare_arguments(original.clone());
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

async fn prepare_tool_call(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    tool_call: &ToolCall,
    config: &AgentLoopConfig,
    signal: &Option<CancellationToken>,
) -> Preparation {
    let Some(tool) = current_context.find_tool(&tool_call.name) else {
        return Preparation::Immediate {
            result: create_error_tool_result(format!("Tool {} not found", tool_call.name)),
            is_error: true,
            args: Value::Object(tool_call.arguments.clone()),
        };
    };

    let prepared_tool_call = match prepare_tool_call_arguments(&tool, tool_call) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Preparation::Immediate {
                result: create_error_tool_result(error.to_string()),
                is_error: true,
                args: Value::Object(tool_call.arguments.clone()),
            };
        }
    };
    let mut validated_args = match validate_tool_arguments(&llm_tool(&tool), &prepared_tool_call) {
        Ok(args) => args,
        Err(error) => {
            return Preparation::Immediate {
                result: create_error_tool_result(error.to_string()),
                is_error: true,
                args: Value::Object(tool_call.arguments.clone()),
            };
        }
    };

    if let Some(before_tool_call) = &config.before_tool_call {
        let args_cell = Arc::new(Mutex::new(validated_args));
        let before_context = BeforeToolCallContext {
            assistant_message: assistant_message.clone(),
            tool_call: tool_call.clone(),
            args: Arc::clone(&args_cell),
            context: current_context.clone(),
        };
        let before_result = match before_tool_call(before_context, signal.clone()).await {
            Ok(result) => result,
            Err(error) => {
                return Preparation::Immediate {
                    result: create_error_tool_result(error.to_string()),
                    is_error: true,
                    args: Value::Object(tool_call.arguments.clone()),
                };
            }
        };
        validated_args = args_cell
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if is_aborted(signal) {
            return Preparation::Immediate {
                result: create_error_tool_result("Operation aborted"),
                is_error: true,
                args: validated_args,
            };
        }
        if before_result.as_ref().is_some_and(|result| result.block == Some(true)) {
            let result = before_result.expect("checked above");
            let mut tool_result =
                create_error_tool_result(result.reason.unwrap_or_else(|| "Tool execution was blocked".to_owned()));
            if result.terminate == Some(true) {
                tool_result.terminate = Some(true);
            }
            return Preparation::Immediate {
                result: tool_result,
                is_error: true,
                args: validated_args,
            };
        }
    }
    if is_aborted(signal) {
        return Preparation::Immediate {
            result: create_error_tool_result("Operation aborted"),
            is_error: true,
            args: validated_args,
        };
    }
    Preparation::Prepared(PreparedToolCall {
        tool_call: prepared_tool_call,
        tool,
        args: validated_args,
    })
}

async fn execute_prepared_tool_call(
    prepared: PreparedToolCall,
    signal: &Option<CancellationToken>,
    emit: &AgentEventSink,
    started_at: f64,
) -> ExecutedToolCallOutcome {
    let accepting_updates = Arc::new(AtomicBool::new(true));
    let update_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));

    let on_update: AgentToolUpdateCallback = {
        let accepting_updates = Arc::clone(&accepting_updates);
        let update_tasks = Arc::clone(&update_tasks);
        let emit = Arc::clone(emit);
        let tool_call_id = prepared.tool_call.id.clone();
        let tool_name = prepared.tool_call.name.clone();
        let args = prepared.args.clone();
        Arc::new(move |partial_result: AgentToolResult| {
            if !accepting_updates.load(Ordering::SeqCst) {
                return;
            }
            let event = AgentEvent::ToolExecutionUpdate {
                tool_call_id: tool_call_id.clone(),
                tool_name: tool_name.clone(),
                args: args.clone(),
                partial_result: cap_tool_result(partial_result),
            };
            let emit = Arc::clone(&emit);
            let task = tokio::spawn(async move {
                emit(event).await;
            });
            update_tasks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(task);
        })
    };

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
    let outcome = match await_with_abort(execution_future, signal).await {
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
    };
    accepting_updates.store(false, Ordering::SeqCst);
    let tasks: Vec<tokio::task::JoinHandle<()>> = update_tasks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .drain(..)
        .collect();
    for task in tasks {
        let _ = task.await;
    }
    outcome
}

async fn finalize_executed_tool_call(
    current_context: &AgentContext,
    assistant_message: &AssistantMessage,
    prepared: PreparedToolCall,
    executed: ExecutedToolCallOutcome,
    config: &AgentLoopConfig,
    signal: &Option<CancellationToken>,
) -> FinalizedToolCallOutcome {
    let mut result = executed.result;
    let mut is_error = executed.is_error;

    if let Some(after_tool_call) = &config.after_tool_call {
        let after_context = AfterToolCallContext {
            assistant_message: assistant_message.clone(),
            tool_call: prepared.tool_call.clone(),
            args: prepared.args.clone(),
            result: result.clone(),
            is_error,
            context: current_context.clone(),
        };
        match after_tool_call(after_context, signal.clone()).await {
            Ok(Some(after_result)) => {
                if let Some(content) = after_result.content {
                    result.content = content;
                }
                if let Some(details) = after_result.details {
                    result.details = Some(details);
                }
                if let Some(usage) = after_result.usage {
                    result.usage = Some(usage);
                }
                if let Some(terminate) = after_result.terminate {
                    result.terminate = Some(terminate);
                }
                if let Some(after_is_error) = after_result.is_error {
                    is_error = after_is_error;
                }
            }
            Ok(None) => {}
            Err(error) => {
                result = create_error_tool_result(error.to_string());
                is_error = true;
            }
        }
    }

    FinalizedToolCallOutcome {
        tool_call: prepared.tool_call,
        result,
        is_error,
    }
}

fn create_error_tool_result(message: impl Into<String>) -> AgentToolResult {
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

fn cap_tool_result_details(details: Option<Value>) -> Option<Value> {
    let details = details?;
    let text = serde_json::to_string(&details).ok()?;
    if !is_tool_result_text_too_large(&text) {
        return Some(details);
    }
    Some(serde_json::json!({
        "truncated": true,
        "reason": "tool result details exceeded storage cap",
        "maxBytes": MAX_TOOL_RESULT_TEXT_BYTES,
        "maxLines": MAX_TOOL_RESULT_TEXT_LINES,
        "preview": cap_tool_result_text(&text),
    }))
}

fn cap_tool_result_text(text: &str) -> String {
    if !is_tool_result_text_too_large(text) {
        return text.to_owned();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let line_capped = if lines.len() > MAX_TOOL_RESULT_TEXT_LINES {
        lines[lines.len() - MAX_TOOL_RESULT_TEXT_LINES..].join("\n")
    } else {
        text.to_owned()
    };
    let mut byte_capped: Vec<u16> = line_capped.encode_utf16().collect();
    while !byte_capped.is_empty() && String::from_utf16_lossy(&byte_capped).len() > MAX_TOOL_RESULT_TEXT_BYTES {
        let start = std::cmp::max(1, byte_capped.len() / 10);
        byte_capped = byte_capped[start..].to_vec();
    }
    format!(
        "[tool output truncated: max {} bytes / {} lines]\n{}",
        MAX_TOOL_RESULT_TEXT_BYTES,
        MAX_TOOL_RESULT_TEXT_LINES,
        String::from_utf16_lossy(&byte_capped)
    )
}

fn is_tool_result_text_too_large(text: &str) -> bool {
    text.split('\n').count() > MAX_TOOL_RESULT_TEXT_LINES || text.len() > MAX_TOOL_RESULT_TEXT_BYTES
}

async fn emit_tool_execution_start(
    tool_call: &ToolCall,
    preparation: &Preparation,
    started_at: f64,
    emit: &AgentEventSink,
) {
    emit_event(
        emit,
        AgentEvent::ToolExecutionStart {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            args: preparation.args(),
            started_at,
        },
    )
    .await;
}

async fn emit_tool_execution_end(finalized: &FinalizedToolCallOutcome, started_at: f64, emit: &AgentEventSink) {
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
    .await;
}

fn create_tool_result_message(finalized: &FinalizedToolCallOutcome) -> ToolResultMessage {
    ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: finalized.tool_call.id.clone(),
        tool_name: finalized.tool_call.name.clone(),
        content: finalized.result.content.clone(),
        details: finalized.result.details.clone(),
        usage: finalized.result.usage.clone(),
        added_tool_names: finalized.result.added_tool_names.clone(),
        is_error: finalized.is_error,
        timestamp: pi_ai::request::now_millis(),
    }
}

async fn emit_tool_result_message(tool_result_message: &ToolResultMessage, emit: &AgentEventSink) {
    emit_event(
        emit,
        AgentEvent::MessageStart {
            message: tool_result_message.clone().into(),
        },
    )
    .await;
    emit_event(
        emit,
        AgentEvent::MessageEnd {
            message: tool_result_message.clone().into(),
        },
    )
    .await;
}

async fn emit_event(emit: &AgentEventSink, event: AgentEvent) {
    emit(event).await;
}

fn is_aborted(signal: &Option<CancellationToken>) -> bool {
    signal.as_ref().is_some_and(CancellationToken::is_cancelled)
}

fn throw_if_aborted(signal: &Option<CancellationToken>) -> Result<(), AgentError> {
    if is_aborted(signal) {
        Err(AgentError::run_aborted())
    } else {
        Ok(())
    }
}

async fn await_with_abort<T>(
    future: impl Future<Output = T>,
    signal: &Option<CancellationToken>,
) -> Result<T, AgentError> {
    let Some(signal) = signal else {
        return Ok(future.await);
    };
    if signal.is_cancelled() {
        return Err(AgentError::run_aborted());
    }
    tokio::select! {
        biased;
        () = signal.cancelled() => Err(AgentError::run_aborted()),
        value = future => Ok(value),
    }
}

fn now_ms_f64() -> f64 {
    pi_ai::request::now_millis() as f64
}

/// Event stream type produced by [`agent_loop`] and [`agent_loop_continue`].
pub type AgentEventStream = EventStream<AgentEvent, Vec<AgentMessage>>;

/// Assistant stream alias used by scripted test stream functions.
pub type ScriptedAssistantStream = AssistantMessageEventStream;
