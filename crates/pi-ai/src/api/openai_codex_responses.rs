//! Port of `packages/ai/src/api/openai-codex-responses.ts`.
//!
//! Codex-specific transport around the shared Responses conversion and state
//! machine: SSE with idle timeouts, WebSocket transports (including cached
//! continuation), retries, and the session WebSocket cache.

use crate::api::openai_sdk::ingress_metadata_text;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use indexmap::IndexMap;
use pi_js_compat::abort::{AbortController, AbortSignal};
use serde_json::{Map, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::api::lazy::{LazyApi, lazy_api};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_responses_shared::{
    ConvertResponsesMessagesOptions, ConvertResponsesToolsOptions, OpenAIResponsesStreamOptions, ResponsesEventSource,
    ResponsesStreamFailure, convert_responses_messages, convert_responses_tools, process_responses_stream,
};
use crate::api::simple_options::build_base_options;
use crate::http::default_fetch;
use crate::models::clamp_thinking_level;
use crate::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ProviderStreams, SimpleStreamOptions,
    StreamOptions, now_millis,
};
use crate::session_resources::register_session_resource_cleanup;
use crate::types::{
    AssistantMessage, AssistantMessageEvent, Context, DoneReason, ErrorReason, Message as ChatMessage, Model,
    ModelThinkingLevel, ProviderRetryEvent, StopReason, ThinkingLevel, Transport, Usage,
};
use crate::utils::diagnostics::{
    DiagnosticErrorInfo, ThrownValue, append_assistant_message_diagnostic, create_assistant_message_diagnostic,
    format_thrown_value,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::random_uuid::random_uuid;
use pi_js_compat::date::date_parse;
use pi_js_compat::number::{js_number_value, parse_number};

pub const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api";
const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";
const DEFAULT_MAX_RETRIES: f64 = 0.0;
const BASE_DELAY_MS: f64 = 1000.0;
const DEFAULT_MAX_RETRY_DELAY_MS: f64 = 60_000.0;
const DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS: f64 = 15_000.0;
const WEBSOCKET_PING_INTERVAL_MS: u64 = 30_000;
const REQUEST_COMPRESSION_ZSTD_LEVEL: i32 = 3;
const CODEX_TOOL_CALL_PROVIDERS: [&str; 5] = [
    "openai",
    "openai-codex",
    "openai-codex-gc",
    "openai-codex-team",
    "opencode",
];
const WEBSOCKET_MESSAGE_TOO_BIG_CLOSE_CODE: u16 = 1009;
const WEBSOCKET_CONNECTION_LIMIT_REACHED_CODE: &str = "websocket_connection_limit_reached";
const OPENAI_BETA_RESPONSES_WEBSOCKETS: &str = "responses_websockets=2026-02-06";
const SESSION_WEBSOCKET_CACHE_TTL_MS: u64 = 5 * 60 * 1000;
const SESSION_WEBSOCKET_MAX_AGE_MS: u64 = 55 * 60 * 1000;
const CODEX_RESPONSE_STATUSES: [&str; 6] = [
    "completed",
    "incomplete",
    "failed",
    "cancelled",
    "queued",
    "in_progress",
];

/// `OpenAICodexResponsesOptions`.
#[derive(Clone, Default)]
pub struct OpenAICodexResponsesOptions {
    pub stream: StreamOptions,
    pub reasoning_effort: Option<CodexReasoningEffort>,
    /// `None` (absent or explicit null) defaults to `"auto"` when reasoning is requested.
    pub reasoning_summary: Option<ReasoningSummary>,
    pub service_tier: Option<ServiceTier>,
    pub text_verbosity: Option<TextVerbosity>,
}

impl OpenAICodexResponsesOptions {
    /// Reads the API-specific camelCase fields from [`StreamOptions::extra`].
    pub fn from_stream_options(stream: StreamOptions) -> Result<Self, String> {
        let extra = &stream.extra;
        Ok(Self {
            reasoning_effort: parse_extra(extra, "reasoningEffort")?,
            reasoning_summary: parse_extra(extra, "reasoningSummary")?,
            service_tier: extra
                .get("serviceTier")
                .map(|value| {
                    serde_json::from_value(value.clone()).map_err(|error| format!("Invalid serviceTier: {error}"))
                })
                .transpose()?,
            text_verbosity: parse_extra(extra, "textVerbosity")?,
            stream,
        })
    }
}

fn parse_extra<T: serde::de::DeserializeOwned>(extra: &Map<String, Value>, key: &str) -> Result<Option<T>, String> {
    match extra.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| format!("Invalid {key}: {error}")),
    }
}

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $literal:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $(#[serde(rename = $literal)] $variant),+
        }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $literal),+
                }
            }
        }
    };
}

string_enum!(CodexReasoningEffort {
    None => "none",
    Minimal => "minimal",
    Low => "low",
    Medium => "medium",
    High => "high",
    Xhigh => "xhigh",
    Max => "max",
    Ultra => "ultra",
});

string_enum!(ReasoningSummary {
    Auto => "auto",
    Concise => "concise",
    Detailed => "detailed",
    Off => "off",
    On => "on",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceTier {
    Auto,
    Default,
    Flex,
    Scale,
    Priority,
    #[serde(untagged)]
    Null,
}

impl ServiceTier {
    pub const fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Auto => Some("auto"),
            Self::Default => Some("default"),
            Self::Flex => Some("flex"),
            Self::Scale => Some("scale"),
            Self::Priority => Some("priority"),
            Self::Null => None,
        }
    }
}

string_enum!(TextVerbosity {
    Low => "low",
    Medium => "medium",
    High => "high",
});

impl From<ThinkingLevel> for CodexReasoningEffort {
    fn from(level: ThinkingLevel) -> Self {
        match level {
            ThinkingLevel::Minimal => Self::Minimal,
            ThinkingLevel::Low => Self::Low,
            ThinkingLevel::Medium => Self::Medium,
            ThinkingLevel::High => Self::High,
            ThinkingLevel::Xhigh => Self::Xhigh,
            ThinkingLevel::Max => Self::Max,
            ThinkingLevel::Ultra => Self::Ultra,
        }
    }
}

impl From<ModelThinkingLevel> for CodexReasoningEffort {
    fn from(level: ModelThinkingLevel) -> Self {
        match level {
            ModelThinkingLevel::Off => Self::None,
            ModelThinkingLevel::Minimal => Self::Minimal,
            ModelThinkingLevel::Low => Self::Low,
            ModelThinkingLevel::Medium => Self::Medium,
            ModelThinkingLevel::High => Self::High,
            ModelThinkingLevel::Xhigh => Self::Xhigh,
            ModelThinkingLevel::Max => Self::Max,
            ModelThinkingLevel::Ultra => Self::Ultra,
        }
    }
}

/// `OpenAICodexWebSocketDebugStats`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAICodexWebSocketDebugStats {
    pub requests: u64,
    pub connections_created: u64,
    pub connections_reused: u64,
    pub cached_context_requests: u64,
    pub store_true_requests: u64,
    pub full_context_requests: u64,
    pub delta_requests: u64,
    pub last_input_items: JsValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_delta_input_items: Option<JsValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_previous_response_id: Option<JsValue>,
    pub websocket_failures: u64,
    #[serde(rename = "lastWebSocketError", skip_serializing_if = "Option::is_none")]
    pub last_websocket_error: Option<JsString>,
}

impl Default for OpenAICodexWebSocketDebugStats {
    fn default() -> Self {
        Self {
            requests: 0,
            connections_created: 0,
            connections_reused: 0,
            cached_context_requests: 0,
            store_true_requests: 0,
            full_context_requests: 0,
            delta_requests: 0,
            last_input_items: JsValue::Number(0.0),
            last_delta_input_items: None,
            last_previous_response_id: None,
            websocket_failures: 0,
            last_websocket_error: None,
        }
    }
}

/// `stream`.
pub fn stream(model: Model, context: Context, options: OpenAICodexResponsesOptions) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        run_stream(model, context, options, sender).await;
    });
    stream
}

/// `streamSimple`.
pub fn stream_simple(
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> Result<AssistantMessageEventStream, CallbackError> {
    let api_key = options
        .stream
        .request
        .api_key
        .clone()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| CallbackError::new(format!("No API key for provider: {}", model.provider)))?;

    let base = build_base_options(&model, &context, Some(&options), Some(&api_key));
    let reasoning_effort = options.reasoning.and_then(|reasoning| {
        let clamped = clamp_thinking_level(&model, model_thinking_level(reasoning));
        (clamped != ModelThinkingLevel::Off).then(|| CodexReasoningEffort::from(clamped))
    });

    Ok(stream(
        model,
        context,
        OpenAICodexResponsesOptions {
            stream: base,
            reasoning_effort,
            ..OpenAICodexResponsesOptions::default()
        },
    ))
}

/// `OpenAICodexResponsesApi`.
pub struct OpenAICodexResponsesApi;

impl ProviderStreams for OpenAICodexResponsesApi {
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        Ok(match OpenAICodexResponsesOptions::from_stream_options(options) {
            Ok(options) => stream(model, context, options),
            Err(message) => crate::request::error_stream(&model, &message),
        })
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        // Frozen direct export throws before returning; lazy.ts owns its catch.
        stream_simple(model, context, options)
    }
}

/// `openAICodexResponsesApi()`: the lazy module wrapper.
pub fn openai_codex_responses_api() -> LazyApi {
    lazy_api(Arc::new(|| {
        Box::pin(async { Ok(Arc::new(OpenAICodexResponsesApi) as Arc<dyn ProviderStreams>) })
    }))
}

async fn run_stream(
    model: Model,
    context: Context,
    options: OpenAICodexResponsesOptions,
    sender: AssistantMessageEventSender,
) {
    let mut output = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis() as f64);
    let result = execute_stream(&model, &context, &options, &mut output, &sender).await;
    match result {
        Ok(()) => match done_reason(output.stop_reason) {
            Ok(reason) => {
                sender.push(AssistantMessageEvent::Done {
                    reason,
                    message: output,
                });
            }
            Err(message) => fail_stream(&mut output, &options, &sender, &message),
        },
        Err(error) => fail_stream(&mut output, &options, &sender, error.message()),
    }
}

fn fail_stream(
    output: &mut AssistantMessage,
    options: &OpenAICodexResponsesOptions,
    sender: &AssistantMessageEventSender,
    message: impl Into<JsString>,
) {
    for block in &mut output.content {
        if let crate::types::AssistantContent::ToolCall(block) = block {
            block.partial_json = None;
        }
    }
    let aborted = is_aborted(options.stream.request.signal.as_ref());
    output.stop_reason = if aborted {
        StopReason::Aborted
    } else {
        StopReason::Error
    };
    output.error_message = Some(message.into());
    sender.push(AssistantMessageEvent::Error {
        reason: if aborted {
            ErrorReason::Aborted
        } else {
            ErrorReason::Error
        },
        error: output.clone(),
    });
    sender.end(Some(output.clone()));
}

fn done_reason(stop_reason: StopReason) -> Result<DoneReason, String> {
    match stop_reason {
        StopReason::Stop => Ok(DoneReason::Stop),
        StopReason::Length => Ok(DoneReason::Length),
        StopReason::ToolUse => Ok(DoneReason::ToolUse),
        StopReason::Error => Ok(DoneReason::Error),
        StopReason::Aborted => Err("Request was aborted".to_owned()),
    }
}

fn model_thinking_level(level: ThinkingLevel) -> ModelThinkingLevel {
    match level {
        ThinkingLevel::Minimal => ModelThinkingLevel::Minimal,
        ThinkingLevel::Low => ModelThinkingLevel::Low,
        ThinkingLevel::Medium => ModelThinkingLevel::Medium,
        ThinkingLevel::High => ModelThinkingLevel::High,
        ThinkingLevel::Xhigh => ModelThinkingLevel::Xhigh,
        ThinkingLevel::Max => ModelThinkingLevel::Max,
        ThinkingLevel::Ultra => ModelThinkingLevel::Ultra,
    }
}

async fn execute_stream(
    model: &Model,
    context: &Context,
    options: &OpenAICodexResponsesOptions,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
) -> Result<(), CodexStreamError> {
    let api_key = options
        .stream
        .request
        .api_key
        .as_ref()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| CodexStreamError::Transport(format!("No API key for provider: {}", model.provider).into()))?;
    let account_id = extract_account_id(api_key)?;
    let session_id = options.stream.session_id.as_ref();
    let cache_session_id = session_id.filter(|id| !id.is_empty());
    let prompt_cache_key = clamp_openai_prompt_cache_key(session_id);
    let body = build_request_body(model, context, options, prompt_cache_key.as_ref())?;
    let mut body = body;
    if let Some(on_payload) = &options.stream.request.on_payload
        && let Some(next_body) = on_payload(body.clone(), model.clone())
            .await
            .map_err(format_codex_callback_error)?
    {
        body = next_body;
    }
    let websocket_request_id = match cache_session_id {
        Some(id) => id.clone(),
        None => random_uuid()
            .map_err(|error| format_codex_callback_error(error.into()))?
            .into(),
    };
    let sse_headers = build_sse_headers(
        model.headers.as_ref(),
        options.stream.request.headers.as_ref(),
        &account_id,
        api_key,
        session_id,
    )?;
    let websocket_headers = build_websocket_headers(
        model.headers.as_ref(),
        options.stream.request.headers.as_ref(),
        &account_id,
        api_key,
        &websocket_request_id,
    )?;
    let account_id = js_json::to_js_string(&account_id).map_err(codex_conversion_error)?;
    let account_id = account_id.to_utf8().expect("validated account header is scalar");
    let body_json =
        serde_json::to_string(&body).map_err(|error| CodexStreamError::Transport(error.to_string().into()))?;
    let http_timeout_ms = normalize_timeout_ms(options.stream.request.timeout_ms)?;
    let websocket_connect_timeout_ms = normalize_timeout_ms(options.stream.websocket_connect_timeout_ms)?;
    let transport = options.stream.transport.unwrap_or(Transport::Auto);
    let mut start_emitted = false;

    if transport != Transport::Sse {
        let execution = WebSocketExecution {
            model,
            options,
            body,
            body_json: &body_json,
            headers: websocket_headers,
            http_timeout_ms,
            websocket_connect_timeout_ms,
            cache_session_id,
            account_id: &account_id,
        };
        return execute_websocket(&execution, output, sender, &mut start_emitted).await;
    }

    let compressed_body = compress_request_body_zstd(&body_json);
    let mut sse_headers = sse_headers;
    if compressed_body.is_some() {
        set_header(&mut sse_headers, "content-encoding", "zstd")?;
    }
    let sse_body = compressed_body.unwrap_or_else(|| body_json.into_bytes());

    let response = fetch_sse_with_retries(model, options, &sse_headers, sse_body, http_timeout_ms).await?;

    if !start_emitted {
        sender.push(AssistantMessageEvent::Start {
            partial: output.clone(),
        });
    }

    let response_headers = crate::utils::headers::headers_to_record(response.headers.iter().cloned());
    let http_request_id = response_headers
        .get("x-request-id")
        .map(|id| JsString::from(id.as_str()));
    let source = SseEventSource::new(response.body, http_timeout_ms, options.stream.request.signal.clone());
    let mut mapped = CodexMappedSource::new(source, http_request_id);
    let result = process_responses_stream(
        &mut mapped,
        output,
        sender,
        model,
        Some(&codex_stream_options(model, options)),
    )
    .await;
    drop(mapped);
    result.map_err(CodexStreamError::from_responses_failure)?;
    if is_aborted(options.stream.request.signal.as_ref()) {
        return Err(CodexStreamError::Transport("Request was aborted".into()));
    }
    Ok(())
}

fn codex_stream_options(model: &Model, options: &OpenAICodexResponsesOptions) -> OpenAIResponsesStreamOptions {
    let model_id = model.id.clone();
    OpenAIResponsesStreamOptions {
        service_tier: options.service_tier.and_then(ServiceTier::as_str).map(str::to_owned),
        resolve_service_tier: Some(Arc::new(|response_tier, request_tier| {
            resolve_codex_service_tier(response_tier, request_tier).map(str::to_owned)
        })),
        apply_service_tier_pricing: Some(Arc::new(move |usage: &mut Usage, service_tier: Option<&str>| {
            apply_service_tier_pricing(usage, service_tier, &model_id);
        })),
    }
}

fn build_request_body(
    model: &Model,
    context: &Context,
    options: &OpenAICodexResponsesOptions,
    prompt_cache_key: Option<&JsString>,
) -> Result<JsValue, CodexStreamError> {
    let messages = convert_responses_messages(
        model,
        context,
        &codex_tool_call_providers(),
        Some(&ConvertResponsesMessagesOptions {
            include_system_prompt: Some(false),
        }),
    )
    .map_err(CodexStreamError::Transport)?;

    let mut body = JsObject::new();
    body.insert("model".into(), JsValue::String(model.id.clone().into()));
    body.insert("store".into(), JsValue::Bool(false));
    body.insert("stream".into(), JsValue::Bool(true));
    body.insert(
        "instructions".into(),
        JsValue::String(
            context
                .system_prompt
                .clone()
                .filter(|prompt| !prompt.is_empty())
                .unwrap_or_else(|| "You are a helpful assistant.".into()),
        ),
    );
    body.insert("input".into(), JsValue::Array(messages));
    body.insert(
        "text".into(),
        object(vec![(
            "verbosity",
            JsValue::String(
                options
                    .text_verbosity
                    .map(TextVerbosity::as_str)
                    .unwrap_or("low")
                    .into(),
            ),
        )]),
    );
    body.insert(
        "include".into(),
        JsValue::Array(vec![JsValue::String("reasoning.encrypted_content".into())]),
    );
    if let Some(prompt_cache_key) = prompt_cache_key {
        body.insert("prompt_cache_key".into(), JsValue::String(prompt_cache_key.into()));
    }
    body.insert("tool_choice".into(), JsValue::String("auto".into()));
    body.insert("parallel_tool_calls".into(), JsValue::Bool(true));

    if let Some(temperature) = options.stream.temperature {
        body.insert("temperature".into(), js_number_value(temperature).into());
    }
    if let Some(service_tier) = options.service_tier {
        let value = match service_tier.as_str() {
            Some(tier) => JsValue::String(tier.into()),
            None => JsValue::Null,
        };
        body.insert("service_tier".into(), value);
    }
    if let Some(tools) = context.tools.as_deref().filter(|tools| !tools.is_empty()) {
        body.insert(
            "tools".into(),
            convert_responses_tools(tools, Some(&ConvertResponsesToolsOptions { strict: Some(None) })),
        );
    }
    if let Some(reasoning_effort) = options.reasoning_effort {
        let effort = reasoning_effort_value(model, reasoning_effort);
        if let Some(effort) = effort {
            body.insert(
                "reasoning".into(),
                object(vec![
                    ("effort", JsValue::String(effort)),
                    (
                        "summary",
                        JsValue::String(
                            options
                                .reasoning_summary
                                .map(ReasoningSummary::as_str)
                                .unwrap_or("auto")
                                .into(),
                        ),
                    ),
                ]),
            );
        }
    }

    Ok(JsValue::Object(body))
}

// Frozen536-537 applies ?? to off/selected values; the result stays native JSON text.
fn reasoning_effort_value(model: &Model, effort: CodexReasoningEffort) -> Option<JsString> {
    let key = match effort {
        CodexReasoningEffort::None => ModelThinkingLevel::Off,
        CodexReasoningEffort::Minimal => ModelThinkingLevel::Minimal,
        CodexReasoningEffort::Low => ModelThinkingLevel::Low,
        CodexReasoningEffort::Medium => ModelThinkingLevel::Medium,
        CodexReasoningEffort::High => ModelThinkingLevel::High,
        CodexReasoningEffort::Xhigh => ModelThinkingLevel::Xhigh,
        CodexReasoningEffort::Max => ModelThinkingLevel::Max,
        CodexReasoningEffort::Ultra => ModelThinkingLevel::Ultra,
    };
    match model.thinking_level_map.as_ref().and_then(|map| map.get(&key)) {
        Some(Some(mapped)) => Some(mapped.clone()),
        // TypeScript uses `??`, so an explicit `null` falls back to the logical effort.
        Some(None) | None => Some(effort.as_str().into()),
    }
}

fn get_service_tier_cost_multiplier(model_id: &str, service_tier: Option<&str>) -> f64 {
    match service_tier {
        Some("flex") => 0.5,
        Some("priority") => {
            if model_id == "gpt-5.5" {
                2.5
            } else {
                2.0
            }
        }
        _ => 1.0,
    }
}

fn apply_service_tier_pricing(usage: &mut Usage, service_tier: Option<&str>, model_id: &str) {
    let multiplier = get_service_tier_cost_multiplier(model_id, service_tier);
    if multiplier == 1.0 {
        return;
    }
    usage.cost.input *= multiplier;
    usage.cost.output *= multiplier;
    usage.cost.cache_read *= multiplier;
    usage.cost.cache_write *= multiplier;
    usage.cost.total = usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
}

fn resolve_codex_service_tier<'a>(
    response_service_tier: Option<&'a str>,
    request_service_tier: Option<&'a str>,
) -> Option<&'a str> {
    if response_service_tier == Some("default") && matches!(request_service_tier, Some("flex" | "priority")) {
        return request_service_tier;
    }
    response_service_tier.or(request_service_tier)
}

fn object(entries: Vec<(&str, JsValue)>) -> JsValue {
    let mut map = JsObject::new();
    for (key, value) in entries {
        map.insert(key.into(), value);
    }
    JsValue::Object(map)
}

fn codex_tool_call_providers() -> std::collections::BTreeSet<String> {
    CODEX_TOOL_CALL_PROVIDERS
        .iter()
        .map(|name| (*name).to_owned())
        .collect()
}

fn normalize_timeout_ms(value: Option<f64>) -> Result<Option<f64>, CodexStreamError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if !value.is_finite() || value < 0.0 {
        return Err(CodexStreamError::Transport(
            format!("Invalid timeoutMs: {}", value).into(),
        ));
    }
    Ok(Some(value.floor()))
}

fn compress_request_body_zstd(body_json: &str) -> Option<Vec<u8>> {
    zstd::stream::encode_all(body_json.as_bytes(), REQUEST_COMPRESSION_ZSTD_LEVEL).ok()
}

fn is_aborted(signal: Option<&AbortSignal>) -> bool {
    signal.is_some_and(AbortSignal::aborted)
}

type CodexHeaders = IndexMap<String, Vec<JsString>>;

fn convert_codex_header_pair(name: &str, value: &JsString) -> Result<(String, String), CodexStreamError> {
    let name = crate::request::header_value(&name.into())?;
    let value = crate::request::header_value(value)?;
    Ok((name, value))
}

fn normalize_codex_header_name(name: &str, method: &str) -> Result<String, CodexStreamError> {
    if HeaderName::from_bytes(name.as_bytes()).is_err() {
        return Err(CodexStreamError::Transport(
            format!("{method}: \"{name}\" is an invalid header name.").into(),
        ));
    }
    Ok(name.to_ascii_lowercase())
}

fn normalize_codex_header_value(value: &str, method: &str) -> Result<JsString, CodexStreamError> {
    let value = value.trim_matches([' ', '\t', '\r', '\n']);
    if value.contains(['\0', '\r', '\n']) {
        return Err(CodexStreamError::Transport(
            format!("{method}: \"{value}\" is an invalid header value.").into(),
        ));
    }
    Ok(value.into())
}

fn set_header(headers: &mut CodexHeaders, name: &str, value: impl Into<JsString>) -> Result<(), CodexStreamError> {
    let (name, value) = convert_codex_header_pair(name, &value.into())?;
    let name = normalize_codex_header_name(&name, "Headers.set")?;
    let value = normalize_codex_header_value(&value, "Headers.set")?;
    headers.insert(name, vec![value]);
    Ok(())
}

fn append_codex_header(headers: &mut CodexHeaders, name: String, value: String) -> Result<(), CodexStreamError> {
    let name = normalize_codex_header_name(&name, "Headers.append")?;
    let value = normalize_codex_header_value(&value, "Headers.append")?;
    let separate_values = name == "set-cookie";
    let values = headers.entry(name).or_default();
    if separate_values || values.is_empty() {
        values.push(value);
    } else {
        values[0] = JsString::join([values[0].clone(), value], ", ");
    }
    Ok(())
}

fn delete_header(headers: &mut CodexHeaders, name: &str) -> Result<(), CodexStreamError> {
    let name = crate::request::header_value(&name.into())?;
    let name = normalize_codex_header_name(&name, "Headers.delete")?;
    headers.shift_remove(&name);
    Ok(())
}

fn header_value<'a>(headers: &'a IndexMap<String, String>, name: &str) -> Option<&'a str> {
    headers.get(&name.to_ascii_lowercase()).map(String::as_str)
}

fn apply_provider_headers(
    headers: &mut CodexHeaders,
    additional: Option<&crate::types::ProviderHeaders>,
) -> Result<(), CodexStreamError> {
    let record: JsObject = additional
        .into_iter()
        .flatten()
        .map(|(name, value)| {
            (
                name.as_str().into(),
                value.clone().map(JsValue::String).unwrap_or(JsValue::Null),
            )
        })
        .collect();
    for (name, value) in js_json::object_entries_lossless(&record) {
        let name = name.to_utf8().expect("configured header names are scalar");
        match value.as_string() {
            None => delete_header(headers, &name)?,
            Some(value) => set_header(headers, &name, value.clone())?,
        }
    }
    Ok(())
}

fn build_initial_codex_headers(
    init_headers: Option<&IndexMap<String, JsString>>,
) -> Result<CodexHeaders, CodexStreamError> {
    let record: JsObject = init_headers
        .into_iter()
        .flatten()
        .map(|(name, value)| (name.as_str().into(), JsValue::String(value.clone())))
        .collect();
    // WebIDL converts the entire record before Headers.append validates fields.
    let initial = js_json::object_entries_lossless(&record)
        .into_iter()
        .map(|(name, value)| {
            let name = name.to_utf8().expect("configured header names are scalar");
            convert_codex_header_pair(&name, value.as_string().expect("initial header values are strings"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut headers = IndexMap::new();
    for (name, value) in initial {
        append_codex_header(&mut headers, name, value)?;
    }
    Ok(headers)
}

fn build_base_codex_headers(
    init_headers: Option<&IndexMap<String, JsString>>,
    additional_headers: Option<&crate::types::ProviderHeaders>,
    account_id: &JsValue,
    token: &JsString,
) -> Result<CodexHeaders, CodexStreamError> {
    let mut headers = build_initial_codex_headers(init_headers)?;
    apply_provider_headers(&mut headers, additional_headers)?;
    set_header(
        &mut headers,
        "authorization",
        pi_js_compat::js_format!("Bearer {}", token),
    )?;
    let account_id = js_json::to_js_string(account_id).map_err(codex_conversion_error)?;
    set_header(&mut headers, "chatgpt-account-id", account_id)?;
    set_header(&mut headers, "originator", "pi")?;
    set_header(&mut headers, "user-agent", codex_user_agent())?;
    Ok(headers)
}

fn codex_user_agent() -> String {
    format!("pi ({} {}; {})", platform_name(), platform_release(), platform_arch())
}

fn platform_name() -> &'static str {
    std::env::consts::OS
}

fn platform_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        other => other,
    }
}

fn platform_release() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|release| release.trim().to_owned())
        .unwrap_or_else(|_| std::env::consts::OS.to_owned())
}

fn build_sse_headers(
    init_headers: Option<&IndexMap<String, JsString>>,
    additional_headers: Option<&crate::types::ProviderHeaders>,
    account_id: &JsValue,
    token: &JsString,
    session_id: Option<&JsString>,
) -> Result<CodexHeaders, CodexStreamError> {
    let mut headers = build_base_codex_headers(init_headers, additional_headers, account_id, token)?;
    set_header(&mut headers, "openai-beta", "responses=experimental")?;
    set_header(&mut headers, "accept", "text/event-stream")?;
    set_header(&mut headers, "content-type", "application/json")?;
    if let Some(session_id) = session_id.filter(|id| !id.is_empty()) {
        set_header(&mut headers, "session-id", session_id)?;
        set_header(&mut headers, "x-client-request-id", session_id)?;
    }
    Ok(headers)
}

fn build_websocket_headers(
    init_headers: Option<&IndexMap<String, JsString>>,
    additional_headers: Option<&crate::types::ProviderHeaders>,
    account_id: &JsValue,
    token: &JsString,
    request_id: &JsString,
) -> Result<IndexMap<String, JsString>, CodexStreamError> {
    let mut headers = build_base_codex_headers(init_headers, additional_headers, account_id, token)?;
    delete_header(&mut headers, "accept")?;
    delete_header(&mut headers, "content-type")?;
    delete_header(&mut headers, "openai-beta")?;
    set_header(&mut headers, "openai-beta", OPENAI_BETA_RESPONSES_WEBSOCKETS)?;
    set_header(&mut headers, "x-client-request-id", request_id)?;
    set_header(&mut headers, "session-id", request_id)?;
    Ok(headers
        .into_iter()
        .map(|(name, values)| (name, values.last().expect("header has a value").clone()))
        .collect())
}

fn resolve_codex_url(base_url: &JsString) -> JsString {
    let mut normalized = if base_url.trim().is_empty() {
        DEFAULT_CODEX_BASE_URL.into()
    } else {
        base_url.clone()
    };
    while normalized.ends_with('/') {
        normalized = normalized.slice(0, normalized.len() - 1);
    }
    if normalized.ends_with("/codex/responses") {
        normalized
    } else if normalized.ends_with("/codex") {
        normalized + "/responses"
    } else {
        normalized + "/codex/responses"
    }
}

fn resolve_codex_websocket_url(base_url: &JsString) -> JsString {
    let url = resolve_codex_url(base_url);
    if let Some(rest) = url.strip_prefix("https://") {
        JsString::from("wss://") + rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        JsString::from("ws://") + rest
    } else {
        url
    }
}

fn extract_account_id(token: &JsString) -> Result<JsValue, CodexStreamError> {
    let failure = || CodexStreamError::Transport("Failed to extract accountId from token".to_owned().into());
    let parts: Vec<JsString> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(failure());
    }
    let payload = pi_js_compat::base64::atob(&parts[1]).map_err(|_| failure())?;
    // atob returns a Latin-1 JS string, not UTF-8-decoded JSON bytes.
    let payload: String = payload.into_iter().map(char::from).collect();
    let payload = js_json::parse_lossless(&payload).map_err(|_| failure())?;
    payload
        .get(JWT_CLAIM_PATH)
        .and_then(|claim| claim.get("chatgpt_account_id"))
        .filter(|account_id| js_json::is_truthy(account_id))
        .cloned()
        .ok_or_else(failure)
}

/// Failure raised while consuming Codex SSE/WebSocket events.
#[derive(Debug, Clone, PartialEq)]
pub enum CodexStreamError {
    Api {
        code: Option<String>,
        message: JsString,
        payload: Option<JsValue>,
    },
    Protocol {
        message: JsString,
        payload: Option<JsValue>,
    },
    Transport(JsString),
    Native(Box<CallbackError>),
}

impl CodexStreamError {
    fn from_responses_failure<E>(failure: ResponsesStreamFailure<E>) -> Self
    where
        E: Into<CodexStreamError>,
    {
        match failure {
            ResponsesStreamFailure::Source(error) => error.into(),
            ResponsesStreamFailure::Message(message) => Self::Transport(message),
        }
    }

    pub fn message(&self) -> &JsString {
        match self {
            Self::Api { message, .. } | Self::Protocol { message, .. } | Self::Transport(message) => message,
            Self::Native(error) => &error.message,
        }
    }

    fn is_non_transport(&self) -> bool {
        matches!(self, Self::Api { .. } | Self::Protocol { .. })
    }

    fn is_websocket_connection_limit_reached(&self) -> bool {
        matches!(
            self,
            Self::Api {
                code: Some(code),
                ..
            } if code == WEBSOCKET_CONNECTION_LIMIT_REACHED_CODE
        )
    }

    fn thrown_value(&self) -> ThrownValue {
        match self {
            Self::Api { code, message, .. } => ThrownValue::Error(DiagnosticErrorInfo {
                name: Some("CodexApiError".to_owned()),
                message: message.clone(),
                stack: None,
                code: code.clone().map(crate::utils::diagnostics::DiagnosticCode::String),
            }),
            Self::Protocol { message, .. } => ThrownValue::Error(DiagnosticErrorInfo {
                name: Some("CodexProtocolError".to_owned()),
                message: message.clone(),
                stack: None,
                code: None,
            }),
            Self::Native(error) => {
                let value = error.thrown_value().value();
                let code = value.get("code").and_then(|code| match code {
                    JsValue::Number(number) => Some(crate::utils::diagnostics::DiagnosticCode::Number(*number)),
                    JsValue::String(text) => Some(crate::utils::diagnostics::DiagnosticCode::String(
                        text.to_utf8().expect("native error code is scalar"),
                    )),
                    _ => None,
                });
                ThrownValue::Error(DiagnosticErrorInfo {
                    name: Some(error.name.to_utf8().expect("native error class is scalar")),
                    message: error.message.clone(),
                    stack: None,
                    code,
                })
            }
            Self::Transport(message) => ThrownValue::Error(DiagnosticErrorInfo {
                name: Some("Error".to_owned()),
                message: message.clone(),
                stack: None,
                code: None,
            }),
        }
    }
}

impl std::fmt::Display for CodexStreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(self.message()))
    }
}

impl std::error::Error for CodexStreamError {}

impl From<CallbackError> for CodexStreamError {
    fn from(error: CallbackError) -> Self {
        Self::Transport(error.message)
    }
}

fn is_terminal_rate_limit_error(error_text: &str) -> bool {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            regex::Regex::new(
                "(?i)GoUsageLimitError|FreeUsageLimitError|Monthly usage limit reached|available balance|insufficient_quota|out of budget|quota exceeded|billing",
            )
            .expect("terminal rate-limit pattern is valid")
        })
        .is_match(error_text)
}

fn is_retryable_error(status: u16, error_text: &str) -> bool {
    if status == 429 && is_terminal_rate_limit_error(error_text) {
        return false;
    }
    if matches!(status, 429 | 500 | 502 | 503 | 504) {
        return true;
    }
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            regex::Regex::new("(?i)rate.?limit|overloaded|service.?unavailable|upstream.?connect|connection.?refused")
                .expect("retryable error pattern is valid")
        })
        .is_match(error_text)
}

fn get_retry_after_delay_ms(headers: &IndexMap<String, String>) -> Option<f64> {
    if let Some(retry_after_ms) = header_value(headers, "retry-after-ms") {
        let millis = parse_number(retry_after_ms);
        if millis.is_finite() {
            return Some(millis.max(0.0));
        }
    }

    let retry_after = header_value(headers, "retry-after").filter(|value| !value.is_empty())?;
    let seconds = parse_number(retry_after);
    if seconds.is_finite() {
        return Some((seconds * 1000.0).max(0.0));
    }

    let date = date_parse(retry_after);
    if date.is_nan() {
        return None;
    }
    Some((date - now_millis() as f64).max(0.0))
}

fn cap_retry_delay_ms(delay_ms: f64, options: &OpenAICodexResponsesOptions) -> f64 {
    let maximum = options
        .stream
        .request
        .max_retry_delay_ms
        .unwrap_or(DEFAULT_MAX_RETRY_DELAY_MS);
    if maximum > 0.0 {
        pi_js_compat::number::js_math_min(delay_ms, maximum)
    } else {
        delay_ms
    }
}

async fn sleep_abortable(ms: f64, signal: Option<&AbortSignal>) -> Result<(), CallbackError> {
    if is_aborted(signal) {
        return Err(CallbackError::new("Request was aborted"));
    }
    let delay = tokio::time::sleep(codex_timer_duration(ms));
    match signal {
        Some(signal) => tokio::select! {
            () = delay => Ok(()),
            _ = signal.wait() => Err(CallbackError::new("Request was aborted")),
        },
        None => {
            delay.await;
            Ok(())
        }
    }
}

fn codex_timer_duration(milliseconds: f64) -> Duration {
    let normalized = pi_js_compat::node_timers::normalize_timeout_delay(milliseconds);
    Duration::from_millis(normalized.trunc() as u64)
}

fn format_codex_callback_error(error: CallbackError) -> CodexStreamError {
    use crate::utils::error_body::{
        ErrorBody, ProviderError, ProviderErrorInput, format_provider_error, normalize_provider_error,
    };
    let input = match error.thrown_value() {
        pi_js_compat::abort::AbortReason::Value(value) => ProviderErrorInput::Value(value),
        pi_js_compat::abort::AbortReason::Error(error) | pi_js_compat::abort::AbortReason::DomException(error) => {
            let value = error.value();
            ProviderErrorInput::Error(Box::new(ProviderError {
                message: error.message,
                status_code: value.get("statusCode").and_then(JsValue::as_f64),
                status: value.get("status").and_then(JsValue::as_f64),
                body: value.get("body").cloned().map(ErrorBody::Json),
                error: value.get("error").cloned(),
                metadata_http_status_code: value
                    .get("$metadata")
                    .and_then(|value| value.get("httpStatusCode"))
                    .and_then(JsValue::as_f64),
                response_status_code: value
                    .get("$response")
                    .and_then(|value| value.get("statusCode"))
                    .and_then(JsValue::as_f64),
                response_body: value
                    .get("$response")
                    .and_then(|value| value.get("body"))
                    .cloned()
                    .map(ErrorBody::Json),
            }))
        }
    };
    CodexStreamError::Transport(format_provider_error(&normalize_provider_error(&input), None))
}

fn retry_error_as_native_error(error: CallbackError) -> Result<CallbackError, CodexStreamError> {
    if matches!(error.thrown_value(), pi_js_compat::abort::AbortReason::Value(_)) {
        return error
            .error_or_string_message()
            .map(CallbackError::new)
            .map_err(format_codex_callback_error);
    }
    Ok(error)
}

async fn fetch_sse_attempt(
    model: &Model,
    options: &OpenAICodexResponsesOptions,
    headers: &CodexHeaders,
    body: &[u8],
    http_timeout_ms: Option<f64>,
    attempt: f64,
    max_retries: f64,
) -> Result<Option<HttpResponse>, CallbackError> {
    let request = HttpRequest {
        method: "POST".into(),
        url: crate::request::request_url(&resolve_codex_url(&model.base_url))?,
        headers: headers
            .iter()
            .flat_map(|(name, values)| {
                values
                    .iter()
                    .map(move |value| crate::request::header_value(value).map(|value| (name.clone(), value)))
            })
            .collect::<Result<_, _>>()?,
        body: Some(body.to_vec()),
    };
    let fetch = options.stream.request.fetch.clone().unwrap_or_else(default_fetch);
    let response = fetch_with_header_timeout(
        fetch.as_ref(),
        request,
        options.stream.request.signal.clone(),
        http_timeout_ms,
    )
    .await?;
    if let Some(on_response) = &options.stream.request.on_response {
        on_response(
            crate::types::ProviderResponse {
                status: f64::from(response.status),
                headers: crate::utils::headers::headers_to_record(response.headers.iter().cloned()),
            },
            model.clone(),
        )
        .await?;
    }
    if (200..300).contains(&response.status) {
        return Ok(Some(response));
    }
    let error_text = crate::request::read_body_text(response.body).await?;
    let header_map = crate::utils::headers::headers_to_record(response.headers.iter().cloned());
    if attempt < max_retries && is_retryable_error(response.status, &error_text) {
        let delay_ms = match get_retry_after_delay_ms(&header_map) {
            Some(delay) if response.status == 429 => cap_retry_delay_ms(delay, options),
            Some(delay) => delay,
            None => BASE_DELAY_MS * 2f64.powf(attempt),
        };
        notify_retry(
            options,
            attempt + 1.0,
            max_retries + 1.0,
            delay_ms,
            format!("HTTP {}", response.status),
            model,
        )?;
        sleep_abortable(delay_ms, options.stream.request.signal.as_ref()).await?;
        return Ok(None);
    }
    let info = parse_error_response(response.status, &response.status_text, &error_text);
    let header_id = header_map
        .get("x-request-id")
        .filter(|id| !id.is_empty())
        .map(|id| JsString::from(id.as_str()));
    let request_id = header_id.as_ref().or(info.request_id.as_ref());
    let message = info.friendly_message.map(JsValue::String).unwrap_or(info.message);
    let message = pi_js_compat::js_json::to_js_string(&message).map_err(codex_conversion_error)?;
    Err(CallbackError::new(format_codex_request_id(message, request_id)))
}

async fn fetch_sse_with_retries(
    model: &Model,
    options: &OpenAICodexResponsesOptions,
    headers: &CodexHeaders,
    body: Vec<u8>,
    http_timeout_ms: Option<f64>,
) -> Result<HttpResponse, CodexStreamError> {
    let max_retries = options.stream.request.max_retries.unwrap_or(DEFAULT_MAX_RETRIES);
    let mut attempt = 0.0;
    let mut last_error = None;
    while attempt <= max_retries {
        if is_aborted(options.stream.request.signal.as_ref()) {
            return Err(CodexStreamError::Transport("Request was aborted".into()));
        }
        match fetch_sse_attempt(model, options, headers, &body, http_timeout_ms, attempt, max_retries).await {
            Ok(Some(response)) => return Ok(response),
            Ok(None) => {}
            Err(error) => {
                if error.name == "AbortError" || error.message == "Request was aborted" {
                    return Err(CodexStreamError::Transport("Request was aborted".into()));
                }
                let error = retry_error_as_native_error(error)?;
                if attempt < max_retries && !error.message.contains("usage limit") {
                    let delay_ms = BASE_DELAY_MS * 2f64.powf(attempt);
                    notify_retry(
                        options,
                        attempt + 1.0,
                        max_retries + 1.0,
                        delay_ms,
                        error.message.clone(),
                        model,
                    )
                    .map_err(format_codex_callback_error)?;
                    sleep_abortable(delay_ms, options.stream.request.signal.as_ref())
                        .await
                        .map_err(format_codex_callback_error)?;
                    last_error = Some(error);
                } else {
                    return Err(format_codex_callback_error(error));
                }
            }
        }
        attempt += 1.0;
    }
    Err(last_error
        .map(format_codex_callback_error)
        .unwrap_or_else(|| CodexStreamError::Transport("Failed after retries".into())))
}

fn notify_retry(
    options: &OpenAICodexResponsesOptions,
    attempt: f64,
    max_attempts: f64,
    delay_ms: f64,
    reason: impl Into<JsString>,
    model: &Model,
) -> Result<(), CallbackError> {
    if let Some(on_retry) = &options.stream.on_retry {
        on_retry(
            ProviderRetryEvent {
                attempt,
                max_attempts: Some(max_attempts),
                delay_ms: Some(delay_ms),
                reason: reason.into(),
                fallback_transport: None,
            },
            model,
        )?;
    }
    Ok(())
}

async fn fetch_with_header_timeout(
    fetch: &dyn Fetch,
    request: HttpRequest,
    signal: Option<AbortSignal>,
    timeout_ms: Option<f64>,
) -> Result<HttpResponse, CallbackError> {
    let timeout_signal = timeout_ms
        .filter(|timeout| *timeout > 0.0)
        .map(AbortSignal::timeout)
        .transpose()
        .map_err(CallbackError::from)?;
    let combined = crate::utils::abort_signals::combine_abort_signals(&[signal.clone(), timeout_signal.clone()]);
    // Frozen awaits fetch itself; transports that ignore abort may still succeed.
    let result = fetch.fetch(request, combined.signal().cloned()).await;
    combined.cleanup();
    if result.is_err() && timeout_signal.as_ref().is_some_and(AbortSignal::aborted) && !is_aborted(signal.as_ref()) {
        return Err(CallbackError::new(format!(
            "Codex SSE response headers timed out after {}ms",
            pi_js_compat::number::js_number_to_string(timeout_ms.expect("timeout signal has duration"))
        )));
    }
    result
}

struct ParsedErrorResponse {
    message: JsValue,
    friendly_message: Option<JsString>,
    request_id: Option<JsString>,
}

fn trimmed_request_id(value: Option<&JsValue>) -> Option<JsString> {
    value
        .and_then(JsValue::as_string)
        .map(JsString::trim)
        .filter(|id| !id.is_empty())
}

fn extract_codex_request_id(event: &JsValue) -> Option<JsString> {
    if let Some(headers) = event.get("headers").and_then(JsValue::as_object) {
        let header = pi_js_compat::js_json::object_entries_lossless(headers)
            .into_iter()
            .find(|(name, _)| name.to_lowercase() == "x-request-id");
        if let Some(id) = trimmed_request_id(header.map(|(_, value)| value)) {
            return Some(id);
        }
    }
    let response = event.get("response");
    for value in [
        Some(event),
        event.get("error"),
        response,
        response.and_then(|response| response.get("error")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(id) = trimmed_request_id(value.get("request_id")) {
            return Some(id);
        }
    }
    None
}

fn format_codex_request_id(message: JsString, request_id: Option<&JsString>) -> JsString {
    match request_id.map(JsString::trim).filter(|id| !id.is_empty()) {
        Some(id) => pi_js_compat::js_format!("{}\nOpenAI request ID: {}", message, id),
        None => message,
    }
}

fn codex_conversion_error(message: &'static str) -> CallbackError {
    pi_js_compat::node_error::NativeNodeError::new("TypeError", message).into()
}

fn apply_http_error_fields(parsed: &JsValue, status: u16, info: &mut ParsedErrorResponse) -> Result<(), CallbackError> {
    use pi_js_compat::js_json::{is_truthy, to_js_number, to_js_string};
    info.request_id = extract_codex_request_id(parsed);
    let Some(error) = parsed.get("error").filter(|value| is_truthy(value)) else {
        return Ok(());
    };
    let empty_code = JsValue::String(JsString::default());
    let code = error
        .get("code")
        .filter(|value| is_truthy(value))
        .or_else(|| error.get("type").filter(|value| is_truthy(value)))
        .unwrap_or(&empty_code);
    let code = to_js_string(code).map_err(codex_conversion_error)?;
    let folded = crate::utils::error_body::fold_ascii_error_text(&code);
    let usage_limit = ["usage_limit_reached", "usage_not_included", "rate_limit_exceeded"]
        .iter()
        .any(|pattern| folded.contains(*pattern));
    if usage_limit || status == 429 {
        let plan = match error.get("plan_type").filter(|value| is_truthy(value)) {
            Some(JsValue::String(plan)) => pi_js_compat::js_format!(" ({} plan)", plan.to_lowercase()),
            Some(_) => return Err(codex_conversion_error("err.plan_type.toLowerCase is not a function")),
            None => JsString::default(),
        };
        let when = match error.get("resets_at").filter(|value| is_truthy(value)) {
            Some(reset) => {
                let reset = to_js_number(reset).map_err(codex_conversion_error)?;
                let minutes = pi_js_compat::number::math_round((reset * 1000.0 - now_millis() as f64) / 60_000.0);
                let minutes = pi_js_compat::number::js_math_max(0.0, minutes);
                JsString::from(format!(
                    " Try again in ~{} min.",
                    pi_js_compat::number::js_number_to_string(minutes)
                ))
            }
            None => JsString::default(),
        };
        info.friendly_message =
            Some(pi_js_compat::js_format!("You have hit your ChatGPT usage limit{}.{}", plan, when).trim());
    }
    if let Some(message) = error.get("message").filter(|value| is_truthy(value)) {
        info.message = message.clone();
    } else if let Some(friendly) = &info.friendly_message {
        info.message = JsValue::String(friendly.clone());
    }
    Ok(())
}

fn parse_error_response(status: u16, status_text: &str, raw: &str) -> ParsedErrorResponse {
    let message = if !raw.is_empty() {
        raw
    } else if !status_text.is_empty() {
        status_text
    } else {
        "Request failed"
    };
    let mut info = ParsedErrorResponse {
        message: JsValue::String(message.into()),
        friendly_message: None,
        request_id: None,
    };
    if let Ok(parsed) = pi_js_compat::js_json::parse_lossless(raw) {
        // Frozen catches JSON/coercion failures here, retaining fields already
        // assigned (notably request_id), rather than validating a schema first.
        if apply_http_error_fields(&parsed, status, &mut info).is_err() {
            return info;
        }
    }
    info
}

enum SseStep {
    Event(JsValue),
    Skip,
    Incomplete,
}

struct SseEventSource {
    body: Box<dyn HttpBody>,
    buffer: String,
    decoder: pi_js_compat::text_decoder::Utf8StreamDecoder,
    timeout_ms: Option<f64>,
    signal: Option<AbortSignal>,
    eof: bool,
}

impl SseEventSource {
    fn new(body: Box<dyn HttpBody>, timeout_ms: Option<f64>, signal: Option<AbortSignal>) -> Self {
        Self {
            body,
            buffer: String::new(),
            decoder: pi_js_compat::text_decoder::Utf8StreamDecoder::new(),
            timeout_ms,
            signal,
            eof: false,
        }
    }

    fn take_event(&mut self) -> Result<SseStep, CodexStreamError> {
        let Some(index) = self.buffer.find("\n\n") else {
            return Ok(SseStep::Incomplete);
        };
        let remaining = self.buffer.split_off(index + 2);
        let mut chunk = std::mem::replace(&mut self.buffer, remaining);
        chunk.truncate(index);
        let data = chunk
            .split('\n')
            .filter(|line| line.starts_with("data:"))
            .map(|line| pi_js_compat::string::js_trim(&line[5..]))
            .collect::<Vec<_>>()
            .join("\n");
        let data = pi_js_compat::string::js_trim(&data);
        if data.is_empty() || data == "[DONE]" {
            return Ok(SseStep::Skip);
        }
        pi_js_compat::js_json::parse_lossless(data)
            .map(SseStep::Event)
            .map_err(|error| CodexStreamError::Protocol {
                message: JsString::join(["Invalid Codex SSE JSON: ".into(), error.into_message()], ""),
                payload: Some(JsValue::String(data.into())),
            })
    }

    async fn read_chunk(&mut self) -> Result<Option<Vec<u8>>, CodexStreamError> {
        let signal = self.signal.clone();
        let body = self.body.as_mut();
        let read = body.next_chunk();
        tokio::pin!(read);
        let abort = async {
            match &signal {
                Some(signal) => signal.wait().await,
                None => std::future::pending().await,
            }
        };

        let result = match self.timeout_ms.filter(|timeout| *timeout > 0.0) {
            Some(timeout_ms) => {
                tokio::select! {
                    chunk = &mut read => chunk,
                    () = tokio::time::sleep(codex_timer_duration(timeout_ms)) => {
                        return Err(CodexStreamError::Transport(format!(
                            "Codex SSE body read timed out after {timeout_ms}ms"
                        ).into()));
                    }
                    _ = abort => {
                        return Err(CodexStreamError::Transport("Request was aborted".to_owned().into()));
                    }
                }
            }
            None => {
                tokio::select! {
                    chunk = &mut read => chunk,
                    _ = abort => {
                        return Err(CodexStreamError::Transport("Request was aborted".to_owned().into()));
                    }
                }
            }
        };

        match result {
            Some(Ok(bytes)) => Ok(Some(bytes)),
            Some(Err(error)) => {
                if is_aborted(self.signal.as_ref()) {
                    Err(CodexStreamError::Transport("Request was aborted".to_owned().into()))
                } else {
                    Err(CodexStreamError::Transport(error.message))
                }
            }
            None => Ok(None),
        }
    }
}

impl ResponsesEventSource for SseEventSource {
    type Error = CodexStreamError;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<JsValue>, Self::Error>> {
        Box::pin(async move {
            loop {
                if is_aborted(self.signal.as_ref()) {
                    return Err(CodexStreamError::Transport("Request was aborted".to_owned().into()));
                }
                match self.take_event()? {
                    SseStep::Event(event) => return Ok(Some(event)),
                    SseStep::Skip => continue,
                    SseStep::Incomplete => {}
                }
                if self.eof {
                    return Ok(None);
                }
                match self.read_chunk().await? {
                    Some(bytes) => self.buffer.push_str(&self.decoder.decode(&bytes)),
                    None => self.eof = true,
                }
            }
        })
    }
}

struct CodexMappedSource<S> {
    inner: S,
    finished: bool,
    http_request_id: Option<JsString>,
}

impl<S> CodexMappedSource<S> {
    fn new(inner: S, http_request_id: Option<JsString>) -> Self {
        Self {
            inner,
            finished: false,
            http_request_id,
        }
    }
}

impl<S> ResponsesEventSource for CodexMappedSource<S>
where
    S: ResponsesEventSource<Error = CodexStreamError> + Send,
{
    type Error = CodexStreamError;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<JsValue>, Self::Error>> {
        Box::pin(async move {
            if self.finished {
                return Ok(None);
            }
            loop {
                let Some(event) = self.inner.next_event().await? else {
                    return Ok(None);
                };
                match map_codex_event(event, self.http_request_id.as_ref())? {
                    Some(event) => {
                        if matches!(
                            event.get("type").and_then(ingress_metadata_text).as_deref(),
                            Some("response.completed" | "response.incomplete")
                        ) {
                            self.finished = true;
                        }
                        return Ok(Some(event));
                    }
                    None => continue,
                }
            }
        })
    }
}

fn map_codex_event(
    mut event: JsValue,
    http_request_id: Option<&JsString>,
) -> Result<Option<JsValue>, CodexStreamError> {
    if matches!(event, JsValue::Null) {
        return Err(CodexStreamError::Transport(
            "Cannot read properties of null (reading 'type')".into(),
        ));
    }
    crate::api::openai_responses_shared::validate_responses_metadata(&event)
        .map_err(|error| CodexStreamError::Transport(error.into()))?;
    let Some(event_type) = event
        .get("type")
        .and_then(ingress_metadata_text)
        .as_deref()
        .map(str::to_owned)
    else {
        return Ok(None);
    };

    match event_type.as_str() {
        "error" => {
            let (code, message) = extract_codex_event_error(&event);
            let detail = message
                .filter(|message| !message.is_empty())
                .or_else(|| {
                    code.as_ref()
                        .filter(|code| !code.is_empty())
                        .map(|code| JsString::from(code.as_str()))
                })
                .unwrap_or_else(|| pi_js_compat::js_json::stringify_lossless(&event).into());
            let message = pi_js_compat::js_format!("Codex error: {}", detail);
            let request_id = http_request_id
                .filter(|id| !id.is_empty())
                .cloned()
                .or_else(|| extract_codex_request_id(&event));
            Err(CodexStreamError::Api {
                message: format_codex_request_id(message, request_id.as_ref()),
                code,
                payload: Some(event),
            })
        }
        "response.failed" => {
            let response = event.get("response").cloned().unwrap_or(JsValue::Null);
            let code = response
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(ingress_metadata_text)
                .as_deref()
                .map(str::to_owned);
            let default_message = JsValue::String("Codex response failed".into());
            let message = response
                .get("error")
                .and_then(|error| error.get("message"))
                .filter(|value| pi_js_compat::js_json::is_truthy(value))
                .unwrap_or(&default_message);
            let message = pi_js_compat::js_json::to_js_string(message)
                .map_err(|message| CodexStreamError::Transport(message.into()))?;
            let request_id = http_request_id
                .filter(|id| !id.is_empty())
                .cloned()
                .or_else(|| extract_codex_request_id(&event));
            Err(CodexStreamError::Api {
                message: format_codex_request_id(message, request_id.as_ref()),
                code,
                payload: Some(event),
            })
        }
        "response.done" | "response.completed" | "response.incomplete" => {
            let response = event.get("response").cloned().unwrap_or(JsValue::Null);
            let normalized = normalize_codex_response(response);
            let object = event.as_object_mut().expect("codex event is an object");
            object.insert(
                "type".into(),
                JsValue::String(
                    if event_type == "response.incomplete" {
                        "response.incomplete"
                    } else {
                        "response.completed"
                    }
                    .into(),
                ),
            );
            object.insert("response".into(), normalized);
            Ok(Some(event))
        }
        _ => Ok(Some(event)),
    }
}

fn normalize_codex_response(response: JsValue) -> JsValue {
    let JsValue::Object(mut object) = response else {
        return response;
    };
    let status = object
        .get("status")
        .and_then(ingress_metadata_text)
        .as_deref()
        .filter(|status| CODEX_RESPONSE_STATUSES.contains(status))
        .map(str::to_owned);
    match status {
        Some(status) => {
            object.insert("status".into(), JsValue::String(status.into()));
        }
        None => {
            object.insert("status".into(), JsValue::Null);
        }
    }
    JsValue::Object(object)
}

fn extract_codex_event_error(event: &JsValue) -> (Option<String>, Option<JsString>) {
    let nested = event.get("error").filter(|error| error.is_object());
    let code = event.get("code").and_then(ingress_metadata_text).or_else(|| {
        nested
            .and_then(|error| error.get("code"))
            .and_then(ingress_metadata_text)
    });
    let message = event.get("message").and_then(JsValue::as_string).cloned().or_else(|| {
        nested
            .and_then(|error| error.get("message"))
            .and_then(JsValue::as_string)
            .cloned()
    });
    (code, message)
}

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct WsConnection {
    stream: Option<WsStream>,
    closed: bool,
}

impl WsConnection {
    fn new(stream: WsStream) -> Self {
        Self {
            stream: Some(stream),
            closed: false,
        }
    }

    fn take_stream(&mut self) -> Option<WsStream> {
        self.stream.take()
    }

    fn is_reusable(&self) -> bool {
        !self.closed && self.stream.is_some()
    }
}

struct CacheEntry {
    socket: Option<WsConnection>,
    busy: bool,
    created_at: tokio::time::Instant,
    idle_cancel: AbortController,
    continuation: Option<CachedWebSocketContinuationState>,
}

#[derive(Clone, Default)]
struct CachedWebSocketContinuationState {
    last_request_body: JsValue,
    last_response_id: String,
    last_response_items: Vec<JsValue>,
}

struct WsLease {
    connection: Option<WsConnection>,
    cache_key: Option<(JsString, String)>,
    entry: Option<Arc<Mutex<CacheEntry>>>,
    reused: bool,
}

impl WsLease {
    fn take_connection(&mut self) -> Option<WsConnection> {
        self.connection.take()
    }
}

type WebSocketSessionCache = BTreeMap<JsString, BTreeMap<String, Arc<Mutex<CacheEntry>>>>;

fn websocket_session_cache() -> &'static Mutex<WebSocketSessionCache> {
    static CACHE: OnceLock<Mutex<WebSocketSessionCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn websocket_debug_stats_store() -> &'static Mutex<BTreeMap<JsString, OpenAICodexWebSocketDebugStats>> {
    static STATS: OnceLock<Mutex<BTreeMap<JsString, OpenAICodexWebSocketDebugStats>>> = OnceLock::new();
    STATS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn cache_entry_mutex(entry: &Arc<Mutex<CacheEntry>>) -> std::sync::MutexGuard<'_, CacheEntry> {
    entry.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn cache_lock() -> std::sync::MutexGuard<'static, WebSocketSessionCache> {
    websocket_session_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `getOpenAICodexWebSocketDebugStats`.
pub fn get_openai_codex_websocket_debug_stats(session_id: &JsString) -> Option<OpenAICodexWebSocketDebugStats> {
    websocket_debug_stats_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(session_id)
        .cloned()
}

/// `resetOpenAICodexWebSocketDebugStats`.
pub fn reset_openai_codex_websocket_debug_stats(session_id: Option<&JsString>) {
    let mut stats = websocket_debug_stats_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match session_id {
        Some(session_id) => {
            stats.remove(session_id);
        }
        None => stats.clear(),
    }
}

fn update_websocket_debug_stats(session_id: &JsString, update: impl FnOnce(&mut OpenAICodexWebSocketDebugStats)) {
    let mut stats = websocket_debug_stats_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = stats.entry(session_id.to_owned()).or_default();
    update(entry);
}

/// `closeOpenAICodexWebSocketSessions`.
pub fn close_openai_codex_websocket_sessions(session_id: Option<&JsString>) {
    let mut cache = cache_lock();
    let mut sockets = Vec::new();
    match session_id.filter(|session_id| !session_id.is_empty()) {
        Some(session_id) => {
            if let Some(account_entries) = cache.remove(session_id) {
                for entry in account_entries.into_values() {
                    let mut entry = cache_entry_mutex(&entry);
                    entry.idle_cancel.abort(None);
                    if let Some(socket) = entry.socket.take() {
                        sockets.push(socket);
                    }
                }
            }
        }
        None => {
            for account_entries in std::mem::take(&mut *cache).into_values() {
                for entry in account_entries.into_values() {
                    let mut entry = cache_entry_mutex(&entry);
                    entry.idle_cancel.abort(None);
                    if let Some(socket) = entry.socket.take() {
                        sockets.push(socket);
                    }
                }
            }
        }
    }
    drop(cache);
    // The source's `socket.close()` returns before the close frame is written;
    // the frame needs a runtime, so without one the sockets are only dropped.
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        for socket in sockets {
            runtime.spawn(close_websocket(socket, 1000, "debug_close"));
        }
    }
}

fn register_websocket_cleanup() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        let _ = register_session_resource_cleanup(Box::new(|session_id| {
            close_openai_codex_websocket_sessions(session_id);
            Ok(())
        }));
    });
}

fn record_websocket_failure(session_id: Option<&JsString>, error: &CodexStreamError) {
    let Some(session_id) = session_id else {
        return;
    };
    update_websocket_debug_stats(session_id, |stats| {
        stats.websocket_failures += 1;
        stats.last_websocket_error = Some(format_thrown_value(&error.thrown_value()));
    });
}

async fn close_websocket(mut connection: WsConnection, code: u16, reason: &str) {
    let Some(mut stream) = connection.stream.take() else {
        return;
    };
    let frame = CloseFrame {
        code: CloseCode::from(code),
        reason: reason.to_owned().into(),
    };
    let _ = stream.close(Some(frame)).await;
}

async fn connect_websocket(
    url: &JsString,
    headers: &IndexMap<String, JsString>,
    signal: Option<AbortSignal>,
    connect_timeout_ms: Option<f64>,
) -> Result<WsConnection, CodexStreamError> {
    if is_aborted(signal.as_ref()) {
        return Err(CodexStreamError::Transport("Request was aborted".to_owned().into()));
    }
    let url = crate::request::request_url(url).map_err(|error| CodexStreamError::Transport(error.message))?;
    let mut request = url
        .into_client_request()
        .map_err(|error| CodexStreamError::Transport(error.to_string().into()))?;
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| CodexStreamError::Transport(error.to_string().into()))?;
        let value = crate::request::header_bytes(value).map_err(|error| CodexStreamError::Transport(error.message))?;
        let value =
            HeaderValue::from_bytes(&value).map_err(|error| CodexStreamError::Transport(error.to_string().into()))?;
        request.headers_mut().insert(name, value);
    }

    let connect = tokio_tungstenite::connect_async(request);
    tokio::pin!(connect);
    let abort = async {
        match &signal {
            Some(signal) => signal.wait().await,
            None => std::future::pending().await,
        }
    };
    let timeout_ms = connect_timeout_ms.unwrap_or(DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS);

    let result = if timeout_ms > 0.0 {
        tokio::select! {
            result = &mut connect => Some(result),
            () = tokio::time::sleep(codex_timer_duration(timeout_ms)) => {
                return Err(CodexStreamError::Transport(format!(
                    "WebSocket connect timeout after {timeout_ms}ms"
                ).into()));
            }
            _ = abort => {
                return Err(CodexStreamError::Transport("Request was aborted".to_owned().into()));
            }
        }
    } else {
        tokio::select! {
            result = &mut connect => Some(result),
            _ = abort => {
                return Err(CodexStreamError::Transport("Request was aborted".to_owned().into()));
            }
        }
    };

    match result {
        Some(Ok((stream, _response))) => Ok(WsConnection::new(stream)),
        Some(Err(error)) => {
            if is_aborted(signal.as_ref()) {
                Err(CodexStreamError::Transport("Request was aborted".to_owned().into()))
            } else {
                Err(CodexStreamError::Transport(websocket_error_message(&error).into()))
            }
        }
        None => Err(CodexStreamError::Transport("WebSocket error".to_owned().into())),
    }
}

fn websocket_error_message(error: &tokio_tungstenite::tungstenite::Error) -> String {
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            let status = response.status();
            let body = response
                .body()
                .as_ref()
                .map(|body| String::from_utf8_lossy(body).into_owned())
                .unwrap_or_default();
            if body.is_empty() {
                format!("WebSocket error: HTTP {status}")
            } else {
                format!("WebSocket error: HTTP {status}: {body}")
            }
        }
        other => format!("WebSocket error: {other}"),
    }
}

enum AcquireDecision {
    Reuse(Arc<Mutex<CacheEntry>>, WsConnection),
    Expired(Option<WsConnection>),
    Busy,
    Fresh,
}

fn take_cached_connection(session_id: &JsString, account_id: &str) -> AcquireDecision {
    let mut cache = cache_lock();
    let Some(entry) = cache
        .get(session_id)
        .and_then(|entries| entries.get(account_id))
        .cloned()
    else {
        return AcquireDecision::Fresh;
    };
    let mut entry_state = cache_entry_mutex(&entry);
    entry_state.idle_cancel.abort(None);
    if entry_state.busy {
        return AcquireDecision::Busy;
    }
    let expired = entry_state.created_at.elapsed() >= Duration::from_millis(SESSION_WEBSOCKET_MAX_AGE_MS);
    let socket = entry_state.socket.take();
    match socket {
        Some(socket) if !expired && socket.is_reusable() => {
            entry_state.busy = true;
            drop(entry_state);
            AcquireDecision::Reuse(entry, socket)
        }
        socket => {
            drop(entry_state);
            remove_cached_entry(&mut cache, session_id, account_id, &entry);
            AcquireDecision::Expired(socket)
        }
    }
}

async fn acquire_websocket(
    url: &JsString,
    headers: &IndexMap<String, JsString>,
    session_id: Option<&JsString>,
    account_id: &str,
    signal: Option<AbortSignal>,
    connect_timeout_ms: Option<f64>,
) -> Result<WsLease, CodexStreamError> {
    let Some(session_id) = session_id else {
        let connection = connect_websocket(url, headers, signal, connect_timeout_ms).await?;
        return Ok(WsLease {
            connection: Some(connection),
            cache_key: None,
            entry: None,
            reused: false,
        });
    };

    let decision = take_cached_connection(session_id, account_id);

    match decision {
        AcquireDecision::Reuse(entry, connection) => Ok(WsLease {
            connection: Some(connection),
            cache_key: Some((session_id.to_owned(), account_id.to_owned())),
            entry: Some(entry),
            reused: true,
        }),
        AcquireDecision::Expired(socket) => {
            if let Some(socket) = socket {
                close_websocket(socket, 1000, "connection_age_limit").await;
            }
            let connection = connect_websocket(url, headers, signal, connect_timeout_ms).await?;
            let entry = insert_cached_entry(session_id, account_id);
            Ok(WsLease {
                connection: Some(connection),
                cache_key: Some((session_id.to_owned(), account_id.to_owned())),
                entry: Some(entry),
                reused: false,
            })
        }
        AcquireDecision::Busy => {
            let connection = connect_websocket(url, headers, signal, connect_timeout_ms).await?;
            Ok(WsLease {
                connection: Some(connection),
                cache_key: None,
                entry: None,
                reused: false,
            })
        }
        AcquireDecision::Fresh => {
            let connection = connect_websocket(url, headers, signal, connect_timeout_ms).await?;
            let entry = insert_cached_entry(session_id, account_id);
            Ok(WsLease {
                connection: Some(connection),
                cache_key: Some((session_id.to_owned(), account_id.to_owned())),
                entry: Some(entry),
                reused: false,
            })
        }
    }
}

fn insert_cached_entry(session_id: &JsString, account_id: &str) -> Arc<Mutex<CacheEntry>> {
    let entry = Arc::new(Mutex::new(CacheEntry {
        socket: None,
        busy: true,
        created_at: tokio::time::Instant::now(),
        idle_cancel: AbortController::new(),
        continuation: None,
    }));
    cache_lock()
        .entry(session_id.to_owned())
        .or_default()
        .insert(account_id.to_owned(), Arc::clone(&entry));
    entry
}

fn remove_cached_entry(
    cache: &mut WebSocketSessionCache,
    session_id: &JsString,
    account_id: &str,
    entry: &Arc<Mutex<CacheEntry>>,
) {
    if let Some(entries) = cache.get_mut(session_id) {
        if entries
            .get(account_id)
            .is_some_and(|current| Arc::ptr_eq(current, entry))
        {
            entries.remove(account_id);
        }
        if entries.is_empty() {
            cache.remove(session_id);
        }
    }
}

async fn release_websocket(lease: WsLease, connection: Option<WsConnection>, keep: bool) {
    let Some((session_id, account_id)) = lease.cache_key else {
        if let Some(connection) = connection {
            close_websocket(connection, 1000, "done").await;
        }
        return;
    };
    let Some(entry) = lease.entry else {
        if let Some(connection) = connection {
            close_websocket(connection, 1000, "done").await;
        }
        return;
    };
    let reusable = keep && connection.as_ref().is_some_and(WsConnection::is_reusable);
    if !reusable {
        {
            let mut cache = cache_lock();
            let mut entry_state = cache_entry_mutex(&entry);
            entry_state.idle_cancel.abort(None);
            entry_state.busy = false;
            entry_state.socket = None;
            remove_cached_entry(&mut cache, &session_id, &account_id, &entry);
        }
        if let Some(connection) = connection {
            close_websocket(connection, 1000, "done").await;
        }
        return;
    }

    let idle_cancel = {
        let mut entry_state = cache_entry_mutex(&entry);
        entry_state.socket = connection;
        entry_state.busy = false;
        entry_state.idle_cancel = AbortController::new();
        entry_state.idle_cancel.signal()
    };
    schedule_session_websocket_expiry(session_id, account_id, entry, idle_cancel);
}

fn schedule_session_websocket_expiry(
    session_id: JsString,
    account_id: String,
    entry: Arc<Mutex<CacheEntry>>,
    idle_cancel: AbortSignal,
) {
    tokio::spawn(async move {
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(SESSION_WEBSOCKET_CACHE_TTL_MS)) => {}
            _ = idle_cancel.wait() => return,
        }
        let socket = {
            let mut cache = cache_lock();
            let mut entry_state = cache_entry_mutex(&entry);
            if entry_state.busy {
                return;
            }
            let Some(socket) = entry_state.socket.take() else {
                return;
            };
            remove_cached_entry(&mut cache, &session_id, &account_id, &entry);
            socket
        };
        close_websocket(socket, 1000, "idle_timeout").await;
    });
}

struct WebSocketExecution<'a> {
    model: &'a Model,
    options: &'a OpenAICodexResponsesOptions,
    body: JsValue,
    body_json: &'a str,
    headers: IndexMap<String, JsString>,
    http_timeout_ms: Option<f64>,
    websocket_connect_timeout_ms: Option<f64>,
    cache_session_id: Option<&'a JsString>,
    account_id: &'a str,
}

async fn execute_websocket(
    execution: &WebSocketExecution<'_>,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    start_emitted: &mut bool,
) -> Result<(), CodexStreamError> {
    register_websocket_cleanup();
    let transport = execution.options.stream.transport.unwrap_or(Transport::Auto);
    let use_cached_context = matches!(transport, Transport::WebsocketCached | Transport::Auto);
    let mut retried_connection_limit = false;

    loop {
        let websocket_started = Arc::new(AtomicBool::new(false));
        let start_emitted_flag = Arc::new(AtomicBool::new(*start_emitted));
        let result = process_websocket_stream(
            execution,
            output,
            sender,
            use_cached_context,
            websocket_started.clone(),
            start_emitted_flag.clone(),
        )
        .await;
        *start_emitted = start_emitted_flag.load(Ordering::SeqCst);

        match result {
            Ok(()) => {
                if is_aborted(execution.options.stream.request.signal.as_ref()) {
                    return Err(CodexStreamError::Transport("Request was aborted".to_owned().into()));
                }
                return Ok(());
            }
            Err(error) => {
                let aborted = is_aborted(execution.options.stream.request.signal.as_ref());
                let started = websocket_started.load(Ordering::SeqCst);
                let connection_limit_before_start = !started && error.is_websocket_connection_limit_reached();
                if !aborted && connection_limit_before_start && !retried_connection_limit {
                    retried_connection_limit = true;
                    if let Some(on_retry) = &execution.options.stream.on_retry {
                        on_retry(
                            ProviderRetryEvent {
                                attempt: 1.0,
                                max_attempts: None,
                                delay_ms: None,
                                reason: "WebSocket connection limit reached, reconnecting".into(),
                                fallback_transport: None,
                            },
                            execution.model,
                        )
                        .map_err(format_codex_callback_error)?;
                    }
                    continue;
                }
                if aborted || (error.is_non_transport() && !connection_limit_before_start) {
                    return Err(if aborted {
                        CodexStreamError::Transport("Request was aborted".to_owned().into())
                    } else {
                        error
                    });
                }
                let mut details = Map::new();
                details.insert(
                    "configuredTransport".to_owned(),
                    Value::String(transport_name(transport).to_owned()),
                );
                details.insert("eventsEmitted".to_owned(), Value::Bool(started));
                details.insert(
                    "phase".to_owned(),
                    Value::String(
                        if started {
                            "after_message_stream_start"
                        } else {
                            "before_message_stream_start"
                        }
                        .to_owned(),
                    ),
                );
                details.insert("requestBytes".to_owned(), Value::from(execution.body_json.len() as u64));
                append_assistant_message_diagnostic(
                    &mut output.diagnostics,
                    create_assistant_message_diagnostic(
                        "provider_transport_failure",
                        &error.thrown_value(),
                        Some(details),
                    ),
                );
                record_websocket_failure(execution.cache_session_id, &error);
                return Err(error);
            }
        }
    }
}

fn retain_busy_websocket_setup_error(mut lease: WsLease, error: CallbackError) -> CodexStreamError {
    // Frozen setup throws before the release try/finally. Keep the busy socket
    // reachable by explicit session cleanup, rather than inventing recovery.
    if let Some(entry) = lease.entry.clone() {
        cache_entry_mutex(&entry).socket = lease.take_connection();
    }
    CodexStreamError::Native(Box::new(error))
}

fn transport_name(transport: Transport) -> &'static str {
    match transport {
        Transport::Sse => "sse",
        Transport::Websocket => "websocket",
        Transport::WebsocketCached => "websocket-cached",
        Transport::Auto => "auto",
    }
}

async fn process_websocket_stream(
    execution: &WebSocketExecution<'_>,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    use_cached_context: bool,
    websocket_started: Arc<AtomicBool>,
    start_emitted: Arc<AtomicBool>,
) -> Result<(), CodexStreamError> {
    let mut lease = acquire_websocket(
        &resolve_codex_websocket_url(&execution.model.base_url),
        &execution.headers,
        execution.cache_session_id,
        execution.account_id,
        execution.options.stream.request.signal.clone(),
        execution.websocket_connect_timeout_ms,
    )
    .await?;
    let entry = lease.entry.clone();
    let request_body = if use_cached_context {
        match &entry {
            Some(entry) => {
                let mut entry_state = cache_entry_mutex(entry);
                build_cached_websocket_request_body(&mut entry_state, &execution.body)
            }
            None => Ok(execution.body.clone()),
        }
    } else {
        Ok(execution.body.clone())
    };
    let request_body = match request_body {
        Ok(body) => body,
        Err(error) => return Err(retain_busy_websocket_setup_error(lease, error)),
    };
    let body = &execution.body;

    if let Some(session_id) = execution.cache_session_id {
        let null_body = matches!(request_body, JsValue::Null);
        let reused = lease.reused;
        let request_items = request_body
            .get("input")
            .and_then(payload_length)
            .filter(|length| !matches!(length, JsValue::Null))
            .unwrap_or(JsValue::Number(0.0));
        let previous_response_id = request_body
            .get("previous_response_id")
            .filter(|value| js_json::is_truthy(value))
            .cloned();
        let store_true = request_body.get("store").and_then(JsValue::as_bool) == Some(true);
        update_websocket_debug_stats(session_id, |stats| {
            stats.requests += 1;
            if reused {
                stats.connections_reused += 1;
            } else {
                stats.connections_created += 1;
            }
            if use_cached_context {
                stats.cached_context_requests += 1;
            }
            if null_body {
                return;
            }
            if store_true {
                stats.store_true_requests += 1;
            }
            stats.last_input_items = request_items.clone();
            match previous_response_id {
                Some(previous_response_id) => {
                    stats.delta_requests += 1;
                    stats.last_delta_input_items = Some(request_items);
                    stats.last_previous_response_id = Some(previous_response_id);
                }
                None => {
                    stats.full_context_requests += 1;
                    stats.last_delta_input_items = None;
                    stats.last_previous_response_id = None;
                }
            }
        });
        if null_body {
            return Err(retain_busy_websocket_setup_error(
                lease,
                codex_type_error("Cannot read properties of null (reading 'store')"),
            ));
        }
    }

    let mut connection = lease.take_connection().ok_or_else(|| {
        CodexStreamError::Transport("WebSocket transport is not available in this runtime".to_owned().into())
    })?;
    let stream = connection.take_stream().ok_or_else(|| {
        CodexStreamError::Transport("WebSocket transport is not available in this runtime".to_owned().into())
    })?;
    let (mut sink, split_stream) = stream.split();
    let frame = response_create_frame(&request_body);
    let frame_json =
        serde_json::to_string(&frame).map_err(|error| CodexStreamError::Transport(error.to_string().into()))?;
    if let Err(error) = sink.send(Message::text(frame_json)).await {
        connection.closed = true;
        release_websocket(lease, None, false).await;
        return Err(CodexStreamError::Transport(websocket_error_message(&error).into()));
    }

    let mut source = WsEventSource::new(
        sink,
        split_stream,
        WsEventSourceConfig {
            signal: execution.options.stream.request.signal.clone(),
            idle_timeout_ms: execution.http_timeout_ms,
            sender: sender.clone(),
            partial: output.clone(),
            websocket_started,
            start_emitted,
        },
    );
    let result = process_responses_stream(
        &mut source,
        output,
        sender,
        execution.model,
        Some(&codex_stream_options(execution.model, execution.options)),
    )
    .await;
    let mut connection = source.into_connection();

    let aborted = is_aborted(execution.options.stream.request.signal.as_ref());
    let result = result.map_err(CodexStreamError::from_responses_failure).and_then(|()| {
        if !aborted && use_cached_context {
            save_websocket_continuation(execution.model, body, entry.as_ref(), output)?;
        }
        Ok(())
    });
    if result.is_err() {
        if let Some(entry) = &entry {
            cache_entry_mutex(entry).continuation = None;
        }
        connection.closed = true;
    }
    let keep_connection = !aborted && result.is_ok();
    release_websocket(lease, Some(connection), keep_connection).await;
    result
}

fn save_websocket_continuation(
    model: &Model,
    body: &JsValue,
    entry: Option<&Arc<Mutex<CacheEntry>>>,
    output: &AssistantMessage,
) -> Result<(), CodexStreamError> {
    let Some(entry) = entry else {
        return Ok(());
    };
    let Some(response_id) = output.response_id.as_ref().filter(|id| !id.is_empty()) else {
        return Ok(());
    };
    let items = convert_response_items(model, output).map_err(CodexStreamError::Transport)?;
    cache_entry_mutex(entry).continuation = Some(CachedWebSocketContinuationState {
        last_request_body: body.clone(),
        last_response_id: response_id.clone(),
        last_response_items: items,
    });
    Ok(())
}

fn response_create_frame(body: &JsValue) -> JsValue {
    let mut frame = JsObject::new();
    frame.insert("type".into(), JsValue::String("response.create".into()));
    frame.extend(spread_payload_properties(body));
    JsValue::Object(frame)
}

fn spread_payload_properties(body: &JsValue) -> JsObject {
    let mut frame = JsObject::new();
    match body {
        JsValue::Object(object) => {
            for (key, value) in object {
                frame.insert(key.clone(), value.clone());
            }
        }
        JsValue::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                frame.insert(index.to_string().into(), value.clone());
            }
        }
        JsValue::String(text) => {
            for (index, unit) in text.units().iter().enumerate() {
                frame.insert(
                    index.to_string().into(),
                    JsValue::String(JsString::from_utf16(vec![*unit])),
                );
            }
        }
        JsValue::Null | JsValue::Bool(_) | JsValue::Number(_) => {}
    }
    frame
}

fn convert_response_items(model: &Model, output: &AssistantMessage) -> Result<Vec<JsValue>, JsString> {
    let response_context = Context {
        system_prompt: None,
        messages: vec![ChatMessage::Assistant(Box::new(output.clone()))],
        tools: None,
    };
    convert_responses_messages(
        model,
        &response_context,
        &codex_tool_call_providers(),
        Some(&ConvertResponsesMessagesOptions {
            include_system_prompt: Some(false),
        }),
    )
    .map(|items| {
        items
            .into_iter()
            .filter(|item| {
                !item
                    .get("type")
                    .and_then(JsValue::as_string)
                    .is_some_and(|kind| kind == "function_call_output")
            })
            .collect()
    })
}

fn codex_type_error(message: impl Into<JsString>) -> CallbackError {
    pi_js_compat::node_error::NativeNodeError::from_constructor(
        pi_js_compat::node_error::NativeErrorClass::TypeError,
        message,
    )
    .into()
}

fn request_body_without_input(body: &JsValue) -> Result<JsValue, CallbackError> {
    if matches!(body, JsValue::Null) {
        return Err(codex_type_error(
            "Cannot destructure property 'input' of 'body' as it is null.",
        ));
    }
    let mut rest = spread_payload_properties(body);
    rest.shift_remove("input");
    rest.shift_remove("previous_response_id");
    Ok(JsValue::Object(rest))
}

fn request_bodies_match_except_input(a: &JsValue, b: &JsValue) -> Result<bool, CallbackError> {
    Ok(js_json::stringify_lossless(&request_body_without_input(a)?)
        == js_json::stringify_lossless(&request_body_without_input(b)?))
}

fn input_or_empty(body: &JsValue) -> JsValue {
    body.get("input")
        .filter(|value| !matches!(value, JsValue::Null))
        .cloned()
        .unwrap_or_else(|| JsValue::Array(Vec::new()))
}

fn payload_length(value: &JsValue) -> Option<JsValue> {
    match value {
        JsValue::Array(values) => Some(JsValue::Number(values.len() as f64)),
        JsValue::String(text) => Some(JsValue::Number(text.units().len() as f64)),
        JsValue::Object(properties) => properties.get("length").cloned(),
        _ => None,
    }
}

fn spread_cached_input(value: JsValue) -> Result<Vec<JsValue>, CallbackError> {
    match value {
        JsValue::Array(values) => Ok(values),
        JsValue::String(text) => {
            let mut index = 0;
            let mut values = Vec::new();
            while index < text.units().len() {
                let paired = (0xd800..=0xdbff).contains(&text.units()[index])
                    && text
                        .units()
                        .get(index + 1)
                        .is_some_and(|unit| (0xdc00..=0xdfff).contains(unit));
                let end = index + if paired { 2 } else { 1 };
                values.push(JsValue::String(text.slice(index, end)));
                index = end;
            }
            Ok(values)
        }
        _ => Err(codex_type_error(
            "(continuation.lastRequestBody.input ?? []) is not iterable",
        )),
    }
}

fn slice_cached_input(value: &JsValue, start: usize, end: usize) -> Result<JsValue, CallbackError> {
    match value {
        JsValue::Array(values) => Ok(JsValue::Array(
            values[start.min(values.len())..end.min(values.len())].to_vec(),
        )),
        JsValue::String(text) => Ok(JsValue::String(
            text.slice(start.min(text.units().len()), end.min(text.units().len())),
        )),
        _ => Err(codex_type_error("currentInput.slice is not a function")),
    }
}

fn cached_websocket_input_delta(
    body: &JsValue,
    continuation: &CachedWebSocketContinuationState,
) -> Result<Option<JsValue>, CallbackError> {
    if !request_bodies_match_except_input(body, &continuation.last_request_body)? {
        return Ok(None);
    }
    let current = input_or_empty(body);
    let mut baseline = spread_cached_input(input_or_empty(&continuation.last_request_body))?;
    baseline.extend(continuation.last_response_items.iter().cloned());
    let length = payload_length(&current)
        .map(|value| js_json::to_js_number(&value))
        .transpose()
        .map_err(codex_type_error)?
        .unwrap_or(f64::NAN);
    if length < baseline.len() as f64 {
        return Ok(None);
    }
    let prefix = slice_cached_input(&current, 0, baseline.len())?;
    if js_json::stringify_lossless(&prefix) != js_json::stringify_lossless(&JsValue::Array(baseline.clone())) {
        return Ok(None);
    }
    slice_cached_input(&current, baseline.len(), usize::MAX).map(Some)
}

fn build_cached_websocket_request_body(entry: &mut CacheEntry, body: &JsValue) -> Result<JsValue, CallbackError> {
    let Some(continuation) = entry.continuation.clone() else {
        return Ok(body.clone());
    };
    let Some(delta) = cached_websocket_input_delta(body, &continuation)? else {
        entry.continuation = None;
        return Ok(body.clone());
    };
    if continuation.last_response_id.is_empty() {
        entry.continuation = None;
        return Ok(body.clone());
    }
    let mut object = spread_payload_properties(body);
    object.insert(
        "previous_response_id".into(),
        JsValue::String(continuation.last_response_id.into()),
    );
    object.insert("input".into(), delta);
    Ok(JsValue::Object(object))
}

enum WsWait {
    Message(Option<Result<Message, tokio_tungstenite::tungstenite::Error>>),
    Ping,
    Idle,
    Aborted,
}

struct WsEventSourceConfig {
    signal: Option<AbortSignal>,
    idle_timeout_ms: Option<f64>,
    sender: AssistantMessageEventSender,
    partial: AssistantMessage,
    websocket_started: Arc<AtomicBool>,
    start_emitted: Arc<AtomicBool>,
}

struct WsEventSource {
    sink: SplitSink<WsStream, Message>,
    stream: SplitStream<WsStream>,
    signal: Option<AbortSignal>,
    idle_timeout_ms: Option<f64>,
    queue: VecDeque<JsValue>,
    done: bool,
    failed: Option<CodexStreamError>,
    saw_completion: bool,
    closed: bool,
    first_event: bool,
    sender: AssistantMessageEventSender,
    partial: AssistantMessage,
    websocket_started: Arc<AtomicBool>,
    start_emitted: Arc<AtomicBool>,
    ping_interval: tokio::time::Interval,
    idle_deadline: Option<tokio::time::Instant>,
}

impl WsEventSource {
    fn new(sink: SplitSink<WsStream, Message>, stream: SplitStream<WsStream>, config: WsEventSourceConfig) -> Self {
        Self {
            sink,
            stream,
            signal: config.signal,
            idle_timeout_ms: config.idle_timeout_ms,
            queue: VecDeque::new(),
            done: false,
            failed: None,
            saw_completion: false,
            closed: false,
            first_event: true,
            sender: config.sender,
            partial: config.partial,
            websocket_started: config.websocket_started,
            start_emitted: config.start_emitted,
            ping_interval: tokio::time::interval_at(
                tokio::time::Instant::now() + Duration::from_millis(WEBSOCKET_PING_INTERVAL_MS),
                Duration::from_millis(WEBSOCKET_PING_INTERVAL_MS),
            ),
            idle_deadline: None,
        }
    }

    fn into_connection(self) -> WsConnection {
        let Self {
            sink, stream, closed, ..
        } = self;
        match sink.reunite(stream) {
            Ok(stream) => WsConnection {
                stream: Some(stream),
                closed,
            },
            Err(_) => WsConnection {
                stream: None,
                closed: true,
            },
        }
    }

    fn emit_start_if_needed(&mut self) {
        if !self.first_event {
            return;
        }
        self.first_event = false;
        self.websocket_started.store(true, Ordering::SeqCst);
        if !self.start_emitted.swap(true, Ordering::SeqCst) {
            self.sender.push(AssistantMessageEvent::Start {
                partial: self.partial.clone(),
            });
        }
    }

    fn fail(&mut self, error: CodexStreamError) {
        self.failed = Some(error);
        self.done = true;
    }

    fn handle_text(&mut self, text: &str) {
        let parsed: JsValue = match pi_js_compat::js_json::parse_lossless(text) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.fail(CodexStreamError::Protocol {
                    message: JsString::join(["Invalid Codex WebSocket JSON: ".into(), error.into_message()], ""),
                    payload: Some(JsValue::String(text.into())),
                });
                return;
            }
        };
        if matches!(parsed, JsValue::Null) {
            self.fail(CodexStreamError::Protocol {
                message: "Invalid Codex WebSocket JSON: Cannot read properties of null (reading 'type')".into(),
                payload: Some(JsValue::String(text.into())),
            });
            return;
        }
        if let Err(error) = crate::api::openai_responses_shared::validate_responses_metadata(&parsed) {
            self.fail(CodexStreamError::Transport(error.into()));
            return;
        }
        let event_type = parsed.get("type").and_then(ingress_metadata_text).unwrap_or_default();
        let terminal = matches!(
            event_type.as_str(),
            "response.completed" | "response.done" | "response.incomplete"
        );
        if terminal {
            self.saw_completion = true;
        }
        match map_codex_event(parsed, None) {
            Ok(Some(event)) => {
                self.queue.push_back(event);
                if terminal {
                    self.done = true;
                }
            }
            Ok(None) => {}
            Err(error) => self.fail(error),
        }
    }

    async fn wait(&mut self) -> WsWait {
        let abort = async {
            match &self.signal {
                Some(signal) => signal.wait().await,
                None => std::future::pending().await,
            }
        };
        let idle_sleep = self.idle_timeout_ms.filter(|timeout| *timeout > 0.0).map(|timeout_ms| {
            let deadline = *self
                .idle_deadline
                .get_or_insert_with(|| tokio::time::Instant::now() + codex_timer_duration(timeout_ms));
            tokio::time::sleep_until(deadline)
        });
        tokio::select! {
            biased;
            _ = self.ping_interval.tick() => WsWait::Ping,
            message = self.stream.next() => WsWait::Message(message),
            () = async {
                match idle_sleep {
                    Some(sleep) => sleep.await,
                    None => std::future::pending().await,
                }
            } => WsWait::Idle,
            _ = abort => WsWait::Aborted,
        }
    }

    async fn close_with(&mut self, code: u16, reason: &str) {
        self.closed = true;
        let frame = CloseFrame {
            code: CloseCode::from(code),
            reason: reason.to_owned().into(),
        };
        let _ = self.sink.send(Message::Close(Some(frame))).await;
    }
}

impl ResponsesEventSource for WsEventSource {
    type Error = CodexStreamError;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<JsValue>, Self::Error>> {
        Box::pin(async move {
            loop {
                if is_aborted(self.signal.as_ref()) {
                    self.closed = true;
                    return Err(CodexStreamError::Transport("Request was aborted".to_owned().into()));
                }
                if let Some(event) = self.queue.pop_front() {
                    self.emit_start_if_needed();
                    return Ok(Some(event));
                }
                if self.done {
                    return match self.failed.take() {
                        Some(error) => Err(error),
                        None => Ok(None),
                    };
                }

                let outcome = self.wait().await;
                if matches!(outcome, WsWait::Message(_)) {
                    self.idle_deadline = None;
                }
                match outcome {
                    WsWait::Message(Some(Ok(Message::Text(text)))) => {
                        self.handle_text(text.as_str());
                    }
                    WsWait::Message(Some(Ok(Message::Binary(bytes)))) => {
                        let mut decoder = pi_js_compat::text_decoder::Utf8StreamDecoder::new();
                        let mut text = decoder.decode(&bytes);
                        text.push_str(&decoder.finish());
                        self.handle_text(&text);
                    }
                    WsWait::Message(Some(Ok(Message::Close(frame)))) => {
                        self.closed = true;
                        if !self.saw_completion {
                            self.fail(close_error(frame.as_ref()));
                        }
                        self.done = true;
                    }
                    WsWait::Message(Some(Ok(Message::Ping(_))))
                    | WsWait::Message(Some(Ok(Message::Pong(_))))
                    | WsWait::Message(Some(Ok(Message::Frame(_)))) => {}
                    WsWait::Message(Some(Err(error))) => {
                        self.closed = true;
                        self.fail(CodexStreamError::Transport(format!("WebSocket error: {error}").into()));
                    }
                    WsWait::Message(None) => {
                        self.closed = true;
                        if !self.saw_completion {
                            self.fail(CodexStreamError::Transport("WebSocket closed".to_owned().into()));
                        }
                        self.done = true;
                    }
                    WsWait::Ping => {
                        let _ = self.sink.send(Message::Ping(Vec::new().into())).await;
                    }
                    WsWait::Idle => {
                        let timeout_ms = self.idle_timeout_ms.unwrap_or_default();
                        self.close_with(1000, "idle_timeout").await;
                        self.fail(CodexStreamError::Transport(
                            format!("WebSocket idle timeout after {timeout_ms}ms").into(),
                        ));
                    }
                    WsWait::Aborted => {
                        self.closed = true;
                        self.fail(CodexStreamError::Transport("Request was aborted".to_owned().into()));
                    }
                }
            }
        })
    }
}

fn close_error(frame: Option<&CloseFrame>) -> CodexStreamError {
    let Some(frame) = frame else {
        return CodexStreamError::Transport("WebSocket closed".to_owned().into());
    };
    let code: u16 = frame.code.into();
    let reason = frame.reason.as_str();
    let code_text = format!(" {code}");
    let mut reason_text = if reason.is_empty() {
        String::new()
    } else {
        format!(" {reason}")
    };
    if reason_text.is_empty() && code == WEBSOCKET_MESSAGE_TOO_BIG_CLOSE_CODE {
        reason_text = " message too big".to_owned();
    }
    let message = pi_js_compat::string::js_trim(&format!("WebSocket closed{code_text}{reason_text}")).to_owned();
    let error = pi_js_compat::node_error::NativeNodeError::new("WebSocketCloseError", message)
        .with_property("code", JsValue::Number(f64::from(code)));
    CodexStreamError::Native(Box::new(error.into()))
}
