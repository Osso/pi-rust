//! Port of `packages/ai/src/api/google-shared.ts`.
//!
//! Shared request conversion and stop-reason helpers for the Google
//! Generative AI and Google Vertex adapters.

use pi_js_compat::js_json::{JsObject, JsValue};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::api::transform_messages::transform_messages_with;
use crate::types::{
    AssistantContent, AssistantMessage, Context, Message, Modality, Model, ModelThinkingLevel, StopReason, Tool,
    ToolResultMessage, UserContent,
};
use crate::utils::sanitize_unicode::sanitize_surrogates;

/// Frozen Google API response-token ceiling, exclusive of 65536.
pub const GOOGLE_MAX_OUTPUT_TOKENS: f64 = 65535.0;

pub fn clamp_google_max_output_tokens(max_tokens: f64) -> f64 {
    if max_tokens > GOOGLE_MAX_OUTPUT_TOKENS {
        GOOGLE_MAX_OUTPUT_TOKENS
    } else {
        max_tokens
    }
}

/// Latest alias rejects an explicit thinkingConfig; the server owns thinking.
pub fn uses_server_managed_thinking(model_id: &str) -> bool {
    model_id.to_lowercase() == "gemini-flash-latest"
}

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
    pub role: String,
    pub parts: Vec<JsValue>,
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
    model_id.starts_with("claude-") || model_id.starts_with("gpt-oss-")
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
pub fn convert_messages(model: &Model, context: &Context, now: f64) -> Vec<GoogleContent> {
    let require_ids = requires_tool_call_id(&model.id);
    let normalize_tool_call_id = |id: &str, _: &Model, _: &AssistantMessage| -> String {
        if require_ids {
            let sanitized: String = id
                .encode_utf16()
                .map(|unit| {
                    char::from_u32(u32::from(unit))
                        .filter(|character| character.is_ascii_alphanumeric() || *character == '_' || *character == '-')
                        .unwrap_or('_')
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
            parts: vec![pi_js_compat::js_json!({"text": sanitize_surrogates(text)})],
            role: "user".into(),
        }),
        UserContent::Blocks(blocks) => {
            let parts: Vec<JsValue> = blocks
                .iter()
                .map(|block| match block {
                    crate::types::TextOrImageContent::Text(text) => {
                        pi_js_compat::js_json!({"text": sanitize_surrogates(&text.text)})
                    }
                    crate::types::TextOrImageContent::Image(image) => pi_js_compat::js_json!({
                        "inlineData": {"mimeType": image.mime_type, "data": image.data}
                    }),
                })
                .collect();
            if !parts.is_empty() {
                contents.push(GoogleContent {
                    parts,
                    role: "user".into(),
                });
            }
        }
    }
}

fn convert_assistant_message(model: &Model, assistant: &AssistantMessage, contents: &mut Vec<GoogleContent>) {
    let same_provider_and_model = assistant.provider == model.provider && assistant.model == model.id;
    let parts: Vec<JsValue> = assistant
        .content
        .iter()
        .filter_map(|block| convert_assistant_block(model, same_provider_and_model, block))
        .collect();
    if parts.is_empty() {
        return;
    }
    contents.push(GoogleContent {
        parts,
        role: "model".into(),
    });
}

fn convert_assistant_block(model: &Model, same_provider_and_model: bool, block: &AssistantContent) -> Option<JsValue> {
    match block {
        AssistantContent::Text(text) => convert_text_block(same_provider_and_model, text),
        AssistantContent::Thinking(thinking) => convert_thinking_block(same_provider_and_model, thinking),
        AssistantContent::ToolCall(call) => Some(convert_tool_call_block(model, same_provider_and_model, call)),
    }
}

fn convert_text_block(same_provider_and_model: bool, text: &crate::types::TextContent) -> Option<JsValue> {
    if text.text.trim().is_empty() {
        return None;
    }
    let signature = resolve_thought_signature(same_provider_and_model, text.text_signature.as_deref());
    let mut part = JsObject::new();
    part.insert("text".into(), JsValue::String(sanitize_surrogates(&text.text).into()));
    insert_signature(&mut part, signature);
    Some(JsValue::Object(part))
}

fn convert_thinking_block(same_provider_and_model: bool, thinking: &crate::types::ThinkingContent) -> Option<JsValue> {
    if thinking.thinking.trim().is_empty() {
        return None;
    }
    if !same_provider_and_model {
        return Some(pi_js_compat::js_json!({"text": sanitize_surrogates(&thinking.thinking)}));
    }
    let signature = resolve_thought_signature(same_provider_and_model, thinking.thinking_signature.as_deref());
    let mut part = JsObject::new();
    part.insert("thought".into(), JsValue::Bool(true));
    part.insert(
        "text".into(),
        JsValue::String(sanitize_surrogates(&thinking.thinking).into()),
    );
    insert_signature(&mut part, signature);
    Some(JsValue::Object(part))
}

fn convert_tool_call_block(model: &Model, same_provider_and_model: bool, call: &crate::types::ToolCall) -> JsValue {
    let signature = resolve_thought_signature(same_provider_and_model, call.thought_signature.as_deref());
    let model_id = model.id.to_lowercase();
    let thinking_model = model.reasoning || model_id.contains("gemini-3") || model_id.contains("gemini-2.5");
    let requires_sentinel = model.api == "google-generative-ai" && thinking_model;
    let signature = signature.or_else(|| requires_sentinel.then(|| "skip_thought_signature_validator".to_owned()));
    let mut function_call = JsObject::new();
    function_call.insert("name".into(), JsValue::String(call.name.clone().into()));
    function_call.insert("args".into(), JsValue::Object(call.arguments.clone()));
    if requires_tool_call_id(&model.id) {
        function_call.insert("id".into(), JsValue::String(call.id.clone().into()));
    }
    let mut part = JsObject::new();
    part.insert("functionCall".into(), JsValue::Object(function_call));
    insert_signature(&mut part, signature);
    JsValue::Object(part)
}

fn convert_tool_result(model: &Model, result: &ToolResultMessage, contents: &mut Vec<GoogleContent>) {
    let text_result = joined_text_content(result);
    let image_content = supported_image_content(model, result);
    let has_images = !image_content.is_empty();
    let multimodal = supports_multimodal_function_response(&model.id);

    let part = function_response_part(model, result, &text_result, &image_content);
    append_function_response(contents, part);

    if has_images && !multimodal {
        let mut parts = vec![pi_js_compat::js_json!({"text": "Tool result image:"})];
        parts.extend(image_parts(&image_content));
        contents.push(GoogleContent {
            parts,
            role: "user".into(),
        });
    }
}

fn joined_text_content(result: &ToolResultMessage) -> String {
    result
        .content
        .iter()
        .filter_map(|content| match content {
            crate::types::TextOrImageContent::Text(text) => Some(sanitize_surrogates(&text.text)),
            crate::types::TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn supported_image_content<'a>(model: &Model, result: &'a ToolResultMessage) -> Vec<&'a crate::types::ImageContent> {
    if !model.input.contains(&Modality::Image) {
        return Vec::new();
    }
    result
        .content
        .iter()
        .filter_map(|content| match content {
            crate::types::TextOrImageContent::Image(image) => Some(image),
            crate::types::TextOrImageContent::Text(_) => None,
        })
        .collect()
}

fn function_response_part(
    model: &Model,
    result: &ToolResultMessage,
    text_result: &str,
    image_content: &[&crate::types::ImageContent],
) -> JsValue {
    let has_text = !text_result.is_empty();
    let response_value = if has_text {
        sanitize_surrogates(text_result)
    } else if image_content.is_empty() {
        String::new()
    } else {
        "(see attached image)".into()
    };

    let mut function_response = JsObject::new();
    function_response.insert("name".into(), JsValue::String(result.tool_name.clone().into()));
    let response_key = if result.is_error { "error" } else { "output" };
    function_response.insert(
        "response".into(),
        pi_js_compat::js_json!({response_key: response_value}),
    );
    if !image_content.is_empty() && supports_multimodal_function_response(&model.id) {
        function_response.insert("parts".into(), JsValue::Array(image_parts(image_content)));
    }
    if requires_tool_call_id(&model.id) {
        function_response.insert("id".into(), JsValue::String(result.tool_call_id.clone().into()));
    }
    JsValue::object([("functionResponse", JsValue::Object(function_response))])
}

fn image_parts(image_content: &[&crate::types::ImageContent]) -> Vec<JsValue> {
    image_content
        .iter()
        .map(|image| pi_js_compat::js_json!({"inlineData": {"mimeType": image.mime_type, "data": image.data}}))
        .collect()
}

fn append_function_response(contents: &mut Vec<GoogleContent>, part: JsValue) {
    match contents.last_mut() {
        Some(last) if last.role == "user" && last.parts.iter().any(|part| part.get("functionResponse").is_some()) => {
            last.parts.push(part);
        }
        _ => contents.push(GoogleContent {
            parts: vec![part],
            role: "user".into(),
        }),
    }
}

fn insert_signature(part: &mut JsObject, signature: Option<String>) {
    if let Some(signature) = signature {
        part.insert("thoughtSignature".into(), JsValue::String(signature.into()));
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
pub fn convert_tools(tools: &[Tool], use_parameters: bool) -> Option<Vec<JsValue>> {
    if tools.is_empty() {
        return None;
    }
    let mut declarations = Vec::with_capacity(tools.len());
    for tool in tools {
        let parameters = tool.parameters.clone();
        let mut declaration = JsObject::new();
        declaration.insert("name".into(), JsValue::String(tool.name.clone().into()));
        declaration.insert("description".into(), JsValue::String(tool.description.clone()));
        if use_parameters {
            declaration.insert("parameters".into(), sanitize_for_open_api(&parameters).into());
        } else {
            declaration.insert("parametersJsonSchema".into(), parameters.into());
        }
        declarations.push(JsValue::Object(declaration));
    }
    Some(vec![pi_js_compat::js_json!({"functionDeclarations": declarations})])
}

/// Map a tool choice string to `FunctionCallingConfigMode`.
pub fn map_tool_choice(choice: &str) -> FunctionCallingConfigMode {
    match choice {
        "none" => FunctionCallingConfigMode::None,
        "any" => FunctionCallingConfigMode::Any,
        _ => FunctionCallingConfigMode::Auto,
    }
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

/// `gemini-3[.N]-pro` (case-insensitive).
pub(crate) fn is_gemini3_pro_id(model_id: &str) -> bool {
    gemini3_suffix_matches(model_id, "-pro")
}

/// `gemini-3[.N]-flash`, plus the `gemini-flash-latest` aliases.
pub(crate) fn is_gemini3_flash_id(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
    if id == "gemini-flash-latest" || id == "gemini-flash-lite-latest" {
        return true;
    }
    gemini3_suffix_matches(model_id, "-flash")
}

/// `gemma-4` / `gemma4`.
pub(crate) fn is_gemma4_id(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
    id.contains("gemma4") || id.contains("gemma-4")
}

fn gemini3_suffix_matches(model_id: &str, suffix: &str) -> bool {
    let id = model_id.to_lowercase();
    id.match_indices("gemini-3").any(|(index, prefix)| {
        let rest = &id[index + prefix.len()..];
        let Some(version) = rest.strip_prefix('.') else {
            return rest.starts_with(suffix);
        };
        let digits = version.bytes().take_while(u8::is_ascii_digit).count();
        digits > 0 && version[digits..].starts_with(suffix)
    })
}

/// `ThinkingLevel` → `ModelThinkingLevel` for `clampThinkingLevel`.
pub(crate) fn model_thinking_level(level: crate::types::ThinkingLevel) -> ModelThinkingLevel {
    match level {
        crate::types::ThinkingLevel::Minimal => ModelThinkingLevel::Minimal,
        crate::types::ThinkingLevel::Low => ModelThinkingLevel::Low,
        crate::types::ThinkingLevel::Medium => ModelThinkingLevel::Medium,
        crate::types::ThinkingLevel::High => ModelThinkingLevel::High,
        crate::types::ThinkingLevel::Xhigh => ModelThinkingLevel::Xhigh,
        crate::types::ThinkingLevel::Max => ModelThinkingLevel::Max,
        crate::types::ThinkingLevel::Ultra => ModelThinkingLevel::Ultra,
    }
}

/// Effort name for the shared thinking switches (`None` for `off`/`xhigh`/`max`/`ultra`).
pub(crate) fn thinking_effort_name(effort: ModelThinkingLevel) -> Option<&'static str> {
    match effort {
        ModelThinkingLevel::Minimal => Some("minimal"),
        ModelThinkingLevel::Low => Some("low"),
        ModelThinkingLevel::Medium => Some("medium"),
        ModelThinkingLevel::High => Some("high"),
        ModelThinkingLevel::Off | ModelThinkingLevel::Xhigh | ModelThinkingLevel::Max | ModelThinkingLevel::Ultra => {
            None
        }
    }
}

/// Build the shared `GenerateContentParameters` object both adapters send.
///
/// `thinking_config` is the already-resolved `thinkingConfig` value (the two
/// adapters map thinking levels/budgets differently).
pub(crate) struct GenerateContentParamsInput<'a> {
    pub model: &'a Model,
    pub context: &'a Context,
    pub temperature: Option<f64>,
    pub max_tokens: Option<f64>,
    pub tool_choice: Option<&'a str>,
    pub thinking_config: Option<Value>,
    pub request_aborted: bool,
    pub now: f64,
}

pub(crate) fn build_generate_content_params(
    input: GenerateContentParamsInput<'_>,
) -> Result<JsObject, GoogleRequestError> {
    let GenerateContentParamsInput {
        model,
        context,
        temperature,
        max_tokens,
        tool_choice,
        thinking_config,
        request_aborted,
        now,
    } = input;
    let contents = convert_messages(model, context, now);
    let tools = context.tools.as_deref().filter(|tools| !tools.is_empty());
    let function_calling_mode = tools
        .and(tool_choice)
        .filter(|choice| !choice.is_empty())
        .map(map_tool_choice);

    let mut config = JsObject::new();
    if let Some(temperature) = temperature {
        config.insert("temperature".into(), pi_js_compat::js_json!(temperature));
    }
    if let Some(max_tokens) = max_tokens {
        config.insert(
            "maxOutputTokens".into(),
            pi_js_compat::js_json!(clamp_google_max_output_tokens(max_tokens)),
        );
    }
    if let Some(system_prompt) = context.system_prompt.as_ref().filter(|prompt| !prompt.is_empty()) {
        config.insert(
            "systemInstruction".into(),
            JsValue::String(sanitize_surrogates(system_prompt).into()),
        );
    }
    if let Some(tools) = tools {
        let declarations = convert_tools(tools, false);
        if let Some(declarations) = declarations {
            config.insert("tools".into(), JsValue::Array(declarations));
        }
    }
    if let Some(mode) = function_calling_mode {
        config.insert(
            "toolConfig".into(),
            pi_js_compat::js_json!({"functionCallingConfig": {"mode": mode}}),
        );
    }
    if let Some(thinking_config) = thinking_config {
        config.insert("thinkingConfig".into(), thinking_config.into());
    }
    if request_aborted {
        return Err(GoogleRequestError::new("Request aborted", None));
    }

    let mut params = JsObject::new();
    params.insert("model".into(), JsValue::String(model.id.clone().into()));
    let contents = contents
        .iter()
        .map(pi_js_compat::js_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| GoogleRequestError::new(error.to_string(), None))?;
    params.insert("contents".into(), JsValue::Array(contents));
    params.insert("config".into(), JsValue::Object(config));
    Ok(params)
}

/// Failure raised by a Google SDK request.
#[derive(Debug, Clone, PartialEq)]
pub struct GoogleRequestError {
    pub message: pi_js_compat::js_json::JsString,
    pub status: Option<f64>,
}

impl GoogleRequestError {
    pub fn new(message: impl Into<pi_js_compat::js_json::JsString>, status: Option<f64>) -> Self {
        Self {
            message: message.into(),
            status,
        }
    }
}

impl std::fmt::Display for GoogleRequestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}

impl std::error::Error for GoogleRequestError {}
