use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::types::{
    AssistantContent, AssistantMessage, Context, Message, StopReason, TextOrImageContent, Tool, Usage, UserContent,
};

/// Estimated context usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageEstimate {
    /// Estimated total context tokens.
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub tokens: f64,
    /// Tokens reported by the most recent applicable assistant usage block.
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub usage_tokens: f64,
    /// Estimated tokens after the most recent applicable assistant usage block.
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub trailing_tokens: f64,
    /// Index of the applicable message that provided usage, or null when none exists.
    pub last_usage_index: Option<usize>,
}

/// `string | (TextContent | ImageContent)[]` content of user and tool-result messages.
pub enum TextOrImageInput<'a> {
    String(&'a str),
    Utf16(&'a pi_js_compat::js_json::JsString),
    Blocks(&'a [TextOrImageContent]),
}

const CHARS_PER_TOKEN: f64 = 4.0;
const ESTIMATED_IMAGE_CHARS: usize = 4800;

pub fn calculate_context_tokens(usage: &Usage) -> f64 {
    if usage.total_tokens == 0.0 || usage.total_tokens.is_nan() {
        usage.input + usage.output + usage.cache_read + usage.cache_write
    } else {
        usage.total_tokens
    }
}

pub fn estimate_text_tokens(text: &str) -> f64 {
    (js_len(text) as f64 / CHARS_PER_TOKEN).ceil()
}

pub fn estimate_text_and_image_content_tokens(content: TextOrImageInput<'_>) -> f64 {
    let chars = match content {
        TextOrImageInput::String(text) => js_len(text),
        TextOrImageInput::Utf16(text) => text.len(),
        TextOrImageInput::Blocks(blocks) => blocks
            .iter()
            .map(|block| match block {
                TextOrImageContent::Text(text) => text.text.len(),
                TextOrImageContent::Image(_) => ESTIMATED_IMAGE_CHARS,
            })
            .sum(),
    };
    (chars as f64 / CHARS_PER_TOKEN).ceil()
}

pub fn estimate_assistant_message_tokens(message: &AssistantMessage) -> f64 {
    let chars: usize = message
        .content
        .iter()
        .map(|block| match block {
            AssistantContent::Text(text) => text.text.len(),
            AssistantContent::Thinking(thinking) => thinking.thinking.len(),
            AssistantContent::ToolCall(call) => {
                js_len(&call.name)
                    + js_len(&pi_js_compat::js_json::stringify_lossless(
                        &pi_js_compat::js_json::JsValue::Object(call.arguments.clone()),
                    ))
            }
        })
        .sum();
    (chars as f64 / CHARS_PER_TOKEN).ceil()
}

pub fn estimate_message_tokens(message: &Message) -> f64 {
    match message {
        Message::User(user) => estimate_text_and_image_content_tokens(match &user.content {
            UserContent::Text(text) => TextOrImageInput::Utf16(text),
            UserContent::Blocks(blocks) => TextOrImageInput::Blocks(blocks),
        }),
        Message::ToolResult(result) => {
            estimate_text_and_image_content_tokens(TextOrImageInput::Blocks(&result.content))
        }
        Message::Assistant(assistant) => estimate_assistant_message_tokens(assistant),
    }
}

/// `estimateContextTokens(messages)` (the array branch of the TypeScript union).
pub fn estimate_messages_tokens(messages: &[Message]) -> ContextUsageEstimate {
    estimate_messages(messages)
}

/// `estimateContextTokens(context)`.
pub fn estimate_context_tokens(context: &Context) -> ContextUsageEstimate {
    let estimate = estimate_messages(&context.messages);
    match estimate.last_usage_index {
        Some(index) => add_added_tool_tokens(context, estimate, index),
        None => add_prefix_tokens(context, estimate),
    }
}

impl ContextUsageEstimate {
    fn with_extra_tokens(self, extra: f64) -> Self {
        Self {
            tokens: self.tokens + extra,
            usage_tokens: self.usage_tokens,
            trailing_tokens: self.trailing_tokens + extra,
            last_usage_index: self.last_usage_index,
        }
    }
}

fn add_added_tool_tokens(
    context: &Context,
    estimate: ContextUsageEstimate,
    usage_index: usize,
) -> ContextUsageEstimate {
    let added_names = added_tool_names(context, usage_index);
    let added_tool_tokens = estimate_added_tools_tokens(context.tools.as_deref(), &added_names);
    estimate.with_extra_tokens(added_tool_tokens)
}

fn added_tool_names(context: &Context, usage_index: usize) -> BTreeSet<String> {
    context.messages[usage_index + 1..]
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult(result) => result.added_tool_names.clone(),
            _ => None,
        })
        .flatten()
        .collect()
}

fn estimate_added_tools_tokens(tools: Option<&[Tool]>, added_names: &BTreeSet<String>) -> f64 {
    let added_tools: Option<Vec<Tool>> = tools.map(|tools| {
        tools
            .iter()
            .filter(|tool| added_names.contains(&tool.name))
            .cloned()
            .collect()
    });
    estimate_tools_tokens(added_tools.as_deref())
}

fn add_prefix_tokens(context: &Context, estimate: ContextUsageEstimate) -> ContextUsageEstimate {
    let system_tokens = context
        .system_prompt
        .as_deref()
        .map(estimate_text_tokens)
        .unwrap_or(0.0);
    let tool_tokens = estimate_tools_tokens(context.tools.as_deref());
    estimate.with_extra_tokens(system_tokens + tool_tokens)
}

fn estimate_messages(messages: &[Message]) -> ContextUsageEstimate {
    match last_assistant_usage(messages) {
        Some((index, usage)) => {
            let usage_tokens = calculate_context_tokens(usage);
            let trailing_tokens = messages[index + 1..].iter().map(estimate_message_tokens).sum();
            ContextUsageEstimate {
                tokens: usage_tokens + trailing_tokens,
                usage_tokens,
                trailing_tokens,
                last_usage_index: Some(index),
            }
        }
        None => {
            let tokens = messages.iter().map(estimate_message_tokens).sum();
            ContextUsageEstimate {
                tokens,
                usage_tokens: 0.0,
                trailing_tokens: tokens,
                last_usage_index: None,
            }
        }
    }
}

fn last_assistant_usage(messages: &[Message]) -> Option<(usize, &Usage)> {
    let mut latest_prefix_timestamp = f64::NEG_INFINITY;
    let mut usage_info = None;
    for (index, message) in messages.iter().enumerate() {
        if let Message::Assistant(assistant) = message {
            let usage_applies_to_prefix = assistant.timestamp as f64 >= latest_prefix_timestamp;
            if usage_applies_to_prefix
                && !matches!(assistant.stop_reason, StopReason::Aborted | StopReason::Error)
                && calculate_context_tokens(&assistant.usage) > 0.0
            {
                usage_info = Some((index, &assistant.usage));
            }
        }
        latest_prefix_timestamp = latest_prefix_timestamp.max(message_timestamp(message) as f64);
    }
    usage_info
}

fn message_timestamp(message: &Message) -> i64 {
    match message {
        Message::User(user) => user.timestamp,
        Message::Assistant(assistant) => assistant.timestamp,
        Message::ToolResult(result) => result.timestamp,
    }
}

fn estimate_tools_tokens(tools: Option<&[Tool]>) -> f64 {
    match tools {
        Some(tools) if !tools.is_empty() => estimate_text_tokens(&safe_json_stringify(tools)),
        _ => 0.0,
    }
}

fn safe_json_stringify(value: &(impl Serialize + ?Sized)) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[unserializable]".to_owned())
}

/// JavaScript `String.length`: UTF-16 code units, not Unicode scalar values.
fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}
