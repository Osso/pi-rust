//! Port of `packages/ai/src/api/openai-codex-responses.ts`.
//!
//! Codex-specific transport around the shared Responses conversion and state
//! machine: SSE with idle timeouts, WebSocket transports (including cached
//! continuation), retries, and the session WebSocket cache.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::Engine as _;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

use crate::api::constrained_sampling::{ConstrainedSamplingError, create_grammar_tool_input_properties};
use crate::api::lazy::{LazyApi, LazyApiCapabilities, lazy_api};
use crate::api::openai_prompt_cache::clamp_openai_prompt_cache_key;
use crate::api::openai_responses_shared::{
    ConvertResponsesMessagesOptions, ConvertResponsesToolsOptions, OpenAIResponsesStreamOptions,
    ResponsesDeferredToolsMode, ResponsesEventSource, ResponsesStreamFailure, convert_responses_messages,
    convert_responses_tools, process_responses_stream, push_responses_done,
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
    ModelCompat, ModelThinkingLevel, OpenAIResponsesCompat, ProviderRetryEvent, StopReason, ThinkingLevel, Tool,
    Transport, Usage,
};
use crate::utils::deferred_tools::split_deferred_tools;
use crate::utils::diagnostics::{
    DiagnosticErrorInfo, ThrownValue, append_assistant_message_diagnostic, create_assistant_message_diagnostic,
    format_thrown_value,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::uuid::uuidv7;

pub const DEFAULT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api";
const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";
const DEFAULT_MAX_RETRIES: f64 = 0.0;
const BASE_DELAY_MS: f64 = 1000.0;
const DEFAULT_MAX_RETRY_DELAY_MS: f64 = 60_000.0;
const DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS: f64 = 15_000.0;
const WEBSOCKET_PING_INTERVAL_MS: u64 = 30_000;
const REQUEST_COMPRESSION_ZSTD_LEVEL: i32 = 3;
const CODEX_TOOL_CALL_PROVIDERS: [&str; 4] = ["openai", "openai-codex", "openai-codex-gc", "opencode"];
const WEBSOCKET_MESSAGE_TOO_BIG_CLOSE_CODE: u16 = 1009;
const WEBSOCKET_CONNECTION_LIMIT_REACHED_CODE: &str = "websocket_connection_limit_reached";
const PREVIOUS_RESPONSE_NOT_FOUND_CODE: &str = "previous_response_not_found";
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
    pub tool_choice: Option<CodexToolChoice>,
}

impl OpenAICodexResponsesOptions {
    /// Reads the API-specific camelCase fields from [`StreamOptions::extra`].
    pub fn from_stream_options(stream: StreamOptions) -> Result<Self, String> {
        let extra = &stream.extra;
        Ok(Self {
            reasoning_effort: parse_extra(extra, "reasoningEffort")?,
            reasoning_summary: parse_extra(extra, "reasoningSummary")?,
            service_tier: parse_extra(extra, "serviceTier")?,
            text_verbosity: parse_extra(extra, "textVerbosity")?,
            tool_choice: parse_extra(extra, "toolChoice")?,
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

string_enum!(ServiceTier {
    Auto => "auto",
    Default => "default",
    Flex => "flex",
    Priority => "priority",
});

string_enum!(TextVerbosity {
    Low => "low",
    Medium => "medium",
    High => "high",
});

string_enum!(CodexToolChoice {
    Auto => "auto",
    None => "none",
    Required => "required",
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
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAICodexWebSocketDebugStats {
    pub requests: u64,
    pub connections_created: u64,
    pub connections_reused: u64,
    pub cached_context_requests: u64,
    pub store_true_requests: u64,
    pub full_context_requests: u64,
    pub delta_requests: u64,
    pub last_input_items: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_delta_input_items: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_previous_response_id: Option<String>,
    pub websocket_failures: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_websocket_error: Option<String>,
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
pub fn stream_simple(model: Model, context: Context, options: SimpleStreamOptions) -> AssistantMessageEventStream {
    let Some(api_key) = options.stream.request.api_key.clone().filter(|key| !key.is_empty()) else {
        return crate::request::error_stream(&model, &format!("No API key for provider: {}", model.provider));
    };

    let base = build_base_options(&model, &context, Some(&options), Some(&api_key));
    let reasoning_effort = options.reasoning.and_then(|reasoning| {
        let clamped = clamp_thinking_level(&model, model_thinking_level(reasoning));
        (clamped != ModelThinkingLevel::Off).then(|| CodexReasoningEffort::from(clamped))
    });

    stream(
        model,
        context,
        OpenAICodexResponsesOptions {
            stream: base,
            reasoning_effort,
            ..OpenAICodexResponsesOptions::default()
        },
    )
}

/// `OpenAICodexResponsesApi`.
pub struct OpenAICodexResponsesApi;

impl ProviderStreams for OpenAICodexResponsesApi {
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        match OpenAICodexResponsesOptions::from_stream_options(options) {
            Ok(options) => stream(model, context, options),
            Err(message) => crate::request::error_stream(&model, &message),
        }
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

/// `openAICodexResponsesApi()`: the lazy module wrapper.
pub fn openai_codex_responses_api() -> LazyApi {
    lazy_api(
        Arc::new(|| Box::pin(async { Ok(Arc::new(OpenAICodexResponsesApi) as Arc<dyn ProviderStreams>) })),
        LazyApiCapabilities::default(),
    )
}

async fn run_stream(
    model: Model,
    context: Context,
    options: OpenAICodexResponsesOptions,
    sender: AssistantMessageEventSender,
) {
    let mut output = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis());
    let result = execute_stream(&model, &context, &options, &mut output, &sender).await;
    match result {
        Ok(()) => match done_reason(output.stop_reason) {
            Ok(reason) => {
                let _ = push_responses_done(&sender, &output);
                sender.push(AssistantMessageEvent::Done {
                    reason,
                    message: output,
                });
            }
            Err(message) => fail_stream(&mut output, &options, &sender, &message),
        },
        Err(error) => fail_stream(&mut output, &options, &sender, &error.to_string()),
    }
}

fn fail_stream(
    output: &mut AssistantMessage,
    options: &OpenAICodexResponsesOptions,
    sender: &AssistantMessageEventSender,
    message: &str,
) {
    let aborted = is_aborted(options.stream.request.signal.as_ref());
    output.stop_reason = if aborted {
        StopReason::Aborted
    } else {
        StopReason::Error
    };
    output.error_message = Some(message.to_owned());
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
        StopReason::Deferred => Ok(DoneReason::Deferred),
        StopReason::Pending => Err("Codex stream ended without a stop reason".to_owned()),
        StopReason::Error | StopReason::Aborted => Err("An unknown error occurred".to_owned()),
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

fn responses_compat(model: &Model) -> Option<&OpenAIResponsesCompat> {
    match &model.compat {
        Some(ModelCompat::OpenAIResponses(compat)) => Some(compat),
        _ => None,
    }
}

fn supports_strict_mode(model: &Model) -> bool {
    responses_compat(model)
        .and_then(|compat| compat.supports_strict_mode)
        .unwrap_or(true)
}

fn supports_grammar_tools(model: &Model) -> bool {
    responses_compat(model)
        .and_then(|compat| compat.supports_openai_grammar_tools)
        .unwrap_or(false)
}

fn deferred_tools_mode(model: &Model) -> Option<ResponsesDeferredToolsMode> {
    let compat = responses_compat(model)?;
    if compat.supports_additional_tools == Some(true) {
        Some(ResponsesDeferredToolsMode::AdditionalTools)
    } else if compat.supports_tool_search == Some(true) {
        Some(ResponsesDeferredToolsMode::ToolSearch)
    } else {
        None
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
        .as_deref()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| CodexStreamError::Transport(format!("No API key for provider: {}", model.provider)))?;
    let account_id = extract_account_id(api_key)?;
    let grammar_tool_input_properties =
        create_grammar_tool_input_properties(context.tools.as_deref(), supports_grammar_tools(model))
            .map_err(|error| CodexStreamError::Transport(error.to_string()))?;
    let cache_session_id = if options.stream.cache_retention == Some(crate::types::CacheRetention::None) {
        None
    } else {
        options.stream.session_id.as_deref()
    };
    let codex_session_id = clamp_openai_prompt_cache_key(cache_session_id).map(str::to_owned);
    let mut body = build_request_body(
        model,
        context,
        options,
        codex_session_id.as_deref(),
        &grammar_tool_input_properties,
    )?;
    if let Some(on_payload) = &options.stream.request.on_payload
        && let Some(next_body) = on_payload(body.clone(), model.clone())
            .await
            .map_err(|error| CodexStreamError::Transport(error.message))?
    {
        body = next_body;
    }
    let websocket_request_id = codex_session_id.clone().unwrap_or_else(uuidv7);
    let sse_headers = build_sse_headers(
        model.headers.as_ref(),
        options.stream.request.headers.as_ref(),
        &account_id,
        api_key,
        codex_session_id.as_deref(),
    );
    let websocket_headers = build_websocket_headers(
        model.headers.as_ref(),
        options.stream.request.headers.as_ref(),
        &account_id,
        api_key,
        &websocket_request_id,
    );
    let body_json = serde_json::to_string(&body).map_err(|error| CodexStreamError::Transport(error.to_string()))?;
    let http_timeout_ms = normalize_timeout_ms(options.stream.request.timeout_ms)?;
    let websocket_connect_timeout_ms = normalize_timeout_ms(options.stream.websocket_connect_timeout_ms)?;
    let transport = options.stream.transport.unwrap_or(Transport::Auto);
    let mut start_emitted = false;

    if transport != Transport::Sse {
        let execution = WebSocketExecution {
            model,
            context,
            options,
            body,
            body_json: &body_json,
            headers: websocket_headers,
            http_timeout_ms,
            websocket_connect_timeout_ms,
            cache_session_id,
            account_id: &account_id,
            grammar_tool_input_properties: &grammar_tool_input_properties,
        };
        return execute_websocket(&execution, output, sender, &mut start_emitted).await;
    }

    let compressed_body = compress_request_body_zstd(&body_json);
    let mut sse_headers = sse_headers;
    if compressed_body.is_some() {
        set_header(&mut sse_headers, "content-encoding", "zstd");
    }
    let sse_body = compressed_body.unwrap_or_else(|| body_json.into_bytes());

    let (response, request_token) =
        fetch_sse_with_retries(model, options, &sse_headers, sse_body, http_timeout_ms).await?;

    if !start_emitted {
        sender.push(AssistantMessageEvent::Start {
            partial: output.clone(),
        });
    }

    let end_turn = Arc::new(Mutex::new(None));
    let source = SseEventSource::new(
        response.body,
        http_timeout_ms,
        options.stream.request.signal.clone(),
        request_token,
    );
    let mut mapped = CodexMappedSource::new(source, end_turn.clone());
    let result = process_responses_stream(
        &mut mapped,
        output,
        sender,
        model,
        Some(&codex_stream_options(model, options, &grammar_tool_input_properties)),
    )
    .await;
    drop(mapped);
    if let Some(value) = *end_turn.lock().expect("codex end-turn mutex poisoned") {
        output.end_turn = Some(value);
    }
    result.map_err(CodexStreamError::from_responses_failure)?;
    assert_successful_output(output)
}

fn codex_stream_options(
    model: &Model,
    options: &OpenAICodexResponsesOptions,
    grammar_tool_input_properties: &BTreeMap<String, String>,
) -> OpenAIResponsesStreamOptions {
    let model_id = model.id.clone();
    OpenAIResponsesStreamOptions {
        service_tier: options.service_tier.map(|tier| tier.as_str().to_owned()),
        grammar_tool_input_properties: Some(grammar_tool_input_properties.clone()),
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
    cache_session_id: Option<&str>,
    grammar_tool_input_properties: &BTreeMap<String, String>,
) -> Result<Value, CodexStreamError> {
    let supports_strict = supports_strict_mode(model);
    let supports_grammar = supports_grammar_tools(model);
    let deferred_mode = deferred_tools_mode(model);
    let tool_placement = split_deferred_tools(context, deferred_mode.is_some());
    let deferred_tools: Option<BTreeMap<String, Tool>> = deferred_mode
        .is_some()
        .then(|| tool_placement.deferred.iter().cloned().collect());
    let messages = convert_responses_messages(
        model,
        context,
        &codex_tool_call_providers(),
        Some(&ConvertResponsesMessagesOptions {
            include_system_prompt: Some(false),
            grammar_tool_input_properties: Some(grammar_tool_input_properties.clone()),
            deferred_tools,
            deferred_tools_mode: deferred_mode,
            tool_options: Some(ConvertResponsesToolsOptions {
                strict: Some(None),
                supports_strict_mode: Some(supports_strict),
                supports_openai_grammar_tools: Some(supports_grammar),
                defer_loading: None,
            }),
        }),
    )
    .map_err(CodexStreamError::Transport)?;

    let mut body = Map::new();
    body.insert("model".to_owned(), Value::String(model.id.clone()));
    body.insert("store".to_owned(), Value::Bool(false));
    body.insert("stream".to_owned(), Value::Bool(true));
    body.insert(
        "instructions".to_owned(),
        Value::String(
            context
                .system_prompt
                .clone()
                .filter(|prompt| !prompt.is_empty())
                .unwrap_or_else(|| "You are a helpful assistant.".to_owned()),
        ),
    );
    body.insert("input".to_owned(), Value::Array(messages));
    body.insert(
        "text".to_owned(),
        object(vec![(
            "verbosity",
            Value::String(
                options
                    .text_verbosity
                    .map(TextVerbosity::as_str)
                    .unwrap_or("low")
                    .to_owned(),
            ),
        )]),
    );
    body.insert(
        "include".to_owned(),
        Value::Array(vec![Value::String("reasoning.encrypted_content".to_owned())]),
    );
    if let Some(cache_session_id) = cache_session_id {
        body.insert(
            "prompt_cache_key".to_owned(),
            Value::String(cache_session_id.to_owned()),
        );
    }
    body.insert(
        "tool_choice".to_owned(),
        Value::String(
            options
                .tool_choice
                .map(CodexToolChoice::as_str)
                .unwrap_or("auto")
                .to_owned(),
        ),
    );
    body.insert("parallel_tool_calls".to_owned(), Value::Bool(true));

    if let Some(temperature) = options.stream.temperature {
        body.insert("temperature".to_owned(), js_number_value(temperature));
    }
    if let Some(service_tier) = options.service_tier {
        body.insert(
            "service_tier".to_owned(),
            Value::String(service_tier.as_str().to_owned()),
        );
    }
    if !tool_placement.immediate.is_empty() {
        body.insert(
            "tools".to_owned(),
            convert_responses_tools(
                &tool_placement.immediate,
                Some(&ConvertResponsesToolsOptions {
                    strict: Some(None),
                    supports_strict_mode: Some(supports_strict),
                    supports_openai_grammar_tools: Some(supports_grammar),
                    defer_loading: None,
                }),
            )
            .map_err(|error: ConstrainedSamplingError| CodexStreamError::Transport(error.to_string()))?,
        );
    }
    if let Some(reasoning_effort) = options.reasoning_effort {
        let effort = reasoning_effort_value(model, reasoning_effort);
        if let Some(effort) = effort {
            body.insert(
                "reasoning".to_owned(),
                object(vec![
                    ("effort", Value::String(effort)),
                    (
                        "summary",
                        Value::String(
                            options
                                .reasoning_summary
                                .map(ReasoningSummary::as_str)
                                .unwrap_or("auto")
                                .to_owned(),
                        ),
                    ),
                ]),
            );
        }
    }

    Ok(Value::Object(body))
}

fn reasoning_effort_value(model: &Model, effort: CodexReasoningEffort) -> Option<String> {
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
        Some(None) => None,
        None => Some(effort.as_str().to_owned()),
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

fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key.to_owned(), value);
    }
    Value::Object(map)
}

fn js_number_value(value: f64) -> Value {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e21 {
        return Value::from(value as i64);
    }
    Value::from(value)
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
        return Err(CodexStreamError::Transport(format!("Invalid timeoutMs: {}", value)));
    }
    Ok(Some(value.floor()))
}

fn compress_request_body_zstd(body_json: &str) -> Option<Vec<u8>> {
    zstd::stream::encode_all(body_json.as_bytes(), REQUEST_COMPRESSION_ZSTD_LEVEL).ok()
}

fn is_aborted(signal: Option<&CancellationToken>) -> bool {
    signal.is_some_and(CancellationToken::is_cancelled)
}

fn set_header(headers: &mut BTreeMap<String, String>, name: &str, value: impl Into<String>) {
    headers.insert(name.to_ascii_lowercase(), value.into());
}

fn delete_header(headers: &mut BTreeMap<String, String>, name: &str) {
    headers.remove(&name.to_ascii_lowercase());
}

fn header_value<'a>(headers: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    headers.get(&name.to_ascii_lowercase()).map(String::as_str)
}

fn apply_provider_headers(headers: &mut BTreeMap<String, String>, additional: Option<&crate::types::ProviderHeaders>) {
    for (key, value) in additional.into_iter().flatten() {
        match value {
            None => delete_header(headers, key),
            Some(value) => set_header(headers, key, value.clone()),
        }
    }
}

fn build_base_codex_headers(
    init_headers: Option<&BTreeMap<String, String>>,
    additional_headers: Option<&crate::types::ProviderHeaders>,
    account_id: &str,
    token: &str,
) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    for (name, value) in init_headers.into_iter().flatten() {
        set_header(&mut headers, name, value.clone());
    }
    apply_provider_headers(&mut headers, additional_headers);
    set_header(&mut headers, "authorization", format!("Bearer {token}"));
    set_header(&mut headers, "chatgpt-account-id", account_id);
    set_header(&mut headers, "originator", "pi");
    set_header(&mut headers, "user-agent", codex_user_agent());
    headers
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
    init_headers: Option<&BTreeMap<String, String>>,
    additional_headers: Option<&crate::types::ProviderHeaders>,
    account_id: &str,
    token: &str,
    session_id: Option<&str>,
) -> BTreeMap<String, String> {
    let mut headers = build_base_codex_headers(init_headers, additional_headers, account_id, token);
    set_header(&mut headers, "openai-beta", "responses=experimental");
    set_header(&mut headers, "accept", "text/event-stream");
    set_header(&mut headers, "content-type", "application/json");
    if let Some(session_id) = session_id {
        set_header(&mut headers, "session-id", session_id);
        set_header(&mut headers, "x-client-request-id", session_id);
    }
    headers
}

fn build_websocket_headers(
    init_headers: Option<&BTreeMap<String, String>>,
    additional_headers: Option<&crate::types::ProviderHeaders>,
    account_id: &str,
    token: &str,
    request_id: &str,
) -> BTreeMap<String, String> {
    let mut headers = build_base_codex_headers(init_headers, additional_headers, account_id, token);
    delete_header(&mut headers, "accept");
    delete_header(&mut headers, "content-type");
    delete_header(&mut headers, "openai-beta");
    set_header(&mut headers, "openai-beta", OPENAI_BETA_RESPONSES_WEBSOCKETS);
    set_header(&mut headers, "x-client-request-id", request_id);
    set_header(&mut headers, "session-id", request_id);
    headers
}

fn resolve_codex_url(base_url: &str) -> String {
    let raw = if base_url.trim().is_empty() {
        DEFAULT_CODEX_BASE_URL
    } else {
        base_url
    };
    let normalized = raw.trim_end_matches('/');
    if normalized.ends_with("/codex/responses") {
        normalized.to_owned()
    } else if normalized.ends_with("/codex") {
        format!("{normalized}/responses")
    } else {
        format!("{normalized}/codex/responses")
    }
}

fn resolve_codex_websocket_url(base_url: &str) -> String {
    let url = resolve_codex_url(base_url);
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        url
    }
}

fn extract_account_id(token: &str) -> Result<String, CodexStreamError> {
    let failure = || CodexStreamError::Transport("Failed to extract accountId from token".to_owned());
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(failure());
    }
    let payload = base64::engine::general_purpose::STANDARD
        .decode(parts[1])
        .map_err(|_| failure())?;
    let payload: Value = serde_json::from_slice(&payload).map_err(|_| failure())?;
    payload
        .get(JWT_CLAIM_PATH)
        .and_then(|claim| claim.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .filter(|account_id| !account_id.is_empty())
        .map(str::to_owned)
        .ok_or_else(failure)
}

fn assert_successful_output(output: &AssistantMessage) -> Result<(), CodexStreamError> {
    match output.stop_reason {
        StopReason::Pending => Err(CodexStreamError::Transport(
            "Codex stream ended without a stop reason".to_owned(),
        )),
        StopReason::Error | StopReason::Aborted => Err(CodexStreamError::Transport(
            output
                .error_message
                .clone()
                .unwrap_or_else(|| "An unknown error occurred".to_owned()),
        )),
        _ => Ok(()),
    }
}

/// Failure raised while consuming Codex SSE/WebSocket events.
#[derive(Debug, Clone, PartialEq)]
pub enum CodexStreamError {
    Api {
        code: Option<String>,
        message: String,
        payload: Option<Value>,
    },
    Protocol {
        message: String,
        payload: Option<Value>,
    },
    Transport(String),
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

    fn is_previous_response_not_found(&self) -> bool {
        matches!(
            self,
            Self::Api {
                code: Some(code),
                ..
            } if code == PREVIOUS_RESPONSE_NOT_FOUND_CODE
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
        match self {
            Self::Api { message, .. } | Self::Protocol { message, .. } | Self::Transport(message) => {
                formatter.write_str(message)
            }
        }
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

fn get_retry_after_delay_ms(headers: &BTreeMap<String, String>) -> Option<f64> {
    if let Some(retry_after_ms) = header_value(headers, "retry-after-ms")
        && let Ok(millis) = retry_after_ms.trim().parse::<f64>()
        && millis.is_finite()
    {
        return Some(millis.max(0.0));
    }

    let retry_after = header_value(headers, "retry-after")?;
    if let Ok(seconds) = retry_after.trim().parse::<f64>()
        && seconds.is_finite()
    {
        return Some((seconds * 1000.0).max(0.0));
    }

    let date = httpdate::parse_http_date(retry_after.trim()).ok()?;
    let date_millis = date
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())?;
    Some((date_millis - now_millis()).max(0) as f64)
}

fn validate_retry_delay_ms(delay_ms: f64, options: &OpenAICodexResponsesOptions) -> Result<f64, CodexStreamError> {
    let max_retry_delay_ms = options
        .stream
        .request
        .max_retry_delay_ms
        .unwrap_or(DEFAULT_MAX_RETRY_DELAY_MS);
    if max_retry_delay_ms > 0.0 && delay_ms > max_retry_delay_ms {
        return Err(CodexStreamError::Transport(format!(
            "Server requested {}s retry delay (max: {}s)",
            (delay_ms / 1000.0).ceil(),
            (max_retry_delay_ms / 1000.0).ceil()
        )));
    }
    Ok(delay_ms)
}

async fn sleep_abortable(ms: f64, signal: Option<&CancellationToken>) -> Result<(), CodexStreamError> {
    if is_aborted(signal) {
        return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
    }
    let delay = Duration::from_secs_f64(ms.max(0.0) / 1000.0);
    match signal {
        Some(signal) => tokio::select! {
            () = tokio::time::sleep(delay) => Ok(()),
            () = signal.cancelled() => Err(CodexStreamError::Transport("Request was aborted".to_owned())),
        },
        None => {
            tokio::time::sleep(delay).await;
            Ok(())
        }
    }
}

async fn fetch_sse_with_retries(
    model: &Model,
    options: &OpenAICodexResponsesOptions,
    headers: &BTreeMap<String, String>,
    body: Vec<u8>,
    http_timeout_ms: Option<f64>,
) -> Result<(HttpResponse, CancellationToken), CodexStreamError> {
    let fetch = options.stream.request.fetch.clone().unwrap_or_else(default_fetch);
    let max_retries = options.stream.request.max_retries.unwrap_or(DEFAULT_MAX_RETRIES);
    let url = resolve_codex_url(&model.base_url);
    let mut attempt = 0.0_f64;
    let mut last_error: Option<CodexStreamError> = None;

    while attempt <= max_retries {
        if is_aborted(options.stream.request.signal.as_ref()) {
            return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
        }

        let request = HttpRequest {
            method: "POST".to_owned(),
            url: url.clone(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            body: Some(body.clone()),
        };
        let response = fetch_with_header_timeout(
            fetch.as_ref(),
            request,
            options.stream.request.signal.clone(),
            http_timeout_ms,
        )
        .await;

        let (response, request_token) = match response {
            Ok(pair) => pair,
            Err(error) => {
                if is_aborted(options.stream.request.signal.as_ref()) {
                    return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
                }
                last_error = Some(error.clone());
                if attempt < max_retries {
                    let delay_ms = BASE_DELAY_MS * 2f64.powf(attempt);
                    notify_retry(
                        options,
                        attempt + 1.0,
                        max_retries + 1.0,
                        delay_ms,
                        &error.to_string(),
                        model,
                    );
                    sleep_abortable(delay_ms, options.stream.request.signal.as_ref()).await?;
                    attempt += 1.0;
                    continue;
                }
                return Err(error);
            }
        };

        if let Some(on_response) = &options.stream.request.on_response {
            on_response(
                crate::types::ProviderResponse {
                    status: f64::from(response.status),
                    headers: response.headers.iter().cloned().collect(),
                },
                model.clone(),
            )
            .await
            .map_err(|error| CodexStreamError::Transport(error.message))?;
        }

        if (200..300).contains(&response.status) {
            return Ok((response, request_token));
        }

        let mut response = response;
        let error_text = read_all_text(response.body.as_mut()).await?;
        if attempt < max_retries && is_retryable_error(response.status, &error_text) {
            let header_map: BTreeMap<String, String> = response.headers.iter().cloned().collect();
            let delay_ms = match get_retry_after_delay_ms(&header_map) {
                Some(delay_ms) => validate_retry_delay_ms(delay_ms, options)?,
                None => BASE_DELAY_MS * 2f64.powf(attempt),
            };
            notify_retry(
                options,
                attempt + 1.0,
                max_retries + 1.0,
                delay_ms,
                &format!("HTTP {}", response.status),
                model,
            );
            sleep_abortable(delay_ms, options.stream.request.signal.as_ref()).await?;
            attempt += 1.0;
            continue;
        }

        let info = parse_error_response(response.status, &error_text);
        return Err(CodexStreamError::Transport(
            info.friendly_message.unwrap_or(info.message),
        ));
    }

    Err(last_error.unwrap_or_else(|| CodexStreamError::Transport("Failed after retries".to_owned())))
}

fn notify_retry(
    options: &OpenAICodexResponsesOptions,
    attempt: f64,
    max_attempts: f64,
    delay_ms: f64,
    reason: &str,
    model: &Model,
) {
    if let Some(on_retry) = &options.stream.on_retry {
        on_retry(
            ProviderRetryEvent {
                attempt,
                max_attempts: Some(max_attempts),
                delay_ms: Some(delay_ms),
                reason: reason.to_owned(),
                fallback_transport: None,
            },
            model,
        );
    }
}

async fn fetch_with_header_timeout(
    fetch: &dyn Fetch,
    request: HttpRequest,
    signal: Option<CancellationToken>,
    timeout_ms: Option<f64>,
) -> Result<(HttpResponse, CancellationToken), CodexStreamError> {
    if is_aborted(signal.as_ref()) {
        return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
    }
    let request_token = signal.as_ref().map(CancellationToken::child_token).unwrap_or_default();
    let future = fetch.fetch(request, Some(request_token.clone()));
    tokio::pin!(future);

    match timeout_ms.filter(|timeout| *timeout > 0.0) {
        Some(timeout_ms) => {
            let abort = async {
                match &signal {
                    Some(signal) => signal.cancelled().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                result = &mut future => map_fetch_result(result, signal.as_ref()).map(|response| (response, request_token)),
                () = tokio::time::sleep(Duration::from_secs_f64(timeout_ms / 1000.0)) => {
                    request_token.cancel();
                    Err(CodexStreamError::Transport(format!(
                        "Codex SSE response headers timed out after {timeout_ms}ms"
                    )))
                }
                () = abort => {
                    request_token.cancel();
                    Err(CodexStreamError::Transport("Request was aborted".to_owned()))
                }
            }
        }
        None => {
            let abort = async {
                match &signal {
                    Some(signal) => signal.cancelled().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                result = &mut future => map_fetch_result(result, signal.as_ref()).map(|response| (response, request_token)),
                () = abort => {
                    request_token.cancel();
                    Err(CodexStreamError::Transport("Request was aborted".to_owned()))
                }
            }
        }
    }
}

fn map_fetch_result(
    result: Result<HttpResponse, CallbackError>,
    signal: Option<&CancellationToken>,
) -> Result<HttpResponse, CodexStreamError> {
    match result {
        Ok(response) => Ok(response),
        Err(error) => {
            if is_aborted(signal) || error.message == crate::http::ABORT_ERROR_MESSAGE {
                Err(CodexStreamError::Transport("Request was aborted".to_owned()))
            } else {
                Err(CodexStreamError::Transport(error.message))
            }
        }
    }
}

async fn read_all_text(body: &mut dyn HttpBody) -> Result<String, CodexStreamError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend_from_slice(&chunk.map_err(|error| CodexStreamError::Transport(error.message))?);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

struct ParsedErrorResponse {
    message: String,
    friendly_message: Option<String>,
}

fn parse_error_response(status: u16, raw: &str) -> ParsedErrorResponse {
    let mut message = if raw.is_empty() {
        "Request failed".to_owned()
    } else {
        raw.to_owned()
    };
    let mut friendly_message = None;

    if let Ok(parsed) = serde_json::from_str::<Value>(raw)
        && let Some(error) = parsed.get("error").filter(|error| !error.is_null())
    {
        let code = error
            .get("code")
            .and_then(Value::as_str)
            .or_else(|| error.get("type").and_then(Value::as_str))
            .unwrap_or("");
        if regex::Regex::new("(?i)usage_limit_reached|usage_not_included|rate_limit_exceeded")
            .is_ok_and(|pattern| pattern.is_match(code))
            || status == 429
        {
            let plan = error
                .get("plan_type")
                .and_then(Value::as_str)
                .map(|plan| format!(" ({} plan)", plan.to_lowercase()))
                .unwrap_or_default();
            let when = error.get("resets_at").and_then(Value::as_f64).map(|resets_at| {
                let minutes = (((resets_at * 1000.0) - now_millis() as f64) / 60_000.0)
                    .round()
                    .max(0.0);
                format!(" Try again in ~{minutes} min.")
            });
            let when = when.unwrap_or_default();
            friendly_message = Some(
                format!("You have hit your ChatGPT usage limit{plan}.{when}")
                    .trim()
                    .to_owned(),
            );
        }
        if let Some(error_message) = error
            .get("message")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            message = error_message.to_owned();
        } else if let Some(friendly) = &friendly_message {
            message = friendly.clone();
        }
    }

    ParsedErrorResponse {
        message,
        friendly_message,
    }
}

enum SseStep {
    Event(Value),
    Skip,
    Incomplete,
}

struct SseEventSource {
    body: Box<dyn HttpBody>,
    buffer: Vec<u8>,
    timeout_ms: Option<f64>,
    signal: Option<CancellationToken>,
    request_token: CancellationToken,
    eof: bool,
}

impl SseEventSource {
    fn new(
        body: Box<dyn HttpBody>,
        timeout_ms: Option<f64>,
        signal: Option<CancellationToken>,
        request_token: CancellationToken,
    ) -> Self {
        Self {
            body,
            buffer: Vec::new(),
            timeout_ms,
            signal,
            request_token,
            eof: false,
        }
    }

    fn take_event(&mut self) -> Result<SseStep, CodexStreamError> {
        let Some(index) = find_subslice(&self.buffer, b"\n\n") else {
            return Ok(SseStep::Incomplete);
        };
        let chunk = self.buffer[..index].to_vec();
        self.buffer.drain(..index + 2);
        let chunk = String::from_utf8_lossy(&chunk);
        let data = chunk
            .split('\n')
            .filter(|line| line.starts_with("data:"))
            .map(|line| line[5..].trim())
            .collect::<Vec<_>>()
            .join("\n");
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return Ok(SseStep::Skip);
        }
        serde_json::from_str(data)
            .map(SseStep::Event)
            .map_err(|error| CodexStreamError::Protocol {
                message: format!("Invalid Codex SSE JSON: {error}"),
                payload: Some(Value::String(data.to_owned())),
            })
    }

    async fn read_chunk(&mut self) -> Result<Option<Vec<u8>>, CodexStreamError> {
        let signal = self.signal.clone();
        let body = self.body.as_mut();
        let read = body.next_chunk();
        tokio::pin!(read);
        let abort = async {
            match &signal {
                Some(signal) => signal.cancelled().await,
                None => std::future::pending().await,
            }
        };

        let result = match self.timeout_ms.filter(|timeout| *timeout > 0.0) {
            Some(timeout_ms) => {
                tokio::select! {
                    chunk = &mut read => chunk,
                    () = tokio::time::sleep(Duration::from_secs_f64(timeout_ms / 1000.0)) => {
                        self.request_token.cancel();
                        return Err(CodexStreamError::Transport(format!(
                            "Codex SSE body read timed out after {timeout_ms}ms"
                        )));
                    }
                    () = abort => {
                        self.request_token.cancel();
                        return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
                    }
                }
            }
            None => {
                tokio::select! {
                    chunk = &mut read => chunk,
                    () = abort => {
                        self.request_token.cancel();
                        return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
                    }
                }
            }
        };

        match result {
            Some(Ok(bytes)) => Ok(Some(bytes)),
            Some(Err(error)) => {
                if is_aborted(self.signal.as_ref()) {
                    Err(CodexStreamError::Transport("Request was aborted".to_owned()))
                } else {
                    Err(CodexStreamError::Transport(error.message))
                }
            }
            None => Ok(None),
        }
    }
}

impl Drop for SseEventSource {
    fn drop(&mut self) {
        self.request_token.cancel();
    }
}

impl ResponsesEventSource for SseEventSource {
    type Error = CodexStreamError;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<Value>, Self::Error>> {
        Box::pin(async move {
            loop {
                if is_aborted(self.signal.as_ref()) {
                    self.request_token.cancel();
                    return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
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
                    Some(bytes) => self.buffer.extend_from_slice(&bytes),
                    None => self.eof = true,
                }
            }
        })
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

struct CodexMappedSource<S> {
    inner: S,
    end_turn: Arc<Mutex<Option<bool>>>,
}

impl<S> CodexMappedSource<S> {
    fn new(inner: S, end_turn: Arc<Mutex<Option<bool>>>) -> Self {
        Self { inner, end_turn }
    }
}

impl<S> ResponsesEventSource for CodexMappedSource<S>
where
    S: ResponsesEventSource<Error = CodexStreamError> + Send,
{
    type Error = CodexStreamError;

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<Value>, Self::Error>> {
        Box::pin(async move {
            loop {
                let Some(event) = self.inner.next_event().await? else {
                    return Ok(None);
                };
                match map_codex_event(event, &self.end_turn)? {
                    Some(event) => return Ok(Some(event)),
                    None => continue,
                }
            }
        })
    }
}

fn map_codex_event(mut event: Value, end_turn: &Arc<Mutex<Option<bool>>>) -> Result<Option<Value>, CodexStreamError> {
    let Some(event_type) = event.get("type").and_then(Value::as_str).map(str::to_owned) else {
        return Ok(None);
    };

    match event_type.as_str() {
        "error" => {
            let (code, message) = extract_codex_event_error(&event);
            Err(CodexStreamError::Api {
                message: format!(
                    "Codex error: {}",
                    message
                        .or(code.clone())
                        .unwrap_or_else(|| serde_json::to_string(&event).unwrap_or_default())
                ),
                code,
                payload: Some(event),
            })
        }
        "response.failed" => {
            let response = event.get("response").cloned().unwrap_or(Value::Null);
            let code = response
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let message = response
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| "Codex response failed".to_owned());
            Err(CodexStreamError::Api {
                message,
                code,
                payload: Some(event),
            })
        }
        "response.done" | "response.completed" | "response.incomplete" => {
            let response = event.get("response").cloned().unwrap_or(Value::Null);
            if let Some(value) = response.get("end_turn").and_then(Value::as_bool) {
                *end_turn.lock().expect("codex end-turn mutex poisoned") = Some(value);
            }
            let normalized = normalize_codex_response(response);
            let object = event.as_object_mut().expect("codex event is an object");
            object.insert(
                "type".to_owned(),
                Value::String(
                    if event_type == "response.incomplete" {
                        "response.incomplete"
                    } else {
                        "response.completed"
                    }
                    .to_owned(),
                ),
            );
            object.insert("response".to_owned(), normalized);
            Ok(Some(event))
        }
        _ => Ok(Some(event)),
    }
}

fn normalize_codex_response(response: Value) -> Value {
    let Value::Object(mut object) = response else {
        return response;
    };
    let status = object
        .get("status")
        .and_then(Value::as_str)
        .filter(|status| CODEX_RESPONSE_STATUSES.contains(status))
        .map(str::to_owned);
    match status {
        Some(status) => {
            object.insert("status".to_owned(), Value::String(status));
        }
        None => {
            object.insert("status".to_owned(), Value::Null);
        }
    }
    Value::Object(object)
}

fn extract_codex_event_error(event: &Value) -> (Option<String>, Option<String>) {
    let nested = event.get("error").filter(|error| error.is_object());
    let code = event
        .get("code")
        .and_then(Value::as_str)
        .or_else(|| nested.and_then(|error| error.get("code")).and_then(Value::as_str))
        .map(str::to_owned);
    let message = event
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| nested.and_then(|error| error.get("message")).and_then(Value::as_str))
        .map(str::to_owned);
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
    idle_cancel: CancellationToken,
    continuation: Option<CachedWebSocketContinuationState>,
}

#[derive(Clone, Default)]
struct CachedWebSocketContinuationState {
    last_request_body: Value,
    last_response_id: String,
    last_response_items: Vec<Value>,
}

struct WsLease {
    connection: Option<WsConnection>,
    cache_key: Option<(String, String)>,
    entry: Option<Arc<Mutex<CacheEntry>>>,
    reused: bool,
}

impl WsLease {
    fn take_connection(&mut self) -> Option<WsConnection> {
        self.connection.take()
    }
}

type WebSocketSessionCache = BTreeMap<String, BTreeMap<String, Arc<Mutex<CacheEntry>>>>;

fn websocket_session_cache() -> &'static Mutex<WebSocketSessionCache> {
    static CACHE: OnceLock<Mutex<WebSocketSessionCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn websocket_debug_stats_store() -> &'static Mutex<BTreeMap<String, OpenAICodexWebSocketDebugStats>> {
    static STATS: OnceLock<Mutex<BTreeMap<String, OpenAICodexWebSocketDebugStats>>> = OnceLock::new();
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
pub fn get_openai_codex_websocket_debug_stats(session_id: &str) -> Option<OpenAICodexWebSocketDebugStats> {
    websocket_debug_stats_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(session_id)
        .cloned()
}

/// `resetOpenAICodexWebSocketDebugStats`.
pub fn reset_openai_codex_websocket_debug_stats(session_id: Option<&str>) {
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

fn update_websocket_debug_stats(session_id: &str, update: impl FnOnce(&mut OpenAICodexWebSocketDebugStats)) {
    let mut stats = websocket_debug_stats_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = stats.entry(session_id.to_owned()).or_default();
    update(entry);
}

/// `closeOpenAICodexWebSocketSessions`.
pub fn close_openai_codex_websocket_sessions(session_id: Option<&str>) {
    let mut cache = cache_lock();
    let mut sockets = Vec::new();
    match session_id {
        Some(session_id) => {
            if let Some(account_entries) = cache.remove(session_id) {
                for entry in account_entries.into_values() {
                    let mut entry = cache_entry_mutex(&entry);
                    entry.idle_cancel.cancel();
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
                    entry.idle_cancel.cancel();
                    if let Some(socket) = entry.socket.take() {
                        sockets.push(socket);
                    }
                }
            }
        }
    }
    drop(cache);
    // Synchronous cleanup drops the connections; the TypeScript close frame is
    // not observable through this API.
    drop(sockets);
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

fn record_websocket_failure(session_id: Option<&str>, error: &CodexStreamError) {
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
    url: &str,
    headers: &BTreeMap<String, String>,
    signal: Option<CancellationToken>,
    connect_timeout_ms: Option<f64>,
) -> Result<WsConnection, CodexStreamError> {
    if is_aborted(signal.as_ref()) {
        return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
    }
    let mut request = url
        .into_client_request()
        .map_err(|error| CodexStreamError::Transport(error.to_string()))?;
    for (name, value) in headers {
        let name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|error| CodexStreamError::Transport(error.to_string()))?;
        let value = HeaderValue::from_str(value).map_err(|error| CodexStreamError::Transport(error.to_string()))?;
        request.headers_mut().insert(name, value);
    }

    let connect = tokio_tungstenite::connect_async(request);
    tokio::pin!(connect);
    let abort = async {
        match &signal {
            Some(signal) => signal.cancelled().await,
            None => std::future::pending().await,
        }
    };
    let timeout_ms = connect_timeout_ms.unwrap_or(DEFAULT_WEBSOCKET_CONNECT_TIMEOUT_MS);

    let result = if timeout_ms > 0.0 {
        tokio::select! {
            result = &mut connect => Some(result),
            () = tokio::time::sleep(Duration::from_secs_f64(timeout_ms / 1000.0)) => {
                return Err(CodexStreamError::Transport(format!(
                    "WebSocket connect timeout after {timeout_ms}ms"
                )));
            }
            () = abort => {
                return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
            }
        }
    } else {
        tokio::select! {
            result = &mut connect => Some(result),
            () = abort => {
                return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
            }
        }
    };

    match result {
        Some(Ok((stream, _response))) => Ok(WsConnection::new(stream)),
        Some(Err(error)) => {
            if is_aborted(signal.as_ref()) {
                Err(CodexStreamError::Transport("Request was aborted".to_owned()))
            } else {
                Err(CodexStreamError::Transport(websocket_error_message(&error)))
            }
        }
        None => Err(CodexStreamError::Transport("WebSocket error".to_owned())),
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

fn take_cached_connection(session_id: &str, account_id: &str) -> AcquireDecision {
    let mut cache = cache_lock();
    let Some(entry) = cache
        .get(session_id)
        .and_then(|entries| entries.get(account_id))
        .cloned()
    else {
        return AcquireDecision::Fresh;
    };
    let mut entry_state = cache_entry_mutex(&entry);
    entry_state.idle_cancel.cancel();
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
    url: &str,
    headers: &BTreeMap<String, String>,
    session_id: Option<&str>,
    account_id: &str,
    signal: Option<CancellationToken>,
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

fn insert_cached_entry(session_id: &str, account_id: &str) -> Arc<Mutex<CacheEntry>> {
    let entry = Arc::new(Mutex::new(CacheEntry {
        socket: None,
        busy: true,
        created_at: tokio::time::Instant::now(),
        idle_cancel: CancellationToken::new(),
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
    session_id: &str,
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
            entry_state.idle_cancel.cancel();
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
        entry_state.idle_cancel = CancellationToken::new();
        entry_state.idle_cancel.clone()
    };
    schedule_session_websocket_expiry(session_id, account_id, entry, idle_cancel);
}

fn schedule_session_websocket_expiry(
    session_id: String,
    account_id: String,
    entry: Arc<Mutex<CacheEntry>>,
    idle_cancel: CancellationToken,
) {
    tokio::spawn(async move {
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(SESSION_WEBSOCKET_CACHE_TTL_MS)) => {}
            () = idle_cancel.cancelled() => return,
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
    context: &'a Context,
    options: &'a OpenAICodexResponsesOptions,
    body: Value,
    body_json: &'a str,
    headers: BTreeMap<String, String>,
    http_timeout_ms: Option<f64>,
    websocket_connect_timeout_ms: Option<f64>,
    cache_session_id: Option<&'a str>,
    account_id: &'a str,
    grammar_tool_input_properties: &'a BTreeMap<String, String>,
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
    let mut retried_missing_continuation = false;

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
                    return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
                }
                return assert_successful_output(output);
            }
            Err(error) => {
                let aborted = is_aborted(execution.options.stream.request.signal.as_ref());
                let started = websocket_started.load(Ordering::SeqCst);
                let connection_limit_before_start = !started && error.is_websocket_connection_limit_reached();
                if !aborted && error.is_previous_response_not_found() && !retried_missing_continuation {
                    retried_missing_continuation = true;
                    continue;
                }
                if !aborted && connection_limit_before_start && !retried_connection_limit {
                    retried_connection_limit = true;
                    if let Some(on_retry) = &execution.options.stream.on_retry {
                        on_retry(
                            ProviderRetryEvent {
                                attempt: 1.0,
                                max_attempts: None,
                                delay_ms: None,
                                reason: "WebSocket connection limit reached, reconnecting".to_owned(),
                                fallback_transport: None,
                            },
                            execution.model,
                        );
                    }
                    continue;
                }
                if aborted || (error.is_non_transport() && !connection_limit_before_start) {
                    return Err(if aborted {
                        CodexStreamError::Transport("Request was aborted".to_owned())
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
            None => execution.body.clone(),
        }
    } else {
        execution.body.clone()
    };
    let body = &execution.body;

    if let Some(session_id) = execution.cache_session_id {
        let reused = lease.reused;
        let request_items = request_body.get("input").and_then(Value::as_array).map_or(0, Vec::len) as u64;
        let previous_response_id = request_body
            .get("previous_response_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let store_true = request_body.get("store").and_then(Value::as_bool) == Some(true);
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
            if store_true {
                stats.store_true_requests += 1;
            }
            stats.last_input_items = request_items;
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
    }

    let mut connection = lease.take_connection().ok_or_else(|| {
        CodexStreamError::Transport("WebSocket transport is not available in this runtime".to_owned())
    })?;
    let stream = connection.take_stream().ok_or_else(|| {
        CodexStreamError::Transport("WebSocket transport is not available in this runtime".to_owned())
    })?;
    let (mut sink, split_stream) = stream.split();
    let frame = response_create_frame(&request_body);
    let frame_json = serde_json::to_string(&frame).map_err(|error| CodexStreamError::Transport(error.to_string()))?;
    if let Err(error) = sink.send(Message::text(frame_json)).await {
        connection.closed = true;
        release_websocket(lease, None, false).await;
        return Err(CodexStreamError::Transport(websocket_error_message(&error)));
    }

    let end_turn = Arc::new(Mutex::new(None));
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
            end_turn: end_turn.clone(),
        },
    );
    let result = process_responses_stream(
        &mut source,
        output,
        sender,
        execution.model,
        Some(&codex_stream_options(
            execution.model,
            execution.options,
            execution.grammar_tool_input_properties,
        )),
    )
    .await;
    let mut connection = source.into_connection();
    if let Some(value) = *end_turn.lock().expect("codex end-turn mutex poisoned") {
        output.end_turn = Some(value);
    }

    let mut keep_connection = true;
    let mut failure = None;
    match result {
        Ok(()) => {
            if output.raw_stop_reason.as_deref() == Some("incomplete.max_output_tokens") {
                failure = Some(CodexStreamError::Transport(
                    "Incomplete response returned, reason: max_output_tokens".to_owned(),
                ));
            } else if is_aborted(execution.options.stream.request.signal.as_ref()) {
                keep_connection = false;
            } else if use_cached_context
                && let Some(entry) = &entry
                && let Some(response_id) = &output.response_id
            {
                let items = convert_response_items(
                    execution.model,
                    execution.context,
                    output,
                    execution.grammar_tool_input_properties,
                );
                let continuation = CachedWebSocketContinuationState {
                    last_request_body: body.clone(),
                    last_response_id: response_id.clone(),
                    last_response_items: items,
                };
                cache_entry_mutex(entry).continuation = Some(continuation);
            }
        }
        Err(error) => {
            keep_connection = false;
            failure = Some(CodexStreamError::from_responses_failure(error));
        }
    }

    if failure.is_some() {
        if let Some(entry) = &entry {
            cache_entry_mutex(entry).continuation = None;
        }
        connection.closed = true;
    }

    release_websocket(lease, Some(connection), keep_connection).await;
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
fn response_create_frame(body: &Value) -> Value {
    let mut frame = Map::new();
    frame.insert("type".to_owned(), Value::String("response.create".to_owned()));
    if let Value::Object(object) = body {
        for (key, value) in object {
            frame.insert(key.clone(), value.clone());
        }
    }
    Value::Object(frame)
}

fn convert_response_items(
    model: &Model,
    _context: &Context,
    output: &AssistantMessage,
    grammar_tool_input_properties: &BTreeMap<String, String>,
) -> Vec<Value> {
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
            grammar_tool_input_properties: Some(grammar_tool_input_properties.clone()),
            deferred_tools: None,
            deferred_tools_mode: None,
            tool_options: None,
        }),
    )
    .map(|items| {
        items
            .into_iter()
            .filter(|item| {
                !matches!(
                    item.get("type").and_then(Value::as_str),
                    Some("function_call_output" | "custom_tool_call_output")
                )
            })
            .collect()
    })
    .unwrap_or_default()
}

fn request_body_without_input(body: &Value) -> Value {
    let Value::Object(object) = body else {
        return body.clone();
    };
    let mut rest = object.clone();
    rest.remove("input");
    rest.remove("previous_response_id");
    Value::Object(rest)
}

fn request_bodies_match_except_input(a: &Value, b: &Value) -> bool {
    request_body_without_input(a) == request_body_without_input(b)
}

fn response_inputs_equal(a: &[Value], b: &[Value]) -> bool {
    a == b
}

fn cached_websocket_input_delta(body: &Value, continuation: &CachedWebSocketContinuationState) -> Option<Vec<Value>> {
    if !request_bodies_match_except_input(body, &continuation.last_request_body) {
        return None;
    }
    let current = body.get("input").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut baseline = continuation
        .last_request_body
        .get("input")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    baseline.extend(continuation.last_response_items.clone());
    if current.len() < baseline.len() {
        return None;
    }
    if !response_inputs_equal(&current[..baseline.len()], &baseline) {
        return None;
    }
    Some(current[baseline.len()..].to_vec())
}

fn build_cached_websocket_request_body(entry: &mut CacheEntry, body: &Value) -> Value {
    let Some(continuation) = entry.continuation.clone() else {
        return body.clone();
    };
    let Some(delta) = cached_websocket_input_delta(body, &continuation) else {
        entry.continuation = None;
        return body.clone();
    };
    if continuation.last_response_id.is_empty() {
        entry.continuation = None;
        return body.clone();
    }
    let Value::Object(mut object) = body.clone() else {
        return body.clone();
    };
    object.insert(
        "previous_response_id".to_owned(),
        Value::String(continuation.last_response_id),
    );
    object.insert("input".to_owned(), Value::Array(delta));
    Value::Object(object)
}

enum WsWait {
    Message(Option<Result<Message, tokio_tungstenite::tungstenite::Error>>),
    Ping,
    Idle,
    Aborted,
}

struct WsEventSourceConfig {
    signal: Option<CancellationToken>,
    idle_timeout_ms: Option<f64>,
    sender: AssistantMessageEventSender,
    partial: AssistantMessage,
    websocket_started: Arc<AtomicBool>,
    start_emitted: Arc<AtomicBool>,
    end_turn: Arc<Mutex<Option<bool>>>,
}

struct WsEventSource {
    sink: SplitSink<WsStream, Message>,
    stream: SplitStream<WsStream>,
    signal: Option<CancellationToken>,
    idle_timeout_ms: Option<f64>,
    queue: VecDeque<Value>,
    done: bool,
    failed: Option<CodexStreamError>,
    saw_completion: bool,
    closed: bool,
    first_event: bool,
    sender: AssistantMessageEventSender,
    partial: AssistantMessage,
    websocket_started: Arc<AtomicBool>,
    start_emitted: Arc<AtomicBool>,
    end_turn: Arc<Mutex<Option<bool>>>,
    ping_interval: tokio::time::Interval,
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
            end_turn: config.end_turn,
            ping_interval: tokio::time::interval_at(
                tokio::time::Instant::now() + Duration::from_millis(WEBSOCKET_PING_INTERVAL_MS),
                Duration::from_millis(WEBSOCKET_PING_INTERVAL_MS),
            ),
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
        let parsed: Value = match serde_json::from_str(text) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.fail(CodexStreamError::Protocol {
                    message: format!("Invalid Codex WebSocket JSON: {error}"),
                    payload: Some(Value::String(text.to_owned())),
                });
                return;
            }
        };
        let event_type = parsed.get("type").and_then(Value::as_str).unwrap_or("");
        let terminal = matches!(
            event_type,
            "response.completed" | "response.done" | "response.incomplete"
        );
        if terminal {
            self.saw_completion = true;
        }
        match map_codex_event(parsed, &self.end_turn) {
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
                Some(signal) => signal.cancelled().await,
                None => std::future::pending().await,
            }
        };
        match self.idle_timeout_ms.filter(|timeout| *timeout > 0.0) {
            Some(idle_timeout_ms) => {
                tokio::select! {
                    message = self.stream.next() => WsWait::Message(message),
                    _ = self.ping_interval.tick() => WsWait::Ping,
                    () = tokio::time::sleep(Duration::from_secs_f64(idle_timeout_ms / 1000.0)) => WsWait::Idle,
                    () = abort => WsWait::Aborted,
                }
            }
            None => {
                tokio::select! {
                    message = self.stream.next() => WsWait::Message(message),
                    _ = self.ping_interval.tick() => WsWait::Ping,
                    () = abort => WsWait::Aborted,
                }
            }
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

    fn next_event(&mut self) -> BoxFuture<'_, Result<Option<Value>, Self::Error>> {
        Box::pin(async move {
            loop {
                if is_aborted(self.signal.as_ref()) {
                    self.closed = true;
                    return Err(CodexStreamError::Transport("Request was aborted".to_owned()));
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

                match self.wait().await {
                    WsWait::Message(Some(Ok(Message::Text(text)))) => {
                        self.handle_text(text.as_str());
                    }
                    WsWait::Message(Some(Ok(Message::Binary(bytes)))) => {
                        let text = String::from_utf8_lossy(&bytes);
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
                        self.fail(CodexStreamError::Transport(format!("WebSocket error: {error}")));
                    }
                    WsWait::Message(None) => {
                        self.closed = true;
                        if !self.saw_completion {
                            self.fail(CodexStreamError::Transport("WebSocket closed".to_owned()));
                        }
                        self.done = true;
                    }
                    WsWait::Ping => {
                        let _ = self.sink.send(Message::Ping(Vec::new().into())).await;
                    }
                    WsWait::Idle => {
                        let timeout_ms = self.idle_timeout_ms.unwrap_or_default();
                        self.close_with(1000, "idle_timeout").await;
                        self.fail(CodexStreamError::Transport(format!(
                            "WebSocket idle timeout after {timeout_ms}ms"
                        )));
                    }
                    WsWait::Aborted => {
                        self.closed = true;
                        self.fail(CodexStreamError::Transport("Request was aborted".to_owned()));
                    }
                }
            }
        })
    }
}

fn close_error(frame: Option<&CloseFrame>) -> CodexStreamError {
    let Some(frame) = frame else {
        return CodexStreamError::Transport("WebSocket closed".to_owned());
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
    CodexStreamError::Transport(format!("WebSocket closed{code_text}{reason_text}").trim().to_owned())
}
