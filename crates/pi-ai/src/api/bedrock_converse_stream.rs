//! Port of `packages/ai/src/api/bedrock-converse-stream.ts`.
//!
//! HTTP is always sent through the shared [`Fetch`] contract. SigV4 signing,
//! AWS event-stream framing, and the Node credential chain are implemented in
//! this crate and proven against the pinned TypeScript module through the
//! differential oracle.

use indexmap::IndexMap;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::bedrock_credentials::{env_credentials, profile_credentials, profile_name, profile_region};
use crate::api::bedrock_eventstream::EventStreamDecoder;
use crate::api::bedrock_sigv4::{AwsCredentials, sign_request};
use crate::api::simple_options::{
    adjust_max_tokens_for_thinking, build_base_options, clamp_max_tokens_to_context, clamp_reasoning,
    model_thinking_level,
};
use crate::api::transform_messages::transform_messages_with;
use crate::http::default_fetch;
use crate::models::calculate_cost;
use crate::request::{
    CallbackError, HttpBody, HttpRequest, HttpResponse, OnPayload, ProviderStreams, SimpleStreamOptions, StreamOptions,
    header_value, now_millis, request_url,
};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, DoneReason, ErrorReason,
    JsonObject, Message, Model, ProviderEnv, StopReason, ThinkingBudgets, ThinkingContent, ThinkingLevel, Tool,
    ToolCall, ToolResultContent, ToolResultMessage, Usage,
};
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
    pub region: Option<JsString>,
    pub profile: Option<JsString>,
    pub tool_choice: Option<BedrockToolChoice>,
    pub reasoning: Option<ThinkingLevel>,
    pub thinking_budgets: Option<ThinkingBudgets>,
    pub interleaved_thinking: Option<bool>,
    pub thinking_display: Option<BedrockThinkingDisplay>,
    pub request_metadata: Option<JsonObject>,
    pub bearer_token: Option<JsString>,
}

impl BedrockOptions {
    /// Parse API-specific fields from `StreamOptions::extra`.
    pub fn from_stream_options(stream: StreamOptions) -> Result<Self, String> {
        let extra = stream.extra.clone();
        let take = |key: &str| extra.get(key).cloned();
        Ok(Self {
            region: optional_string(take("region"))?.map(JsString::from),
            profile: optional_string(take("profile"))?.map(JsString::from),
            tool_choice: optional_json::<BedrockToolChoice>(take("toolChoice"))?,
            reasoning: optional_json::<ThinkingLevel>(take("reasoning"))?,
            thinking_budgets: optional_json::<ThinkingBudgets>(take("thinkingBudgets"))?,
            interleaved_thinking: optional_bool(take("interleavedThinking"))?,
            thinking_display: optional_json::<BedrockThinkingDisplay>(take("thinkingDisplay"))?,
            request_metadata: optional_object(take("requestMetadata"))?,
            bearer_token: optional_string(take("bearerToken"))?.map(JsString::from),
            stream,
        })
    }

    pub fn signal(&self) -> Option<pi_js_compat::abort::AbortSignal> {
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
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(stream(model, context, options))
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(stream_simple(model, context, options))
    }
}

async fn run_stream(
    sender: AssistantMessageEventSender,
    model: Model,
    context: Context,
    stream_options: StreamOptions,
) {
    let timestamp = now_millis() as f64;
    let mut output = AssistantMessage::empty("bedrock-converse-stream", &model.provider, &model.id, timestamp);
    let signal = stream_options.request.signal.clone();
    let result = drive(&sender, &model, &context, stream_options, &mut output).await;
    let Err(error) = result else {
        return;
    };

    for block in &mut output.content {
        strip_bedrock_stream_scratch(block);
    }
    output.stop_reason = if signal.as_ref().is_some_and(|signal| signal.aborted()) {
        StopReason::Aborted
    } else {
        StopReason::Error
    };
    output.error_message = Some(format_bedrock_error(&error));
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
) -> Result<(), BedrockError> {
    let options = BedrockOptions::from_stream_options(stream_options)
        .map_err(|message| BedrockError::other(format!("Invalid Bedrock options: {message}")))?;
    let config = load_runtime_config(model, &options).await?;
    let cache_retention = resolve_cache_retention(options.stream.cache_retention, options.stream.request.env.as_ref());
    let inference_max_tokens = options
        .stream
        .max_tokens
        .or_else(|| is_anthropic_claude_model(model).then_some(model.max_tokens));

    let command_input = build_command_input(
        model,
        context,
        &options,
        cache_retention,
        inference_max_tokens,
        output.timestamp,
    )?;
    let mut command_input = command_input;
    if let Some(on_payload) = &options.stream.request.on_payload {
        command_input = apply_on_payload(on_payload, command_input, model).await?;
    }
    let mut body_value = command_input;
    if let Some(object) = body_value.as_object_mut() {
        object.shift_remove(&JsString::from("modelId"));
    }
    let body = serde_json::to_vec(&body_value)
        .map_err(|error| BedrockError::other(format!("Failed to serialize Bedrock request: {error}")))?;
    let response = execute_request(&config, &options, model, body).await?;
    let request_id = response_request_id(&response);
    if let Some(on_response) = &options.stream.request.on_response {
        let mut headers = IndexMap::new();
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
        let Some(chunk) = body.next_chunk().await else {
            break;
        };
        let chunk = chunk.map_err(|error| {
            if options.signal().as_ref().is_some_and(AbortSignal::aborted) {
                BedrockError::abort("aborted")
            } else {
                BedrockError::abort_or_transport(error.message)
            }
        })?;
        decoder.push(&chunk);
        while let Some(message) = decoder
            .next_message()
            .map_err(|error| BedrockError::stream(error.message()).with_request_id(request_id.as_deref()))?
        {
            handle_message(sender, model, output, &message, request_id.as_deref())?;
        }
    }

    if options.signal().as_ref().is_some_and(|signal| signal.aborted()) {
        return Err(BedrockError::abort("Request was aborted"));
    }
    match output.stop_reason {
        StopReason::Error | StopReason::Aborted => Err(BedrockError::stream(
            output
                .error_message
                .clone()
                .unwrap_or_else(|| "An unknown error occurred".to_owned().into()),
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

async fn apply_on_payload(on_payload: &OnPayload, payload: JsValue, model: &Model) -> Result<JsValue, BedrockError> {
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
    if get_provider_env_value("PI_CACHE_RETENTION", env).is_some_and(|value| value == "long") {
        return CacheRetention::Long;
    }
    CacheRetention::Short
}

fn build_command_input(
    model: &Model,
    context: &Context,
    options: &BedrockOptions,
    cache_retention: CacheRetention,
    inference_max_tokens: Option<f64>,
    now: f64,
) -> Result<JsValue, BedrockError> {
    let env = options.stream.request.env.as_ref();
    let mut inference_config = JsObject::new();
    if let Some(max_tokens) = inference_max_tokens {
        inference_config.insert("maxTokens".into(), js_number_value(max_tokens).into());
    }
    if let Some(temperature) = options.stream.temperature {
        inference_config.insert("temperature".into(), js_number_value(temperature).into());
    }

    let mut input = JsObject::new();
    input.insert("modelId".into(), pi_js_compat::js_json!(model.id));
    input.insert(
        "messages".into(),
        JsValue::Array(convert_messages(context, model, cache_retention, env, now)?),
    );
    if let Some(system) = build_system_prompt(context.system_prompt.as_ref(), model, cache_retention, env) {
        input.insert("system".into(), JsValue::Array(system));
    }
    input.insert("inferenceConfig".into(), JsValue::Object(inference_config));
    if let Some(tool_config) = convert_tool_config(context.tools.as_deref(), options.tool_choice.as_ref()) {
        input.insert("toolConfig".into(), tool_config);
    }
    if let Some(fields) = build_additional_model_request_fields(model, options) {
        input.insert("additionalModelRequestFields".into(), fields);
    }
    if let Some(metadata) = &options.request_metadata {
        input.insert("requestMetadata".into(), JsValue::from(Value::Object(metadata.clone())));
    }
    Ok(JsValue::Object(input))
}

fn convert_messages(
    context: &Context,
    model: &Model,
    cache_retention: CacheRetention,
    env: Option<&ProviderEnv>,
    now: f64,
) -> Result<Vec<JsValue>, BedrockError> {
    let normalize = |id: &str, _model: &Model, _source: &AssistantMessage| normalize_tool_call_id(id);
    let transformed = transform_messages_with(&context.messages, model, Some(&normalize), now);
    let mut result = Vec::new();
    let mut index = 0usize;
    while index < transformed.len() {
        match &transformed[index] {
            Message::User(message) => {
                result.push(convert_user_message(message)?);
            }
            Message::Assistant(message) => {
                if let Some(converted) = convert_assistant_message(message, model) {
                    result.push(converted);
                }
            }
            Message::ToolResult(message) => {
                let mut tool_results = Vec::new();
                tool_results.push(convert_tool_result(message)?);
                let mut next = index + 1;
                while let Some(Message::ToolResult(next_message)) = transformed.get(next) {
                    tool_results.push(convert_tool_result(next_message)?);
                    next += 1;
                }
                index = next - 1;
                result.push(object(vec![
                    ("role", pi_js_compat::js_json!("user")),
                    ("content", JsValue::Array(tool_results)),
                ]));
            }
        }
        index += 1;
    }

    if cache_retention != CacheRetention::None
        && supports_prompt_caching(model, env)
        && let Some(JsValue::Object(last)) = result.last_mut()
        && last.get("role") == Some(&pi_js_compat::js_json!("user"))
        && let Some(JsValue::Array(content)) = last.get_mut("content")
    {
        content.push(cache_point(cache_retention));
    }
    Ok(result)
}

fn convert_user_message(message: &crate::types::UserMessage) -> Result<JsValue, BedrockError> {
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
                        content.push(create_image_block(&image.mime_type, &image.data)?);
                    }
                }
            }
            if content.is_empty() {
                content.push(placeholder_text_block());
            }
            content
        }
    };
    Ok(object(vec![
        ("role", pi_js_compat::js_json!("user")),
        ("content", JsValue::Array(content)),
    ]))
}

fn convert_assistant_message(message: &AssistantMessage, model: &Model) -> Option<JsValue> {
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
                        ("toolUseId", pi_js_compat::js_json!(tool_call.id)),
                        ("name", pi_js_compat::js_json!(tool_call.name)),
                        ("input", JsValue::Object(tool_call.arguments.clone())),
                    ]),
                )]));
            }
            AssistantContent::Thinking(thinking) => {
                let text = sanitize_surrogates(&thinking.thinking);
                if JsString::from(text.as_str()).trim().is_empty() {
                    continue;
                }
                if supports_thinking_signature(model) {
                    match thinking
                        .thinking_signature
                        .as_deref()
                        .filter(|signature| !JsString::from(*signature).trim().is_empty())
                    {
                        Some(signature) => content.push(object(vec![(
                            "reasoningContent",
                            object(vec![(
                                "reasoningText",
                                object(vec![
                                    ("text", pi_js_compat::js_json!(text)),
                                    ("signature", pi_js_compat::js_json!(signature)),
                                ]),
                            )]),
                        )])),
                        None => content.push(object(vec![("text", pi_js_compat::js_json!(text))])),
                    }
                } else {
                    content.push(object(vec![(
                        "reasoningContent",
                        object(vec![(
                            "reasoningText",
                            object(vec![("text", pi_js_compat::js_json!(text))]),
                        )]),
                    )]));
                }
            }
        }
    }
    if content.is_empty() {
        return None;
    }
    Some(object(vec![
        ("role", pi_js_compat::js_json!("assistant")),
        ("content", JsValue::Array(content)),
    ]))
}

fn convert_tool_result(message: &ToolResultMessage) -> Result<JsValue, BedrockError> {
    Ok(object(vec![(
        "toolResult",
        object(vec![
            ("toolUseId", pi_js_compat::js_json!(message.tool_call_id)),
            (
                "content",
                JsValue::Array(convert_tool_result_content(&message.content)?),
            ),
            (
                "status",
                pi_js_compat::js_json!(if message.is_error { "error" } else { "success" }),
            ),
        ]),
    )]))
}

fn convert_tool_result_content(content: &[ToolResultContent]) -> Result<Vec<JsValue>, BedrockError> {
    let mut result = Vec::new();
    for block in content {
        match block {
            ToolResultContent::Image(image) => {
                result.push(create_image_block(&image.mime_type, &image.data)?);
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
    Ok(result)
}

fn build_system_prompt(
    system_prompt: Option<&pi_js_compat::js_json::JsString>,
    model: &Model,
    cache_retention: CacheRetention,
    env: Option<&ProviderEnv>,
) -> Option<Vec<JsValue>> {
    let system_prompt = system_prompt.filter(|prompt| !prompt.is_empty())?;
    let mut blocks = vec![object(vec![(
        "text",
        pi_js_compat::js_json!(sanitize_surrogates(system_prompt)),
    )])];
    if cache_retention != CacheRetention::None && supports_prompt_caching(model, env) {
        blocks.push(cache_point(cache_retention));
    }
    Some(blocks)
}

fn cache_point(cache_retention: CacheRetention) -> JsValue {
    let mut point = JsObject::new();
    point.insert("type".into(), pi_js_compat::js_json!("default"));
    if cache_retention == CacheRetention::Long {
        point.insert("ttl".into(), pi_js_compat::js_json!("1h"));
    }
    object(vec![("cachePoint", JsValue::Object(point))])
}

fn non_blank_text_block(text: impl Into<JsString>) -> Option<JsValue> {
    let sanitized = JsString::from(sanitize_surrogates(text));
    if sanitized.trim().is_empty() {
        None
    } else {
        Some(object(vec![("text", pi_js_compat::js_json!(sanitized))]))
    }
}

fn required_text_block(text: impl Into<JsString>) -> JsValue {
    non_blank_text_block(text).unwrap_or_else(placeholder_text_block)
}

fn placeholder_text_block() -> JsValue {
    object(vec![("text", pi_js_compat::js_json!(EMPTY_TEXT_PLACEHOLDER))])
}

fn normalize_tool_call_id(id: &str) -> String {
    id.encode_utf16()
        .take(64)
        .map(|unit| match u8::try_from(unit) {
            Ok(byte) if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') => char::from(byte),
            _ => '_',
        })
        .collect()
}

fn convert_tool_config(tools: Option<&[Tool]>, tool_choice: Option<&BedrockToolChoice>) -> Option<JsValue> {
    let tools = tools.filter(|tools| !tools.is_empty())?;
    if tool_choice == Some(&BedrockToolChoice::None) {
        return None;
    }

    let mut bedrock_tools = Vec::with_capacity(tools.len());
    for tool in tools {
        // Frozen906-910: preserve the tool schema; unknown compat and
        // constrainedSampling metadata never add strictness or rewrite it.
        let mut spec = JsObject::new();
        spec.insert("name".into(), pi_js_compat::js_json!(tool.name));
        spec.insert(
            "inputSchema".into(),
            object(vec![("json", tool.parameters.clone().into())]),
        );
        spec.insert("description".into(), pi_js_compat::js_json!(tool.description));
        bedrock_tools.push(object(vec![("toolSpec", JsValue::Object(spec))]));
    }

    let bedrock_tool_choice = match tool_choice {
        Some(BedrockToolChoice::Auto) => Some(object(vec![("auto", pi_js_compat::js_json!({}))])),
        Some(BedrockToolChoice::Any) => Some(object(vec![("any", pi_js_compat::js_json!({}))])),
        Some(BedrockToolChoice::Tool { name }) => Some(object(vec![(
            "tool",
            object(vec![("name", pi_js_compat::js_json!(name))]),
        )])),
        Some(BedrockToolChoice::None) | None => None,
    };

    let mut config = JsObject::new();
    config.insert("tools".into(), JsValue::Array(bedrock_tools));
    if let Some(choice) = bedrock_tool_choice {
        config.insert("toolChoice".into(), choice);
    }
    Some(JsValue::Object(config))
}

fn create_image_block(mime_type: &str, data: &str) -> Result<JsValue, BedrockError> {
    let format = match mime_type {
        "image/jpeg" | "image/jpg" => "jpeg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        other => return Err(BedrockError::other(format!("Unknown image type: {other}"))),
    };
    let bytes = base64::atob(&JsString::from(data)).map_err(|_| BedrockError::other("Invalid image data"))?;
    Ok(object(vec![(
        "image",
        object(vec![
            ("format", pi_js_compat::js_json!(format)),
            (
                "source",
                object(vec![("bytes", pi_js_compat::js_json!(base64::btoa(&bytes)))]),
            ),
        ]),
    )]))
}

fn object(entries: Vec<(&str, JsValue)>) -> JsValue {
    let mut map = JsObject::new();
    for (key, value) in entries {
        map.insert(key.into(), value);
    }
    JsValue::Object(map)
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

fn model_match_candidates(model_id: &str, model_name: Option<&JsString>) -> Vec<JsString> {
    let values = match model_name {
        Some(name) => vec![JsString::from(model_id), name.clone()],
        None => vec![JsString::from(model_id)],
    };
    let mut candidates = Vec::new();
    for value in values {
        let lower = value.to_lowercase();
        candidates.push(lower.clone());
        candidates.push(collapse_separators(&lower));
    }
    candidates
}

fn collapse_separators(value: &JsString) -> JsString {
    let mut result = JsString::default();
    let mut in_separator = false;
    for &unit in value.units() {
        if pi_js_compat::string::is_js_whitespace_unit(unit) || matches!(unit, 95 | 46 | 58) {
            if !in_separator {
                result.push_unit(u16::from(b'-'));
                in_separator = true;
            }
        } else {
            result.push_unit(unit);
            in_separator = false;
        }
    }
    result
}

fn supports_adaptive_thinking(model_id: &str, model_name: Option<&JsString>) -> bool {
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

// Frozen594-595 returns every mapped string via cast, without scalar projection.
fn map_thinking_level_to_effort(model: &Model, level: Option<ThinkingLevel>) -> JsString {
    if level == Some(ThinkingLevel::Xhigh) && supports_native_xhigh_effort(model) {
        return "xhigh".into();
    }
    if let Some(level) = level
        && let Some(map) = &model.thinking_level_map
        && let Some(Some(mapped)) = map.get(&model_thinking_level(level))
    {
        return mapped.clone();
    }
    match level {
        Some(ThinkingLevel::Minimal | ThinkingLevel::Low) => "low".into(),
        Some(ThinkingLevel::Medium) => "medium".into(),
        Some(ThinkingLevel::High) => "high".into(),
        _ => "high".into(),
    }
}

fn supports_prompt_caching(model: &Model, env: Option<&ProviderEnv>) -> bool {
    let candidates = model_match_candidates(&model.id, Some(&model.name));
    if !candidates.iter().any(|candidate| candidate.contains("claude")) {
        return get_provider_env_value("AWS_BEDROCK_FORCE_CACHE", env).is_some_and(|value| value == "1");
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

fn build_additional_model_request_fields(model: &Model, options: &BedrockOptions) -> Option<JsValue> {
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

    let mut fields = JsObject::new();
    if adaptive {
        let mut thinking = JsObject::new();
        thinking.insert("type".into(), JsValue::String("adaptive".into()));
        if let Some(display) = display {
            thinking.insert("display".into(), json!(display).into());
        }
        fields.insert("thinking".into(), JsValue::Object(thinking));
        fields.insert(
            "output_config".into(),
            object(vec![(
                "effort",
                JsValue::String(map_thinking_level_to_effort(model, Some(reasoning))),
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
        let mut thinking = JsObject::new();
        thinking.insert("type".into(), JsValue::String("enabled".into()));
        thinking.insert("budget_tokens".into(), js_number_value(budget).into());
        if let Some(display) = display {
            thinking.insert("display".into(), json!(display).into());
        }
        fields.insert("thinking".into(), JsValue::Object(thinking));
        if options.interleaved_thinking.unwrap_or(true) {
            fields.insert(
                "anthropic_beta".into(),
                json!(["interleaved-thinking-2025-05-14"]).into(),
            );
        }
    }
    Some(JsValue::Object(fields))
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

fn get_configured_region(options: &BedrockOptions) -> Option<JsString> {
    options
        .region
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| get_provider_env_value("AWS_REGION", options.stream.request.env.as_ref()))
        .or_else(|| get_provider_env_value("AWS_DEFAULT_REGION", options.stream.request.env.as_ref()))
}

fn map_stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("end_turn" | "stop_sequence") => StopReason::Stop,
        Some("max_tokens" | "model_context_window_exceeded") => StopReason::Length,
        Some("tool_use") => StopReason::ToolUse,
        _ => StopReason::Error,
    }
}

fn stream_block_index(block: &AssistantContent) -> Option<f64> {
    match block {
        AssistantContent::Text(block) => block.index,
        AssistantContent::Thinking(block) => block.index,
        AssistantContent::ToolCall(block) => block.index,
    }
}

fn strip_bedrock_stream_scratch(block: &mut AssistantContent) {
    match block {
        AssistantContent::Text(block) => block.index = None,
        AssistantContent::Thinking(block) => block.index = None,
        AssistantContent::ToolCall(block) => {
            block.index = None;
            block.partial_json = None;
        }
    }
}

fn handle_message(
    sender: &AssistantMessageEventSender,
    model: &Model,
    output: &mut AssistantMessage,
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
        let payload = parse_bedrock_payload(&message.payload)?;
        let message_text = pi_js_compat::js_json::stringify_lossless(&payload);
        return Err(BedrockError {
            name,
            message: message_text.into(),
            status: None,
            body: None,
            request_id: request_id.map(str::to_owned),
            service_exception: false,
        });
    }
    let Some(event_type) = message.headers.get(":event-type") else {
        return Ok(());
    };
    let payload = parse_bedrock_payload(&message.payload)?;
    validate_bedrock_metadata(&payload, event_type)?;
    match event_type.as_str() {
        "messageStart" => {
            if payload
                .get("role")
                .and_then(JsValue::as_string)
                .is_none_or(|role| role != "assistant")
            {
                return Err(BedrockError::other(
                    "Unexpected assistant message start but got user message start instead",
                ));
            }
            sender.push(AssistantMessageEvent::Start {
                partial: output.clone(),
            });
        }
        "contentBlockStart" => start_tool_block(sender, output, &payload),
        "contentBlockDelta" => handle_content_block_delta(sender, output, &payload),
        "contentBlockStop" => handle_content_block_stop(sender, output, &payload),
        "messageStop" => {
            let reason = payload
                .get("stopReason")
                .and_then(crate::api::openai_sdk::ingress_metadata_text);
            output.stop_reason = map_stop_reason(reason.as_deref());
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

fn parse_bedrock_payload(bytes: &[u8]) -> Result<JsValue, BedrockError> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| BedrockError::stream(format!("Deserialization error: {error}")))?;
    pi_js_compat::js_json::parse_lossless(text).map_err(|error| BedrockError::stream(error.into_message()))
}

fn validate_bedrock_metadata(payload: &JsValue, event_type: &str) -> Result<(), BedrockError> {
    use crate::api::openai_sdk::validate_metadata_fields;
    let fields: &[&str] = match event_type {
        "messageStart" => &["role"],
        "messageStop" => &["stopReason"],
        _ => &[],
    };
    validate_metadata_fields(payload, fields).map_err(BedrockError::other)?;
    if event_type == "contentBlockStart"
        && let Some(tool) = payload.get("start").and_then(|start| start.get("toolUse"))
    {
        validate_metadata_fields(tool, &["toolUseId", "name"]).map_err(BedrockError::other)?;
    }
    if event_type == "contentBlockDelta"
        && let Some(reasoning) = payload.get("delta").and_then(|delta| delta.get("reasoningContent"))
    {
        validate_metadata_fields(reasoning, &["signature"]).map_err(BedrockError::other)?;
    }
    Ok(())
}

fn start_tool_block(sender: &AssistantMessageEventSender, output: &mut AssistantMessage, payload: &JsValue) {
    let Some(tool_use) = payload.get("start").and_then(|start| start.get("toolUse")) else {
        return;
    };
    let mut block = ToolCall::new(
        tool_use
            .get("toolUseId")
            .and_then(crate::api::openai_sdk::ingress_metadata_text)
            .unwrap_or_default(),
        tool_use
            .get("name")
            .and_then(crate::api::openai_sdk::ingress_metadata_text)
            .unwrap_or_default(),
        JsObject::new(),
    );
    block.partial_json = Some(JsString::default());
    block.index = payload.get("contentBlockIndex").and_then(JsValue::as_f64);
    output.content.push(AssistantContent::ToolCall(block));
    sender.push(AssistantMessageEvent::ToolCallStart {
        content_index: output.content.len() - 1,
        partial: output.clone(),
    });
}

fn handle_content_block_delta(sender: &AssistantMessageEventSender, output: &mut AssistantMessage, payload: &JsValue) {
    let provider_index = payload.get("contentBlockIndex").and_then(JsValue::as_f64);
    let Some(delta) = payload.get("delta") else { return };
    let index = output
        .content
        .iter()
        .position(|block| stream_block_index(block) == provider_index);
    if let Some(text) = delta.get("text").and_then(JsValue::as_string) {
        append_text_delta(sender, output, index, provider_index, text);
    } else if let Some(tool_use) = delta.get("toolUse") {
        if let Some(index) = index {
            append_tool_delta(
                sender,
                output,
                index,
                tool_use
                    .get("input")
                    .and_then(JsValue::as_string)
                    .unwrap_or(&JsString::default()),
            );
        }
    } else if let Some(reasoning) = delta.get("reasoningContent") {
        append_thinking_delta(sender, output, index, provider_index, reasoning);
    }
}

fn append_text_delta(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    index: Option<usize>,
    provider_index: Option<f64>,
    text: &JsString,
) {
    let index = index.unwrap_or_else(|| {
        let mut block = crate::types::TextContent::new("");
        block.index = provider_index;
        output.content.push(AssistantContent::Text(block));
        let index = output.content.len() - 1;
        sender.push(AssistantMessageEvent::TextStart {
            content_index: index,
            partial: output.clone(),
        });
        index
    });
    let AssistantContent::Text(block) = &mut output.content[index] else {
        return;
    };
    block.text.push_str(text);
    sender.push(AssistantMessageEvent::TextDelta {
        content_index: index,
        delta: text.clone(),
        partial: output.clone(),
    });
}

fn append_tool_delta(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    index: usize,
    input: &JsString,
) {
    let AssistantContent::ToolCall(block) = &mut output.content[index] else {
        return;
    };
    let partial_json = block.partial_json.get_or_insert_default();
    partial_json.push_str(input);
    block.arguments = parse_streaming_json(Some(partial_json))
        .as_object()
        .cloned()
        .unwrap_or_default();
    sender.push(AssistantMessageEvent::ToolCallDelta {
        content_index: index,
        delta: input.clone(),
        partial: output.clone(),
    });
}

fn append_thinking_delta(
    sender: &AssistantMessageEventSender,
    output: &mut AssistantMessage,
    index: Option<usize>,
    provider_index: Option<f64>,
    reasoning: &JsValue,
) {
    let index = index.unwrap_or_else(|| {
        let mut block = ThinkingContent::new("");
        block.thinking_signature = Some(String::new());
        block.index = provider_index;
        output.content.push(AssistantContent::Thinking(block));
        let index = output.content.len() - 1;
        sender.push(AssistantMessageEvent::ThinkingStart {
            content_index: index,
            partial: output.clone(),
        });
        index
    });
    if let Some(text) = reasoning
        .get("text")
        .and_then(JsValue::as_string)
        .filter(|text| !text.is_empty())
        && let AssistantContent::Thinking(block) = &mut output.content[index]
    {
        block.thinking.push_str(text);
        sender.push(AssistantMessageEvent::ThinkingDelta {
            content_index: index,
            delta: text.clone(),
            partial: output.clone(),
        });
    }
    if let Some(signature) = reasoning
        .get("signature")
        .and_then(crate::api::openai_sdk::ingress_metadata_text)
        .filter(|signature| !signature.is_empty())
        && let AssistantContent::Thinking(block) = &mut output.content[index]
    {
        block.thinking_signature.get_or_insert_default().push_str(&signature);
    }
}

fn handle_content_block_stop(sender: &AssistantMessageEventSender, output: &mut AssistantMessage, payload: &JsValue) {
    let provider_index = payload.get("contentBlockIndex").and_then(JsValue::as_f64);
    let Some(index) = output
        .content
        .iter()
        .position(|block| stream_block_index(block) == provider_index)
    else {
        return;
    };
    if let AssistantContent::ToolCall(block) = &mut output.content[index] {
        block.arguments = parse_streaming_json(block.partial_json.as_ref())
            .as_object()
            .cloned()
            .unwrap_or_default();
    }
    strip_bedrock_stream_scratch(&mut output.content[index]);
    let event = match &output.content[index] {
        AssistantContent::Text(block) => AssistantMessageEvent::TextEnd {
            content_index: index,
            content: block.text.clone(),
            partial: output.clone(),
        },
        AssistantContent::Thinking(block) => AssistantMessageEvent::ThinkingEnd {
            content_index: index,
            content: block.thinking.clone(),
            partial: output.clone(),
        },
        AssistantContent::ToolCall(block) => AssistantMessageEvent::ToolCallEnd {
            content_index: index,
            tool_call: Box::new(block.clone()),
            partial: output.clone(),
        },
    };
    sender.push(event);
}

fn handle_metadata(usage: &JsValue, model: &Model, output: &mut AssistantMessage) {
    let input = usage.get("inputTokens").and_then(JsValue::as_f64).unwrap_or(0.0);
    let output_tokens = usage.get("outputTokens").and_then(JsValue::as_f64).unwrap_or(0.0);
    let cache_read = usage
        .get("cacheReadInputTokens")
        .and_then(JsValue::as_f64)
        .unwrap_or(0.0);
    let cache_write = usage
        .get("cacheWriteInputTokens")
        .and_then(JsValue::as_f64)
        .unwrap_or(0.0);
    let total = usage
        .get("totalTokens")
        .and_then(JsValue::as_f64)
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
    region: JsString,
    endpoint: JsString,
    credentials: Option<AwsCredentials>,
    bearer_token: Option<JsString>,
    sdk_features: String,
}

async fn load_runtime_config(model: &Model, options: &BedrockOptions) -> Result<RuntimeConfig, BedrockError> {
    let env = options.stream.request.env.as_ref();
    let options_profile = options
        .profile
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| get_provider_env_value("AWS_PROFILE", env));
    let profile = profile_name(options_profile.as_ref(), env);
    let configured_region = get_configured_region(options);
    let has_ambient_profile = get_provider_env_value("AWS_PROFILE", None).is_some();
    let endpoint_region = standard_endpoint_region(&model.base_url).map(JsString::from);
    let use_explicit_endpoint = endpoint_region.is_none() || (configured_region.is_none() && !has_ambient_profile);

    let skip_auth = get_provider_env_value("AWS_BEDROCK_SKIP_AUTH", env).is_some_and(|value| value == "1");
    let bearer_token = options
        .bearer_token
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| get_provider_env_value("AWS_BEARER_TOKEN_BEDROCK", env));
    let use_bearer = bearer_token.is_some() && !skip_auth;

    let region = match arn_region(&model.id).map(JsString::from) {
        Some(region) => region,
        None => match configured_region {
            Some(region) => region,
            None => match endpoint_region.filter(|_| use_explicit_endpoint) {
                Some(region) => region,
                None if !has_ambient_profile => "us-east-1".into(),
                None => profile_region(&profile, env).map(JsString::from).ok_or_else(|| {
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

    let configured_credentials = env_credentials(env);
    let direct_credentials = skip_auth || configured_credentials.is_some();
    let credential_features = if use_bearer {
        "E"
    } else if direct_credentials {
        "E,e"
    } else {
        "E,n"
    };
    let sdk_features = if use_explicit_endpoint {
        format!("N,{credential_features}")
    } else {
        credential_features.to_owned()
    };
    let credentials = if skip_auth {
        Some(AwsCredentials {
            access_key_id: "dummy-access-key".into(),
            secret_access_key: "dummy-secret-key".into(),
            session_token: None,
        })
    } else if use_bearer {
        None
    } else if let Some(credentials) = configured_credentials {
        Some(credentials)
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
        sdk_features,
    })
}

/// `new URL(baseUrl).hostname` matched against an ASCII grammar; the URL
/// parser sees the USVString, so the Node UTF-8 view is exact here.
fn standard_endpoint_region(base_url: &JsString) -> Option<String> {
    let base_url = encode_node_utf8(base_url);
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

fn default_endpoint(region: &JsString) -> JsString {
    let suffix = if region.starts_with("cn-") {
        ".amazonaws.com.cn"
    } else {
        ".amazonaws.com"
    };
    JsString::from("https://bedrock-runtime.") + region + suffix
}

async fn execute_request(
    config: &RuntimeConfig,
    options: &BedrockOptions,
    model: &Model,
    body: Vec<u8>,
) -> Result<HttpResponse, BedrockError> {
    let mut endpoint = config.endpoint.clone();
    while let Some(trimmed) = endpoint.strip_suffix('/') {
        endpoint = trimmed;
    }
    let url = endpoint + "/model/" + crate::api::bedrock_sigv4::aws_escape_uri(&model.id) + "/converse-stream";
    let url = request_url(&url).map_err(|error| BedrockError::other(error.message))?;
    let invocation_id = random_uuid_v4();
    let mut headers = base_headers(&url, &body, &invocation_id, 1, &config.sdk_features);
    apply_custom_headers(&mut headers, options.stream.request.headers.as_ref())
        .map_err(|error| BedrockError::other(error.message))?;
    let fetch = options.stream.request.fetch.clone().unwrap_or_else(default_fetch);

    let mut attempt = 1usize;
    loop {
        set_header(
            &mut headers,
            "amz-sdk-request",
            &format!("attempt={attempt}; max={MAX_ATTEMPTS}"),
        );
        if let Some(token) = &config.bearer_token {
            let authorization = header_value(&(JsString::from("Bearer ") + token))
                .map_err(|error| BedrockError::other(error.message))?;
            set_header_exact(&mut headers, "Authorization", &authorization);
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
            .map_err(|error| BedrockError::other(error.message))?;
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
                if let Some(signal) = options.signal().as_ref().filter(|signal| signal.aborted()) {
                    return Err(smithy_abort_error(signal));
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

fn base_headers(
    url: &str,
    body: &[u8],
    invocation_id: &str,
    attempt: usize,
    sdk_features: &str,
) -> Vec<(String, String)> {
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
            format!(
                "{AWS_SDK_USER_AGENT} ua/2.1 os/linux#unknown lang/js md/nodejs#unknown md/tsc#5.9.3 api/bedrock-runtime#3.1048.0 m/{sdk_features}"
            ),
        ),
        ("host".to_owned(), host.to_owned()),
        ("amz-sdk-invocation-id".to_owned(), invocation_id.to_owned()),
        (
            "amz-sdk-request".to_owned(),
            format!("attempt={attempt}; max={MAX_ATTEMPTS}"),
        ),
    ]
}

fn apply_custom_headers(
    headers: &mut Vec<(String, String)>,
    custom: Option<&crate::types::ProviderHeaders>,
) -> Result<(), CallbackError> {
    let Some(custom) = provider_headers_to_record(custom) else {
        return Ok(());
    };
    for (key, value) in custom {
        if !is_reserved_header(&key) {
            set_header_exact(headers, &key, &header_value(&value)?);
        }
    }
    Ok(())
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

fn smithy_abort_error(signal: &AbortSignal) -> BedrockError {
    use pi_js_compat::abort::AbortReason;
    match signal.reason().expect("aborted request signal") {
        AbortReason::Value(value) if pi_js_compat::js_json::is_truthy(&value) => {
            match pi_js_compat::js_json::to_js_string(&value) {
                Ok(message) => BedrockError::abort(message),
                Err(message) => BedrockError::other(message),
            }
        }
        _ => BedrockError::abort("Request aborted"),
    }
}

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
    signal: Option<pi_js_compat::abort::AbortSignal>,
) -> Result<(), BedrockError> {
    let Some(signal) = signal else {
        tokio::time::sleep(delay).await;
        return Ok(());
    };
    tokio::select! {
        _ = signal.wait() => Err(BedrockError::abort("Request was aborted")),
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
    let code = response_header(headers, "x-amzn-errortype").and_then(|value| {
        value
            .split(':')
            .next()
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(str::to_owned)
    });
    let request_id = response_header(headers, "x-amzn-requestid").map(str::to_owned);
    let text = String::from_utf8_lossy(body).into_owned();
    let parsed = serde_json::from_str::<Value>(&text).ok();
    let parsed_message = parsed.as_ref().and_then(|value| {
        value
            .get("message")
            .or_else(|| value.get("Message"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let message = parsed_message.unwrap_or_else(|| code.clone().unwrap_or_else(|| "UnknownError".to_owned()));
    // Frozen normalizeProviderError stringifies the consumed Node IncomingMessage.
    // Its cycle makes safeJsonStringify return String(responseBody), not raw bytes.
    let body = Some(if parsed.is_some() {
        "[object Object]".to_owned()
    } else {
        text
    });
    BedrockError {
        name: code.clone(),
        message: message.into(),
        status: Some(f64::from(status)),
        body,
        request_id,
        service_exception: code.is_some(),
    }
}

fn response_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn response_request_id(response: &HttpResponse) -> Option<String> {
    response_header(&response.headers, "x-amzn-requestid")
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= MAX_BEDROCK_DIAGNOSTIC_VALUE_CHARS)
        .map(str::to_owned)
}

#[derive(Debug, Clone)]
struct BedrockError {
    name: Option<String>,
    message: pi_js_compat::js_json::JsString,
    status: Option<f64>,
    body: Option<String>,
    request_id: Option<String>,
    service_exception: bool,
}

impl BedrockError {
    fn other(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self {
            name: None,
            message: message.into(),
            status: None,
            body: None,
            request_id: None,
            service_exception: false,
        }
    }

    fn stream(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self::other(message)
    }

    fn transport(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self::other(message)
    }

    fn abort(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self::other(message)
    }

    fn abort_or_transport(message: impl Into<pi_js_compat::js_json::JsString>) -> Self {
        Self::other(message)
    }

    fn with_request_id(mut self, request_id: Option<&str>) -> Self {
        if self.request_id.is_none() {
            self.request_id = request_id.map(str::to_owned);
        }
        self
    }
}

fn format_bedrock_error(error: &BedrockError) -> pi_js_compat::js_json::JsString {
    let message_carries_body = error.body.as_ref().is_none_or(|body| error.message.contains(body));
    let core = match (message_carries_body, error.status, error.body.as_deref()) {
        (false, Some(status), Some(body)) => format!("{}: {body}", js_number_to_string(status)).into(),
        _ => error.message.clone(),
    };
    let hint = if core.to_lowercase().contains("data retention mode") {
        format!(" See {BEDROCK_DATA_RETENTION_DOCS_URL} for supported data retention modes.")
    } else {
        String::new()
    };
    if error.service_exception {
        let prefix = error.name.as_deref().map(bedrock_error_prefix).unwrap_or_default();
        pi_js_compat::js_json::JsString::join([format!("{prefix}: ").into(), core, hint.into()], "")
    } else {
        pi_js_compat::js_json::JsString::join([core, hint.into()], "")
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
