use std::error::Error;
use std::fmt::{self, Display, Formatter};

use pi_ai::types::{
    AssistantContent as AiAssistantContent, AssistantMessage, Modality, Model, ModelThinkingLevel, StopReason,
    TextOrImageContent, ToolCall, ToolResultMessage, Usage as AiUsage, UserContent as AiUserContent, UserMessage,
};
use pi_protocol::{
    AssistantContent, AssistantStopReason, AssistantTranscriptItem, AssistantTranscriptState, JsonValue, ModelCost,
    ModelInput, ModelMetadata, ModelRef, TextContent, ThinkingContent, ThinkingLevel, ToolCallContent, ToolContent,
    ToolTranscriptItem, ToolTranscriptState, TranscriptItem, Usage, UsageCost, UsageInput, UserContent,
    UserTranscriptItem,
};

/// A lossy conversion rejected before it can cross the protocol boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolBridgeError {
    message: String,
}

impl ProtocolBridgeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ProtocolBridgeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ProtocolBridgeError {}

fn identifier(value: &str, label: &str) -> Result<String, ProtocolBridgeError> {
    if value.is_empty() {
        return Err(ProtocolBridgeError::new(format!("{label} must be a non-empty string")));
    }
    Ok(value.to_string())
}

fn timestamp(value: i64) -> Result<u64, ProtocolBridgeError> {
    u64::try_from(value).map_err(|_| ProtocolBridgeError::new("Protocol timestamps must be non-negative integers"))
}

fn non_negative_integer(value: f64) -> Option<u64> {
    if !value.is_finite() {
        return None;
    }
    Some(value.max(0.0).floor() as u64)
}

fn non_negative_number(value: f64) -> f64 {
    if value.is_finite() { value.max(0.0) } else { 0.0 }
}

fn positive_token_limit(value: f64, label: &str) -> Result<u64, ProtocolBridgeError> {
    if !value.is_finite() {
        return Err(ProtocolBridgeError::new(format!(
            "Model {label} must be a finite number"
        )));
    }
    Ok((value.floor() as u64).max(1))
}

/// Validates and copies a value from an execution boundary into the protocol's JSON-compatible subset.
///
/// The typed Rust boundary already excludes JavaScript values that have no JSON representation
/// (non-finite numbers, bigint, undefined, functions, symbols, circular references, sparse arrays).
pub fn to_protocol_json_value(value: &serde_json::Value) -> JsonValue {
    JsonValue::from_serde_json(value)
}

/// Lossily sanitize diagnostic tool details that must not affect execution semantics.
///
/// `serde_json::Value` cannot represent the non-JSON inputs the TypeScript sanitizer repairs,
/// so every typed input value already has a JSON-compatible representation.
pub fn sanitize_protocol_details(value: &serde_json::Value) -> Option<JsonValue> {
    Some(to_protocol_json_value(value))
}

pub fn to_protocol_usage(usage: Option<&AiUsage>) -> Option<Usage> {
    let usage = usage?;
    Some(Usage::new(UsageInput {
        input: non_negative_integer(usage.input).unwrap_or(0),
        output: non_negative_integer(usage.output).unwrap_or(0),
        cache_read: non_negative_integer(usage.cache_read).unwrap_or(0),
        cache_write: non_negative_integer(usage.cache_write).unwrap_or(0),
        reasoning: non_negative_integer_option(usage.reasoning),
        total_tokens: non_negative_integer(usage.total_tokens).unwrap_or(0),
        cost: UsageCost::new(
            non_negative_number(usage.cost.input),
            non_negative_number(usage.cost.output),
            non_negative_number(usage.cost.cache_read),
            non_negative_number(usage.cost.cache_write),
            non_negative_number(usage.cost.total),
        )
        .expect("normalized usage costs are finite and non-negative"),
    }))
}

fn non_negative_integer_option(value: Option<f64>) -> Option<u64> {
    value.and_then(non_negative_integer)
}

pub fn to_protocol_model_metadata(model: &Model, authenticated: bool) -> Result<ModelMetadata, ProtocolBridgeError> {
    let input = model
        .input
        .iter()
        .map(|modality| match modality {
            Modality::Text => ModelInput::Text,
            Modality::Image => ModelInput::Image,
        })
        .collect();
    ModelMetadata::new(pi_protocol::ModelMetadataInput {
        provider: identifier(&model.provider, "Model provider")?,
        id: identifier(&model.id, "Model id")?,
        name: identifier(&model.name, "Model name")?,
        api: identifier(&model.api, "Model API")?,
        reasoning: model.reasoning,
        input,
        context_window: positive_token_limit(model.context_window, "contextWindow")?,
        max_tokens: positive_token_limit(model.max_tokens, "maxTokens")?,
        cost: ModelCost::new(
            non_negative_number(model.cost.rates.input),
            non_negative_number(model.cost.rates.output),
            non_negative_number(model.cost.rates.cache_read),
            non_negative_number(model.cost.rates.cache_write),
        )
        .expect("normalized model costs are finite and non-negative"),
        supported_thinking_levels: supported_thinking_levels(model),
        authenticated,
    })
    .map_err(|error| ProtocolBridgeError::new(error.to_string()))
}

const EXTENDED_THINKING_LEVELS: [ModelThinkingLevel; 8] = [
    ModelThinkingLevel::Off,
    ModelThinkingLevel::Minimal,
    ModelThinkingLevel::Low,
    ModelThinkingLevel::Medium,
    ModelThinkingLevel::High,
    ModelThinkingLevel::Xhigh,
    ModelThinkingLevel::Max,
    ModelThinkingLevel::Ultra,
];

/// Port of `getSupportedThinkingLevels` restricted to protocol-v1 levels (no `ultra`).
fn supported_thinking_levels(model: &Model) -> Vec<ThinkingLevel> {
    if !model.reasoning {
        return vec![ThinkingLevel::Off];
    }
    EXTENDED_THINKING_LEVELS
        .iter()
        .filter(|level| {
            let mapped = model.thinking_level_map.as_ref().and_then(|levels| levels.get(level));
            if mapped == Some(&None) {
                return false;
            }
            match level {
                ModelThinkingLevel::Xhigh | ModelThinkingLevel::Max | ModelThinkingLevel::Ultra => mapped.is_some(),
                _ => true,
            }
        })
        .filter_map(|level| match level {
            ModelThinkingLevel::Off => Some(ThinkingLevel::Off),
            ModelThinkingLevel::Minimal => Some(ThinkingLevel::Minimal),
            ModelThinkingLevel::Low => Some(ThinkingLevel::Low),
            ModelThinkingLevel::Medium => Some(ThinkingLevel::Medium),
            ModelThinkingLevel::High => Some(ThinkingLevel::High),
            ModelThinkingLevel::Xhigh => Some(ThinkingLevel::XHigh),
            ModelThinkingLevel::Max => Some(ThinkingLevel::Max),
            ModelThinkingLevel::Ultra => None,
        })
        .collect()
}

fn to_protocol_user_content(content: &AiUserContent) -> Vec<UserContent> {
    match content {
        AiUserContent::Text(text) => vec![UserContent::Text(TextContent::new(text.clone()))],
        AiUserContent::Blocks(parts) => parts.iter().map(to_protocol_user_content_part).collect(),
    }
}

fn to_protocol_user_content_part(part: &TextOrImageContent) -> UserContent {
    match part {
        TextOrImageContent::Text(text) => UserContent::Text(TextContent::new(text.text.clone())),
        TextOrImageContent::Image(image) => UserContent::Image(
            pi_protocol::ImageContent::new(image.data.clone(), image.mime_type.clone())
                .expect("image MIME types are validated before protocol conversion"),
        ),
    }
}

pub fn to_protocol_user_message(message: &UserMessage, id: &str) -> Result<UserTranscriptItem, ProtocolBridgeError> {
    UserTranscriptItem::new(
        identifier(id, "Transcript item id")?,
        to_protocol_user_content(&message.content),
        timestamp(message.timestamp)?,
    )
    .map_err(|error| ProtocolBridgeError::new(error.to_string()))
}

fn to_protocol_assistant_content(message: &AssistantMessage) -> Result<Vec<AssistantContent>, ProtocolBridgeError> {
    message
        .content
        .iter()
        .map(|part| match part {
            AiAssistantContent::Text(text) => Ok(AssistantContent::Text(TextContent::new(text.text.clone()))),
            AiAssistantContent::Thinking(thinking) => Ok(AssistantContent::Thinking(ThinkingContent::new(
                thinking.thinking.clone(),
                thinking.redacted,
            ))),
            AiAssistantContent::ToolCall(call) => Ok(AssistantContent::ToolCall(
                ToolCallContent::new(
                    identifier(&call.id, "Tool call id")?,
                    identifier(&call.name, "Tool call name")?,
                    to_protocol_json_value(&serde_json::Value::Object(call.arguments.clone())),
                )
                .map_err(|error| ProtocolBridgeError::new(error.to_string()))?,
            )),
        })
        .collect()
}

fn assistant_state(message: &AssistantMessage) -> Result<AssistantTranscriptState, ProtocolBridgeError> {
    match message.stop_reason {
        StopReason::Pending => Ok(AssistantTranscriptState::Streaming),
        StopReason::Stop => Ok(AssistantTranscriptState::Complete {
            stop_reason: AssistantStopReason::Stop,
        }),
        StopReason::Length => Ok(AssistantTranscriptState::Complete {
            stop_reason: AssistantStopReason::Length,
        }),
        StopReason::ToolUse => Ok(AssistantTranscriptState::Complete {
            stop_reason: AssistantStopReason::ToolUse,
        }),
        StopReason::Deferred => Err(ProtocolBridgeError::new(
            "Deferred assistant messages are not supported by protocol v1",
        )),
        StopReason::Error => {
            if message.error_message.as_deref() == Some("") {
                return Err(ProtocolBridgeError::new("Assistant error messages must not be empty"));
            }
            Ok(AssistantTranscriptState::Error {
                error_message: message.error_message.clone(),
            })
        }
        StopReason::Aborted => Ok(AssistantTranscriptState::Aborted {
            error_message: message.error_message.clone(),
        }),
    }
}

pub fn to_protocol_assistant_message(
    message: &AssistantMessage,
    id: &str,
) -> Result<AssistantTranscriptItem, ProtocolBridgeError> {
    let response_model = message
        .response_model
        .as_deref()
        .map(|model| identifier(model, "Assistant response model"))
        .transpose()?;
    AssistantTranscriptItem::new(pi_protocol::AssistantTranscriptItemInput {
        id: identifier(id, "Transcript item id")?,
        content: to_protocol_assistant_content(message)?,
        model: ModelRef::new(
            identifier(&message.provider, "Assistant provider")?,
            identifier(&message.model, "Assistant model")?,
        )
        .map_err(|error| ProtocolBridgeError::new(error.to_string()))?,
        response_model,
        usage: to_protocol_usage(Some(&message.usage)),
        timestamp: timestamp(message.timestamp)?,
        state: assistant_state(message)?,
    })
    .map_err(|error| ProtocolBridgeError::new(error.to_string()))
}

fn to_protocol_tool_content(part: &TextOrImageContent) -> ToolContent {
    match part {
        TextOrImageContent::Text(text) => ToolContent::Text(TextContent::new(text.text.clone())),
        TextOrImageContent::Image(image) => ToolContent::Image(
            pi_protocol::ImageContent::new(image.data.clone(), image.mime_type.clone())
                .expect("image MIME types are validated before protocol conversion"),
        ),
    }
}

pub fn to_protocol_tool_result_message(
    message: &ToolResultMessage,
    id: &str,
    call: &ToolCall,
) -> Result<ToolTranscriptItem, ProtocolBridgeError> {
    let call_id = identifier(&call.id, "Tool call id")?;
    let call_name = identifier(&call.name, "Tool call name")?;
    let result_call_id = identifier(&message.tool_call_id, "Tool result call id")?;
    if result_call_id != call_id {
        return Err(ProtocolBridgeError::new(format!(
            "Tool result {result_call_id} does not match tool call {call_id}"
        )));
    }
    let result_name = identifier(&message.tool_name, "Tool result name")?;
    if result_name != call_name {
        return Err(ProtocolBridgeError::new(format!(
            "Tool result {result_name} does not match tool call {call_name}"
        )));
    }
    let state = if message.is_error {
        ToolTranscriptState::Error
    } else {
        ToolTranscriptState::Complete
    };
    ToolTranscriptItem::new(pi_protocol::ToolTranscriptItemInput {
        id: identifier(id, "Transcript item id")?,
        tool_call_id: call_id,
        tool_name: call_name,
        input: to_protocol_json_value(&serde_json::Value::Object(call.arguments.clone())),
        content: message.content.iter().map(to_protocol_tool_content).collect(),
        details: message.details.as_ref().and_then(sanitize_protocol_details),
        usage: to_protocol_usage(message.usage.as_ref()),
        timestamp: timestamp(message.timestamp)?,
        state,
    })
    .map_err(|error| ProtocolBridgeError::new(error.to_string()))
}

/// Converts a transcript item produced by the bridge into the protocol union.
pub const fn transcript_item(item: AssistantTranscriptItem) -> TranscriptItem {
    TranscriptItem::Assistant(item)
}
