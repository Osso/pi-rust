//! Port of `packages/agent-core/src/proxy.ts`.
//!
//! The TypeScript module uses the global `fetch`. Rust takes an explicit
//! [`Fetch`] implementation (defaulting to the shared reqwest client), which
//! keeps the proxy stream testable without stubbing globals.

use std::collections::HashMap;
use std::sync::Arc;

use pi_ai::http::default_fetch;
use pi_ai::request::{CallbackError, Fetch, HttpBody, HttpRequest, SimpleStreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, ErrorReason, Model, StopReason,
    TextContent, ThinkingContent, ToolCall, Usage,
};
use pi_ai::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use pi_ai::utils::json_parse::parse_streaming_json;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

/// Proxy event types: the server strips `partial` to reduce bandwidth.
#[derive(Debug, Clone)]
pub enum ProxyAssistantMessageEvent {
    Start,
    TextStart {
        content_index: usize,
    },
    TextDelta {
        content_index: usize,
        delta: JsString,
    },
    TextEnd {
        content_index: usize,
        content_signature: Option<String>,
    },
    ThinkingStart {
        content_index: usize,
    },
    ThinkingDelta {
        content_index: usize,
        delta: JsString,
    },
    ThinkingEnd {
        content_index: usize,
        content_signature: Option<String>,
    },
    ToolcallStart {
        content_index: usize,
        id: String,
        tool_name: String,
    },
    ToolcallDelta {
        content_index: usize,
        delta: JsString,
    },
    ToolcallEnd {
        content_index: usize,
    },
    Done {
        reason: DoneReason,
        usage: Usage,
    },
    Error {
        reason: ErrorReason,
        error_message: Option<JsString>,
        usage: Usage,
    },
    Unknown,
}

impl<'de> Deserialize<'de> for ProxyAssistantMessageEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        decode_proxy_event(&value).map_err(serde::de::Error::custom)
    }
}

fn proxy_field<T: serde::de::DeserializeOwned>(value: &JsValue, name: &'static str) -> Result<T, serde_json::Error> {
    let field = value
        .get(name)
        .ok_or_else(|| <serde_json::Error as serde::de::Error>::missing_field(name))?;
    pi_js_compat::js_json::from_value(field)
}

fn optional_proxy_field<T: serde::de::DeserializeOwned>(
    value: &JsValue,
    name: &'static str,
) -> Result<Option<T>, serde_json::Error> {
    value
        .get(name)
        .map(pi_js_compat::js_json::from_value)
        .transpose()
        .map(Option::flatten)
}

fn decode_proxy_event(value: &JsValue) -> Result<ProxyAssistantMessageEvent, serde_json::Error> {
    use ProxyAssistantMessageEvent as Event;
    let kind: JsString = proxy_field(value, "type")?;
    let index = || proxy_field(value, "contentIndex");
    let delta = || proxy_field(value, "delta");
    let signature = || optional_proxy_field(value, "contentSignature");
    Ok(match kind {
        kind if kind == "start" => Event::Start,
        kind if kind == "text_start" => Event::TextStart {
            content_index: index()?,
        },
        kind if kind == "text_delta" => Event::TextDelta {
            content_index: index()?,
            delta: delta()?,
        },
        kind if kind == "text_end" => Event::TextEnd {
            content_index: index()?,
            content_signature: signature()?,
        },
        kind if kind == "thinking_start" => Event::ThinkingStart {
            content_index: index()?,
        },
        kind if kind == "thinking_delta" => Event::ThinkingDelta {
            content_index: index()?,
            delta: delta()?,
        },
        kind if kind == "thinking_end" => Event::ThinkingEnd {
            content_index: index()?,
            content_signature: signature()?,
        },
        kind if kind == "toolcall_start" => Event::ToolcallStart {
            content_index: index()?,
            id: proxy_field(value, "id")?,
            tool_name: proxy_field(value, "toolName")?,
        },
        kind if kind == "toolcall_delta" => Event::ToolcallDelta {
            content_index: index()?,
            delta: delta()?,
        },
        kind if kind == "toolcall_end" => Event::ToolcallEnd {
            content_index: index()?,
        },
        kind if kind == "done" => Event::Done {
            reason: proxy_field(value, "reason")?,
            usage: proxy_field(value, "usage")?,
        },
        kind if kind == "error" => Event::Error {
            reason: proxy_field(value, "reason")?,
            error_message: optional_proxy_field(value, "errorMessage")?,
            usage: proxy_field(value, "usage")?,
        },
        _ => Event::Unknown,
    })
}

/// Options for [`stream_proxy`].
#[derive(Clone, Default)]
pub struct ProxyStreamOptions {
    /// Serializable stream options forwarded to the proxy server.
    pub stream_options: SimpleStreamOptions,
    /// Local abort signal for the proxy request.
    pub signal: Option<CancellationToken>,
    /// Auth token for the proxy server.
    pub auth_token: String,
    /// Proxy server URL (for example `https://genai.example.com`).
    pub proxy_url: String,
    /// HTTP implementation; defaults to the shared reqwest client.
    pub fetch: Option<Arc<dyn Fetch>>,
}

/// Streams a proxied request, reconstructing `partial` client-side.
pub fn stream_proxy(model: Model, context: Context, options: ProxyStreamOptions) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        run_proxy(sender, model, context, options).await;
    });
    stream
}

struct PartialState {
    message: AssistantMessage,
    content: Vec<Option<AssistantContent>>,
    partial_json: HashMap<usize, JsString>,
}

impl PartialState {
    fn new(model: &Model) -> Self {
        Self {
            message: AssistantMessage::empty(
                model.api.clone(),
                model.provider.clone(),
                model.id.clone(),
                pi_ai::request::now_millis(),
            ),
            content: Vec::new(),
            partial_json: HashMap::new(),
        }
    }

    fn sync(&mut self) {
        self.message.content = self.content.iter().flatten().cloned().collect();
    }
}

async fn run_proxy(sender: AssistantMessageEventSender, model: Model, context: Context, options: ProxyStreamOptions) {
    let mut state = PartialState::new(&model);
    match proxy_events(&sender, &model, &context, &options, &mut state).await {
        Ok(()) => sender.end(None),
        Err(message) => {
            let reason = if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                StopReason::Aborted
            } else {
                StopReason::Error
            };
            state.message.stop_reason = reason;
            state.message.error_message = Some(message);
            let error_reason = match reason {
                StopReason::Aborted => ErrorReason::Aborted,
                _ => ErrorReason::Error,
            };
            sender.push(AssistantMessageEvent::Error {
                reason: error_reason,
                error: state.message,
            });
            sender.end(None);
        }
    }
}

async fn proxy_events(
    sender: &AssistantMessageEventSender,
    model: &Model,
    context: &Context,
    options: &ProxyStreamOptions,
    state: &mut PartialState,
) -> Result<(), JsString> {
    let fetch = options.fetch.clone().unwrap_or_else(default_fetch);
    let body = serde_json::to_vec(&pi_js_compat::js_json!({
        "model": model,
        "context": context,
        "options": build_proxy_request_options(&options.stream_options),
    }))
    .map_err(|error| JsString::from(error.to_string()))?;

    let request = HttpRequest {
        method: "POST".to_owned(),
        url: format!("{}/api/stream", options.proxy_url),
        headers: vec![
            ("Authorization".to_owned(), format!("Bearer {}", options.auth_token)),
            ("Content-Type".to_owned(), "application/json".to_owned()),
        ],
        body: Some(body),
    };

    let mut response = fetch
        .fetch(request, options.signal.clone())
        .await
        .map_err(|error| error.message)?;

    if !(200..300).contains(&response.status) {
        let mut body = Vec::new();
        while let Some(chunk) = read_proxy_chunk(response.body.as_mut(), &options.signal).await {
            match chunk {
                Ok(chunk) => body.extend(chunk),
                Err(_) => {
                    // Frozen response.json() failures preserve the HTTP status,
                    // even if an earlier chunk contained a parseable error body.
                    body.clear();
                    break;
                }
            }
        }
        return Err(proxy_error_message(response.status, &response.status_text, &body));
    }

    let mut buffer: Vec<u8> = Vec::new();
    while let Some(chunk) = read_proxy_chunk(response.body.as_mut(), &options.signal).await {
        if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err("Request aborted by user".into());
        }
        buffer.extend(chunk.map_err(|error| error.message)?);
        drain_lines(&mut buffer, sender, state)?;
    }

    if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err("Request aborted by user".into());
    }
    Ok(())
}

async fn read_proxy_chunk(
    body: &mut dyn HttpBody,
    signal: &Option<CancellationToken>,
) -> Option<Result<Vec<u8>, CallbackError>> {
    let Some(signal) = signal else {
        return body.next_chunk().await;
    };
    tokio::select! {
        biased;
        () = signal.cancelled() => Some(Err(CallbackError::new("Request aborted by user"))),
        chunk = body.next_chunk() => chunk,
    }
}

fn proxy_error_message(status: u16, status_text: &str, body: &[u8]) -> JsString {
    let fallback = format!("Proxy error: {status} {status_text}");
    let parsed: Option<pi_js_compat::js_json::JsValue> = serde_json::from_slice(body).ok();
    parsed
        .as_ref()
        .and_then(|value| value.get("error"))
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .filter(|error| !error.is_empty())
        .map(|error| JsString::from("Proxy error: ") + error)
        .unwrap_or_else(|| fallback.into())
}

fn drain_lines(
    buffer: &mut Vec<u8>,
    sender: &AssistantMessageEventSender,
    state: &mut PartialState,
) -> Result<(), JsString> {
    while let Some(line_feed) = buffer.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = buffer.drain(..=line_feed).collect();
        let line = String::from_utf8_lossy(&line);
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        let proxy_event: ProxyAssistantMessageEvent =
            serde_json::from_str(data).map_err(|error| JsString::from(error.to_string()))?;
        if let Some(event) = process_proxy_event(proxy_event, state)? {
            sender.push(event);
        }
    }
    Ok(())
}

fn process_proxy_event(
    proxy_event: ProxyAssistantMessageEvent,
    state: &mut PartialState,
) -> Result<Option<AssistantMessageEvent>, String> {
    match proxy_event {
        ProxyAssistantMessageEvent::Start => Ok(Some(AssistantMessageEvent::Start {
            partial: state.message.clone(),
        })),
        ProxyAssistantMessageEvent::TextStart { content_index } => process_text_start(state, content_index).map(Some),
        ProxyAssistantMessageEvent::TextDelta { content_index, delta } => {
            process_text_delta(state, content_index, delta).map(Some)
        }
        ProxyAssistantMessageEvent::TextEnd {
            content_index,
            content_signature,
        } => process_text_end(state, content_index, content_signature).map(Some),
        ProxyAssistantMessageEvent::ThinkingStart { content_index } => {
            process_thinking_start(state, content_index).map(Some)
        }
        ProxyAssistantMessageEvent::ThinkingDelta { content_index, delta } => {
            process_thinking_delta(state, content_index, delta).map(Some)
        }
        ProxyAssistantMessageEvent::ThinkingEnd {
            content_index,
            content_signature,
        } => process_thinking_end(state, content_index, content_signature).map(Some),
        ProxyAssistantMessageEvent::ToolcallStart {
            content_index,
            id,
            tool_name,
        } => process_toolcall_start(state, content_index, id, tool_name).map(Some),
        ProxyAssistantMessageEvent::ToolcallDelta { content_index, delta } => {
            process_toolcall_delta(state, content_index, delta).map(Some)
        }
        ProxyAssistantMessageEvent::ToolcallEnd { content_index } => process_toolcall_end(state, content_index),
        ProxyAssistantMessageEvent::Done { reason, usage } => process_done(state, reason, usage).map(Some),
        ProxyAssistantMessageEvent::Error {
            reason,
            error_message,
            usage,
        } => process_error(state, reason, error_message, usage).map(Some),
        ProxyAssistantMessageEvent::Unknown => Ok(None),
    }
}

fn process_text_start(state: &mut PartialState, content_index: usize) -> Result<AssistantMessageEvent, String> {
    set_slot(
        &mut state.content,
        content_index,
        Some(AssistantContent::Text(TextContent::new(""))),
    )?;
    state.sync();
    Ok(AssistantMessageEvent::TextStart {
        content_index,
        partial: state.message.clone(),
    })
}

fn process_text_delta(
    state: &mut PartialState,
    content_index: usize,
    delta: JsString,
) -> Result<AssistantMessageEvent, String> {
    text_mut(&mut state.content, content_index, "text_delta")?
        .text
        .push_str(delta.clone());
    state.sync();
    Ok(AssistantMessageEvent::TextDelta {
        content_index,
        delta,
        partial: state.message.clone(),
    })
}

fn process_text_end(
    state: &mut PartialState,
    content_index: usize,
    content_signature: Option<String>,
) -> Result<AssistantMessageEvent, String> {
    let text = text_mut(&mut state.content, content_index, "text_end")?;
    text.text_signature = content_signature;
    let content = text.text.clone();
    state.sync();
    Ok(AssistantMessageEvent::TextEnd {
        content_index,
        content,
        partial: state.message.clone(),
    })
}

fn process_thinking_start(state: &mut PartialState, content_index: usize) -> Result<AssistantMessageEvent, String> {
    set_slot(
        &mut state.content,
        content_index,
        Some(AssistantContent::Thinking(ThinkingContent::new(""))),
    )?;
    state.sync();
    Ok(AssistantMessageEvent::ThinkingStart {
        content_index,
        partial: state.message.clone(),
    })
}

fn process_thinking_delta(
    state: &mut PartialState,
    content_index: usize,
    delta: JsString,
) -> Result<AssistantMessageEvent, String> {
    thinking_mut(&mut state.content, content_index, "thinking_delta")?
        .thinking
        .push_str(delta.clone());
    state.sync();
    Ok(AssistantMessageEvent::ThinkingDelta {
        content_index,
        delta,
        partial: state.message.clone(),
    })
}

fn process_thinking_end(
    state: &mut PartialState,
    content_index: usize,
    content_signature: Option<String>,
) -> Result<AssistantMessageEvent, String> {
    let thinking = thinking_mut(&mut state.content, content_index, "thinking_end")?;
    thinking.thinking_signature = content_signature;
    let content = thinking.thinking.clone();
    state.sync();
    Ok(AssistantMessageEvent::ThinkingEnd {
        content_index,
        content,
        partial: state.message.clone(),
    })
}

fn process_toolcall_start(
    state: &mut PartialState,
    content_index: usize,
    id: String,
    tool_name: String,
) -> Result<AssistantMessageEvent, String> {
    set_slot(
        &mut state.content,
        content_index,
        Some(AssistantContent::ToolCall(ToolCall::new(
            id,
            tool_name,
            JsObject::new(),
        ))),
    )?;
    state.partial_json.insert(content_index, JsString::default());
    state.sync();
    Ok(AssistantMessageEvent::ToolCallStart {
        content_index,
        partial: state.message.clone(),
    })
}

fn process_toolcall_delta(
    state: &mut PartialState,
    content_index: usize,
    delta: JsString,
) -> Result<AssistantMessageEvent, String> {
    let partial_json = state.partial_json.entry(content_index).or_default();
    partial_json.push_str(delta.clone());
    let arguments = streaming_arguments(partial_json);
    tool_call_mut(&mut state.content, content_index, "toolcall_delta")?.arguments = arguments;
    state.sync();
    Ok(AssistantMessageEvent::ToolCallDelta {
        content_index,
        delta,
        partial: state.message.clone(),
    })
}

fn process_toolcall_end(
    state: &mut PartialState,
    content_index: usize,
) -> Result<Option<AssistantMessageEvent>, String> {
    let Some(Some(AssistantContent::ToolCall(tool_call))) = state.content.get(content_index) else {
        return Ok(None);
    };
    let tool_call = tool_call.clone();
    state.partial_json.remove(&content_index);
    state.sync();
    Ok(Some(AssistantMessageEvent::ToolCallEnd {
        content_index,
        tool_call: Box::new(tool_call),
        partial: state.message.clone(),
    }))
}

fn process_done(state: &mut PartialState, reason: DoneReason, usage: Usage) -> Result<AssistantMessageEvent, String> {
    state.message.stop_reason = stop_reason_for_done(reason);
    state.message.usage = usage;
    Ok(AssistantMessageEvent::Done {
        reason,
        message: state.message.clone(),
    })
}

fn process_error(
    state: &mut PartialState,
    reason: ErrorReason,
    error_message: Option<JsString>,
    usage: Usage,
) -> Result<AssistantMessageEvent, String> {
    state.message.stop_reason = stop_reason_for_error(reason);
    state.message.error_message = error_message;
    state.message.usage = usage;
    Ok(AssistantMessageEvent::Error {
        reason,
        error: state.message.clone(),
    })
}

fn streaming_arguments(partial_json: &JsString) -> JsObject {
    match parse_streaming_json(Some(partial_json)) {
        JsValue::Object(object) => object,
        _ => JsObject::new(),
    }
}

fn set_slot(
    content: &mut Vec<Option<AssistantContent>>,
    index: usize,
    value: Option<AssistantContent>,
) -> Result<(), String> {
    if index > content.len() {
        return Err("Sparse proxy content index".to_owned());
    }
    if index == content.len() {
        content.push(value);
    } else {
        content[index] = value;
    }
    Ok(())
}

fn text_mut<'a>(
    content: &'a mut [Option<AssistantContent>],
    index: usize,
    event: &str,
) -> Result<&'a mut TextContent, String> {
    match content.get_mut(index) {
        Some(Some(AssistantContent::Text(text))) => Ok(text),
        _ => Err(format!("Received {event} for non-text content")),
    }
}

fn thinking_mut<'a>(
    content: &'a mut [Option<AssistantContent>],
    index: usize,
    event: &str,
) -> Result<&'a mut ThinkingContent, String> {
    match content.get_mut(index) {
        Some(Some(AssistantContent::Thinking(thinking))) => Ok(thinking),
        _ => Err(format!("Received {event} for non-thinking content")),
    }
}

fn tool_call_mut<'a>(
    content: &'a mut [Option<AssistantContent>],
    index: usize,
    event: &str,
) -> Result<&'a mut ToolCall, String> {
    match content.get_mut(index) {
        Some(Some(AssistantContent::ToolCall(tool_call))) => Ok(tool_call),
        _ => Err(format!("Received {event} for non-toolCall content")),
    }
}

fn stop_reason_for_done(reason: DoneReason) -> StopReason {
    match reason {
        DoneReason::Stop => StopReason::Stop,
        DoneReason::Length => StopReason::Length,
        DoneReason::ToolUse => StopReason::ToolUse,
        DoneReason::Deferred => StopReason::Deferred,
    }
}

fn stop_reason_for_error(reason: ErrorReason) -> StopReason {
    match reason {
        ErrorReason::Aborted => StopReason::Aborted,
        ErrorReason::Error => StopReason::Error,
    }
}

fn build_proxy_request_options(options: &SimpleStreamOptions) -> Value {
    let mut object = Map::new();
    let stream = &options.stream;
    insert_some(&mut object, "temperature", stream.temperature);
    insert_some(&mut object, "maxTokens", stream.max_tokens);
    insert_some(&mut object, "reasoning", options.reasoning);
    insert_some(&mut object, "cacheRetention", stream.cache_retention);
    insert_some(&mut object, "sessionId", stream.session_id.clone());
    insert_some(&mut object, "headers", stream.request.headers.clone());
    insert_some(&mut object, "metadata", stream.metadata.clone());
    insert_some(&mut object, "transport", stream.transport);
    insert_some(&mut object, "thinkingBudgets", options.thinking_budgets.clone());
    insert_some(&mut object, "maxRetryDelayMs", stream.request.max_retry_delay_ms);
    Value::Object(object)
}

fn insert_some<T: serde::Serialize>(object: &mut Map<String, Value>, key: &str, value: Option<T>) {
    if let Some(value) = value
        && let Ok(value) = serde_json::to_value(value)
    {
        object.insert(key.to_owned(), value);
    }
}
