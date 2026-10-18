//! Port of `packages/ai/src/api/google-shared.ts`.
//!
//! Shared request conversion and stop-reason helpers for the Google
//! Generative AI and Google Vertex adapters.

use std::future::Future;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::api::constrained_sampling::{
    ConstrainedSamplingError, get_json_schema_tool_parameters, resolve_json_schema_strict_sampling,
};
use crate::api::transform_messages::transform_messages_with;
use crate::types::{
    AssistantContent, AssistantMessage, Context, Message, Modality, Model, StopReason, Tool, ToolResultMessage,
    UserContent,
};
use crate::utils::provider_retry::{
    ProviderError, ProviderErrorHeaders, ProviderRequestError, ProviderRetryOptions, retry_provider_request,
};
use crate::utils::sanitize_unicode::sanitize_surrogates;

/// Thinking level for Gemini 3 models; mirrors Google's `ThinkingLevel` enum values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GoogleThinkingLevel {
    #[serde(rename = "THINKING_LEVEL_UNSPECIFIED")]
    Unspecified,
    Minimal,
    Low,
    Medium,
    High,
}

/// Gemini `FunctionCallingConfigMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FunctionCallingConfigMode {
    Auto,
    None,
    Any,
    Validated,
}

/// One `Content` in the Gemini request body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoogleContent {
    pub parts: Vec<Value>,
    pub role: String,
}

/// Whether a streamed part is thinking content (`thought: true`).
///
/// `thoughtSignature` alone never marks thinking content.
pub fn is_thinking_part(thought: Option<bool>) -> bool {
    thought == Some(true)
}

/// Keeps the last non-empty thought signature within the current streamed block.
pub fn retain_thought_signature(existing: Option<&str>, incoming: Option<&str>) -> Option<String> {
    match incoming {
        Some(incoming) if !incoming.is_empty() => Some(incoming.to_owned()),
        _ => existing.map(str::to_owned),
    }
}

/// Models that require explicit tool call IDs in function calls/responses.
pub fn requires_tool_call_id(model_id: &str) -> bool {
    let major = gemini_major_version(model_id);
    model_id.starts_with("claude-") || model_id.starts_with("gpt-oss-") || major.is_some_and(|major| major >= 3)
}

fn gemini_major_version(model_id: &str) -> Option<u32> {
    let lowered = model_id.to_lowercase();
    let rest = lowered
        .strip_prefix("gemini-live-")
        .or_else(|| lowered.strip_prefix("gemini-"))?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn supports_multimodal_function_response(model_id: &str) -> bool {
    match gemini_major_version(model_id) {
        Some(major) => major >= 3,
        None => true,
    }
}

/// Convert internal messages to Gemini `Content[]`.
///
/// `now` feeds the synthetic tool-result insertion of [`transform_messages_with`]
/// (TypeScript uses `Date.now()`).
pub fn convert_messages(model: &Model, context: &Context, now: i64) -> Vec<GoogleContent> {
    let require_ids = requires_tool_call_id(&model.id);
    let normalize_tool_call_id = |id: &str, _: &Model, _: &AssistantMessage| -> String {
        if require_ids {
            let sanitized: String = id
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                        character
                    } else {
                        '_'
                    }
                })
                .collect();
            sanitized.chars().take(64).collect()
        } else {
            id.to_owned()
        }
    };
    let transformed = transform_messages_with(&context.messages, model, Some(&normalize_tool_call_id), now);

    let mut contents: Vec<GoogleContent> = Vec::new();
    for message in &transformed {
        match message {
            Message::User(user) => convert_user_message(&user.content, &mut contents),
            Message::Assistant(assistant) => convert_assistant_message(model, assistant, &mut contents),
            Message::ToolResult(result) => convert_tool_result(model, result, &mut contents),
        }
    }
    contents
}

fn convert_user_message(content: &UserContent, contents: &mut Vec<GoogleContent>) {
    match content {
        UserContent::Text(text) => contents.push(GoogleContent {
            parts: vec![json!({"text": sanitize_surrogates(text)})],
            role: "user".to_owned(),
        }),
        UserContent::Blocks(blocks) => {
            let parts: Vec<Value> = blocks
                .iter()
                .map(|block| match block {
                    crate::types::TextOrImageContent::Text(text) => {
                        json!({"text": sanitize_surrogates(&text.text)})
                    }
                    crate::types::TextOrImageContent::Image(image) => json!({
                        "inlineData": {"mimeType": image.mime_type, "data": image.data}
                    }),
                })
                .collect();
            if !parts.is_empty() {
                contents.push(GoogleContent {
                    parts,
                    role: "user".to_owned(),
                });
            }
        }
    }
}

fn convert_assistant_message(model: &Model, assistant: &AssistantMessage, contents: &mut Vec<GoogleContent>) {
    let same_provider_and_model = assistant.provider == model.provider && assistant.model == model.id;
    let mut parts: Vec<Value> = Vec::new();

    for block in &assistant.content {
        match block {
            AssistantContent::Text(text) => {
                let signature = resolve_thought_signature(same_provider_and_model, text.text_signature.as_deref());
                if text.text.trim().is_empty() && signature.is_none() {
                    continue;
                }
                let mut part = Map::new();
                part.insert(
                    "text".to_owned(),
                    Value::String(sanitize_surrogates(&text.text).to_owned()),
                );
                insert_signature(&mut part, signature);
                parts.push(Value::Object(part));
            }
            AssistantContent::Thinking(thinking) => {
                if same_provider_and_model {
                    let signature =
                        resolve_thought_signature(same_provider_and_model, thinking.thinking_signature.as_deref());
                    if thinking.thinking.trim().is_empty() && signature.is_none() {
                        continue;
                    }
                    let mut part = Map::new();
                    part.insert("thought".to_owned(), Value::Bool(true));
                    part.insert(
                        "text".to_owned(),
                        Value::String(sanitize_surrogates(&thinking.thinking).to_owned()),
                    );
                    insert_signature(&mut part, signature);
                    parts.push(Value::Object(part));
                } else {
                    if thinking.thinking.trim().is_empty() {
                        continue;
                    }
                    parts.push(json!({"text": sanitize_surrogates(&thinking.thinking)}));
                }
            }
            AssistantContent::ToolCall(call) => {
                let signature = resolve_thought_signature(same_provider_and_model, call.thought_signature.as_deref());
                let mut function_call = Map::new();
                function_call.insert("name".to_owned(), Value::String(call.name.clone()));
                function_call.insert("args".to_owned(), Value::Object(call.arguments.clone()));
                if requires_tool_call_id(&model.id) {
                    function_call.insert("id".to_owned(), Value::String(call.id.clone()));
                }
                let mut part = Map::new();
                part.insert("functionCall".to_owned(), Value::Object(function_call));
                insert_signature(&mut part, signature);
                parts.push(Value::Object(part));
            }
        }
    }

    if parts.is_empty() {
        return;
    }
    contents.push(GoogleContent {
        parts,
        role: "model".to_owned(),
    });
}

fn convert_tool_result(model: &Model, result: &ToolResultMessage, contents: &mut Vec<GoogleContent>) {
    let text_result = result
        .content
        .iter()
        .filter_map(|content| match content {
            crate::types::TextOrImageContent::Text(text) => Some(text.text.as_str()),
            crate::types::TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let image_content: Vec<&crate::types::ImageContent> = if model.input.contains(&Modality::Image) {
        result
            .content
            .iter()
            .filter_map(|content| match content {
                crate::types::TextOrImageContent::Image(image) => Some(image),
                crate::types::TextOrImageContent::Text(_) => None,
            })
            .collect()
    } else {
        Vec::new()
    };

    let has_text = !text_result.is_empty();
    let has_images = !image_content.is_empty();
    let multimodal = supports_multimodal_function_response(&model.id);
    let response_value = if has_text {
        sanitize_surrogates(&text_result).to_owned()
    } else if has_images {
        "(see attached image)".to_owned()
    } else {
        String::new()
    };
    let image_parts: Vec<Value> = image_content
        .iter()
        .map(|image| json!({"inlineData": {"mimeType": image.mime_type, "data": image.data}}))
        .collect();

    let mut function_response = Map::new();
    function_response.insert("name".to_owned(), Value::String(result.tool_name.clone()));
    let response_key = if result.is_error { "error" } else { "output" };
    function_response.insert("response".to_owned(), json!({response_key: response_value}));
    if has_images && multimodal {
        function_response.insert("parts".to_owned(), Value::Array(image_parts.clone()));
    }
    if requires_tool_call_id(&model.id) {
        function_response.insert("id".to_owned(), Value::String(result.tool_call_id.clone()));
    }
    let function_response_part = json!({"functionResponse": function_response});

    match contents.last_mut() {
        Some(last) if last.role == "user" && last.parts.iter().any(|part| part.get("functionResponse").is_some()) => {
            last.parts.push(function_response_part);
        }
        _ => contents.push(GoogleContent {
            parts: vec![function_response_part],
            role: "user".to_owned(),
        }),
    }

    if has_images && !multimodal {
        let mut parts = vec![json!({"text": "Tool result image:"})];
        parts.extend(image_parts);
        contents.push(GoogleContent {
            parts,
            role: "user".to_owned(),
        });
    }
}

fn insert_signature(part: &mut Map<String, Value>, signature: Option<String>) {
    if let Some(signature) = signature {
        part.insert("thoughtSignature".to_owned(), Value::String(signature));
    }
}

fn is_valid_thought_signature(signature: Option<&str>) -> bool {
    let Some(signature) = signature else {
        return false;
    };
    if signature.is_empty() || signature.len() % 4 != 0 {
        return false;
    }
    let unpadded = signature.trim_end_matches('=');
    let padding = signature.len() - unpadded.len();
    padding <= 2
        && !unpadded.is_empty()
        && unpadded
            .bytes()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/'))
}

fn resolve_thought_signature(same_provider_and_model: bool, signature: Option<&str>) -> Option<String> {
    if same_provider_and_model && is_valid_thought_signature(signature) {
        signature.map(str::to_owned)
    } else {
        None
    }
}

const JSON_SCHEMA_META_DECLARATIONS: [&str; 8] = [
    "$schema",
    "$id",
    "$anchor",
    "$dynamicAnchor",
    "$vocabulary",
    "$comment",
    "$defs",
    "definitions",
];

/// Strip JSON Schema meta-declarations for the legacy OpenAPI `parameters` field.
fn sanitize_for_open_api(schema: &Value) -> Value {
    let Value::Object(object) = schema else {
        return schema.clone();
    };
    let mut result = Map::new();
    for (key, value) in object {
        if JSON_SCHEMA_META_DECLARATIONS.contains(&key.as_str()) {
            continue;
        }
        result.insert(key.clone(), sanitize_for_open_api(value));
    }
    Value::Object(result)
}

/// Convert tools to Gemini function declarations.
pub fn convert_tools(
    tools: &[Tool],
    use_parameters: bool,
    supports_strict_mode: bool,
) -> Result<Option<Vec<Value>>, ConstrainedSamplingError> {
    if tools.is_empty() {
        return Ok(None);
    }
    let mut declarations = Vec::with_capacity(tools.len());
    for tool in tools {
        let strict = resolve_json_schema_strict_sampling(tool, supports_strict_mode)?;
        let parameters = get_json_schema_tool_parameters(tool, strict)?;
        let mut declaration = Map::new();
        declaration.insert("name".to_owned(), Value::String(tool.name.clone()));
        declaration.insert("description".to_owned(), Value::String(tool.description.clone()));
        if use_parameters {
            declaration.insert("parameters".to_owned(), sanitize_for_open_api(&parameters));
        } else {
            declaration.insert("parametersJsonSchema".to_owned(), parameters);
        }
        declarations.push(Value::Object(declaration));
    }
    Ok(Some(vec![json!({"functionDeclarations": declarations})]))
}

/// Gemini 3+ enforces required function parameters in validated tool-calling modes.
pub fn supports_google_strict_tool_sampling(model_id: &str) -> bool {
    gemini_major_version(model_id).is_some_and(|major| major >= 3)
}

/// Map a tool choice string to `FunctionCallingConfigMode`.
pub fn map_tool_choice(choice: &str) -> FunctionCallingConfigMode {
    match choice {
        "none" => FunctionCallingConfigMode::None,
        "any" => FunctionCallingConfigMode::Any,
        _ => FunctionCallingConfigMode::Auto,
    }
}

pub fn resolve_google_function_calling_mode(
    tools: &[Tool],
    tool_choice: Option<&str>,
    supports_strict_mode: bool,
) -> Result<Option<FunctionCallingConfigMode>, ConstrainedSamplingError> {
    let mut use_strict_mode = false;
    for tool in tools {
        if resolve_json_schema_strict_sampling(tool, supports_strict_mode)? == Some(true) {
            use_strict_mode = true;
            break;
        }
    }
    if matches!(tool_choice, Some("none" | "any")) {
        return Ok(tool_choice.map(map_tool_choice));
    }
    if use_strict_mode {
        return Ok(Some(FunctionCallingConfigMode::Validated));
    }
    Ok(tool_choice.map(map_tool_choice))
}

/// Map a Gemini `FinishReason` to a Pi stop reason.
pub fn map_stop_reason(reason: &str) -> Result<StopReason, String> {
    match reason {
        "STOP" => Ok(StopReason::Stop),
        "MAX_TOKENS" => Ok(StopReason::Length),
        "BLOCKLIST"
        | "PROHIBITED_CONTENT"
        | "SPII"
        | "SAFETY"
        | "IMAGE_SAFETY"
        | "IMAGE_PROHIBITED_CONTENT"
        | "IMAGE_RECITATION"
        | "IMAGE_OTHER"
        | "RECITATION"
        | "FINISH_REASON_UNSPECIFIED"
        | "OTHER"
        | "LANGUAGE"
        | "MALFORMED_FUNCTION_CALL"
        | "UNEXPECTED_TOOL_CALL"
        | "NO_IMAGE" => Ok(StopReason::Error),
        unknown => Err(format!("Unhandled stop reason: {unknown}")),
    }
}

/// Map a raw finish-reason string to a Pi stop reason.
pub fn map_stop_reason_string(reason: &str) -> StopReason {
    match reason {
        "STOP" => StopReason::Stop,
        "MAX_TOKENS" => StopReason::Length,
        _ => StopReason::Error,
    }
}

/// Failure raised by a Google request or by its retry wrapper.
#[derive(Debug, Clone, PartialEq)]
pub struct GoogleRequestError {
    pub message: String,
    pub status: Option<f64>,
}

impl GoogleRequestError {
    pub fn new(message: impl Into<String>, status: Option<f64>) -> Self {
        Self {
            message: message.into(),
            status,
        }
    }
}

impl std::fmt::Display for GoogleRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GoogleRequestError {}

/// `Pick<StreamOptions, "maxRetries" | "maxRetryDelayMs" | "signal">`.
#[derive(Debug, Clone, Default)]
pub struct GoogleRetryOptions {
    pub max_retries: Option<f64>,
    pub max_retry_delay_ms: Option<f64>,
    pub signal: Option<tokio_util::sync::CancellationToken>,
}

/// Run a Google request with the shared provider retry policy.
///
/// The SDK error shape only carries `status`; the missing `headers` property is
/// normalized to an empty header set so the shared policy can classify it.
pub async fn retry_google_request<T, F, Fut>(
    mut request: F,
    options: GoogleRetryOptions,
) -> Result<T, GoogleRequestError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, GoogleRequestError>>,
{
    let max_retries = options.max_retries.unwrap_or(0.0).max(0.0) as usize;
    let result = retry_provider_request(
        || {
            let future = request();
            async move {
                future.await.map_err(|error| match error.status {
                    Some(status) => ProviderRequestError::Provider(ProviderError {
                        message: error.message,
                        status: Some(status),
                        headers: ProviderErrorHeaders::new(),
                        body: None,
                        inner_error: None,
                    }),
                    None => ProviderRequestError::Other(error),
                })
            }
        },
        ProviderRetryOptions {
            max_retries,
            max_retry_delay_ms: options.max_retry_delay_ms,
            signal: options.signal.clone(),
        },
    )
    .await;

    result.map_err(|error| match error {
        ProviderRequestError::Provider(error) => GoogleRequestError::new(error.message, error.status),
        ProviderRequestError::Other(error) => error,
        ProviderRequestError::Abort(_) => GoogleRequestError::new("Request aborted", None),
        ProviderRequestError::RetryDelayLimit(error) => GoogleRequestError::new(error.to_string(), None),
    })
}
