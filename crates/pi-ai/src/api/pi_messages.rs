//! Port of `packages/ai/src/api/pi-messages.ts`.
//!
//! Streams pi's own message protocol: one POST of `{ model, context, options }`
//! to `<baseUrl>/messages`, then an SSE stream of serialized assistant-message
//! events plus a terminal `done`/`error` event.

use pi_js_compat::js_json::JsValue;
use std::collections::BTreeMap;
use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::api::lazy::{LazyApi, LazyApiCapabilities, lazy_api};
use crate::request::{HttpRequest, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, DoneReason, ErrorReason, Model,
    ProviderEnv, ProviderResponse, StopReason, TextContent, ThinkingContent, ThinkingLevel, ToolCall, Usage,
};
use crate::utils::diagnostics::{
    AssistantMessageDiagnostic, DiagnosticCode, DiagnosticErrorInfo, ThrownValue, append_assistant_message_diagnostic,
    create_assistant_message_diagnostic,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::headers::{headers_to_record, provider_headers_to_record};
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::provider_env::get_provider_env_value;
use pi_js_compat::number::js_number_value;

const MAX_DIAGNOSTIC_BODY_CHARS: usize = 8192;

/// Provider-specific tool-choice selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PiMessagesToolChoice {
    Auto,
    None,
    Required,
    Function { name: String },
}

/// `PiMessagesOptions`.
#[derive(Clone, Default)]
pub struct PiMessagesOptions {
    pub stream: StreamOptions,
    pub reasoning: Option<ThinkingLevel>,
    pub tool_choice: Option<PiMessagesToolChoice>,
    /// Ask the backend for debug metadata (e.g. routing response headers).
    pub debug: bool,
}

impl PiMessagesOptions {
    pub fn from_stream_options(options: StreamOptions) -> Self {
        let tool_choice = options.extra.get("toolChoice").and_then(parse_tool_choice);
        let reasoning = options
            .extra
            .get("reasoning")
            .and_then(|value| serde_json::from_value(value.clone()).ok());
        let debug = options.extra.get("debug").and_then(Value::as_bool).unwrap_or(false);
        Self {
            stream: options,
            reasoning,
            tool_choice,
            debug,
        }
    }
}

fn parse_tool_choice(value: &Value) -> Option<PiMessagesToolChoice> {
    match value {
        Value::String(choice) => match choice.as_str() {
            "auto" => Some(PiMessagesToolChoice::Auto),
            "none" => Some(PiMessagesToolChoice::None),
            "required" => Some(PiMessagesToolChoice::Required),
            _ => None,
        },
        Value::Object(object) => {
            let name = object.get("function")?.get("name")?.as_str()?;
            Some(PiMessagesToolChoice::Function { name: name.to_owned() })
        }
        _ => None,
    }
}

fn tool_choice_value(choice: &PiMessagesToolChoice) -> Value {
    match choice {
        PiMessagesToolChoice::Auto => json!("auto"),
        PiMessagesToolChoice::None => json!("none"),
        PiMessagesToolChoice::Required => json!("required"),
        PiMessagesToolChoice::Function { name } => {
            json!({"type": "function", "function": {"name": name}})
        }
    }
}

/// Impact summary of a server-side message rewrite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiMessagesRewriteImpact {
    pub policy_id: String,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub policy_version: f64,
    pub changed: bool,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub token_count_change: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub message_count_change: f64,
    pub system_prompt_changed: bool,
}

/// Serialized assistant-message event as sent by a pi-messages backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PiMessagesEvent {
    Start,
    TextStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
    },
    TextDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
    },
    TextEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: String,
        #[serde(rename = "contentSignature", skip_serializing_if = "Option::is_none")]
        content_signature: Option<String>,
    },
    ThinkingStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
    },
    ThinkingDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
    },
    ThinkingEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: String,
        #[serde(rename = "contentSignature", skip_serializing_if = "Option::is_none")]
        content_signature: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        redacted: Option<bool>,
    },
    #[serde(rename = "toolcall_start")]
    ToolCallStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
    },
    #[serde(rename = "toolcall_delta")]
    ToolCallDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
    },
    #[serde(rename = "toolcall_end")]
    ToolCallEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        #[serde(rename = "toolCall")]
        tool_call: ToolCall,
    },
    Done {
        reason: DoneReason,
        usage: Usage,
        #[serde(rename = "responseId", skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rewrite: Option<PiMessagesRewriteImpact>,
    },
    Error {
        reason: ErrorReason,
        usage: Usage,
        #[serde(rename = "errorMessage", skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
        #[serde(rename = "responseId", skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rewrite: Option<PiMessagesRewriteImpact>,
    },
}

type PiMessagesErrorBody = Map<String, Value>;

/// `PiMessagesResponseError`.
#[derive(Debug, Clone, PartialEq)]
pub struct PiMessagesResponseError {
    pub message: String,
    pub code: Option<String>,
    pub diagnostic_details: Map<String, Value>,
}

impl std::fmt::Display for PiMessagesResponseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PiMessagesResponseError {}

/// pi-messages API implementation as a `ProviderStreams` module.
#[derive(Debug, Clone, Copy, Default)]
pub struct PiMessagesApi;

impl ProviderStreams for PiMessagesApi {
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        stream(model, context, PiMessagesOptions::from_stream_options(options))
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        stream_simple(model, context, options)
    }
}

/// Stream pi-messages events from a backend.
pub fn stream(model: Model, context: Context, options: PiMessagesOptions) -> AssistantMessageEventStream {
    let (sender, event_stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        run_stream(model, context, options, sender).await;
    });
    event_stream
}

async fn run_stream(model: Model, context: Context, options: PiMessagesOptions, sender: AssistantMessageEventSender) {
    match produce_stream(&model, &context, &options, &sender).await {
        Ok(()) => {}
        Err(error) => {
            let aborted = options
                .stream
                .request
                .signal
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled);
            let event = create_error_event(&model, &error, aborted);
            sender.push(event);
        }
    }
}

async fn produce_stream(
    model: &Model,
    context: &Context,
    options: &PiMessagesOptions,
    sender: &AssistantMessageEventSender,
) -> Result<(), PiMessagesError> {
    let api_key = require_api_key(model, options)?;
    let url = messages_url(model, options)?;
    let body = send_messages_request(model, context, options, api_key, &url).await?;
    consume_events(model, options, body, sender).await
}

fn require_api_key<'a>(model: &Model, options: &'a PiMessagesOptions) -> Result<&'a str, PiMessagesError> {
    options
        .stream
        .request
        .api_key
        .as_deref()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| PiMessagesError::Message(format!("No API key provided for provider \"{}\"", model.provider)))
}

async fn send_messages_request(
    model: &Model,
    context: &Context,
    options: &PiMessagesOptions,
    api_key: &str,
    url: &Url,
) -> Result<Box<dyn crate::request::HttpBody>, PiMessagesError> {
    let payload = resolve_payload(model, context, options).await?;
    let body = serde_json::to_vec(&payload).map_err(|error| PiMessagesError::Message(error.to_string()))?;
    let headers = build_headers(api_key, options);
    let fetch = options
        .stream
        .request
        .fetch
        .clone()
        .unwrap_or_else(crate::http::default_fetch);
    let response = fetch
        .fetch(
            HttpRequest {
                method: "POST".to_owned(),
                url: url.to_string(),
                headers: headers
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect(),
                body: Some(body),
            },
            options.stream.request.signal.clone(),
        )
        .await
        .map_err(|error| PiMessagesError::Message(error.message))?;

    if let Some(on_response) = &options.stream.request.on_response {
        on_response(
            ProviderResponse {
                status: f64::from(response.status),
                headers: headers_to_record(response.headers.clone()),
            },
            model.clone(),
        )
        .await
        .map_err(|error| PiMessagesError::Message(error.message))?;
    }

    if !(200..300).contains(&response.status) {
        let body = read_body_text(response.body).await?;
        return Err(PiMessagesError::Response(create_response_error(
            model,
            url,
            response.status,
            &response.status_text,
            &body,
        )));
    }
    Ok(response.body)
}

async fn resolve_payload(
    model: &Model,
    context: &Context,
    options: &PiMessagesOptions,
) -> Result<JsValue, PiMessagesError> {
    let payload = build_payload(model, context, options)?;
    let Some(on_payload) = &options.stream.request.on_payload else {
        return Ok(payload);
    };
    let next_payload = on_payload(payload.clone(), model.clone())
        .await
        .map_err(|error| PiMessagesError::Message(error.message))?;
    Ok(next_payload.unwrap_or(payload))
}

async fn consume_events(
    model: &Model,
    options: &PiMessagesOptions,
    mut body: Box<dyn crate::request::HttpBody>,
    sender: &AssistantMessageEventSender,
) -> Result<(), PiMessagesError> {
    let mut converter = EventConverter::new(model);
    let mut decoder = Utf8StreamDecoder::new();
    let mut buffer = String::new();
    let mut finished = false;
    while !finished {
        if user_aborted(options) {
            return Err(aborted_error());
        }
        let chunk = tokio::select! {
            () = cancelled(&options.stream.request.signal) => return Err(aborted_error()),
            chunk = body.next_chunk() => chunk,
        };
        match chunk {
            Some(Ok(bytes)) => buffer.push_str(&decoder.push(&bytes, false)),
            Some(Err(error)) => return Err(PiMessagesError::Message(error.message)),
            None => {
                buffer.push_str(&decoder.push(&[], true));
                finished = true;
            }
        }
        buffer = buffer.replace("\r\n", "\n");
        if dispatch_frames(&mut buffer, &mut converter, sender)? {
            return Ok(());
        }
    }

    if buffer.trim().is_empty() {
        return Err(no_terminal_error(model));
    }
    if dispatch_frame(&buffer, &mut converter, sender)? {
        return Ok(());
    }
    Err(no_terminal_error(model))
}

fn user_aborted(options: &PiMessagesOptions) -> bool {
    options
        .stream
        .request
        .signal
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
}

fn aborted_error() -> PiMessagesError {
    PiMessagesError::Message(crate::http::ABORT_ERROR_MESSAGE.to_owned())
}

fn no_terminal_error(model: &Model) -> PiMessagesError {
    PiMessagesError::Message(format!("{} stream ended without a terminal event", model.provider))
}

fn dispatch_frames(
    buffer: &mut String,
    converter: &mut EventConverter,
    sender: &AssistantMessageEventSender,
) -> Result<bool, PiMessagesError> {
    while let Some(index) = buffer.find("\n\n") {
        let raw = buffer[..index].to_owned();
        buffer.drain(..index + 2);
        if dispatch_frame(&raw, converter, sender)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn dispatch_frame(
    raw: &str,
    converter: &mut EventConverter,
    sender: &AssistantMessageEventSender,
) -> Result<bool, PiMessagesError> {
    let Some(pi_event) = parse_event(raw)? else {
        return Ok(false);
    };
    let event = converter.convert(pi_event)?;
    let terminal = matches!(
        event,
        AssistantMessageEvent::Done { .. } | AssistantMessageEvent::Error { .. }
    );
    sender.push(event);
    Ok(terminal)
}

/// Maps provider-agnostic `SimpleStreamOptions` to pi-messages options.
pub fn stream_simple(model: Model, context: Context, options: SimpleStreamOptions) -> AssistantMessageEventStream {
    let extra = PiMessagesOptions::from_stream_options(options.stream.clone());
    stream(
        model,
        context,
        PiMessagesOptions {
            stream: options.stream,
            reasoning: options.reasoning,
            tool_choice: extra.tool_choice,
            debug: extra.debug,
        },
    )
}

async fn cancelled(signal: &Option<CancellationToken>) {
    match signal {
        Some(signal) => signal.cancelled().await,
        None => std::future::pending().await,
    }
}

fn messages_url(model: &Model, options: &PiMessagesOptions) -> Result<Url, PiMessagesError> {
    let base = model.base_url.trim_end_matches('/');
    let mut url =
        Url::parse(&format!("{base}/messages")).map_err(|error| PiMessagesError::Message(error.to_string()))?;
    if options.debug {
        url.query_pairs_mut().append_pair("debug", "1");
    }
    Ok(url)
}

fn build_payload(model: &Model, context: &Context, options: &PiMessagesOptions) -> Result<JsValue, PiMessagesError> {
    use pi_js_compat::js_json::{JsValue, to_value};
    let context = to_value(context).map_err(|error| PiMessagesError::Message(error.to_string()))?;
    Ok(JsValue::object([
        ("model", JsValue::String(model.id.clone().into())),
        ("context", context),
        ("options", JsValue::from(Value::Object(build_payload_options(options)))),
    ]))
}

fn build_payload_options(options: &PiMessagesOptions) -> Map<String, Value> {
    let mut payload_options = Map::new();
    if let Some(temperature) = options.stream.temperature {
        payload_options.insert("temperature".to_owned(), js_number_value(temperature));
    }
    if let Some(max_tokens) = options.stream.max_tokens {
        payload_options.insert("maxTokens".to_owned(), js_number_value(max_tokens));
    }
    if let Some(reasoning) = options.reasoning {
        payload_options.insert(
            "reasoning".to_owned(),
            serde_json::to_value(reasoning).unwrap_or(Value::Null),
        );
    }
    if let Some(cache_retention) =
        resolve_cache_retention(options.stream.cache_retention, options.stream.request.env.as_ref())
    {
        payload_options.insert(
            "cacheRetention".to_owned(),
            serde_json::to_value(cache_retention).unwrap_or(Value::Null),
        );
    }
    if let Some(session_id) = &options.stream.session_id {
        payload_options.insert("sessionId".to_owned(), Value::String(session_id.clone()));
    }
    if let Some(tool_choice) = &options.tool_choice {
        payload_options.insert("toolChoice".to_owned(), tool_choice_value(tool_choice));
    }
    payload_options
}

fn resolve_cache_retention(
    cache_retention: Option<CacheRetention>,
    env: Option<&ProviderEnv>,
) -> Option<CacheRetention> {
    if cache_retention.is_some() {
        return cache_retention;
    }
    match get_provider_env_value("PI_CACHE_RETENTION", env).as_deref() {
        Some("long") => Some(CacheRetention::Long),
        _ => None,
    }
}

fn build_headers(api_key: &str, options: &PiMessagesOptions) -> IndexMap<String, String> {
    let mut headers = IndexMap::from([
        ("authorization".to_owned(), format!("Bearer {api_key}")),
        ("accept".to_owned(), "text/event-stream".to_owned()),
        ("content-type".to_owned(), "application/json".to_owned()),
    ]);
    if let Some(overrides) = provider_headers_to_record(options.stream.request.headers.as_ref()) {
        for (name, value) in overrides {
            headers.insert(name.to_ascii_lowercase(), value);
        }
    }
    headers
}

fn parse_error_body(body: &str) -> Option<PiMessagesErrorBody> {
    let parsed: Value = serde_json::from_str(body).ok()?;
    let object = parsed.as_object()?;
    let error = object.get("error")?;
    if error.is_object() { Some(object.clone()) } else { None }
}

fn truncate_diagnostic_string(value: &str) -> String {
    if value.chars().count() > MAX_DIAGNOSTIC_BODY_CHARS {
        format!("{}…", value.chars().take(MAX_DIAGNOSTIC_BODY_CHARS).collect::<String>())
    } else {
        value.to_owned()
    }
}

fn create_response_error(
    model: &Model,
    url: &Url,
    status: u16,
    status_text: &str,
    body: &str,
) -> PiMessagesResponseError {
    let error_body = parse_error_body(body);
    let error = error_body.as_ref().and_then(|body| body.get("error"));
    let message = error.and_then(|error| error.get("message")).and_then(Value::as_str);
    let code = error
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let suffix = message.unwrap_or(body);
    let code_suffix = code.as_ref().map(|code| format!(" ({code})")).unwrap_or_default();
    let message = format!("{status} {status_text}: {suffix}{code_suffix}");

    let mut details = Map::new();
    details.insert("version".to_owned(), json!(1));
    details.insert("provider".to_owned(), Value::String(model.provider.clone()));
    details.insert("model".to_owned(), Value::String(model.id.clone()));
    details.insert("url".to_owned(), Value::String(url.to_string()));
    details.insert("status".to_owned(), json!(status));
    details.insert("statusText".to_owned(), Value::String(status_text.to_owned()));
    if let Some(error) = error {
        details.insert("error".to_owned(), error.clone());
    }
    if error_body.is_none() {
        details.insert("body".to_owned(), Value::String(truncate_diagnostic_string(body)));
    }
    details.insert("timestampMs".to_owned(), json!(now_millis()));

    PiMessagesResponseError {
        message,
        code,
        diagnostic_details: details,
    }
}

#[derive(Debug, Clone)]
enum PiMessagesError {
    Response(PiMessagesResponseError),
    Message(String),
}

impl PiMessagesError {
    fn message(&self) -> String {
        match self {
            Self::Response(error) => error.message.clone(),
            Self::Message(message) => message.clone(),
        }
    }
}

fn create_error_event(model: &Model, error: &PiMessagesError, aborted: bool) -> AssistantMessageEvent {
    let reason = if aborted {
        ErrorReason::Aborted
    } else {
        ErrorReason::Error
    };
    let mut message = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis());
    message.stop_reason = if aborted {
        StopReason::Aborted
    } else {
        StopReason::Error
    };
    message.error_message = Some(error.message());

    if !aborted && let PiMessagesError::Response(response) = error {
        let diagnostic = create_assistant_message_diagnostic(
            "pi_messages_response_failure",
            &ThrownValue::Error(DiagnosticErrorInfo {
                name: Some("PiMessagesResponseError".to_owned()),
                message: response.message.clone(),
                stack: None,
                code: response.code.clone().map(DiagnosticCode::String),
            }),
            Some(response.diagnostic_details.clone()),
        );
        append_assistant_message_diagnostic(&mut message.diagnostics, diagnostic);
    }

    AssistantMessageEvent::Error { reason, error: message }
}

fn append_rewrite_diagnostic(message: &mut AssistantMessage, rewrite: Option<&PiMessagesRewriteImpact>) {
    let Some(rewrite) = rewrite else {
        return;
    };
    let details = serde_json::to_value(rewrite)
        .ok()
        .and_then(|value| value.as_object().cloned());
    append_assistant_message_diagnostic(
        &mut message.diagnostics,
        AssistantMessageDiagnostic {
            r#type: "pi_messages_rewrite".to_owned(),
            timestamp: now_millis(),
            error: None,
            details,
        },
    );
}

struct EventConverter {
    partial: AssistantMessage,
    tool_json: BTreeMap<usize, String>,
}

impl EventConverter {
    fn new(model: &Model) -> Self {
        Self {
            partial: AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis()),
            tool_json: BTreeMap::new(),
        }
    }

    fn convert(&mut self, event: PiMessagesEvent) -> Result<AssistantMessageEvent, PiMessagesError> {
        match event {
            PiMessagesEvent::Done {
                reason,
                usage,
                response_id,
                rewrite,
            } => {
                self.partial.stop_reason = stop_reason_from_done(reason);
                self.partial.usage = usage;
                self.partial.response_id = response_id;
                append_rewrite_diagnostic(&mut self.partial, rewrite.as_ref());
                Ok(AssistantMessageEvent::Done {
                    reason,
                    message: self.partial.clone(),
                })
            }
            PiMessagesEvent::Error {
                reason,
                usage,
                error_message,
                response_id,
                rewrite,
            } => {
                self.partial.stop_reason = match reason {
                    ErrorReason::Aborted => StopReason::Aborted,
                    ErrorReason::Error => StopReason::Error,
                };
                self.partial.usage = usage;
                self.partial.error_message = error_message;
                self.partial.response_id = response_id;
                append_rewrite_diagnostic(&mut self.partial, rewrite.as_ref());
                Ok(AssistantMessageEvent::Error {
                    reason,
                    error: self.partial.clone(),
                })
            }
            PiMessagesEvent::Start => Ok(AssistantMessageEvent::Start {
                partial: self.partial.clone(),
            }),
            PiMessagesEvent::TextStart { content_index } => {
                self.set_block(content_index, AssistantContent::Text(TextContent::new("")))?;
                Ok(AssistantMessageEvent::TextStart {
                    content_index,
                    partial: self.partial.clone(),
                })
            }
            PiMessagesEvent::TextDelta { content_index, delta } => {
                self.append_text(content_index, &delta)?;
                Ok(AssistantMessageEvent::TextDelta {
                    content_index,
                    delta: delta.into(),
                    partial: self.partial.clone(),
                })
            }
            PiMessagesEvent::TextEnd {
                content_index,
                content,
                content_signature,
            } => {
                self.finish_text(content_index, content.clone(), content_signature)?;
                Ok(AssistantMessageEvent::TextEnd {
                    content_index,
                    content: content.into(),
                    partial: self.partial.clone(),
                })
            }
            PiMessagesEvent::ThinkingStart { content_index } => {
                self.set_block(content_index, AssistantContent::Thinking(ThinkingContent::new("")))?;
                Ok(AssistantMessageEvent::ThinkingStart {
                    content_index,
                    partial: self.partial.clone(),
                })
            }
            PiMessagesEvent::ThinkingDelta { content_index, delta } => {
                self.append_thinking(content_index, &delta)?;
                Ok(AssistantMessageEvent::ThinkingDelta {
                    content_index,
                    delta: delta.into(),
                    partial: self.partial.clone(),
                })
            }
            PiMessagesEvent::ThinkingEnd {
                content_index,
                content,
                content_signature,
                redacted,
            } => {
                self.finish_thinking(content_index, content.clone(), content_signature, redacted)?;
                Ok(AssistantMessageEvent::ThinkingEnd {
                    content_index,
                    content: content.into(),
                    partial: self.partial.clone(),
                })
            }
            PiMessagesEvent::ToolCallStart {
                content_index,
                id,
                tool_name,
            } => {
                self.set_block(
                    content_index,
                    AssistantContent::ToolCall(ToolCall::new(id, tool_name, Map::new())),
                )?;
                self.tool_json.insert(content_index, String::new());
                Ok(AssistantMessageEvent::ToolCallStart {
                    content_index,
                    partial: self.partial.clone(),
                })
            }
            PiMessagesEvent::ToolCallDelta { content_index, delta } => {
                let json = format!(
                    "{}{}",
                    self.tool_json.get(&content_index).cloned().unwrap_or_default(),
                    delta
                );
                self.tool_json.insert(content_index, json.clone());
                let parsed = parse_streaming_json(Some(&json));
                self.set_tool_arguments(content_index, parsed)?;
                Ok(AssistantMessageEvent::ToolCallDelta {
                    content_index,
                    delta,
                    partial: self.partial.clone(),
                })
            }
            PiMessagesEvent::ToolCallEnd {
                content_index,
                tool_call,
            } => {
                self.finish_tool_call(content_index, tool_call.clone())?;
                self.tool_json.remove(&content_index);
                Ok(AssistantMessageEvent::ToolCallEnd {
                    content_index,
                    tool_call: Box::new(tool_call),
                    partial: self.partial.clone(),
                })
            }
        }
    }

    fn set_block(&mut self, index: usize, block: AssistantContent) -> Result<(), PiMessagesError> {
        if index > self.partial.content.len() {
            return Err(PiMessagesError::Message(format!(
                "pi-messages content index {index} is out of order"
            )));
        }
        if index == self.partial.content.len() {
            self.partial.content.push(block);
        } else {
            self.partial.content[index] = block;
        }
        Ok(())
    }

    fn append_text(&mut self, index: usize, delta: &str) -> Result<(), PiMessagesError> {
        match self.partial.content.get_mut(index) {
            Some(AssistantContent::Text(text)) => {
                text.text.push_str(delta);
                Ok(())
            }
            _ => Err(PiMessagesError::Message(format!(
                "pi-messages text delta without a text block at {index}"
            ))),
        }
    }

    fn append_thinking(&mut self, index: usize, delta: &str) -> Result<(), PiMessagesError> {
        match self.partial.content.get_mut(index) {
            Some(AssistantContent::Thinking(thinking)) => {
                thinking.thinking.push_str(delta);
                Ok(())
            }
            _ => Err(PiMessagesError::Message(format!(
                "pi-messages thinking delta without a thinking block at {index}"
            ))),
        }
    }

    fn finish_text(&mut self, index: usize, content: String, signature: Option<String>) -> Result<(), PiMessagesError> {
        match self.partial.content.get_mut(index) {
            Some(AssistantContent::Text(text)) => {
                text.text = content.into();
                text.text_signature = signature;
                Ok(())
            }
            _ => Err(PiMessagesError::Message(format!(
                "pi-messages text end without a text block at {index}"
            ))),
        }
    }

    fn finish_thinking(
        &mut self,
        index: usize,
        content: String,
        signature: Option<String>,
        redacted: Option<bool>,
    ) -> Result<(), PiMessagesError> {
        match self.partial.content.get_mut(index) {
            Some(AssistantContent::Thinking(thinking)) => {
                thinking.thinking = content.into();
                thinking.thinking_signature = signature;
                thinking.redacted = redacted;
                Ok(())
            }
            _ => Err(PiMessagesError::Message(format!(
                "pi-messages thinking end without a thinking block at {index}"
            ))),
        }
    }

    fn set_tool_arguments(&mut self, index: usize, parsed: Value) -> Result<(), PiMessagesError> {
        match self.partial.content.get_mut(index) {
            Some(AssistantContent::ToolCall(tool_call)) => {
                tool_call.arguments = arguments_object(parsed);
                Ok(())
            }
            _ => Err(PiMessagesError::Message(format!(
                "pi-messages tool delta without a tool block at {index}"
            ))),
        }
    }

    fn finish_tool_call(&mut self, index: usize, tool_call: ToolCall) -> Result<(), PiMessagesError> {
        match self.partial.content.get_mut(index) {
            Some(AssistantContent::ToolCall(block)) => {
                block.id = tool_call.id;
                block.name = tool_call.name;
                block.arguments = tool_call.arguments;
                Ok(())
            }
            _ => Err(PiMessagesError::Message(format!(
                "pi-messages tool end without a tool block at {index}"
            ))),
        }
    }
}

fn arguments_object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(object) => object,
        _ => Map::new(),
    }
}

fn stop_reason_from_done(reason: DoneReason) -> StopReason {
    match reason {
        DoneReason::Stop => StopReason::Stop,
        DoneReason::Length => StopReason::Length,
        DoneReason::ToolUse => StopReason::ToolUse,
        DoneReason::Deferred => StopReason::Deferred,
    }
}

fn parse_event(raw: &str) -> Result<Option<PiMessagesEvent>, PiMessagesError> {
    let data = raw
        .split('\n')
        .find(|line| line.starts_with("data:"))
        .map(|line| line[5..].trim());
    match data {
        Some(data) if !data.is_empty() && data != "[DONE]" => {
            let value =
                pi_js_compat::js_json::parse(data).map_err(|error| PiMessagesError::Message(error.to_string()))?;
            serde_json::from_value(value)
                .map(Some)
                .map_err(|error| PiMessagesError::Message(error.to_string()))
        }
        _ => Ok(None),
    }
}

async fn read_body_text(mut body: Box<dyn crate::request::HttpBody>) -> Result<String, PiMessagesError> {
    let mut decoder = Utf8StreamDecoder::new();
    let mut text = String::new();
    loop {
        match body.next_chunk().await {
            Some(Ok(bytes)) => text.push_str(&decoder.push(&bytes, false)),
            Some(Err(error)) => return Err(PiMessagesError::Message(error.message)),
            None => {
                text.push_str(&decoder.push(&[], true));
                return Ok(text);
            }
        }
    }
}

struct Utf8StreamDecoder {
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    fn new() -> Self {
        Self { pending: Vec::new() }
    }

    fn push(&mut self, bytes: &[u8], final_chunk: bool) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    output.push_str(text);
                    self.pending.clear();
                    break;
                }
                Err(error) => {
                    let valid_up_to = error.valid_up_to();
                    if valid_up_to > 0 {
                        output.push_str(
                            std::str::from_utf8(&self.pending[..valid_up_to]).expect("valid_up_to marks valid UTF-8"),
                        );
                        self.pending.drain(..valid_up_to);
                    }
                    match error.error_len() {
                        Some(length) => {
                            output.push('\u{fffd}');
                            self.pending.drain(..length.min(self.pending.len()));
                        }
                        None if final_chunk => {
                            output.push_str(&String::from_utf8_lossy(&self.pending));
                            self.pending.clear();
                            break;
                        }
                        None => break,
                    }
                }
            }
        }
        output
    }
}

/// `piMessagesApi()`: the lazy module wrapper from `pi-messages.lazy.ts`.
pub fn pi_messages_api() -> LazyApi {
    lazy_api(
        Arc::new(|| Box::pin(async { Ok(Arc::new(PiMessagesApi) as Arc<dyn ProviderStreams>) })),
        LazyApiCapabilities::default(),
    )
}
