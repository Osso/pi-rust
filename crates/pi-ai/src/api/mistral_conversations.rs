//! Port of `packages/ai/src/api/mistral-conversations.ts`.
//!
//! Streams the native Mistral Chat Completions endpoint. The TypeScript module
//! speaks the SDK wire shape and serializes it with `toMistralWirePayload`;
//! this port builds the same payload shape as ordered JSON and applies the
//! identical camelCase-to-snake_case remapping.

use pi_js_compat::js_json::{JsObject, JsValue};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use indexmap::IndexMap;
use serde_json::{Map, Value, json};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::api::constrained_sampling::{get_json_schema_tool_parameters, resolve_json_schema_strict_sampling};
use crate::api::lazy::{LazyApi, LazyApiCapabilities, lazy_api};
use crate::api::simple_options::{build_base_options, model_thinking_level};
use crate::api::transform_messages::transform_messages_with;
use crate::http::ABORT_ERROR_MESSAGE;
use crate::models::{calculate_cost, clamp_thinking_level};
use crate::request::{HttpRequest, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, DoneReason, ErrorReason,
    ImageContent, Message, Modality, Model, ModelThinkingLevel, StopReason, TextContent, ThinkingContent, Tool,
    ToolCall, ToolResultContent, UserContent,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::hash::short_hash;
use crate::utils::headers::headers_to_record;
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

/// Provider-specific reasoning effort (`reasoning_effort`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MistralReasoningEffort {
    Disabled,
    High,
}

impl MistralReasoningEffort {
    fn wire_value(self) -> &'static str {
        match self {
            Self::Disabled => "none",
            Self::High => "high",
        }
    }
}

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
        let reasoning_effort = match options.extra.get("reasoningEffort").and_then(Value::as_str) {
            Some("none") => Some(MistralReasoningEffort::Disabled),
            Some("high") => Some(MistralReasoningEffort::High),
            _ => None,
        };
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
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        stream(model, context, MistralOptions::from_stream_options(options))
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

/// Stream responses from the native Mistral Chat Completions endpoint.
pub fn stream(model: Model, context: Context, options: MistralOptions) -> AssistantMessageEventStream {
    let (sender, event_stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        run_stream(model, context, options, sender).await;
    });
    event_stream
}

async fn run_stream(model: Model, context: Context, options: MistralOptions, sender: AssistantMessageEventSender) {
    let mut output = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis());
    match produce_stream(&model, &context, &options, &sender, &mut output).await {
        Ok(()) => {}
        Err(error) => {
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
    let transformed = transform_messages_with(&context.messages, model, Some(&normalize), now_millis());

    let mut payload = JsValue::from(build_chat_payload(model, context, &transformed, options)?);
    if let Some(on_payload) = &options.stream.request.on_payload
        && let Some(next_payload) = on_payload(payload.clone(), model.clone())
            .await
            .map_err(|error| MistralError::message(error.message))?
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
    if output.stop_reason == StopReason::Pending {
        return Err(MistralError::message("Mistral stream ended without a finish reason"));
    }
    if matches!(output.stop_reason, StopReason::Aborted | StopReason::Error) {
        return Err(MistralError::message(
            output
                .error_message
                .clone()
                .unwrap_or_else(|| "An unknown error occurred".to_owned()),
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

fn required_api_key<'a>(model: &Model, options: &'a StreamOptions) -> Result<&'a str, MistralError> {
    options
        .request
        .api_key
        .as_deref()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| MistralError::message(format!("No API key for provider: {}", model.provider)))
}

fn user_aborted(options: &MistralOptions) -> bool {
    options
        .stream
        .request
        .signal
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
}

fn done_reason(reason: StopReason) -> Option<DoneReason> {
    match reason {
        StopReason::Stop => Some(DoneReason::Stop),
        StopReason::Length => Some(DoneReason::Length),
        StopReason::ToolUse => Some(DoneReason::ToolUse),
        StopReason::Deferred => Some(DoneReason::Deferred),
        StopReason::Pending | StopReason::Error | StopReason::Aborted => None,
    }
}

/// Maps provider-agnostic `SimpleStreamOptions` to Mistral options.
pub fn stream_simple(model: Model, context: Context, options: SimpleStreamOptions) -> AssistantMessageEventStream {
    let api_key = options.stream.request.api_key.clone().filter(|key| !key.is_empty());
    let Some(api_key) = api_key else {
        return crate::request::error_stream(&model, &format!("No API key for provider: {}", model.provider));
    };

    let base = build_base_options(&model, &context, Some(&options), Some(&api_key));
    let reasoning = options.reasoning.and_then(|level| {
        let clamped = clamp_thinking_level(&model, model_thinking_level(level));
        (clamped != ModelThinkingLevel::Off).then_some(clamped)
    });
    let should_use_reasoning = model.reasoning && reasoning.is_some();

    stream(
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
    )
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
        .and_then(|value| value.as_deref());
    match mapped {
        Some("none") => MistralReasoningEffort::Disabled,
        _ => MistralReasoningEffort::High,
    }
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
    Message(String),
    Http {
        status: u16,
        status_text: String,
        body: String,
    },
}

impl MistralError {
    fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

fn format_mistral_error(error: &MistralError) -> String {
    match error {
        MistralError::Http {
            status,
            body,
            status_text,
        } => {
            let body_text = body.trim();
            if body_text.is_empty() {
                let message = if status_text.is_empty() {
                    format!("Request failed with status {status}")
                } else {
                    status_text.clone()
                };
                return format!("Mistral API error ({status}): {message}");
            }
            format!(
                "Mistral API error ({status}): {}",
                truncate_error_text(body_text, MAX_MISTRAL_ERROR_BODY_CHARS)
            )
        }
        MistralError::Message(message) => message.clone(),
    }
}

/// JavaScript `text.length` counts UTF-16 code units; truncation therefore
/// stops at the last whole character that fits the code-unit budget.
fn truncate_error_text(text: &str, max_chars: usize) -> String {
    let length: usize = text.encode_utf16().count();
    if length <= max_chars {
        return text.to_owned();
    }
    let mut used = 0;
    let mut truncated = String::new();
    for character in text.chars() {
        let width = character.len_utf16();
        if used + width > max_chars {
            break;
        }
        used += width;
        truncated.push(character);
    }
    format!("{truncated}... [truncated {} chars]", length - max_chars)
}

struct MistralResponse {
    body: Box<dyn crate::request::HttpBody>,
    signal: RequestSignal,
}

async fn request_mistral_stream(
    model: &Model,
    payload: &JsValue,
    api_key: &str,
    options: &MistralOptions,
) -> Result<MistralResponse, MistralError> {
    let url = chat_completions_url(model)?;
    let headers = build_mistral_headers(model, api_key, options);
    let wire_payload = to_mistral_wire_payload(payload)?;
    let body = serde_json::to_vec(&wire_payload).map_err(|error| MistralError::message(error.to_string()))?;
    let signal = RequestSignal::start(
        options.stream.request.signal.clone(),
        options.stream.request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
    );
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
            Some(signal.token.clone()),
        )
        .await
        .map_err(|error| signal_error(&signal, error.message))?;

    if let Some(on_response) = &options.stream.request.on_response {
        on_response(
            crate::types::ProviderResponse {
                status: f64::from(response.status),
                headers: headers_to_record(response.headers.clone()),
            },
            model.clone(),
        )
        .await
        .map_err(|error| MistralError::message(error.message))?;
    }

    if !(200..300).contains(&response.status) {
        let body = read_response_text(response.body, &signal).await?;
        return Err(MistralError::Http {
            status: response.status,
            status_text: response.status_text,
            body,
        });
    }

    Ok(MistralResponse {
        body: response.body,
        signal,
    })
}

fn chat_completions_url(model: &Model) -> Result<Url, MistralError> {
    let mut base = Url::parse(&model.base_url).map_err(|error| MistralError::message(error.to_string()))?;
    let path = base.path().trim_end_matches('/');
    base.set_path(&format!("{path}/"));
    base.join("v1/chat/completions")
        .map_err(|error| MistralError::message(error.to_string()))
}

fn build_mistral_headers(model: &Model, api_key: &str, options: &MistralOptions) -> IndexMap<String, String> {
    let mut headers = IndexMap::from([
        ("accept".to_owned(), "text/event-stream".to_owned()),
        ("authorization".to_owned(), format!("Bearer {api_key}")),
        ("content-type".to_owned(), "application/json".to_owned()),
    ]);
    apply_model_header_overrides(&mut headers, model.headers.as_ref());
    apply_mistral_header_overrides(&mut headers, options.stream.request.headers.as_ref());
    let has_explicit_affinity = model_has_header_override(model.headers.as_ref(), "x-affinity")
        || provider_has_header_override(options.stream.request.headers.as_ref(), "x-affinity");
    if should_use_prompt_caching(options) && !has_explicit_affinity {
        headers.insert(
            "x-affinity".to_owned(),
            options.stream.session_id.clone().unwrap_or_default(),
        );
    }
    headers
}

fn model_has_header_override(headers: Option<&IndexMap<String, String>>, target: &str) -> bool {
    headers.is_some_and(|headers| headers.keys().any(|name| name.eq_ignore_ascii_case(target)))
}

fn provider_has_header_override(headers: Option<&crate::types::ProviderHeaders>, target: &str) -> bool {
    headers.is_some_and(|headers| headers.keys().any(|name| name.eq_ignore_ascii_case(target)))
}

fn apply_model_header_overrides(headers: &mut IndexMap<String, String>, overrides: Option<&IndexMap<String, String>>) {
    let Some(overrides) = overrides else {
        return;
    };
    for (name, value) in overrides {
        headers.insert(name.to_ascii_lowercase(), value.clone());
    }
}

fn apply_mistral_header_overrides(
    headers: &mut IndexMap<String, String>,
    overrides: Option<&crate::types::ProviderHeaders>,
) {
    let Some(overrides) = overrides else {
        return;
    };
    for (name, value) in overrides {
        let name = name.to_ascii_lowercase();
        match value {
            Some(value) => {
                headers.insert(name, value.clone());
            }
            None => {
                headers.shift_remove(&name);
            }
        }
    }
}
fn should_use_prompt_caching(options: &MistralOptions) -> bool {
    options.stream.cache_retention != Some(CacheRetention::None)
        && options
            .stream
            .session_id
            .as_deref()
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

    Ok(JsValue::Object(wire))
}

fn to_mistral_wire_message(message: &JsValue) -> Result<JsValue, MistralError> {
    let Some(message) = message.as_object() else {
        return Err(MistralError::message("Mistral message must be an object"));
    };
    let mut wire = message.clone();
    remap_mistral_property(&mut wire, "toolCalls", "tool_calls");
    remap_mistral_property(&mut wire, "toolCallId", "tool_call_id");
    if let Some(JsValue::Array(content)) = wire.get("content").cloned() {
        let content: Vec<JsValue> = content
            .iter()
            .map(to_mistral_wire_content_chunk)
            .collect::<Result<_, _>>()?;
        wire.insert("content".into(), JsValue::Array(content));
    }
    Ok(JsValue::Object(wire))
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
) -> Result<Value, MistralError> {
    let mut payload = Map::new();
    payload.insert("model".to_owned(), Value::String(model.id.clone()));
    payload.insert("stream".to_owned(), Value::Bool(true));
    payload.insert(
        "messages".to_owned(),
        Value::Array(to_chat_messages(messages, model.input.contains(&Modality::Image))?),
    );

    if let Some(tools) = &context.tools
        && !tools.is_empty()
    {
        payload.insert("tools".to_owned(), Value::Array(to_function_tools(tools)?));
    }
    if let Some(temperature) = options.stream.temperature {
        payload.insert("temperature".to_owned(), js_number_value(temperature));
    }
    if let Some(max_tokens) = options.stream.max_tokens {
        payload.insert("maxTokens".to_owned(), js_number_value(max_tokens));
    }
    if let Some(tool_choice) = &options.tool_choice {
        payload.insert("toolChoice".to_owned(), tool_choice_value(tool_choice));
    }
    if options.prompt_mode == Some(MistralPromptMode::Reasoning) {
        payload.insert("promptMode".to_owned(), Value::String("reasoning".to_owned()));
    }
    if let Some(reasoning_effort) = options.reasoning_effort {
        payload.insert(
            "reasoningEffort".to_owned(),
            Value::String(reasoning_effort.wire_value().to_owned()),
        );
    }
    if should_use_prompt_caching(options) {
        payload.insert(
            "promptCacheKey".to_owned(),
            Value::String(options.stream.session_id.clone().unwrap_or_default()),
        );
    }

    if let Some(system_prompt) = &context.system_prompt
        && !system_prompt.is_empty()
        && let Some(Value::Array(entries)) = payload.get_mut("messages")
    {
        entries.insert(
            0,
            json!({"role": "system", "content": sanitize_surrogates(system_prompt)}),
        );
    }

    Ok(Value::Object(payload))
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

fn to_function_tools(tools: &[Tool]) -> Result<Vec<Value>, MistralError> {
    tools
        .iter()
        .map(|tool| {
            let strict = resolve_json_schema_strict_sampling(tool, true)
                .map_err(|error| MistralError::message(error.message))?;
            let parameters =
                get_json_schema_tool_parameters(tool, strict).map_err(|error| MistralError::message(error.message))?;
            Ok(json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": parameters,
                    "strict": strict.unwrap_or(false),
                }
            }))
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
                    "index": 0,
                }));
            }
        }
    }

    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String("assistant".to_owned()));
    message.insert("prefix".to_owned(), Value::Bool(false));
    if !content_parts.is_empty() {
        message.insert("content".to_owned(), Value::Array(content_parts));
    }
    if !tool_calls.is_empty() {
        message.insert("toolCalls".to_owned(), Value::Array(tool_calls));
    }
    if message.len() > 2 {
        result.push(Value::Object(message));
    }
}

fn build_tool_result_text(tool_result: &crate::types::ToolResultMessage, supports_images: bool) -> String {
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
    let trimmed = text.trim();
    let error_prefix = if tool_result.is_error { "[tool error] " } else { "" };

    if !trimmed.is_empty() {
        let image_suffix = if has_images && !supports_images {
            "\n[tool image omitted: model does not support images]"
        } else {
            ""
        };
        return format!("{error_prefix}{trimmed}{image_suffix}");
    }

    if has_images {
        if supports_images {
            return if tool_result.is_error {
                "[tool error] (see attached image)"
            } else {
                "(see attached image)"
            }
            .to_owned();
        }
        return if tool_result.is_error {
            "[tool error] (image omitted: model does not support images)"
        } else {
            "(image omitted: model does not support images)"
        }
        .to_owned();
    }

    if tool_result.is_error {
        "[tool error] (no tool output)"
    } else {
        "(no tool output)"
    }
    .to_owned()
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
        if signal.is_cancelled() {
            return Err(MistralError::message(signal.error_message()));
        }
        let chunk = tokio::select! {
            () = signal.token.cancelled() => {
                return Err(MistralError::message(signal.error_message()));
            }
            chunk = body.next_chunk() => chunk,
        };
        match chunk {
            Some(Ok(bytes)) => buffer.push_str(&decoder.push(&bytes, false)),
            Some(Err(error)) => {
                return Err(MistralError::message(if signal.is_cancelled() {
                    signal.error_message()
                } else {
                    error.message
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
    finalize_tool_blocks(output, sender, &state.tool_blocks, &mut state.partial_args);
    Ok(())
}

#[derive(Default)]
struct StreamState {
    current_block: Option<CurrentBlock>,
    tool_blocks: Vec<(String, usize)>,
    partial_args: BTreeMap<usize, pi_js_compat::js_json::JsString>,
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
    update_response_id(output, &chunk);
    update_usage(model, output, &chunk);

    let Some(choice) = chunk
        .get("choices")
        .and_then(Value::as_array)
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
    if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
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
    content: &Value,
    current_block: &mut Option<CurrentBlock>,
) {
    match content {
        Value::String(text) => append_text_delta(output, sender, text, current_block),
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(text) => append_text_delta(output, sender, text, current_block),
                    Value::Object(object) => {
                        let item_type = object.get("type").and_then(Value::as_str).unwrap_or_default();
                        if item_type == "thinking" {
                            append_thinking_delta(output, sender, object, current_block);
                        } else if item_type == "text" {
                            let text = object.get("text").and_then(Value::as_str).unwrap_or_default();
                            append_text_delta(output, sender, text, current_block);
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
    text: &str,
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
    let text_delta = sanitize_surrogates(text).to_owned();
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
    object: &Map<String, Value>,
    current_block: &mut Option<CurrentBlock>,
) {
    let delta_text = object
        .get("thinking")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .filter(|text| !text.is_empty())
                .collect::<String>()
        })
        .unwrap_or_default();
    let thinking_delta = sanitize_surrogates(&delta_text).to_owned();
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
    tool_call: &Value,
    state: &mut StreamState,
) {
    if state.current_block.is_some() {
        finish_current_block(output, sender, state.current_block.as_ref());
        state.current_block = None;
    }

    let id = tool_call.get("id").and_then(Value::as_str).unwrap_or_default();
    let call_id = if !id.is_empty() && id != "null" {
        id.to_owned()
    } else {
        let index = tool_call.get("index").and_then(Value::as_f64).unwrap_or(0.0);
        derive_mistral_tool_call_id(&format!("toolcall:{}", js_number_to_string(index)), 0)
    };
    let index_value = tool_call.get("index").and_then(Value::as_f64).unwrap_or(0.0);
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
                .and_then(Value::as_str)
                .unwrap_or_default();
            output.content.push(AssistantContent::ToolCall(ToolCall::new(
                call_id,
                name,
                Default::default(),
            )));
            let index = output.content.len() - 1;
            state.tool_blocks.push((key.clone(), index));
            state.partial_args.insert(index, Default::default());
            sender.push(AssistantMessageEvent::ToolCallStart {
                content_index: index,
                partial: output.clone(),
            });
            index
        }
    };

    let function = tool_call.get("function");
    let args_delta = match function.and_then(|function| function.get("arguments")) {
        Some(Value::String(arguments)) => arguments.clone(),
        Some(arguments) => serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_owned()),
        None => "{}".to_owned(),
    };
    let buffer = state.partial_args.entry(block_index).or_default();
    buffer.push_str(&args_delta);
    let parsed = parse_streaming_json(Some(buffer));
    if let Some(AssistantContent::ToolCall(block)) = output.content.get_mut(block_index) {
        block.arguments = arguments_object(parsed);
    }
    sender.push(AssistantMessageEvent::ToolCallDelta {
        content_index: block_index,
        delta: args_delta.into(),
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
    partial_args: &mut BTreeMap<usize, pi_js_compat::js_json::JsString>,
) {
    for (_, index) in tool_blocks {
        let index = *index;
        if !matches!(output.content.get(index), Some(AssistantContent::ToolCall(_))) {
            continue;
        }
        if let Some(buffer) = partial_args.remove(&index) {
            let parsed = parse_streaming_json(Some(&buffer));
            if let Some(AssistantContent::ToolCall(block)) = output.content.get_mut(index) {
                block.arguments = arguments_object(parsed);
            }
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

fn update_response_id(output: &mut AssistantMessage, chunk: &Value) {
    if output.response_id.as_deref().is_none_or(str::is_empty)
        && let Some(id) = chunk.get("id").and_then(Value::as_str)
        && !id.is_empty()
    {
        output.response_id = Some(id.to_owned());
    }
}

fn update_usage(model: &Model, output: &mut AssistantMessage, chunk: &Value) {
    let Some(usage) = chunk.get("usage").filter(|usage| usage.is_object()) else {
        return;
    };
    let prompt_tokens = usage.get("prompt_tokens").and_then(Value::as_f64).unwrap_or(0.0);
    let cached_prompt_tokens = cached_prompt_tokens(usage, prompt_tokens);
    output.usage.input = (prompt_tokens - cached_prompt_tokens).max(0.0);
    output.usage.output = usage.get("completion_tokens").and_then(Value::as_f64).unwrap_or(0.0);
    output.usage.cache_read = cached_prompt_tokens;
    output.usage.cache_write = 0.0;
    let total_tokens = usage.get("total_tokens").and_then(Value::as_f64).unwrap_or(0.0);
    output.usage.total_tokens = if total_tokens == 0.0 {
        output.usage.input + output.usage.output + output.usage.cache_read + output.usage.cache_write
    } else {
        total_tokens
    };
    output.usage.cost = calculate_cost(model, &output.usage);
}

fn cached_prompt_tokens(usage: &Value, prompt_tokens: f64) -> f64 {
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
    .find_map(Value::as_f64)
    .unwrap_or(0.0);
    let cached = if raw.is_finite() { raw } else { 0.0 };
    prompt_tokens.min(cached.max(0.0))
}

fn apply_finish_reason(output: &mut AssistantMessage, choice: &Value) {
    let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) else {
        return;
    };
    if reason.is_empty() {
        return;
    }
    output.raw_stop_reason = Some(reason.to_owned());
    match reason {
        "stop" => output.stop_reason = StopReason::Stop,
        "length" | "model_length" => output.stop_reason = StopReason::Length,
        "tool_calls" => output.stop_reason = StopReason::ToolUse,
        "error" => {
            output.stop_reason = StopReason::Error;
            output.error_message = Some("Provider stopped with: error".to_owned());
        }
        other => {
            output.stop_reason = StopReason::Error;
            output.error_message = Some(format!("Provider stopped with: {other}"));
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
    Event(Value),
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
    let parsed: Value =
        pi_js_compat::js_json::parse(&data).map_err(|error| MistralError::message(error.to_string()))?;
    if !parsed.is_object() || !parsed.get("choices").is_some_and(Value::is_array) {
        return Err(MistralError::message("Invalid Mistral streaming event"));
    }
    Ok(ParsedEvent::Event(parsed))
}

async fn read_response_text(
    mut body: Box<dyn crate::request::HttpBody>,
    signal: &RequestSignal,
) -> Result<String, MistralError> {
    let mut decoder = Utf8StreamDecoder::new();
    let mut text = String::new();
    loop {
        if signal.is_cancelled() {
            return Err(MistralError::message(signal.error_message()));
        }
        let chunk = tokio::select! {
            () = signal.token.cancelled() => {
                return Err(MistralError::message(signal.error_message()));
            }
            chunk = body.next_chunk() => chunk,
        };
        match chunk {
            Some(Ok(bytes)) => text.push_str(&decoder.push(&bytes, false)),
            Some(Err(error)) => {
                return Err(MistralError::message(if signal.is_cancelled() {
                    signal.error_message()
                } else {
                    error.message
                }));
            }
            None => {
                text.push_str(&decoder.push(&[], true));
                return Ok(text);
            }
        }
    }
}

/// Combined request signal: user abort, timeout, or neither.
struct RequestSignal {
    token: CancellationToken,
    reason: Arc<AtomicU8>,
    watcher: Option<JoinHandle<()>>,
}

impl RequestSignal {
    const USER: u8 = 1;
    const TIMEOUT: u8 = 2;

    fn start(user: Option<CancellationToken>, timeout_ms: f64) -> Self {
        let token = CancellationToken::new();
        let reason = Arc::new(AtomicU8::new(0));
        let watcher = spawn_signal_watcher(token.clone(), Arc::clone(&reason), user, timeout_ms);
        Self {
            token,
            reason,
            watcher: Some(watcher),
        }
    }

    fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    fn error_message(&self) -> String {
        match self.reason.load(Ordering::SeqCst) {
            Self::TIMEOUT => "The operation was aborted due to timeout".to_owned(),
            _ => ABORT_ERROR_MESSAGE.to_owned(),
        }
    }
}

impl Drop for RequestSignal {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            watcher.abort();
        }
    }
}

fn spawn_signal_watcher(
    token: CancellationToken,
    reason: Arc<AtomicU8>,
    user: Option<CancellationToken>,
    timeout_ms: f64,
) -> JoinHandle<()> {
    let timeout = Duration::from_millis(timeout_ms.max(0.0) as u64);
    tokio::spawn(async move {
        let timeout = tokio::time::sleep(timeout);
        tokio::pin!(timeout);
        match user {
            Some(user) => {
                tokio::select! {
                    () = user.cancelled() => {
                        reason.compare_exchange(0, RequestSignal::USER, Ordering::SeqCst, Ordering::SeqCst).ok();
                        token.cancel();
                    }
                    () = &mut timeout => {
                        reason.compare_exchange(0, RequestSignal::TIMEOUT, Ordering::SeqCst, Ordering::SeqCst).ok();
                        token.cancel();
                    }
                }
            }
            None => {
                timeout.await;
                reason
                    .compare_exchange(0, RequestSignal::TIMEOUT, Ordering::SeqCst, Ordering::SeqCst)
                    .ok();
                token.cancel();
            }
        }
    })
}

fn signal_error(signal: &RequestSignal, fallback: String) -> MistralError {
    if signal.is_cancelled() {
        MistralError::message(signal.error_message())
    } else {
        MistralError::message(fallback)
    }
}

/// `mistralConversationsApi()`: the lazy module wrapper from
/// `mistral-conversations.lazy.ts`.
pub fn mistral_conversations_api() -> LazyApi {
    lazy_api(
        Arc::new(|| Box::pin(async { Ok(Arc::new(MistralConversationsApi) as Arc<dyn ProviderStreams>) })),
        LazyApiCapabilities::default(),
    )
}
