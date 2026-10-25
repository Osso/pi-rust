//! Port of `packages/ai/src/api/mistral-conversations.ts`.
//!
//! Streams the native Mistral Chat Completions endpoint. The TypeScript module
//! builds a camelCase payload before its SDK serializes the wire shape.
//! This port keeps callback values separate from SDK defaults, field order,
//! and camelCase-to-snake_case remapping.

use crate::api::openai_sdk::{ingress_metadata_text, validate_metadata_fields};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::{JsObject, JsValue};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use pi_js_compat::abort::{AbortReason, AbortSignal};
use serde_json::{Map, Value, json};
use url::Url;

use crate::api::lazy::{LazyApi, lazy_api};
use crate::api::simple_options::{build_base_options, model_thinking_level};
use crate::api::transform_messages::transform_messages_with;
use crate::models::{calculate_cost, clamp_thinking_level};
use crate::request::{CallbackError, HttpRequest, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, DoneReason, ErrorReason,
    ImageContent, Message, Modality, Model, ModelThinkingLevel, StopReason, TextContent, ThinkingContent, Tool,
    ToolCall, ToolResultContent, UserContent,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::hash::short_hash;
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::sanitize_unicode::sanitize_surrogates;
use pi_js_compat::number::{js_number_to_string, js_number_value};

const MISTRAL_TOOL_CALL_ID_LENGTH: usize = 9;
const MAX_MISTRAL_ERROR_BODY_CHARS: usize = 4000;
const DEFAULT_TIMEOUT_MS: f64 = 60_000.0;

/// Provider-specific tool-choice selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MistralToolChoice {
    Auto,
    None,
    Any,
    Required,
    Function { name: String },
}

/// Provider-specific reasoning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MistralPromptMode {
    Reasoning,
}

/// Standard effort tags and native strings forwarded by frozen's map cast.
pub type MistralReasoningEffort = JsString;

/// Provider-specific options for the Mistral API (`MistralOptions`).
#[derive(Clone, Default)]
pub struct MistralOptions {
    pub stream: StreamOptions,
    pub tool_choice: Option<MistralToolChoice>,
    pub prompt_mode: Option<MistralPromptMode>,
    pub reasoning_effort: Option<MistralReasoningEffort>,
}

impl MistralOptions {
    /// Parses the typed Mistral fields from `StreamOptions::extra`.
    pub fn from_stream_options(options: StreamOptions) -> Self {
        let tool_choice = options.extra.get("toolChoice").and_then(parse_tool_choice);
        let prompt_mode = match options.extra.get("promptMode").and_then(Value::as_str) {
            Some("reasoning") => Some(MistralPromptMode::Reasoning),
            _ => None,
        };
        let reasoning_effort = options
            .extra
            .get("reasoningEffort")
            .and_then(Value::as_str)
            .map(Into::into);
        Self {
            stream: options,
            tool_choice,
            prompt_mode,
            reasoning_effort,
        }
    }
}

fn parse_tool_choice(value: &Value) -> Option<MistralToolChoice> {
    match value {
        Value::String(choice) => match choice.as_str() {
            "auto" => Some(MistralToolChoice::Auto),
            "none" => Some(MistralToolChoice::None),
            "any" => Some(MistralToolChoice::Any),
            "required" => Some(MistralToolChoice::Required),
            _ => None,
        },
        Value::Object(object) => {
            let name = object.get("function")?.get("name")?.as_str()?;
            Some(MistralToolChoice::Function { name: name.to_owned() })
        }
        _ => None,
    }
}

/// Mistral API implementation as a `ProviderStreams` module.
#[derive(Debug, Clone, Copy, Default)]
pub struct MistralConversationsApi;

impl ProviderStreams for MistralConversationsApi {
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(stream(model, context, MistralOptions::from_stream_options(options)))
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, crate::request::CallbackError> {
        stream_simple(model, context, options)
    }
}

/// Stream responses from the native Mistral Chat Completions endpoint.
pub fn stream(model: Model, context: Context, options: MistralOptions) -> AssistantMessageEventStream {
    let (sender, event_stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        run_stream(model, context, options, sender).await;
    });
    event_stream
}

async fn run_stream(model: Model, context: Context, options: MistralOptions, sender: AssistantMessageEventSender) {
    let mut output = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis() as f64);
    match produce_stream(&model, &context, &options, &sender, &mut output).await {
        Ok(()) => {}
        Err(error) => {
            for block in &mut output.content {
                if let AssistantContent::ToolCall(block) = block {
                    block.partial_args = None;
                }
            }
            let aborted = user_aborted(&options);
            output.stop_reason = if aborted {
                StopReason::Aborted
            } else {
                StopReason::Error
            };
            output.error_message = Some(format_mistral_error(&error));
            sender.push(AssistantMessageEvent::Error {
                reason: if aborted {
                    ErrorReason::Aborted
                } else {
                    ErrorReason::Error
                },
                error: output.clone(),
            });
            sender.end(Some(output));
        }
    }
}

async fn produce_stream(
    model: &Model,
    context: &Context,
    options: &MistralOptions,
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
) -> Result<(), MistralError> {
    let api_key = required_api_key(model, &options.stream)?;
    let normalizer = Mutex::new(MistralToolCallIdNormalizer::default());
    let normalize = |id: &str, _model: &Model, _source: &AssistantMessage| {
        normalizer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .normalize(id)
    };
    let transformed = transform_messages_with(&context.messages, model, Some(&normalize), now_millis() as f64);

    let mut payload = build_chat_payload(model, context, &transformed, options)?;
    if let Some(on_payload) = &options.stream.request.on_payload
        && let Some(next_payload) = on_payload(payload.clone(), model.clone())
            .await
            .map_err(MistralError::Callback)?
    {
        payload = next_payload;
    }

    let response = request_mistral_stream(model, &payload, api_key, options).await?;
    sender.push(AssistantMessageEvent::Start {
        partial: output.clone(),
    });
    consume_chat_stream(model, output, sender, response.body, &response.signal).await?;

    if user_aborted(options) {
        return Err(MistralError::message("Request was aborted"));
    }
    if matches!(output.stop_reason, StopReason::Aborted | StopReason::Error) {
        return Err(MistralError::message(
            output
                .error_message
                .clone()
                .unwrap_or_else(|| "An unknown error occurred".to_owned().into()),
        ));
    }

    let reason = done_reason(output.stop_reason)
        .ok_or_else(|| MistralError::message("Mistral stream ended without a finish reason"))?;
    sender.push(AssistantMessageEvent::Done {
        reason,
        message: output.clone(),
    });
    sender.end(Some(output.clone()));
    Ok(())
}

fn required_api_key<'a>(model: &Model, options: &'a StreamOptions) -> Result<&'a JsString, MistralError> {
    options
        .request
        .api_key
        .as_ref()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| MistralError::message(format!("No API key for provider: {}", model.provider)))
}

fn user_aborted(options: &MistralOptions) -> bool {
    options.stream.request.signal.as_ref().is_some_and(AbortSignal::aborted)
}

fn done_reason(reason: StopReason) -> Option<DoneReason> {
    match reason {
        StopReason::Stop => Some(DoneReason::Stop),
        StopReason::Length => Some(DoneReason::Length),
        StopReason::ToolUse => Some(DoneReason::ToolUse),
        StopReason::Error | StopReason::Aborted => None,
    }
}

/// Maps provider-agnostic `SimpleStreamOptions` to Mistral options.
pub fn stream_simple(
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> Result<AssistantMessageEventStream, CallbackError> {
    let api_key = options.stream.request.api_key.clone().filter(|key| !key.is_empty());
    let Some(api_key) = api_key else {
        return Err(CallbackError::new(format!(
            "No API key for provider: {}",
            model.provider
        )));
    };

    let base = build_base_options(&model, &context, Some(&options), Some(&api_key));
    let reasoning = options.reasoning.and_then(|level| {
        let clamped = clamp_thinking_level(&model, model_thinking_level(level));
        (clamped != ModelThinkingLevel::Off).then_some(clamped)
    });
    let should_use_reasoning = model.reasoning && reasoning.is_some();

    Ok(stream(
        model.clone(),
        context,
        MistralOptions {
            stream: base,
            tool_choice: None,
            prompt_mode: (should_use_reasoning && uses_prompt_mode_reasoning(&model))
                .then_some(MistralPromptMode::Reasoning),
            reasoning_effort: (should_use_reasoning && uses_reasoning_effort(&model))
                .then(|| map_reasoning_effort(&model, reasoning.unwrap_or(ModelThinkingLevel::High))),
        },
    ))
}

fn uses_reasoning_effort(model: &Model) -> bool {
    matches!(
        model.id.as_str(),
        "mistral-small-2603" | "mistral-small-latest" | "mistral-medium-3.5"
    )
}

fn uses_prompt_mode_reasoning(model: &Model) -> bool {
    model.reasoning && !uses_reasoning_effort(model)
}

fn map_reasoning_effort(model: &Model, level: ModelThinkingLevel) -> MistralReasoningEffort {
    let mapped = model
        .thinking_level_map
        .as_ref()
        .and_then(|map| map.get(&level))
        .and_then(|value| value.as_ref());
    // Frozen633: ?? preserves every string, including empty and unmatched units.
    mapped.cloned().unwrap_or_else(|| "high".into())
}

#[derive(Default)]
struct MistralToolCallIdNormalizer {
    ids: BTreeMap<String, String>,
    owners: BTreeMap<String, String>,
}

impl MistralToolCallIdNormalizer {
    fn normalize(&mut self, id: &str) -> String {
        if let Some(existing) = self.ids.get(id) {
            return existing.clone();
        }
        let mut attempt = 0;
        loop {
            let candidate = derive_mistral_tool_call_id(id, attempt);
            match self.owners.get(&candidate) {
                Some(owner) if owner != id => attempt += 1,
                _ => {
                    self.ids.insert(id.to_owned(), candidate.clone());
                    self.owners.insert(candidate.clone(), id.to_owned());
                    return candidate;
                }
            }
        }
    }
}

fn derive_mistral_tool_call_id(id: &str, attempt: u32) -> String {
    let normalized: String = id.chars().filter(char::is_ascii_alphanumeric).collect();
    if attempt == 0 && normalized.len() == MISTRAL_TOOL_CALL_ID_LENGTH {
        return normalized;
    }
    let seed_base = if normalized.is_empty() { id } else { &normalized };
    let seed = if attempt == 0 {
        seed_base.to_owned()
    } else {
        format!("{seed_base}:{attempt}")
    };
    short_hash(&seed)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MISTRAL_TOOL_CALL_ID_LENGTH)
        .collect()
}

#[derive(Debug, Clone)]
enum MistralError {
    Message(JsString),
    Callback(CallbackError),
    Http {
        status: u16,
        content_type: String,
        body: JsString,
    },
}

impl MistralError {
    fn message(message: impl Into<JsString>) -> Self {
        Self::Message(message.into())
    }
}

fn format_mistral_error(error: &MistralError) -> JsString {
    match error {
        MistralError::Http {
            status,
            body,
            content_type,
        } => {
            let body_text = body.trim();
            if body_text.is_empty() {
                let message = format_mistral_empty_http_error(*status, content_type, body);
                return pi_js_compat::js_format!("Mistral API error ({}): {}", status, message);
            }
            JsString::join(
                [
                    format!("Mistral API error ({status}): ").into(),
                    truncate_error_text(&body_text, MAX_MISTRAL_ERROR_BODY_CHARS),
                ],
                "",
            )
        }
        MistralError::Message(message) => message.clone(),
        MistralError::Callback(error) => format_mistral_callback_error(error),
    }
}

fn format_mistral_callback_error(error: &CallbackError) -> JsString {
    let reason = error.thrown_value();
    let native = match reason {
        AbortReason::Error(error) | AbortReason::DomException(error) => error,
        AbortReason::Value(_) => return error.error_or_json_message(),
    };
    let fields = native.value();
    let Some(status) = fields.get("statusCode").and_then(JsValue::as_f64) else {
        return native.message;
    };
    let body = fields.get("body").and_then(JsValue::as_string).map(JsString::trim);
    let message = match body.filter(|body| !body.is_empty()) {
        Some(body) => truncate_error_text(&body, MAX_MISTRAL_ERROR_BODY_CHARS),
        None => native.message,
    };
    pi_js_compat::js_format!("Mistral API error ({}): {}", js_number_to_string(status), message)
}

fn truncate_error_text(text: &JsString, max_chars: usize) -> JsString {
    crate::utils::error_body::truncate_error_text(text, max_chars)
}

struct MistralResponse {
    body: Box<dyn crate::request::HttpBody>,
    signal: RequestSignal,
}

fn format_mistral_empty_http_error(status: u16, content_type: &str, body: &JsString) -> JsString {
    let mut message = format!("API error occurred: Status {status}");
    let content_type = if content_type.is_empty() { "\"\"" } else { content_type };
    if content_type != "application/json" {
        message.push_str(" Content-Type ");
        if content_type.contains(' ') {
            message.push_str(&format!("\"{content_type}\""));
        } else {
            message.push_str(content_type);
        }
    }
    message.push_str(if body.len() > 100 { "\n" } else { ". " });
    message.push_str("Body: ");
    let display = if body.is_empty() { "\"\"".into() } else { body.clone() };
    (JsString::from(message) + display).trim()
}

async fn request_mistral_stream(
    model: &Model,
    payload: &JsValue,
    api_key: &JsString,
    options: &MistralOptions,
) -> Result<MistralResponse, MistralError> {
    let url = chat_completions_url(model)?;
    let headers = build_mistral_headers(model, api_key, options);
    let wire_payload = to_mistral_wire_payload(payload)?;
    let body = serde_json::to_vec(&wire_payload).map_err(|error| MistralError::message(error.to_string()))?;
    let signal = RequestSignal::start(options.stream.request.signal.clone())?;
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
                    .map(|(name, value)| crate::request::header_value(value).map(|value| (name.clone(), value)))
                    .collect::<Result<_, _>>()
                    .map_err(|error| MistralError::message(error.message))?,
                body: Some(body),
            },
            Some(signal.token.clone()),
        )
        .await
        .map_err(format_mistral_request_rejection)?;
    if !(200..300).contains(&response.status) {
        let body = read_response_text(response.body, &signal).await?;
        return Err(MistralError::Http {
            status: response.status,
            content_type: response
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
            body: body.into(),
        });
    }

    Ok(MistralResponse {
        body: response.body,
        signal,
    })
}

fn chat_completions_url(model: &Model) -> Result<Url, MistralError> {
    let serialized =
        crate::request::request_url(&model.base_url).map_err(|error| MistralError::message(error.message))?;
    let mut base = Url::parse(&serialized).expect("request_url parsed URL");
    let path = base.path().trim_end_matches('/');
    base.set_path(&format!("{path}/"));
    base.join("v1/chat/completions")
        .map_err(|error| MistralError::message(error.to_string()))
}

fn build_mistral_headers(model: &Model, api_key: &JsString, options: &MistralOptions) -> IndexMap<String, JsString> {
    let mut headers = IndexMap::from([
        ("accept".to_owned(), JsString::from("text/event-stream")),
        (
            "authorization".to_owned(),
            pi_js_compat::js_format!("Bearer {}", api_key),
        ),
        ("content-type".to_owned(), JsString::from("application/json")),
        ("cookie".to_owned(), JsString::default()),
    ]);
    headers.extend(mistral_user_headers(model, options));
    headers.insert("user-agent".to_owned(), "mistral-client-typescript/2.2.6".into());
    headers
}

fn mistral_user_headers(model: &Model, options: &MistralOptions) -> IndexMap<String, JsString> {
    let mut original: IndexMap<String, Option<JsString>> = model
        .headers
        .iter()
        .flatten()
        .map(|(name, value)| (name.clone(), Some(value.clone())))
        .collect();
    for (name, value) in options.stream.request.headers.iter().flatten() {
        original.insert(name.clone(), value.clone());
    }
    if should_use_prompt_caching(options)
        && original
            .get("x-affinity")
            .and_then(Option::as_ref)
            .is_none_or(JsString::is_empty)
    {
        original.insert("x-affinity".to_owned(), options.stream.session_id.clone());
    }
    let mut normalized: IndexMap<String, JsString> = IndexMap::new();
    for (name, value) in original {
        let value = value.unwrap_or_else(|| "null".into());
        let name = name.to_ascii_lowercase();
        normalized
            .entry(name.clone())
            .and_modify(|existing| {
                existing.push_str(if name == "cookie" { "; " } else { ", " });
                existing.push_str(&value);
            })
            .or_insert(value);
    }
    normalized
}

fn should_use_prompt_caching(options: &MistralOptions) -> bool {
    options.stream.cache_retention != Some(CacheRetention::None)
        && options
            .stream
            .session_id
            .as_ref()
            .is_some_and(|session_id| !session_id.is_empty())
}

fn to_mistral_wire_payload(payload: &JsValue) -> Result<JsValue, MistralError> {
    let Some(payload) = payload.as_object() else {
        return Err(MistralError::message("Mistral payload must be an object"));
    };
    let mut wire = payload.clone();
    for (source, target) in [
        ("topP", "top_p"),
        ("maxTokens", "max_tokens"),
        ("randomSeed", "random_seed"),
        ("responseFormat", "response_format"),
        ("toolChoice", "tool_choice"),
        ("presencePenalty", "presence_penalty"),
        ("frequencyPenalty", "frequency_penalty"),
        ("parallelToolCalls", "parallel_tool_calls"),
        ("reasoningEffort", "reasoning_effort"),
        ("promptMode", "prompt_mode"),
        ("promptCacheKey", "prompt_cache_key"),
        ("safePrompt", "safe_prompt"),
    ] {
        remap_mistral_property(&mut wire, source, target);
    }

    let messages = wire
        .get("messages")
        .and_then(JsValue::as_array)
        .ok_or_else(|| MistralError::message("Mistral payload messages must be an array"))?
        .clone();
    let messages: Vec<JsValue> = messages.iter().map(to_mistral_wire_message).collect::<Result<_, _>>()?;
    wire.insert("messages".into(), JsValue::Array(messages));
    if let Some(JsValue::Array(tools)) = wire.get("tools") {
        let tools = tools.iter().map(to_mistral_wire_tool).collect::<Result<_, _>>()?;
        wire.insert("tools".into(), JsValue::Array(tools));
    }

    if let Some(JsValue::Object(response_format)) = wire.get("response_format").cloned() {
        let mut wire_response_format = response_format;
        remap_mistral_property(&mut wire_response_format, "jsonSchema", "json_schema");
        if let Some(JsValue::Object(json_schema)) = wire_response_format.get("json_schema").cloned() {
            let mut wire_json_schema = json_schema;
            remap_mistral_property(&mut wire_json_schema, "schemaDefinition", "schema");
            wire_response_format.insert("json_schema".into(), JsValue::Object(wire_json_schema));
        }
        wire.insert("response_format".into(), JsValue::Object(wire_response_format));
    }

    Ok(JsValue::Object(order_mistral_sdk_fields(
        wire,
        &[
            "model",
            "temperature",
            "top_p",
            "max_tokens",
            "stream",
            "stop",
            "random_seed",
            "metadata",
            "messages",
            "response_format",
            "tools",
            "tool_choice",
            "presence_penalty",
            "frequency_penalty",
            "n",
            "prediction",
            "parallel_tool_calls",
            "reasoning_effort",
            "prompt_mode",
            "guardrails",
            "prompt_cache_key",
            "safe_prompt",
        ],
    )))
}

fn order_mistral_sdk_fields(mut record: JsObject, keys: &[&str]) -> JsObject {
    keys.iter()
        .filter_map(|key| record.shift_remove(*key).map(|value| ((*key).into(), value)))
        .collect()
}

fn to_mistral_wire_tool(tool: &JsValue) -> Result<JsValue, MistralError> {
    let mut wire = tool
        .as_object()
        .cloned()
        .ok_or_else(|| MistralError::message("Mistral tool must be an object"))?;
    if let Some(JsValue::Object(function)) = wire.get_mut("function") {
        *function = order_mistral_sdk_fields(
            std::mem::take(function),
            &["name", "description", "strict", "parameters"],
        );
    }
    Ok(JsValue::Object(order_mistral_sdk_fields(wire, &["type", "function"])))
}

fn to_mistral_wire_tool_call(call: &JsValue) -> Result<JsValue, MistralError> {
    let mut wire = call
        .as_object()
        .cloned()
        .ok_or_else(|| MistralError::message("Mistral tool call must be an object"))?;
    wire.entry("id".into())
        .or_insert_with(|| JsValue::String("null".into()));
    wire.entry("index".into()).or_insert_with(|| JsValue::from(json!(0)));
    Ok(JsValue::Object(order_mistral_sdk_fields(
        wire,
        &["id", "type", "function", "index"],
    )))
}

fn to_mistral_wire_message(message: &JsValue) -> Result<JsValue, MistralError> {
    let Some(message) = message.as_object() else {
        return Err(MistralError::message("Mistral message must be an object"));
    };
    let mut wire = message.clone();
    remap_mistral_property(&mut wire, "toolCalls", "tool_calls");
    remap_mistral_property(&mut wire, "toolCallId", "tool_call_id");
    if let Some(JsValue::Array(calls)) = wire.get("tool_calls") {
        let calls = calls.iter().map(to_mistral_wire_tool_call).collect::<Result<_, _>>()?;
        wire.insert("tool_calls".into(), JsValue::Array(calls));
    }
    if let Some(JsValue::Array(content)) = wire.get("content").cloned() {
        let content: Vec<JsValue> = content
            .iter()
            .map(to_mistral_wire_content_chunk)
            .collect::<Result<_, _>>()?;
        wire.insert("content".into(), JsValue::Array(content));
    }
    let keys: &[&str] = match wire
        .get("role")
        .and_then(JsValue::as_string)
        .and_then(|role| role.to_utf8().ok())
        .as_deref()
    {
        Some("assistant") => {
            wire.entry("prefix".into()).or_insert(JsValue::Bool(false));
            &["role", "content", "tool_calls", "prefix"]
        }
        Some("tool") => &["role", "content", "tool_call_id", "name"],
        _ => &["role", "content"],
    };
    Ok(JsValue::Object(order_mistral_sdk_fields(wire, keys)))
}

fn to_mistral_wire_content_chunk(chunk: &JsValue) -> Result<JsValue, MistralError> {
    let Some(chunk) = chunk.as_object() else {
        return Err(MistralError::message("Mistral content chunk must be an object"));
    };
    let mut wire = chunk.clone();
    for (source, target) in [
        ("imageUrl", "image_url"),
        ("documentUrl", "document_url"),
        ("documentName", "document_name"),
        ("fileId", "file_id"),
        ("referenceIds", "reference_ids"),
        ("inputAudio", "input_audio"),
    ] {
        remap_mistral_property(&mut wire, source, target);
    }
    Ok(JsValue::Object(wire))
}

fn remap_mistral_property(record: &mut JsObject, source: &str, target: &str) {
    // TypeScript `delete` preserves the remaining insertion order; serde_json's
    // `remove` is `swap_remove` and would move the last entry into the hole.
    let Some(value) = record.shift_remove(source) else {
        return;
    };
    record.insert(target.into(), value);
}

fn build_chat_payload(
    model: &Model,
    context: &Context,
    messages: &[Message],
    options: &MistralOptions,
) -> Result<JsValue, MistralError> {
    let mut payload = JsObject::new();
    payload.insert("model".into(), JsValue::String(model.id.clone().into()));
    payload.insert("stream".into(), JsValue::Bool(true));
    payload.insert(
        "messages".into(),
        JsValue::from(Value::Array(to_chat_messages(
            messages,
            model.input.contains(&Modality::Image),
        )?)),
    );

    if let Some(tools) = &context.tools
        && !tools.is_empty()
    {
        payload.insert("tools".into(), JsValue::Array(to_function_tools(tools)));
    }
    if let Some(temperature) = options.stream.temperature {
        payload.insert("temperature".into(), js_number_value(temperature).into());
    }
    if let Some(max_tokens) = options.stream.max_tokens {
        payload.insert("maxTokens".into(), js_number_value(max_tokens).into());
    }
    if let Some(tool_choice) = &options.tool_choice {
        payload.insert("toolChoice".into(), tool_choice_value(tool_choice).into());
    }
    if options.prompt_mode == Some(MistralPromptMode::Reasoning) {
        payload.insert("promptMode".into(), JsValue::String("reasoning".into()));
    }
    // Frozen257 applies truthiness after the nullable mapping.
    if let Some(reasoning_effort) = options.reasoning_effort.as_ref().filter(|effort| !effort.is_empty()) {
        payload.insert("reasoningEffort".into(), JsValue::String(reasoning_effort.clone()));
    }

    if let Some(system_prompt) = &context.system_prompt
        && !system_prompt.is_empty()
        && let Some(JsValue::Array(entries)) = payload.get_mut("messages")
    {
        entries.insert(
            0,
            pi_js_compat::js_json!({"role": "system", "content": sanitize_surrogates(system_prompt)}),
        );
    }

    let mut payload = JsValue::Object(payload);
    if should_use_prompt_caching(options) {
        payload.as_object_mut().expect("payload object").insert(
            "promptCacheKey".into(),
            JsValue::String(options.stream.session_id.clone().unwrap_or_default()),
        );
    }
    Ok(payload)
}

fn tool_choice_value(choice: &MistralToolChoice) -> Value {
    match choice {
        MistralToolChoice::Auto => Value::String("auto".to_owned()),
        MistralToolChoice::None => Value::String("none".to_owned()),
        MistralToolChoice::Any => Value::String("any".to_owned()),
        MistralToolChoice::Required => Value::String("required".to_owned()),
        MistralToolChoice::Function { name } => {
            json!({"type": "function", "function": {"name": name}})
        }
    }
}

fn to_function_tools(tools: &[Tool]) -> Vec<JsValue> {
    tools
        .iter()
        .map(|tool| {
            pi_js_compat::js_json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                    "strict": false,
                }
            })
        })
        .collect()
}

fn to_chat_messages(messages: &[Message], supports_images: bool) -> Result<Vec<Value>, MistralError> {
    let mut result = Vec::new();
    for message in messages {
        match message {
            Message::User(user) => to_user_chat_message(user, supports_images, &mut result),
            Message::Assistant(assistant) => to_assistant_chat_message(assistant, &mut result),
            Message::ToolResult(tool_result) => {
                let mut tool_content = vec![json!({
                    "type": "text",
                    "text": build_tool_result_text(tool_result, supports_images),
                })];
                if supports_images {
                    for part in &tool_result.content {
                        if let ToolResultContent::Image(image) = part {
                            tool_content.push(image_content_chunk(image));
                        }
                    }
                }
                result.push(json!({
                    "role": "tool",
                    "toolCallId": tool_result.tool_call_id,
                    "name": tool_result.tool_name,
                    "content": tool_content,
                }));
            }
        }
    }
    Ok(result)
}

fn to_user_chat_message(user: &crate::types::UserMessage, supports_images: bool, result: &mut Vec<Value>) {
    match &user.content {
        UserContent::Text(text) => {
            result.push(json!({"role": "user", "content": sanitize_surrogates(text)}));
        }
        UserContent::Blocks(blocks) => {
            let had_images = blocks
                .iter()
                .any(|item| matches!(item, crate::types::TextOrImageContent::Image(_)));
            let content: Vec<Value> = blocks
                .iter()
                .filter(|item| supports_images || matches!(item, crate::types::TextOrImageContent::Text(_)))
                .map(|item| match item {
                    crate::types::TextOrImageContent::Text(text) => {
                        json!({"type": "text", "text": sanitize_surrogates(&text.text)})
                    }
                    crate::types::TextOrImageContent::Image(image) => image_content_chunk(image),
                })
                .collect();
            if !content.is_empty() {
                result.push(json!({"role": "user", "content": content}));
            } else if had_images && !supports_images {
                result.push(json!({
                    "role": "user",
                    "content": "(image omitted: model does not support images)",
                }));
            }
        }
    }
}

fn image_content_chunk(image: &ImageContent) -> Value {
    json!({
        "type": "image_url",
        "imageUrl": format!("data:{};base64,{}", image.mime_type, image.data),
    })
}

fn to_assistant_chat_message(assistant: &AssistantMessage, result: &mut Vec<Value>) {
    let mut content_parts = Vec::new();
    let mut tool_calls = Vec::new();
    for block in &assistant.content {
        match block {
            AssistantContent::Text(text) => {
                if !text.text.trim().is_empty() {
                    content_parts.push(json!({
                        "type": "text",
                        "text": sanitize_surrogates(&text.text),
                    }));
                }
            }
            AssistantContent::Thinking(thinking) => {
                if !thinking.thinking.trim().is_empty() {
                    content_parts.push(json!({
                        "type": "thinking",
                        "thinking": [{
                            "type": "text",
                            "text": sanitize_surrogates(&thinking.thinking),
                        }],
                    }));
                }
            }
            AssistantContent::ToolCall(tool_call) => {
                tool_calls.push(json!({
                    "id": tool_call.id,
                    "type": "function",
                    "function": {
                        "name": tool_call.name,
                        "arguments": pi_js_compat::js_json::stringify_lossless(&JsValue::Object(tool_call.arguments.clone())),
                    },
                }));
            }
        }
    }

    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String("assistant".to_owned()));
    if !content_parts.is_empty() {
        message.insert("content".to_owned(), Value::Array(content_parts));
    }
    if !tool_calls.is_empty() {
        message.insert("toolCalls".to_owned(), Value::Array(tool_calls));
    }
    if message.len() > 1 {
        result.push(Value::Object(message));
    }
}

fn build_tool_result_text(tool_result: &crate::types::ToolResultMessage, supports_images: bool) -> JsString {
    let text = tool_result
        .content
        .iter()
        .filter_map(|part| match part {
            ToolResultContent::Text(text) => Some(sanitize_surrogates(&text.text)),
            ToolResultContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let has_images = tool_result
        .content
        .iter()
        .any(|part| matches!(part, ToolResultContent::Image(_)));
    let trimmed = JsString::from(text).trim();
    let error_prefix = if tool_result.is_error { "[tool error] " } else { "" };

    if !trimmed.is_empty() {
        let image_suffix = if has_images && !supports_images {
            "\n[tool image omitted: model does not support images]"
        } else {
            ""
        };
        return pi_js_compat::js_format!("{}{}{}", error_prefix, trimmed, image_suffix);
    }

    if has_images {
        if supports_images {
            return if tool_result.is_error {
                "[tool error] (see attached image)"
            } else {
                "(see attached image)"
            }
            .into();
        }
        return if tool_result.is_error {
            "[tool error] (image omitted: model does not support images)"
        } else {
            "(image omitted: model does not support images)"
        }
        .into();
    }

    if tool_result.is_error {
        "[tool error] (no tool output)"
    } else {
        "(no tool output)"
    }
    .into()
}

// --- Streaming -------------------------------------------------------------

async fn consume_chat_stream(
    model: &Model,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    mut body: Box<dyn crate::request::HttpBody>,
    signal: &RequestSignal,
) -> Result<(), MistralError> {
    let mut state = StreamState::default();
    let mut decoder = Utf8StreamDecoder::new();
    let mut buffer = String::new();
    let mut finished = false;
    let mut sentinel = false;

    while !finished {
        if signal.aborted() {
            return Err(MistralError::message(signal.error_message()));
        }
        let chunk = tokio::select! {
            _ = signal.token.wait() => {
                return Err(MistralError::message(signal.error_message()));
            }
            chunk = body.next_chunk() => chunk,
        };
        match chunk {
            Some(Ok(bytes)) => buffer.push_str(&decoder.push(&bytes, false)),
            Some(Err(error)) => {
                return Err(MistralError::message(if signal.aborted() {
                    signal.error_message()
                } else {
                    error.error_or_json_message()
                }));
            }
            None => {
                buffer.push_str(&decoder.push(&[], true));
                finished = true;
            }
        }
        while let Some((index, length)) = find_mistral_event_boundary(&buffer) {
            let raw = buffer[..index].to_owned();
            buffer.drain(..index + length);
            if process_mistral_event(model, output, sender, &mut state, &raw)? {
                sentinel = true;
                break;
            }
        }
        if sentinel {
            break;
        }
    }

    if !sentinel && !buffer.trim().is_empty() {
        process_mistral_event(model, output, sender, &mut state, &buffer)?;
    }

    finish_current_block(output, sender, state.current_block.as_ref());
    finalize_tool_blocks(output, sender, &state.tool_blocks);
    Ok(())
}

#[derive(Default)]
struct StreamState {
    current_block: Option<CurrentBlock>,
    tool_blocks: Vec<(String, usize)>,
}

/// Applies one parsed event; `true` means the `[DONE]` sentinel ended the stream.
fn process_mistral_event(
    model: &Model,
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    state: &mut StreamState,
    raw: &str,
) -> Result<bool, MistralError> {
    let chunk = match parse_mistral_event(raw)? {
        ParsedEvent::Event(chunk) => chunk,
        ParsedEvent::Done => return Ok(true),
        ParsedEvent::Empty => return Ok(false),
    };
    crate::api::openai_sdk::validate_chat_metadata(&chunk).map_err(MistralError::message)?;
    if let Some(content) = chunk
        .get("choices")
        .and_then(JsValue::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(JsValue::as_array)
    {
        for item in content {
            validate_metadata_fields(item, &["type"]).map_err(MistralError::message)?;
        }
    }
    update_response_id(output, &chunk);
    update_usage(model, output, &chunk);

    let Some(choice) = chunk
        .get("choices")
        .and_then(JsValue::as_array)
        .and_then(|choices| choices.first())
    else {
        return Ok(false);
    };
    apply_finish_reason(output, choice);

    let Some(delta) = choice.get("delta") else {
        return Ok(false);
    };
    if let Some(content) = delta.get("content").filter(|value| !value.is_null()) {
        consume_content_delta(output, sender, content, &mut state.current_block);
    }
    if let Some(tool_calls) = delta.get("tool_calls").and_then(JsValue::as_array) {
        for tool_call in tool_calls {
            consume_tool_call_delta(output, sender, tool_call, state);
        }
    }
    Ok(false)
}

#[derive(Clone, Copy)]
enum CurrentBlock {
    Text,
    Thinking,
}

fn block_index(output: &AssistantMessage) -> usize {
    output.content.len().saturating_sub(1)
}

fn finish_current_block(output: &AssistantMessage, sender: &AssistantMessageEventSender, block: Option<&CurrentBlock>) {
    let Some(block) = block else {
        return;
    };
    let index = block_index(output);
    match block {
        CurrentBlock::Text => {
            let content = match output.content.get(index) {
                Some(AssistantContent::Text(text)) => text.text.clone(),
                _ => Default::default(),
            };
            sender.push(AssistantMessageEvent::TextEnd {
                content_index: index,
                content,
                partial: output.clone(),
            });
        }
        CurrentBlock::Thinking => {
            let content = match output.content.get(index) {
                Some(AssistantContent::Thinking(thinking)) => thinking.thinking.clone(),
                _ => String::new().into(),
            };
            sender.push(AssistantMessageEvent::ThinkingEnd {
                content_index: index,
                content,
                partial: output.clone(),
            });
        }
    }
}

fn start_block(
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    current_block: &mut Option<CurrentBlock>,
    block: CurrentBlock,
    content: AssistantContent,
) {
    finish_current_block(output, sender, current_block.as_ref());
    output.content.push(content);
    let index = block_index(output);
    match block {
        CurrentBlock::Text => sender.push(AssistantMessageEvent::TextStart {
            content_index: index,
            partial: output.clone(),
        }),
        CurrentBlock::Thinking => sender.push(AssistantMessageEvent::ThinkingStart {
            content_index: index,
            partial: output.clone(),
        }),
    }
    *current_block = Some(block);
}

fn consume_content_delta(
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    content: &JsValue,
    current_block: &mut Option<CurrentBlock>,
) {
    match content {
        JsValue::String(text) => append_text_delta(output, sender, text, current_block),
        JsValue::Array(items) => {
            for item in items {
                match item {
                    JsValue::String(text) => append_text_delta(output, sender, text, current_block),
                    JsValue::Object(object) => {
                        let item_type = object.get("type").and_then(ingress_metadata_text).unwrap_or_default();
                        if item_type == "thinking" {
                            append_thinking_delta(output, sender, object, current_block);
                        } else if item_type == "text" {
                            let text = object
                                .get("text")
                                .and_then(JsValue::as_string)
                                .cloned()
                                .unwrap_or_default();
                            append_text_delta(output, sender, &text, current_block);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn append_text_delta(
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    text: &pi_js_compat::js_json::JsString,
    current_block: &mut Option<CurrentBlock>,
) {
    if !matches!(current_block, Some(CurrentBlock::Text)) {
        start_block(
            output,
            sender,
            current_block,
            CurrentBlock::Text,
            AssistantContent::Text(TextContent::new("")),
        );
    }
    let index = block_index(output);
    let text_delta = sanitize_surrogates(text);
    if let Some(AssistantContent::Text(block)) = output.content.get_mut(index) {
        block.text.push_str(&text_delta);
    }
    sender.push(AssistantMessageEvent::TextDelta {
        content_index: index,
        delta: text_delta.into(),
        partial: output.clone(),
    });
}

fn append_thinking_delta(
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    object: &JsObject,
    current_block: &mut Option<CurrentBlock>,
) {
    let delta_text = object
        .get("thinking")
        .and_then(JsValue::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(JsValue::as_string))
                .filter(|text| !text.is_empty())
                .fold(pi_js_compat::js_json::JsString::default(), |mut text, part| {
                    text.push_str(part);
                    text
                })
        })
        .unwrap_or_default();
    let thinking_delta = sanitize_surrogates(&delta_text);
    if thinking_delta.is_empty() {
        return;
    }
    if !matches!(current_block, Some(CurrentBlock::Thinking)) {
        start_block(
            output,
            sender,
            current_block,
            CurrentBlock::Thinking,
            AssistantContent::Thinking(ThinkingContent::new("")),
        );
    }
    let index = block_index(output);
    if let Some(AssistantContent::Thinking(block)) = output.content.get_mut(index) {
        block.thinking.push_str(&thinking_delta);
    }
    sender.push(AssistantMessageEvent::ThinkingDelta {
        content_index: index,
        delta: thinking_delta.into(),
        partial: output.clone(),
    });
}

fn consume_tool_call_delta(
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    tool_call: &JsValue,
    state: &mut StreamState,
) {
    if state.current_block.is_some() {
        finish_current_block(output, sender, state.current_block.as_ref());
        state.current_block = None;
    }

    let id = tool_call.get("id").and_then(ingress_metadata_text).unwrap_or_default();
    let call_id = if !id.is_empty() && id != "null" {
        id.to_owned()
    } else {
        let index = tool_call.get("index").and_then(JsValue::as_f64).unwrap_or(0.0);
        derive_mistral_tool_call_id(&format!("toolcall:{}", js_number_to_string(index)), 0)
    };
    let index_value = tool_call.get("index").and_then(JsValue::as_f64).unwrap_or(0.0);
    let key = format!("{call_id}:{}", js_number_to_string(index_value));

    let existing = state
        .tool_blocks
        .iter()
        .find(|(existing_key, _)| existing_key == &key)
        .map(|(_, index)| *index);
    let block_index = match existing {
        Some(index) if matches!(output.content.get(index), Some(AssistantContent::ToolCall(_))) => index,
        _ => {
            let function = tool_call.get("function");
            let name = function
                .and_then(|function| function.get("name"))
                .and_then(ingress_metadata_text)
                .unwrap_or_default();
            let mut block = ToolCall::new(call_id, name, Default::default());
            block.partial_args = Some(Default::default());
            output.content.push(AssistantContent::ToolCall(block));
            let index = output.content.len() - 1;
            state.tool_blocks.push((key.clone(), index));
            sender.push(AssistantMessageEvent::ToolCallStart {
                content_index: index,
                partial: output.clone(),
            });
            index
        }
    };

    let function = tool_call.get("function");
    let args_delta = match function.and_then(|function| function.get("arguments")) {
        Some(JsValue::String(arguments)) => arguments.clone(),
        Some(arguments) => pi_js_compat::js_json::stringify_lossless(arguments).into(),
        None => "{}".into(),
    };
    if let Some(AssistantContent::ToolCall(block)) = output.content.get_mut(block_index) {
        let buffer = block.partial_args.get_or_insert_default();
        buffer.push_str(&args_delta);
        block.arguments = arguments_object(parse_streaming_json(Some(buffer)));
    }
    sender.push(AssistantMessageEvent::ToolCallDelta {
        content_index: block_index,
        delta: args_delta,
        partial: output.clone(),
    });
}

fn arguments_object(value: JsValue) -> pi_js_compat::js_json::JsObject {
    match value {
        JsValue::Object(object) => object,
        _ => Default::default(),
    }
}

fn finalize_tool_blocks(
    output: &mut AssistantMessage,
    sender: &AssistantMessageEventSender,
    tool_blocks: &[(String, usize)],
) {
    for (_, index) in tool_blocks {
        let index = *index;
        if !matches!(output.content.get(index), Some(AssistantContent::ToolCall(_))) {
            continue;
        }
        if let Some(AssistantContent::ToolCall(block)) = output.content.get_mut(index) {
            block.arguments = arguments_object(parse_streaming_json(block.partial_args.as_ref()));
            block.partial_args = None;
        }
        let tool_call = match output.content.get(index) {
            Some(AssistantContent::ToolCall(tool_call)) => tool_call.clone(),
            _ => continue,
        };
        sender.push(AssistantMessageEvent::ToolCallEnd {
            content_index: index,
            tool_call: Box::new(tool_call),
            partial: output.clone(),
        });
    }
}

fn update_response_id(output: &mut AssistantMessage, chunk: &JsValue) {
    if output.response_id.as_deref().is_none_or(str::is_empty)
        && let Some(id) = chunk.get("id").and_then(ingress_metadata_text).as_deref()
        && !id.is_empty()
    {
        output.response_id = Some(id.to_owned());
    }
}

fn update_usage(model: &Model, output: &mut AssistantMessage, chunk: &JsValue) {
    let Some(usage) = chunk.get("usage").filter(|usage| usage.is_object()) else {
        return;
    };
    let prompt_tokens = usage.get("prompt_tokens").and_then(JsValue::as_f64).unwrap_or(0.0);
    let cached_prompt_tokens = cached_prompt_tokens(usage, prompt_tokens);
    output.usage.input = (prompt_tokens - cached_prompt_tokens).max(0.0);
    output.usage.output = usage.get("completion_tokens").and_then(JsValue::as_f64).unwrap_or(0.0);
    output.usage.cache_read = cached_prompt_tokens;
    output.usage.cache_write = 0.0;
    let total_tokens = usage.get("total_tokens").and_then(JsValue::as_f64).unwrap_or(0.0);
    output.usage.total_tokens = if total_tokens == 0.0 {
        output.usage.input + output.usage.output + output.usage.cache_read + output.usage.cache_write
    } else {
        total_tokens
    };
    output.usage.cost = calculate_cost(model, &output.usage);
}

fn cached_prompt_tokens(usage: &JsValue, prompt_tokens: f64) -> f64 {
    let raw = [
        usage
            .get("promptTokensDetails")
            .and_then(|details| details.get("cachedTokens")),
        usage
            .get("prompt_tokens_details")
            .and_then(|details| details.get("cached_tokens")),
        usage
            .get("promptTokenDetails")
            .and_then(|details| details.get("cachedTokens")),
        usage
            .get("prompt_token_details")
            .and_then(|details| details.get("cached_tokens")),
        usage.get("numCachedTokens"),
        usage.get("num_cached_tokens"),
    ]
    .into_iter()
    .flatten()
    .find_map(JsValue::as_f64)
    .unwrap_or(0.0);
    let cached = if raw.is_finite() { raw } else { 0.0 };
    prompt_tokens.min(cached.max(0.0))
}

fn apply_finish_reason(output: &mut AssistantMessage, choice: &JsValue) {
    let Some(reason) = choice.get("finish_reason").and_then(ingress_metadata_text) else {
        return;
    };
    if reason.is_empty() {
        return;
    }
    match reason.as_str() {
        "stop" => output.stop_reason = StopReason::Stop,
        "length" | "model_length" => output.stop_reason = StopReason::Length,
        "tool_calls" => output.stop_reason = StopReason::ToolUse,
        "error" => output.stop_reason = StopReason::Error,
        _ => output.stop_reason = StopReason::Stop,
    }
}

struct Utf8StreamDecoder {
    pending: Vec<u8>,
    at_start: bool,
}

impl Utf8StreamDecoder {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            at_start: true,
        }
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
        if self.at_start && !output.is_empty() {
            self.at_start = false;
            if let Some(text) = output.strip_prefix('\u{feff}') {
                output = text.to_owned();
            }
        }
        output
    }
}

fn find_mistral_event_boundary(buffer: &str) -> Option<(usize, usize)> {
    const PATTERNS: [&str; 8] = [
        "\r\n\r\n", "\r\n\r", "\r\n\n", "\r\r\n", "\n\r\n", "\r\r", "\n\r", "\n\n",
    ];
    let bytes = buffer.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\r' && *byte != b'\n' {
            continue;
        }
        for pattern in PATTERNS {
            if bytes[index..].starts_with(pattern.as_bytes()) {
                return Some((index, pattern.len()));
            }
        }
    }
    None
}

enum ParsedEvent {
    Event(JsValue),
    Done,
    Empty,
}

fn parse_mistral_event(raw: &str) -> Result<ParsedEvent, MistralError> {
    let data = raw
        .split(['\r', '\n'])
        .filter(|line| line.starts_with("data:"))
        .map(|line| line[5..].trim_start())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    if data.is_empty() {
        return Ok(ParsedEvent::Empty);
    }
    if data == "[DONE]" {
        return Ok(ParsedEvent::Done);
    }
    let parsed: JsValue = pi_js_compat::js_json::parse_lossless(&data)
        .map_err(|error| malformed_mistral_json_error(error.into_message()))?;
    validate_mistral_chunk_header(&parsed)?;
    Ok(ParsedEvent::Event(parsed))
}

fn malformed_mistral_json_error(message: JsString) -> MistralError {
    let message = pi_js_compat::js_format!("malformed json: SyntaxError: {}", message);
    let mut issues = vec![pi_js_compat::js_json!({"code":"custom","message":message,"path":["data"]})];
    for (field, expected) in [("id", "string"), ("model", "string"), ("choices", "array")] {
        issues.push(schema_type_issue(field, expected, None).into());
    }
    MistralError::message(pi_js_compat::js_json::stringify_lossless_pretty(&JsValue::Array(
        issues,
    )))
}

fn validate_mistral_chunk_header(chunk: &JsValue) -> Result<(), MistralError> {
    let schema = [
        ("id", "string", false),
        ("object", "string", true),
        ("created", "int", true),
        ("model", "string", false),
        ("choices", "array", false),
    ];
    let mut issues: Vec<_> = schema
        .into_iter()
        .filter_map(|(field, expected, optional)| {
            let value = chunk.get(field);
            if optional && value.is_none() {
                return None;
            }
            chunk_header_issue(field, expected, value)
        })
        .collect();
    if let Some(choices) = chunk.get("choices").and_then(JsValue::as_array) {
        for (index, choice) in choices.iter().enumerate() {
            append_mistral_choice_issues(choice, index, &mut issues);
        }
    }
    if issues.is_empty() {
        return Ok(());
    }
    Err(MistralError::message(
        serde_json::to_string_pretty(&issues).expect("schema errors serialize"),
    ))
}

fn append_mistral_choice_issues(choice: &JsValue, index: usize, issues: &mut Vec<Value>) {
    for (field, expected) in [("index", "int"), ("delta", "object")] {
        let issue = if expected == "int" {
            safe_integer_header_issue(field, choice.get(field))
        } else {
            (!choice.get(field).is_some_and(JsValue::is_object))
                .then(|| schema_type_issue(field, expected, choice.get(field)))
        };
        if let Some(mut issue) = issue {
            issue["path"] = serde_json::json!(["data", "choices", index, field]);
            issues.push(issue);
        }
    }
    let finish = choice.get("finish_reason");
    if finish.is_some_and(|value| value.is_null() || value.as_string().is_some()) {
        return;
    }
    let mut branches = ["stop", "length", "error", "tool_calls"]
        .into_iter()
        .map(|value| {
            vec![serde_json::json!({"code":"invalid_value","values":[value],"path":[],
            "message":format!("Invalid input: expected \"{value}\"")})]
        })
        .collect::<Vec<_>>();
    let mut string_issue = schema_type_issue("finish_reason", "string", finish);
    string_issue["path"] = serde_json::json!([]);
    branches.push(vec![string_issue]);
    issues.push(serde_json::json!({"code":"invalid_union","errors":branches,
        "path":["data","choices",index,"finish_reason"],"message":"Invalid input"}));
}

fn chunk_header_issue(field: &str, expected: &str, value: Option<&JsValue>) -> Option<Value> {
    let valid = match expected {
        "string" => value.is_some_and(|value| value.as_string().is_some()),
        "array" => value.is_some_and(JsValue::is_array),
        "int" => return safe_integer_header_issue(field, value),
        _ => unreachable!("header schema kind"),
    };
    (!valid).then(|| schema_type_issue(field, expected, value))
}

fn safe_integer_header_issue(field: &str, value: Option<&JsValue>) -> Option<Value> {
    const MAX_SAFE_INTEGER: f64 = 9007199254740991.0;
    let Some(number) = value.and_then(JsValue::as_f64).filter(|number| number.fract() == 0.0) else {
        return Some(schema_type_issue(field, "int", value));
    };
    if number.abs() <= MAX_SAFE_INTEGER {
        return None;
    }
    let issue = if number > 0.0 {
        serde_json::json!({"code":"too_big","maximum":9007199254740991u64,
            "note":"Integers must be within the safe integer range.","origin":"int",
            "path":["data",field],"message":"Too big: expected int to be <9007199254740991"})
    } else {
        serde_json::json!({"code":"too_small","minimum":-9007199254740991i64,
            "note":"Integers must be within the safe integer range.","origin":"int",
            "path":["data",field],"message":"Too small: expected int to be >-9007199254740991"})
    };
    Some(issue)
}

fn schema_type_issue(field: &str, expected: &str, value: Option<&JsValue>) -> Value {
    let received = schema_received_type(value);
    let mut issue = serde_json::Map::new();
    issue.insert("expected".into(), serde_json::json!(expected));
    if expected == "int" {
        issue.insert("format".into(), serde_json::json!("safeint"));
    }
    issue.insert("code".into(), serde_json::json!("invalid_type"));
    issue.insert("path".into(), serde_json::json!(["data", field]));
    issue.insert(
        "message".into(),
        serde_json::json!(format!("Invalid input: expected {expected}, received {received}")),
    );
    Value::Object(issue)
}

fn schema_received_type(value: Option<&JsValue>) -> &'static str {
    match value {
        None => "undefined",
        Some(JsValue::Null) => "null",
        Some(JsValue::Bool(_)) => "boolean",
        Some(JsValue::Number(_)) => "number",
        Some(JsValue::String(_)) => "string",
        Some(JsValue::Array(_)) => "array",
        Some(JsValue::Object(_)) => "object",
    }
}

async fn read_response_text(
    mut body: Box<dyn crate::request::HttpBody>,
    signal: &RequestSignal,
) -> Result<String, MistralError> {
    let mut decoder = Utf8StreamDecoder::new();
    let mut text = String::new();
    loop {
        if signal.aborted() {
            return Err(MistralError::message(signal.error_message()));
        }
        let chunk = tokio::select! {
            _ = signal.token.wait() => {
                return Err(MistralError::message(signal.error_message()));
            }
            chunk = body.next_chunk() => chunk,
        };
        match chunk {
            Some(Ok(bytes)) => text.push_str(&decoder.push(&bytes, false)),
            Some(Err(error)) => {
                return Err(MistralError::message(if signal.aborted() {
                    signal.error_message()
                } else {
                    error.error_or_json_message()
                }));
            }
            None => {
                text.push_str(&decoder.push(&[], true));
                return Ok(text);
            }
        }
    }
}

fn mistral_error_to_string(reason: &AbortReason) -> Result<JsString, MistralError> {
    match reason {
        AbortReason::Value(value) => pi_js_compat::js_json::to_js_string(value).map_err(MistralError::message),
        AbortReason::Error(error) | AbortReason::DomException(error) => {
            if error.name.is_empty() {
                return Ok(error.message.clone());
            }
            if error.message.is_empty() {
                return Ok(error.name.clone());
            }
            Ok(JsString::join(
                [error.name.clone(), ": ".into(), error.message.clone()],
                "",
            ))
        }
    }
}

fn mistral_rejection_prefix(reason: &AbortReason) -> &'static str {
    let value = reason.value();
    let name = value.get("name").and_then(JsValue::as_string);
    let code = value.get("code");
    let string_code = code.and_then(JsValue::as_string).and_then(|code| code.to_utf8().ok());
    let generic_abort = string_code
        .as_deref()
        .is_some_and(|code| code.eq_ignore_ascii_case("econnaborted"));
    if name.is_some_and(|name| name == "AbortError") || code == Some(&JsValue::Number(20.0)) || generic_abort {
        return "Request aborted by client";
    }
    if name.is_some_and(|name| name == "TimeoutError") || code == Some(&JsValue::Number(23.0)) {
        return "Request timed out";
    }
    let native_fetch_error = match reason {
        AbortReason::Error(error) if error.is_instance_of(pi_js_compat::node_error::NativeErrorClass::TypeError) => {
            let message = error.message.to_lowercase();
            message.starts_with("fetch failed") || message.starts_with("failed to fetch")
        }
        _ => false,
    };
    if native_fetch_error
        || name.is_some_and(|name| name == "ConnectionError")
        || string_code
            .as_deref()
            .is_some_and(|code| code.eq_ignore_ascii_case("econnreset"))
    {
        return "Unable to make request";
    }
    "Unexpected HTTP client error"
}

fn format_mistral_request_rejection(error: crate::request::CallbackError) -> MistralError {
    let reason = error.thrown_value();
    let prefix = mistral_rejection_prefix(&reason);
    if !pi_js_compat::js_json::is_truthy(&reason.value()) {
        return MistralError::message(prefix);
    }
    match mistral_error_to_string(&reason) {
        Ok(cause) => MistralError::message(JsString::join([prefix.into(), ": ".into(), cause], "")),
        Err(error) => error,
    }
}

/// WHATWG Request's dependent signal; a supplied signal disables the SDK's default deadline.
struct RequestSignal {
    token: AbortSignal,
}

impl RequestSignal {
    fn start(user: Option<AbortSignal>) -> Result<Self, MistralError> {
        let token = match user {
            Some(user) => AbortSignal::any(&[user]),
            None => AbortSignal::timeout(DEFAULT_TIMEOUT_MS).map_err(|error| MistralError::message(error.message))?,
        };
        Ok(Self { token })
    }

    fn aborted(&self) -> bool {
        self.token.aborted()
    }

    fn error_message(&self) -> JsString {
        crate::request::CallbackError::from_rejection(self.token.reason().expect("aborted request"))
            .error_or_json_message()
    }
}

/// `mistralConversationsApi()`: the lazy module wrapper from
/// `mistral-conversations.lazy.ts`.
pub fn mistral_conversations_api() -> LazyApi {
    lazy_api(Arc::new(|| {
        Box::pin(async { Ok(Arc::new(MistralConversationsApi) as Arc<dyn ProviderStreams>) })
    }))
}
