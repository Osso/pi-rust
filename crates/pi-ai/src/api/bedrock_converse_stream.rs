//! Port of `packages/ai/src/api/bedrock-converse-stream.ts`.
//!
//! HTTP is always sent through the shared [`Fetch`] contract. SigV4 signing,
//! AWS event-stream framing, and the Node credential chain are implemented in
//! this crate and proven against the pinned TypeScript module through the
//! differential oracle.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::bedrock_credentials::{env_credentials, profile_credentials, profile_name, profile_region};
use crate::api::bedrock_eventstream::EventStreamDecoder;
use crate::api::bedrock_sigv4::{AwsCredentials, sign_request};
use crate::api::constrained_sampling::{get_json_schema_tool_parameters, resolve_json_schema_strict_sampling};
use crate::api::simple_options::{
    adjust_max_tokens_for_thinking, build_base_options, clamp_max_tokens_to_context, clamp_reasoning,
    model_thinking_level,
};
use crate::api::transform_messages::transform_messages_with;
use crate::http::default_fetch;
use crate::models::calculate_cost;
use crate::request::{
    HttpBody, HttpRequest, HttpResponse, OnPayload, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis,
};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, DoneReason, ErrorReason,
    JsonObject, Message, Model, ProviderEnv, StopReason, ThinkingBudgets, ThinkingContent, ThinkingLevel, Tool,
    ToolCall, ToolResultContent, ToolResultMessage, Usage,
};
use crate::utils::diagnostics::{AssistantMessageDiagnostic, append_assistant_message_diagnostic};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};
use crate::utils::headers::provider_headers_to_record;
use crate::utils::json_parse::parse_streaming_json;
use crate::utils::provider_env::get_provider_env_value;
use crate::utils::sanitize_unicode::sanitize_surrogates;
use pi_js_compat::base64;
use pi_js_compat::date::date_parse;
use pi_js_compat::number::{js_number_to_string, js_number_value, parse_number};

const EMPTY_TEXT_PLACEHOLDER: &str = "<empty>";
const BEDROCK_DATA_RETENTION_DOCS_URL: &str =
    "https://docs.aws.amazon.com/bedrock/latest/userguide/data-retention.html";
const MAX_BEDROCK_DIAGNOSTIC_VALUE_CHARS: usize = 200;
const AWS_SDK_USER_AGENT: &str = "aws-sdk-js/3.1048.0";
const MAX_ATTEMPTS: usize = 3;
const SERVICE: &str = "bedrock";

/// How Claude thinking content is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BedrockThinkingDisplay {
    Summarized,
    Omitted,
}

/// `BedrockOptions["toolChoice"]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BedrockToolChoice {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "any")]
    Any,
    #[serde(rename = "none")]
    None,
    #[serde(rename = "tool")]
    Tool { name: String },
}

impl<'de> Deserialize<'de> for BedrockToolChoice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Name(String),
            Tool { r#type: String, name: String },
        }
        match Repr::deserialize(deserializer)? {
            Repr::Name(name) => match name.as_str() {
                "auto" => Ok(Self::Auto),
                "any" => Ok(Self::Any),
                "none" => Ok(Self::None),
                other => Err(serde::de::Error::custom(format!("unknown Bedrock toolChoice {other}"))),
            },
            Repr::Tool { r#type, name } if r#type == "tool" => Ok(Self::Tool { name }),
            Repr::Tool { r#type, .. } => Err(serde::de::Error::custom(format!(
                "unknown Bedrock toolChoice type {type}"
            ))),
        }
    }
}

/// Typed Bedrock options parsed from [`StreamOptions::extra`].
#[derive(Clone, Default)]
pub struct BedrockOptions {
    pub stream: StreamOptions,
    pub region: Option<String>,
    pub profile: Option<String>,
    pub tool_choice: Option<BedrockToolChoice>,
    pub reasoning: Option<ThinkingLevel>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub interleaved_thinking: Option<bool>,
    pub thinking_display: Option<BedrockThinkingDisplay>,
    pub request_metadata: Option<JsonObject>,
    pub bearer_token: Option<String>,
}

impl BedrockOptions {
    /// Parse API-specific fields from `StreamOptions::extra`.
    pub fn from_stream_options(stream: StreamOptions) -> Result<Self, String> {
        let extra = stream.extra.clone();
        let take = |key: &str| extra.get(key).cloned();
        Ok(Self {
            region: optional_string(take("region"))?,
            profile: optional_string(take("profile"))?,
            tool_choice: optional_json::<BedrockToolChoice>(take("toolChoice"))?,
            reasoning: optional_json::<ThinkingLevel>(take("reasoning"))?,
            thinking_budgets: optional_json::<ThinkingBudgets>(take("thinkingBudgets"))?,
            interleaved_thinking: optional_bool(take("interleavedThinking"))?,
            thinking_display: optional_json::<BedrockThinkingDisplay>(take("thinkingDisplay"))?,
            request_metadata: optional_object(take("requestMetadata"))?,
            bearer_token: optional_string(take("bearerToken"))?,
            stream,
        })
    }

    pub fn signal(&self) -> Option<tokio_util::sync::CancellationToken> {
        self.stream.request.signal.clone()
    }
}

fn optional_string(value: Option<Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err("expected a string".to_owned()),
    }
}

fn optional_bool(value: Option<Value>) -> Result<Option<bool>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(value)),
        Some(_) => Err("expected a boolean".to_owned()),
    }
}

fn optional_object(value: Option<Value>) -> Result<Option<JsonObject>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(value)) => Ok(Some(value)),
        Some(_) => Err("expected an object".to_owned()),
    }
}

fn optional_json<T: for<'de> Deserialize<'de>>(value: Option<Value>) -> Result<Option<T>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|error| error.to_string()),
    }
}

/// `stream`: the Bedrock ConverseStream implementation.
pub fn stream(model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        run_stream(sender, model, context, options).await;
    });
    stream
}

/// `streamSimple`: reasoning-aware wrapper over [`stream`].
pub fn stream_simple(model: Model, context: Context, options: SimpleStreamOptions) -> AssistantMessageEventStream {
    let base = build_base_options(&model, &context, Some(&options), None);
    let Some(reasoning) = options.reasoning else {
        return stream(model, context, base);
    };

    if is_anthropic_claude_model(&model) {
        if supports_adaptive_thinking(&model.id, Some(&model.name)) {
            let mut extra = base.extra.clone();
            extra.insert("reasoning".to_owned(), json!(reasoning));
            if let Some(budgets) = &options.thinking_budgets {
                extra.insert("thinkingBudgets".to_owned(), json!(budgets));
            }
            let mut base = base;
            base.extra = extra;
            return stream(model, context, base);
        }

        let (adjusted_max_tokens, adjusted_thinking_budget) = adjust_max_tokens_for_thinking(
            base.max_tokens,
            model.max_tokens,
            reasoning,
            options.thinking_budgets.as_ref(),
        );
        let max_tokens = clamp_max_tokens_to_context(&model, &context, adjusted_max_tokens);
        let mut budgets = options.thinking_budgets.clone().unwrap_or_default();
        match clamp_reasoning(Some(reasoning)).unwrap_or(ThinkingLevel::High) {
            ThinkingLevel::Minimal => {
                budgets.minimal = Some(adjusted_thinking_budget.min((max_tokens - 1024.0).max(0.0)))
            }
            ThinkingLevel::Low => budgets.low = Some(adjusted_thinking_budget.min((max_tokens - 1024.0).max(0.0))),
            ThinkingLevel::Medium => {
                budgets.medium = Some(adjusted_thinking_budget.min((max_tokens - 1024.0).max(0.0)))
            }
            _ => budgets.high = Some(adjusted_thinking_budget.min((max_tokens - 1024.0).max(0.0))),
        }
        let mut extra = base.extra.clone();
        extra.insert("reasoning".to_owned(), json!(reasoning));
        extra.insert("thinkingBudgets".to_owned(), json!(budgets));
        let mut base = base;
        base.max_tokens = Some(max_tokens);
        base.extra = extra;
        return stream(model, context, base);
    }

    let mut extra = base.extra.clone();
    extra.insert("reasoning".to_owned(), json!(reasoning));
    if let Some(budgets) = &options.thinking_budgets {
        extra.insert("thinkingBudgets".to_owned(), json!(budgets));
    }
    let mut base = base;
    base.extra = extra;
    stream(model, context, base)
}

/// `bedrockProviderModule` as a [`ProviderStreams`] implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct BedrockProviderModule;

impl ProviderStreams for BedrockProviderModule {
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        stream(model, context, options)
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

async fn run_stream(
    sender: AssistantMessageEventSender,
    model: Model,
    context: Context,
    stream_options: StreamOptions,
) {
    let timestamp = now_millis();
    let mut output = AssistantMessage::empty("bedrock-converse-stream", &model.provider, &model.id, timestamp);
    let mut blocks: Vec<BlockState> = Vec::new();
    let signal = stream_options.request.signal.clone();
    let result = drive(&sender, &model, &context, stream_options, &mut output, &mut blocks).await;
    let Err(error) = result else {
        return;
    };

    output.stop_reason = if signal.as_ref().is_some_and(|signal| signal.is_cancelled()) {
        StopReason::Aborted
    } else {
        StopReason::Error
    };
    output.error_message = Some(format_bedrock_error(&error));
    if output.stop_reason == StopReason::Error {
        append_bedrock_failure_diagnostic(&mut output, &error);
    }
    let reason = if output.stop_reason == StopReason::Aborted {
        ErrorReason::Aborted
    } else {
        ErrorReason::Error
    };
    sender.push(AssistantMessageEvent::Error {
        reason,
        error: output.clone(),
    });
    sender.end(Some(output));
}

async fn drive(
    sender: &AssistantMessageEventSender,
    model: &Model,
    context: &Context,
    stream_options: StreamOptions,
    output: &mut AssistantMessage,
    blocks: &mut Vec<BlockState>,
) -> Result<(), BedrockError> {
    let options = BedrockOptions::from_stream_options(stream_options)
        .map_err(|message| BedrockError::other(format!("Invalid Bedrock options: {message}")))?;
    let config = resolve_runtime_config(model, &options).await?;
    let cache_retention = resolve_cache_retention(options.stream.cache_retention, options.stream.request.env.as_ref());
    let supports_strict_mode = model
        .compat
        .as_ref()
        .and_then(|compat| match compat {
            crate::types::ModelCompat::Bedrock(compat) => compat.supports_strict_mode,
            _ => None,
        })
        .unwrap_or(false);
    let inference_max_tokens = options
        .stream
        .max_tokens
        .or_else(|| is_anthropic_claude_model(model).then_some(model.max_tokens));

    let mut command_input = build_command_input(
        model,
        context,
        &options,
        cache_retention,
        supports_strict_mode,
        inference_max_tokens,
        output.timestamp,
    )?;
    if let Some(on_payload) = &options.stream.request.on_payload {
        command_input = apply_on_payload(on_payload, command_input, model).await?;
    }
    let mut body_value = command_input;
    if let Some(object) = body_value.as_object_mut()
        && object.contains_key("modelId")
    {
        let mut without_model_id = Map::new();
        for (key, value) in std::mem::take(object) {
            if key != "modelId" {
                without_model_id.insert(key, value);
            }
        }
        *object = without_model_id;
    }
    let body = serde_json::to_vec(&body_value)
        .map_err(|error| BedrockError::other(format!("Failed to serialize Bedrock request: {error}")))?;
    let response = execute_request(&config, &options, model, body).await?;
    let request_id = response_request_id(&response);
    if let Some(on_response) = &options.stream.request.on_response {
        let mut headers = BTreeMap::new();
        if let Some(request_id) = &request_id {
            headers.insert("x-amzn-requestid".to_owned(), request_id.clone());
        }
        on_response(
            crate::types::ProviderResponse {
                status: f64::from(response.status),
                headers,
            },
            model.clone(),
        )
        .await
        .map_err(|error| BedrockError::other(error.message))?;
    }

    let mut body = response.body;
    let mut decoder = EventStreamDecoder::new();
    loop {
        if options.signal().as_ref().is_some_and(|signal| signal.is_cancelled()) {
            return Err(BedrockError::abort("Request was aborted"));
        }
        let Some(chunk) = body.next_chunk().await else {
            break;
        };
        let chunk = chunk.map_err(|error| BedrockError::abort_or_transport(error.message))?;
        decoder.push(&chunk);
        while let Some(message) = decoder
            .next_message()
            .map_err(|error| BedrockError::stream(error.message()).with_request_id(request_id.as_deref()))?
        {
            handle_message(sender, model, output, blocks, &message, request_id.as_deref())?;
        }
    }

    if options.signal().as_ref().is_some_and(|signal| signal.is_cancelled()) {
        return Err(BedrockError::abort("Request was aborted"));
    }
    match output.stop_reason {
        StopReason::Pending => {
            Err(BedrockError::stream("Bedrock stream ended without a stop reason")
                .with_request_id(request_id.as_deref()))
        }
        StopReason::Error | StopReason::Aborted => Err(BedrockError::stream(
            output
                .error_message
                .clone()
                .unwrap_or_else(|| "An unknown error occurred".to_owned()),
        )
        .with_request_id(request_id.as_deref())),
        reason => {
            let done_reason = match reason {
                StopReason::Stop => DoneReason::Stop,
                StopReason::Length => DoneReason::Length,
                StopReason::ToolUse => DoneReason::ToolUse,
                _ => DoneReason::Stop,
            };
            sender.push(AssistantMessageEvent::Done {
                reason: done_reason,
                message: output.clone(),
            });
            sender.end(Some(output.clone()));
            Ok(())
        }
    }
}

async fn apply_on_payload(on_payload: &OnPayload, payload: Value, model: &Model) -> Result<Value, BedrockError> {
    match on_payload(payload.clone(), model.clone()).await {
        Ok(Some(replacement)) => Ok(replacement),
        Ok(None) => Ok(payload),
        Err(error) => Err(BedrockError::other(error.message)),
    }
}

fn resolve_cache_retention(cache_retention: Option<CacheRetention>, env: Option<&ProviderEnv>) -> CacheRetention {
    if let Some(cache_retention) = cache_retention {
        return cache_retention;
    }
    if get_provider_env_value("PI_CACHE_RETENTION", env).as_deref() == Some("long") {
        return CacheRetention::Long;
    }
    CacheRetention::Short
}

fn build_command_input(
    model: &Model,
    context: &Context,
    options: &BedrockOptions,
    cache_retention: CacheRetention,
    supports_strict_mode: bool,
    inference_max_tokens: Option<f64>,
    now: i64,
) -> Result<Value, BedrockError> {
    let env = options.stream.request.env.as_ref();
    let mut inference_config = Map::new();
    if let Some(max_tokens) = inference_max_tokens {
        inference_config.insert("maxTokens".to_owned(), js_number_value(max_tokens));
    }
    if let Some(temperature) = options.stream.temperature {
        inference_config.insert("temperature".to_owned(), js_number_value(temperature));
    }

    let mut input = Map::new();
    input.insert("modelId".to_owned(), json!(model.id));
    input.insert(
        "messages".to_owned(),
        Value::Array(convert_messages(context, model, cache_retention, env, now)),
    );
    if let Some(system) = build_system_prompt(context.system_prompt.as_deref(), model, cache_retention, env) {
        input.insert("system".to_owned(), Value::Array(system));
    }
    input.insert("inferenceConfig".to_owned(), Value::Object(inference_config));
    if let Some(tool_config) = convert_tool_config(
        context.tools.as_deref(),
        options.tool_choice.as_ref(),
        supports_strict_mode,
    )? {
        input.insert("toolConfig".to_owned(), tool_config);
    }
    if let Some(fields) = build_additional_model_request_fields(model, options) {
        input.insert("additionalModelRequestFields".to_owned(), fields);
    }
    if let Some(metadata) = &options.request_metadata {
        input.insert("requestMetadata".to_owned(), Value::Object(metadata.clone()));
    }
    Ok(Value::Object(input))
}

fn convert_messages(
    context: &Context,
    model: &Model,
    cache_retention: CacheRetention,
    env: Option<&ProviderEnv>,
    now: i64,
) -> Vec<Value> {
    let normalize = |id: &str, _model: &Model, _source: &AssistantMessage| normalize_tool_call_id(id);
    let transformed = transform_messages_with(&context.messages, model, Some(&normalize), now);
    let mut result = Vec::new();
    let mut index = 0usize;
    while index < transformed.len() {
        match &transformed[index] {
            Message::User(message) => {
                result.push(convert_user_message(message));
            }
            Message::Assistant(message) => {
                if let Some(converted) = convert_assistant_message(message, model) {
                    result.push(converted);
                }
            }
            Message::ToolResult(message) => {
                let mut tool_results = Vec::new();
                tool_results.push(convert_tool_result(message));
                let mut next = index + 1;
                while let Some(Message::ToolResult(next_message)) = transformed.get(next) {
                    tool_results.push(convert_tool_result(next_message));
                    next += 1;
                }
                index = next - 1;
                result.push(object(vec![
                    ("role", json!("user")),
                    ("content", Value::Array(tool_results)),
                ]));
            }
        }
        index += 1;
    }

    if cache_retention != CacheRetention::None
        && supports_prompt_caching(model, env)
        && let Some(Value::Object(last)) = result.last_mut()
        && last.get("role") == Some(&json!("user"))
        && let Some(Value::Array(content)) = last.get_mut("content")
    {
        content.push(cache_point(cache_retention));
    }
    result
}

fn convert_user_message(message: &crate::types::UserMessage) -> Value {
    let content = match &message.content {
        crate::types::UserContent::Text(text) => vec![required_text_block(text)],
        crate::types::UserContent::Blocks(blocks) => {
            let mut content = Vec::new();
            for block in blocks {
                match block {
                    crate::types::TextOrImageContent::Text(text) => {
                        if let Some(text) = non_blank_text_block(&text.text) {
                            content.push(text);
                        }
                    }
                    crate::types::TextOrImageContent::Image(image) => {
                        if let Ok(image) = create_image_block(&image.mime_type, &image.data) {
                            content.push(image);
                        }
                    }
                }
            }
            if content.is_empty() {
                content.push(placeholder_text_block());
            }
            content
        }
    };
    object(vec![("role", json!("user")), ("content", Value::Array(content))])
}

fn convert_assistant_message(message: &AssistantMessage, model: &Model) -> Option<Value> {
    if message.content.is_empty() {
        return None;
    }
    let mut content = Vec::new();
    for block in &message.content {
        match block {
            AssistantContent::Text(text) => {
                if let Some(text) = non_blank_text_block(&text.text) {
                    content.push(text);
                }
            }
            AssistantContent::ToolCall(tool_call) => {
                content.push(object(vec![(
                    "toolUse",
                    object(vec![
                        ("toolUseId", json!(tool_call.id)),
                        ("name", json!(tool_call.name)),
                        (
                            "input",
                            sanitize_bedrock_document(&Value::Object(tool_call.arguments.clone())),
                        ),
                    ]),
                )]));
            }
            AssistantContent::Thinking(thinking) => {
                let text = sanitize_surrogates(&thinking.thinking);
                if text.trim().is_empty() {
                    continue;
                }
                if supports_thinking_signature(model) {
                    match thinking
                        .thinking_signature
                        .as_deref()
                        .filter(|signature| !signature.trim().is_empty())
                    {
                        Some(signature) => content.push(object(vec![(
                            "reasoningContent",
                            object(vec![(
                                "reasoningText",
                                object(vec![("text", json!(text)), ("signature", json!(signature))]),
                            )]),
                        )])),
                        None => content.push(object(vec![("text", json!(text))])),
                    }
                } else {
                    content.push(object(vec![(
                        "reasoningContent",
                        object(vec![("reasoningText", object(vec![("text", json!(text))]))]),
                    )]));
                }
            }
        }
    }
    if content.is_empty() {
        return None;
    }
    Some(object(vec![
        ("role", json!("assistant")),
        ("content", Value::Array(content)),
    ]))
}

fn convert_tool_result(message: &ToolResultMessage) -> Value {
    object(vec![(
        "toolResult",
        object(vec![
            ("toolUseId", json!(message.tool_call_id)),
            ("content", Value::Array(convert_tool_result_content(&message.content))),
            ("status", json!(if message.is_error { "error" } else { "success" })),
        ]),
    )])
}

fn convert_tool_result_content(content: &[ToolResultContent]) -> Vec<Value> {
    let mut result = Vec::new();
    for block in content {
        match block {
            ToolResultContent::Image(image) => {
                if let Ok(image) = create_image_block(&image.mime_type, &image.data) {
                    result.push(image);
                }
            }
            ToolResultContent::Text(text) => {
                if let Some(text) = non_blank_text_block(&text.text) {
                    result.push(text);
                }
            }
        }
    }
    if result.is_empty() {
        result.push(placeholder_text_block());
    }
    result
}

fn build_system_prompt(
    system_prompt: Option<&str>,
    model: &Model,
    cache_retention: CacheRetention,
    env: Option<&ProviderEnv>,
) -> Option<Vec<Value>> {
    let system_prompt = system_prompt.filter(|prompt| !prompt.is_empty())?;
    let mut blocks = vec![object(vec![("text", json!(sanitize_surrogates(system_prompt)))])];
    if cache_retention != CacheRetention::None && supports_prompt_caching(model, env) {
        blocks.push(cache_point(cache_retention));
    }
    Some(blocks)
}

fn cache_point(cache_retention: CacheRetention) -> Value {
    let mut point = Map::new();
    point.insert("type".to_owned(), json!("default"));
    if cache_retention == CacheRetention::Long {
        point.insert("ttl".to_owned(), json!("1h"));
    }
    object(vec![("cachePoint", Value::Object(point))])
}

fn non_blank_text_block(text: &str) -> Option<Value> {
    let sanitized = sanitize_surrogates(text);
    if sanitized.trim().is_empty() {
        None
    } else {
        Some(object(vec![("text", json!(sanitized))]))
    }
}

fn required_text_block(text: &str) -> Value {
    non_blank_text_block(text).unwrap_or_else(placeholder_text_block)
}

fn placeholder_text_block() -> Value {
    object(vec![("text", json!(EMPTY_TEXT_PLACEHOLDER))])
}

fn normalize_tool_call_id(id: &str) -> String {
    let mut sanitized: String = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    sanitized.truncate(64);
    sanitized
}

fn sanitize_bedrock_document(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sanitize_bedrock_document).collect()),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .filter(|(key, _)| !key.is_empty())
                .map(|(key, value)| (key.clone(), sanitize_bedrock_document(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn convert_tool_config(
    tools: Option<&[Tool]>,
    tool_choice: Option<&BedrockToolChoice>,
    supports_strict_mode: bool,
) -> Result<Option<Value>, BedrockError> {
    let Some(tools) = tools.filter(|tools| !tools.is_empty()) else {
        return Ok(None);
    };
    if tool_choice == Some(&BedrockToolChoice::None) {
        return Ok(None);
    }

    let mut bedrock_tools = Vec::with_capacity(tools.len());
    for tool in tools {
        let strict = resolve_json_schema_strict_sampling(tool, supports_strict_mode)
            .map_err(|error| BedrockError::other(error.message))?;
        let parameters =
            get_json_schema_tool_parameters(tool, strict).map_err(|error| BedrockError::other(error.message))?;
        let mut spec = Map::new();
        spec.insert("name".to_owned(), json!(tool.name));
        spec.insert("inputSchema".to_owned(), object(vec![("json", parameters)]));
        spec.insert("description".to_owned(), json!(tool.description));
        if strict == Some(true) {
            spec.insert("strict".to_owned(), json!(true));
        }
        bedrock_tools.push(object(vec![("toolSpec", Value::Object(spec))]));
    }

    let bedrock_tool_choice = match tool_choice {
        Some(BedrockToolChoice::Auto) => Some(object(vec![("auto", json!({}))])),
        Some(BedrockToolChoice::Any) => Some(object(vec![("any", json!({}))])),
        Some(BedrockToolChoice::Tool { name }) => Some(object(vec![("tool", object(vec![("name", json!(name))]))])),
        Some(BedrockToolChoice::None) | None => None,
    };

    let mut config = Map::new();
    config.insert("tools".to_owned(), Value::Array(bedrock_tools));
    if let Some(choice) = bedrock_tool_choice {
        config.insert("toolChoice".to_owned(), choice);
    }
    Ok(Some(Value::Object(config)))
}

fn create_image_block(mime_type: &str, data: &str) -> Result<Value, BedrockError> {
    let format = match mime_type {
        "image/jpeg" | "image/jpg" => "jpeg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        other => return Err(BedrockError::other(format!("Unknown image type: {other}"))),
    };
    let bytes = base64::atob(data).map_err(|_| BedrockError::other("Invalid image data"))?;
    Ok(object(vec![(
        "image",
        object(vec![
            ("format", json!(format)),
            ("source", object(vec![("bytes", json!(base64::btoa(&bytes)))])),
        ]),
    )]))
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key.to_owned(), value);
    }
    Value::Object(map)
}

fn is_anthropic_claude_model(model: &Model) -> bool {
    let id = model.id.to_lowercase();
    let name = model.name.to_lowercase();
    id.contains("anthropic.claude")
        || id.contains("anthropic/claude")
        || name.contains("anthropic.claude")
        || name.contains("anthropic/claude")
        || name.contains("claude")
}

fn model_match_candidates(model_id: &str, model_name: Option<&str>) -> Vec<String> {
    let values = match model_name {
        Some(name) => vec![model_id, name],
        None => vec![model_id],
    };
    let mut candidates = Vec::new();
    for value in values {
        let lower = value.to_lowercase();
        candidates.push(lower.clone());
        candidates.push(collapse_separators(&lower));
    }
    candidates
}

fn collapse_separators(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut in_separator = false;
    for character in value.chars() {
        if character.is_whitespace() || matches!(character, '_' | '.' | ':') {
            if !in_separator {
                result.push('-');
                in_separator = true;
            }
        } else {
            result.push(character);
            in_separator = false;
        }
    }
    result
}

fn supports_adaptive_thinking(model_id: &str, model_name: Option<&str>) -> bool {
    model_match_candidates(model_id, model_name).iter().any(|candidate| {
        candidate.contains("opus-4-6")
            || candidate.contains("opus-4-7")
            || candidate.contains("opus-4-8")
            || candidate.contains("opus-5")
            || candidate.contains("sonnet-4-6")
            || candidate.contains("sonnet-5")
            || candidate.contains("fable-5")
    })
}

fn supports_native_xhigh_effort(model: &Model) -> bool {
    model_match_candidates(&model.id, Some(&model.name))
        .iter()
        .any(|candidate| {
            candidate.contains("opus-4-7")
                || candidate.contains("opus-4-8")
                || candidate.contains("opus-5")
                || candidate.contains("sonnet-5")
                || candidate.contains("fable-5")
        })
}

fn map_thinking_level_to_effort(model: &Model, level: Option<ThinkingLevel>) -> String {
    if level == Some(ThinkingLevel::Xhigh) && supports_native_xhigh_effort(model) {
        return "xhigh".to_owned();
    }
    if let Some(level) = level
        && let Some(map) = &model.thinking_level_map
        && let Some(Some(mapped)) = map.get(&model_thinking_level(level))
    {
        return mapped.clone();
    }
    match level {
        Some(ThinkingLevel::Minimal | ThinkingLevel::Low) => "low".to_owned(),
        Some(ThinkingLevel::Medium) => "medium".to_owned(),
        Some(ThinkingLevel::High) => "high".to_owned(),
        _ => "high".to_owned(),
    }
}

fn supports_prompt_caching(model: &Model, env: Option<&ProviderEnv>) -> bool {
    let candidates = model_match_candidates(&model.id, Some(&model.name));
    if !candidates.iter().any(|candidate| candidate.contains("claude")) {
        return get_provider_env_value("AWS_BEDROCK_FORCE_CACHE", env).as_deref() == Some("1");
    }
    candidates.iter().any(|candidate| {
        candidate.contains("fable-5") || candidate.contains("opus-5") || candidate.contains("sonnet-5")
    }) || candidates.iter().any(|candidate| candidate.contains("-4-"))
        || candidates
            .iter()
            .any(|candidate| candidate.contains("claude-3-7-sonnet"))
        || candidates
            .iter()
            .any(|candidate| candidate.contains("claude-3-5-haiku"))
}

fn supports_thinking_signature(model: &Model) -> bool {
    is_anthropic_claude_model(model)
}

fn build_additional_model_request_fields(model: &Model, options: &BedrockOptions) -> Option<Value> {
    let reasoning = options.reasoning?;
    if !model.reasoning {
        return None;
    }
    if !is_anthropic_claude_model(model) {
        return None;
    }

    let display = if is_gov_cloud_bedrock_target(model, options) {
        None
    } else {
        Some(options.thinking_display.unwrap_or(BedrockThinkingDisplay::Summarized))
    };
    let adaptive = supports_adaptive_thinking(&model.id, Some(&model.name));

    let mut fields = Map::new();
    if adaptive {
        let mut thinking = Map::new();
        thinking.insert("type".to_owned(), json!("adaptive"));
        if let Some(display) = display {
            thinking.insert("display".to_owned(), json!(display));
        }
        fields.insert("thinking".to_owned(), Value::Object(thinking));
        fields.insert(
            "output_config".to_owned(),
            object(vec![(
                "effort",
                json!(map_thinking_level_to_effort(model, Some(reasoning))),
            )]),
        );
    } else {
        let level = match reasoning {
            ThinkingLevel::Xhigh | ThinkingLevel::Max | ThinkingLevel::Ultra => ThinkingLevel::High,
            other => other,
        };
        let default_budget = match level {
            ThinkingLevel::Minimal => 1024.0,
            ThinkingLevel::Low => 2048.0,
            ThinkingLevel::Medium => 8192.0,
            _ => 16384.0,
        };
        let budget = options
            .thinking_budgets
            .as_ref()
            .and_then(|budgets| match level {
                ThinkingLevel::Minimal => budgets.minimal,
                ThinkingLevel::Low => budgets.low,
                ThinkingLevel::Medium => budgets.medium,
                _ => budgets.high,
            })
            .unwrap_or(default_budget);
        let mut thinking = Map::new();
        thinking.insert("type".to_owned(), json!("enabled"));
        thinking.insert("budget_tokens".to_owned(), js_number_value(budget));
        if let Some(display) = display {
            thinking.insert("display".to_owned(), json!(display));
        }
        fields.insert("thinking".to_owned(), Value::Object(thinking));
        if options.interleaved_thinking.unwrap_or(true) {
            fields.insert("anthropic_beta".to_owned(), json!(["interleaved-thinking-2025-05-14"]));
        }
    }
    Some(Value::Object(fields))
}

fn is_gov_cloud_bedrock_target(model: &Model, options: &BedrockOptions) -> bool {
    if let Some(region) = get_configured_region(options)
        && region.to_lowercase().starts_with("us-gov-")
    {
        return true;
    }
    let model_id = model.id.to_lowercase();
    model_id.starts_with("us-gov.") || model_id.starts_with("arn:aws-us-gov:")
}

fn get_configured_region(options: &BedrockOptions) -> Option<String> {
    options
        .region
        .clone()
        .or_else(|| get_provider_env_value("AWS_REGION", options.stream.request.env.as_ref()))
        .or_else(|| get_provider_env_value("AWS_DEFAULT_REGION", options.stream.request.env.as_ref()))
}

fn map_stop_reason(reason: Option<&str>) -> (StopReason, Option<String>) {
    match reason {
        Some("end_turn" | "stop_sequence") => (StopReason::Stop, None),
        Some("max_tokens" | "model_context_window_exceeded") => (StopReason::Length, None),
        Some("tool_use") => (StopReason::ToolUse, None),
        Some(reason) if !reason.is_empty() => (StopReason::Error, Some(format!("Provider stopped with: {reason}"))),
        _ => (StopReason::Error, None),
    }
}

struct BlockState {
    content: AssistantContent,
    index: Option<i64>,
    partial_json: Option<String>,
}

fn sync_content(output: &mut AssistantMessage, blocks: &[BlockState]) {
    output.content = blocks.iter().map(|block| block.content.clone()).collect();
}

fn handle_message(
    sender: &AssistantMessageEventSender,
    model: &Model,
    output: &mut AssistantMessage,
    blocks: &mut Vec<BlockState>,
    message: &crate::api::bedrock_eventstream::EventStreamMessage,
    request_id: Option<&str>,
) -> Result<(), BedrockError> {
    let message_type = message.headers.get(":message-type").map(String::as_str);
    if message_type == Some("exception") || message_type == Some("error") {
        let name = message
            .headers
            .get(":exception-type")
            .or_else(|| message.headers.get(":error-code"))
            .cloned();
        let payload: Value = serde_json::from_slice(&message.payload).unwrap_or(Value::Null);
        let message_text = serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_owned());
        return Err(BedrockError {
            name,
            message: message_text,
            status: None,
            body: None,
            request_id: request_id.map(str::to_owned),
            service_exception: false,
        });
    }
    let Some(event_type) = message.headers.get(":event-type") else {
        return Ok(());
    };
    let payload: Value = serde_json::from_slice(&message.payload)
        .map_err(|error| BedrockError::stream(format!("Deserialization error: {error}")))?;

    match event_type.as_str() {
        "messageStart" => {
            let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
            if role != "assistant" {
                return Err(BedrockError::other(
                    "Unexpected assistant message start but got user message start instead",
                ));
            }
            sender.push(AssistantMessageEvent::Start {
                partial: output.clone(),
            });
        }
        "contentBlockStart" => {
            let index = payload.get("contentBlockIndex").and_then(Value::as_i64).unwrap_or(0);
            if let Some(tool_use) = payload.pointer("/start/toolUse") {
                let block = BlockState {
                    content: AssistantContent::ToolCall(ToolCall::new(
                        tool_use.get("toolUseId").and_then(Value::as_str).unwrap_or(""),
                        tool_use.get("name").and_then(Value::as_str).unwrap_or(""),
                        JsonObject::new(),
                    )),
                    index: Some(index),
                    partial_json: Some(String::new()),
                };
                blocks.push(block);
                sync_content(output, blocks);
                sender.push(AssistantMessageEvent::ToolCallStart {
                    content_index: blocks.len() - 1,
                    partial: output.clone(),
                });
            }
        }
        "contentBlockDelta" => {
            handle_content_block_delta(sender, output, blocks, &payload);
        }
        "contentBlockStop" => {
            handle_content_block_stop(sender, output, blocks, &payload);
        }
        "messageStop" => {
            let reason = payload.get("stopReason").and_then(Value::as_str);
            output.raw_stop_reason = reason.map(str::to_owned);
            let (stop_reason, error_message) = map_stop_reason(reason);
            output.stop_reason = stop_reason;
            if let Some(error_message) = error_message {
                output.error_message = Some(error_message);
            }
        }
        "metadata" => {
            if let Some(usage) = payload.get("usage") {
                handle_metadata(usage, model, output);
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_content_block_delta(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    blocks: &mut Vec<BlockState>,
    payload: &Value,
) {
    let content_block_index = payload.get("contentBlockIndex").and_then(Value::as_i64).unwrap_or(0);
    let Some(delta) = payload.get("delta") else {
        return;
    };
    let index = blocks.iter().position(|block| block.index == Some(content_block_index));

    if let Some(text) = delta.get("text").and_then(Value::as_str) {
        let (index, created) = match index {
            Some(index) => (index, false),
            None => {
                blocks.push(BlockState {
                    content: AssistantContent::Text(crate::types::TextContent::new("")),
                    index: Some(content_block_index),
                    partial_json: None,
                });
                (blocks.len() - 1, true)
            }
        };
        if let AssistantContent::Text(block) = &mut blocks[index].content {
            block.text.push_str(text);
        }
        sync_content(output, blocks);
        if created {
            sender.push(AssistantMessageEvent::TextStart {
                content_index: index,
                partial: output.clone(),
            });
        }
        sender.push(AssistantMessageEvent::TextDelta {
            content_index: index,
            delta: text.to_owned(),
            partial: output.clone(),
        });
        return;
    }

    if let Some(tool_use) = delta.get("toolUse")
        && let Some(index) = index
    {
        let input = tool_use.get("input").and_then(Value::as_str).unwrap_or("");
        let mut partial_json = blocks[index].partial_json.take().unwrap_or_default();
        partial_json.push_str(input);
        let arguments = parse_streaming_json(Some(&partial_json))
            .as_object()
            .cloned()
            .unwrap_or_default();
        blocks[index].partial_json = Some(partial_json);
        if let AssistantContent::ToolCall(block) = &mut blocks[index].content {
            block.arguments = arguments;
        }
        sync_content(output, blocks);
        sender.push(AssistantMessageEvent::ToolCallDelta {
            content_index: index,
            delta: input.to_owned(),
            partial: output.clone(),
        });
        return;
    }

    if let Some(reasoning) = delta.get("reasoningContent") {
        let (index, created) = match index {
            Some(index) => (index, false),
            None => {
                blocks.push(BlockState {
                    content: AssistantContent::Thinking(ThinkingContent {
                        r#type: crate::types::ThinkingTag::Tag,
                        thinking: String::new(),
                        thinking_signature: Some(String::new()),
                        redacted: None,
                    }),
                    index: Some(content_block_index),
                    partial_json: None,
                });
                (blocks.len() - 1, true)
            }
        };
        let text = reasoning
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned);
        let signature = reasoning
            .get("signature")
            .and_then(Value::as_str)
            .filter(|signature| !signature.is_empty())
            .map(str::to_owned);
        if let AssistantContent::Thinking(block) = &mut blocks[index].content {
            if let Some(text) = &text {
                block.thinking.push_str(text);
            }
            if let Some(signature) = signature {
                block
                    .thinking_signature
                    .get_or_insert_with(String::new)
                    .push_str(&signature);
            }
        }
        sync_content(output, blocks);
        if created {
            sender.push(AssistantMessageEvent::ThinkingStart {
                content_index: index,
                partial: output.clone(),
            });
        }
        if let Some(text) = text {
            sender.push(AssistantMessageEvent::ThinkingDelta {
                content_index: index,
                delta: text,
                partial: output.clone(),
            });
        }
    }
}

fn handle_content_block_stop(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    blocks: &mut [BlockState],
    payload: &Value,
) {
    let content_block_index = payload.get("contentBlockIndex").and_then(Value::as_i64).unwrap_or(0);
    let Some(index) = blocks.iter().position(|block| block.index == Some(content_block_index)) else {
        return;
    };
    blocks[index].index = None;
    let partial_json = blocks[index].partial_json.take();
    let event = match &mut blocks[index].content {
        AssistantContent::Text(block) => Some(AssistantMessageEvent::TextEnd {
            content_index: index,
            content: block.text.clone(),
            partial: output.clone(),
        }),
        AssistantContent::Thinking(block) => Some(AssistantMessageEvent::ThinkingEnd {
            content_index: index,
            content: block.thinking.clone(),
            partial: output.clone(),
        }),
        AssistantContent::ToolCall(block) => {
            block.arguments = parse_streaming_json(partial_json.as_deref())
                .as_object()
                .cloned()
                .unwrap_or_default();
            let tool_call = block.clone();
            Some(AssistantMessageEvent::ToolCallEnd {
                content_index: index,
                tool_call,
                partial: output.clone(),
            })
        }
    };
    sync_content(output, blocks);
    if let Some(event) = event {
        sender.push(event);
    }
}

fn handle_metadata(usage: &Value, model: &Model, output: &mut AssistantMessage) {
    let input = usage.get("inputTokens").and_then(Value::as_f64).unwrap_or(0.0);
    let output_tokens = usage.get("outputTokens").and_then(Value::as_f64).unwrap_or(0.0);
    let cache_read = usage.get("cacheReadInputTokens").and_then(Value::as_f64).unwrap_or(0.0);
    let cache_write = usage
        .get("cacheWriteInputTokens")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let total = usage
        .get("totalTokens")
        .and_then(Value::as_f64)
        .filter(|total| *total != 0.0)
        .unwrap_or(input + output_tokens);
    output.usage = Usage {
        input,
        output: output_tokens,
        cache_read,
        cache_write,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: total,
        cost: crate::types::UsageCost::default(),
    };
    let cost = calculate_cost(model, &output.usage);
    output.usage.cost = cost;
}

struct RuntimeConfig {
    region: String,
    endpoint: String,
    credentials: Option<AwsCredentials>,
    bearer_token: Option<String>,
}

async fn resolve_runtime_config(model: &Model, options: &BedrockOptions) -> Result<RuntimeConfig, BedrockError> {
    let env = options.stream.request.env.as_ref();
    let options_profile = options
        .profile
        .clone()
        .or_else(|| get_provider_env_value("AWS_PROFILE", env));
    let profile = profile_name(options_profile.as_deref(), env);
    let configured_region = get_configured_region(options);
    let has_ambient_profile = get_provider_env_value("AWS_PROFILE", env).is_some();
    let endpoint_region = standard_endpoint_region(&model.base_url);
    let use_explicit_endpoint = match endpoint_region.as_deref() {
        None => true,
        Some(_) => configured_region.is_none() && !has_ambient_profile,
    };

    let skip_auth = get_provider_env_value("AWS_BEDROCK_SKIP_AUTH", env).as_deref() == Some("1");
    let bearer_token = options
        .bearer_token
        .clone()
        .or_else(|| options.stream.request.api_key.clone())
        .or_else(|| get_provider_env_value("AWS_BEARER_TOKEN_BEDROCK", env));
    let use_bearer = bearer_token.is_some() && !skip_auth;

    let region = match arn_region(&model.id) {
        Some(region) => region,
        None => match configured_region {
            Some(region) => region,
            None => match endpoint_region.filter(|_| use_explicit_endpoint) {
                Some(region) => region,
                None if !has_ambient_profile => "us-east-1".to_owned(),
                None => profile_region(&profile, env).ok_or_else(|| {
                    BedrockError::other("Region is missing; configure AWS_REGION or a profile region")
                })?,
            },
        },
    };

    let endpoint = if use_explicit_endpoint {
        model.base_url.clone()
    } else {
        default_endpoint(&region)
    };

    let credentials = if skip_auth {
        Some(AwsCredentials {
            access_key_id: "dummy-access-key".to_owned(),
            secret_access_key: "dummy-secret-key".to_owned(),
            session_token: None,
        })
    } else if use_bearer {
        None
    } else if options_profile.is_none() {
        match env_credentials(env) {
            Some(credentials) => Some(credentials),
            None => {
                let fetch = options.stream.request.fetch.clone().unwrap_or_else(default_fetch);
                profile_credentials(&profile, env, fetch)
                    .await
                    .map_err(|error| BedrockError::other(error.message))?
                    .ok_or_else(|| BedrockError::other("Could not load credentials from any providers"))?
                    .into()
            }
        }
    } else {
        let fetch = options.stream.request.fetch.clone().unwrap_or_else(default_fetch);
        Some(
            profile_credentials(&profile, env, fetch)
                .await
                .map_err(|error| BedrockError::other(error.message))?
                .ok_or_else(|| BedrockError::other("Could not load credentials from any providers"))?,
        )
    };

    Ok(RuntimeConfig {
        region,
        endpoint,
        credentials,
        bearer_token: use_bearer.then_some(bearer_token).flatten(),
    })
}

fn standard_endpoint_region(base_url: &str) -> Option<String> {
    let rest = base_url.split_once("://")?.1;
    let authority = rest.split('/').next()?;
    let hostname = authority.split(':').next()?.to_lowercase();
    let remainder = hostname
        .strip_prefix("bedrock-runtime-fips.")
        .or_else(|| hostname.strip_prefix("bedrock-runtime."))?;
    let region = remainder
        .strip_suffix(".amazonaws.com.cn")
        .or_else(|| remainder.strip_suffix(".amazonaws.com"))?;
    if region.is_empty()
        || !region
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
    {
        return None;
    }
    Some(region.to_owned())
}

fn arn_region(model_id: &str) -> Option<String> {
    let rest = model_id.strip_prefix("arn:")?;
    let (partition, rest) = rest.split_once(':')?;
    if partition != "aws" && !partition.starts_with("aws-") {
        return None;
    }
    let (service, rest) = rest.split_once(':')?;
    if service != "bedrock" {
        return None;
    }
    let (region, _) = rest.split_once(':')?;
    if region.is_empty() {
        return None;
    }
    Some(region.to_owned())
}

fn default_endpoint(region: &str) -> String {
    if region.starts_with("cn-") {
        format!("https://bedrock-runtime.{region}.amazonaws.com.cn")
    } else {
        format!("https://bedrock-runtime.{region}.amazonaws.com")
    }
}

async fn execute_request(
    config: &RuntimeConfig,
    options: &BedrockOptions,
    model: &Model,
    body: Vec<u8>,
) -> Result<HttpResponse, BedrockError> {
    let url = format!(
        "{}/model/{}/converse-stream",
        config.endpoint.trim_end_matches('/'),
        crate::api::bedrock_sigv4::aws_escape_uri(&model.id)
    );
    let invocation_id = random_uuid_v4();
    let mut headers = base_headers(&url, &body, &invocation_id, 1);
    apply_custom_headers(&mut headers, options.stream.request.headers.as_ref());
    let fetch = options.stream.request.fetch.clone().unwrap_or_else(default_fetch);

    let mut attempt = 1usize;
    loop {
        set_header(
            &mut headers,
            "amz-sdk-request",
            &format!("attempt={attempt}; max={MAX_ATTEMPTS}"),
        );
        if let Some(token) = &config.bearer_token {
            set_header_exact(&mut headers, "Authorization", &format!("Bearer {token}"));
        } else if let Some(credentials) = &config.credentials {
            sign_request(
                "POST",
                &url,
                &mut headers,
                &body,
                credentials,
                (&config.region, SERVICE),
                now_millis(),
            )
            .map_err(|error| BedrockError::other(error.message()))?;
        }

        let request = HttpRequest {
            method: "POST".to_owned(),
            url: url.clone(),
            headers: headers.clone(),
            body: Some(body.clone()),
        };
        let response = match fetch.fetch(request, options.signal()).await {
            Ok(response) => response,
            Err(error) => {
                if options.signal().as_ref().is_some_and(|signal| signal.is_cancelled()) {
                    return Err(BedrockError::abort(error.message));
                }
                if attempt < MAX_ATTEMPTS {
                    let delay = retry_delay(attempt, None);
                    sleep_or_abort(delay, options.signal()).await?;
                    attempt += 1;
                    continue;
                }
                return Err(BedrockError::transport(error.message));
            }
        };

        if (200..300).contains(&response.status) {
            return Ok(response);
        }

        let status = response.status;
        let headers = response.headers;
        let body = read_body(response.body).await?;
        let error = http_error(status, &headers, &body);
        if attempt < MAX_ATTEMPTS && is_retryable(&error) {
            let delay = retry_delay(attempt, retry_after_ms(&headers));
            sleep_or_abort(delay, options.signal()).await?;
            attempt += 1;
            continue;
        }
        return Err(error);
    }
}

fn base_headers(url: &str, body: &[u8], invocation_id: &str, attempt: usize) -> Vec<(String, String)> {
    let host = url
        .split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(rest))
        .unwrap_or(url);
    vec![
        ("content-type".to_owned(), "application/json".to_owned()),
        ("content-length".to_owned(), body.len().to_string()),
        ("x-amz-user-agent".to_owned(), AWS_SDK_USER_AGENT.to_owned()),
        (
            "user-agent".to_owned(),
            format!("{AWS_SDK_USER_AGENT} ua/2.1 os/linux lang/rust api/bedrock-runtime#3.1048.0"),
        ),
        ("host".to_owned(), host.to_owned()),
        ("amz-sdk-invocation-id".to_owned(), invocation_id.to_owned()),
        (
            "amz-sdk-request".to_owned(),
            format!("attempt={attempt}; max={MAX_ATTEMPTS}"),
        ),
    ]
}

fn apply_custom_headers(headers: &mut Vec<(String, String)>, custom: Option<&crate::types::ProviderHeaders>) {
    let Some(custom) = provider_headers_to_record(custom) else {
        return;
    };
    for (key, value) in custom {
        if !is_reserved_header(&key) {
            set_header_exact(headers, &key, &value);
        }
    }
}

fn is_reserved_header(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.starts_with("x-amz-") || lower == "authorization" || lower == "host"
}

fn set_header(headers: &mut Vec<(String, String)>, name: &str, value: &str) {
    if let Some(header) = headers.iter_mut().find(|(key, _)| key.eq_ignore_ascii_case(name)) {
        header.0 = name.to_owned();
        header.1 = value.to_owned();
    } else {
        headers.push((name.to_owned(), value.to_owned()));
    }
}

fn set_header_exact(headers: &mut Vec<(String, String)>, name: &str, value: &str) {
    if let Some(header) = headers.iter_mut().find(|(key, _)| key == name) {
        header.1 = value.to_owned();
    } else {
        headers.push((name.to_owned(), value.to_owned()));
    }
}

fn random_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("OS random source must be available");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut hex = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            hex.push('-');
        }
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0x0f) as usize] as char);
    }
    hex
}

const HEX: &[u8; 16] = b"0123456789abcdef";

async fn read_body(mut body: Box<dyn HttpBody>) -> Result<Vec<u8>, BedrockError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend_from_slice(&chunk.map_err(|error| BedrockError::abort_or_transport(error.message))?);
    }
    Ok(bytes)
}

fn retry_after_ms(headers: &[(String, String)]) -> Option<f64> {
    for (name, value) in headers {
        let lower = name.to_ascii_lowercase();
        if lower == "x-amz-retry-after" {
            // Smithy reads the hint with `Number` and treats an invalid date as
            // an immediate retry.
            let millis = parse_number(value);
            if millis.is_nan() {
                return None;
            }
            return Some(finite_retry_delay(millis));
        }
        if lower == "retry-after" {
            return smithy_retry_after_ms(value);
        }
    }
    None
}

/// Smithy `parseRetryAfterHeader` for `retry-after`: numeric seconds, the
/// `" GMT, <seconds>"` suffix, an HTTP date, or `Date.parse`.
fn smithy_retry_after_ms(value: &str) -> Option<f64> {
    if value.ends_with("GMT") {
        let date = httpdate::parse_http_date(value).ok()?;
        let millis = date.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as f64;
        return Some(finite_retry_delay(millis - now_millis() as f64));
    }
    if let Some(seconds) = value.rsplit_once(" GMT, ").map(|(_, seconds)| seconds)
        && is_plain_decimal(seconds)
    {
        return Some(finite_retry_delay(seconds.parse::<f64>().ok()? * 1000.0));
    }
    if is_plain_decimal(value) {
        return Some(finite_retry_delay(value.parse::<f64>().ok()? * 1000.0));
    }
    let date = date_parse(value);
    (date >= now_millis() as f64).then(|| finite_retry_delay(date - now_millis() as f64))
}

/// Smithy numeric hint pattern: `\d+` or `\d+\.\d+`.
fn is_plain_decimal(value: &str) -> bool {
    match value.split_once('.') {
        Some((whole, fraction)) => {
            !whole.is_empty()
                && !fraction.is_empty()
                && whole.bytes().all(|byte| byte.is_ascii_digit())
                && fraction.bytes().all(|byte| byte.is_ascii_digit())
        }
        None => !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()),
    }
}

/// Smithy builds `new Date(now + delay)`; a non-finite delay makes the date
/// invalid and the retry immediate.
fn finite_retry_delay(delay_ms: f64) -> f64 {
    if delay_ms.is_finite() { delay_ms.max(0.0) } else { 0.0 }
}

fn retry_delay(attempt: usize, retry_after_ms: Option<f64>) -> std::time::Duration {
    if let Some(retry_after_ms) = retry_after_ms {
        return std::time::Duration::from_millis(retry_after_ms.max(0.0) as u64);
    }
    let base = 100u64;
    let exponential = base.saturating_mul(1u64 << (attempt.saturating_sub(1)).min(8));
    let jitter = random_u64() % base.max(1);
    std::time::Duration::from_millis((exponential + jitter).min(20_000))
}

fn random_u64() -> u64 {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("OS random source must be available");
    u64::from_le_bytes(bytes)
}

async fn sleep_or_abort(
    delay: std::time::Duration,
    signal: Option<tokio_util::sync::CancellationToken>,
) -> Result<(), BedrockError> {
    let Some(signal) = signal else {
        tokio::time::sleep(delay).await;
        return Ok(());
    };
    tokio::select! {
        () = signal.cancelled() => Err(BedrockError::abort("Request was aborted")),
        () = tokio::time::sleep(delay) => Ok(()),
    }
}

fn is_retryable(error: &BedrockError) -> bool {
    matches!(error.status, Some(429.0 | 500.0 | 502.0 | 503.0 | 504.0))
        || error.name.as_deref().is_some_and(|name| {
            matches!(
                name,
                "ThrottlingException"
                    | "TooManyRequestsException"
                    | "RequestLimitExceeded"
                    | "RequestThrottled"
                    | "RequestThrottledException"
                    | "ThrottledException"
                    | "Throttling"
                    | "SlowDown"
                    | "BandwidthLimitExceeded"
                    | "LimitExceededException"
                    | "PriorRequestNotComplete"
                    | "ProvisionedThroughputExceededException"
                    | "TransactionInProgressException"
            )
        })
        || error
            .name
            .as_deref()
            .is_some_and(|name| matches!(name, "TimeoutError" | "RequestTimeout" | "RequestTimeoutException"))
}

fn http_error(status: u16, headers: &[(String, String)], body: &[u8]) -> BedrockError {
    let code = header_value(headers, "x-amzn-errortype").and_then(|value| {
        value
            .split(':')
            .next()
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(str::to_owned)
    });
    let request_id = header_value(headers, "x-amzn-requestid").map(str::to_owned);
    let text = String::from_utf8_lossy(body).into_owned();
    let parsed = serde_json::from_str::<Value>(&text).ok();
    let parsed_message = parsed.as_ref().and_then(|value| {
        value
            .get("message")
            .or_else(|| value.get("Message"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let (message, body) = match parsed_message {
        Some(message) => (message, None),
        None => (code.clone().unwrap_or_else(|| "UnknownError".to_owned()), Some(text)),
    };
    BedrockError {
        name: code.clone(),
        message,
        status: Some(f64::from(status)),
        body,
        request_id,
        service_exception: code.is_some(),
    }
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn response_request_id(response: &HttpResponse) -> Option<String> {
    header_value(&response.headers, "x-amzn-requestid")
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= MAX_BEDROCK_DIAGNOSTIC_VALUE_CHARS)
        .map(str::to_owned)
}

#[derive(Debug, Clone)]
struct BedrockError {
    name: Option<String>,
    message: String,
    status: Option<f64>,
    body: Option<String>,
    request_id: Option<String>,
    service_exception: bool,
}

impl BedrockError {
    fn other(message: impl Into<String>) -> Self {
        Self {
            name: None,
            message: message.into(),
            status: None,
            body: None,
            request_id: None,
            service_exception: false,
        }
    }

    fn stream(message: impl Into<String>) -> Self {
        Self::other(message)
    }

    fn transport(message: impl Into<String>) -> Self {
        Self::other(message)
    }

    fn abort(message: impl Into<String>) -> Self {
        Self::other(message)
    }

    fn abort_or_transport(message: impl Into<String>) -> Self {
        Self::other(message)
    }

    fn with_request_id(mut self, request_id: Option<&str>) -> Self {
        if self.request_id.is_none() {
            self.request_id = request_id.map(str::to_owned);
        }
        self
    }
}

fn format_bedrock_error(error: &BedrockError) -> String {
    let message_carries_body = error.body.as_ref().is_none_or(|body| error.message.contains(body));
    let core = match (message_carries_body, error.status, error.body.as_deref()) {
        (false, Some(status), Some(body)) => format!("{}: {body}", js_number_to_string(status)),
        _ => error.message.clone(),
    };
    let hint = if core.to_lowercase().contains("data retention mode") {
        format!(" See {BEDROCK_DATA_RETENTION_DOCS_URL} for supported data retention modes.")
    } else {
        String::new()
    };
    if error.service_exception {
        let prefix = error.name.as_deref().map(bedrock_error_prefix).unwrap_or_default();
        format!("{prefix}: {core}{hint}")
    } else {
        format!("{core}{hint}")
    }
}

fn bedrock_error_prefix(name: &str) -> String {
    match name {
        "InternalServerException" => "Internal server error".to_owned(),
        "ModelStreamErrorException" => "Model stream error".to_owned(),
        "ValidationException" => "Validation error".to_owned(),
        "ThrottlingException" => "Throttling error".to_owned(),
        "ServiceUnavailableException" => "Service unavailable".to_owned(),
        other => other.to_owned(),
    }
}

fn normalize_diagnostic_value(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_BEDROCK_DIAGNOSTIC_VALUE_CHARS {
        return None;
    }
    Some(trimmed.to_owned())
}

fn extract_bedrock_error_code(error: &BedrockError) -> Option<String> {
    let name = error.name.as_deref()?;
    if !name.ends_with("Exception") {
        return None;
    }
    normalize_diagnostic_value(Some(name))
}

fn append_bedrock_failure_diagnostic(output: &mut AssistantMessage, error: &BedrockError) {
    let mut details = Map::new();
    if let Some(status) = error.status
        && status.fract() == 0.0
    {
        details.insert("status".to_owned(), js_number_value(status));
    }
    if let Some(code) = extract_bedrock_error_code(error) {
        details.insert("errorCode".to_owned(), json!(code));
    }
    if let Some(request_id) = normalize_diagnostic_value(error.request_id.as_deref()) {
        details.insert("requestId".to_owned(), json!(request_id));
    }
    if details.is_empty() {
        return;
    }
    append_assistant_message_diagnostic(
        &mut output.diagnostics,
        AssistantMessageDiagnostic {
            r#type: "bedrock_response_failure".to_owned(),
            timestamp: now_millis(),
            error: None,
            details: Some(details),
        },
    );
}

#[cfg(test)]
mod retry_hint_tests {
    use super::*;

    #[test]
    fn retry_after_hints_follow_smithy_parsing() {
        let header = |name: &str, value: &str| vec![(name.to_owned(), value.to_owned())];
        assert_eq!(retry_after_ms(&header("x-amz-retry-after", "0x10")), Some(16.0));
        assert_eq!(retry_after_ms(&header("x-amz-retry-after", " 5 ")), Some(5.0));
        assert_eq!(retry_after_ms(&header("x-amz-retry-after", "nope")), None);
        assert_eq!(retry_after_ms(&header("x-amz-retry-after", "Infinity")), Some(0.0));
        assert_eq!(retry_after_ms(&header("retry-after", "1.5")), Some(1500.0));
        assert_eq!(retry_after_ms(&header("retry-after", "+1")), None);
        assert_eq!(retry_after_ms(&header("retry-after", "0x10")), None);
        assert_eq!(
            retry_after_ms(&header("retry-after", "Mon, 01 Jan 2020 00:00:00 GMT, 30")),
            Some(30_000.0)
        );
    }
}
