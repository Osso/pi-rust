use std::collections::{BTreeMap, BTreeSet};

use crate::types::{
    AssistantContent, AssistantMessage, Message, Modality, Model, StopReason, TextContent, TextOrImageContent,
    ToolCall, ToolResultMessage, UserContent,
};

const NON_VISION_USER_IMAGE_PLACEHOLDER: &str = "(image omitted: model does not support images)";
const NON_VISION_TOOL_IMAGE_PLACEHOLDER: &str = "(tool image omitted: model does not support images)";

/// Normalize tool call IDs for cross-provider compatibility.
///
/// `now` supplies the timestamp for synthetic tool results (`Date.now()` in TypeScript).
pub fn transform_messages(messages: &[Message], model: &Model, now: f64) -> Vec<Message> {
    transform_messages_with(
        messages,
        model,
        None::<&fn(&str, &Model, &AssistantMessage) -> String>,
        now,
    )
}

/// [`transform_messages`] with an optional tool-call-ID normalizer.
pub fn transform_messages_with<N>(
    messages: &[Message],
    model: &Model,
    normalize_tool_call_id: Option<&N>,
    now: f64,
) -> Vec<Message>
where
    N: Fn(&str, &Model, &AssistantMessage) -> String,
{
    let mut tool_call_id_map = BTreeMap::new();
    let image_aware = downgrade_unsupported_images(messages, model);
    let transformed: Vec<Message> = image_aware
        .iter()
        .map(|message| transform_message(message, model, normalize_tool_call_id, &mut tool_call_id_map))
        .collect();
    insert_synthetic_tool_results(transformed, now)
}

fn transform_message<N>(
    message: &Message,
    model: &Model,
    normalize_tool_call_id: Option<&N>,
    tool_call_id_map: &mut BTreeMap<String, String>,
) -> Message
where
    N: Fn(&str, &Model, &AssistantMessage) -> String,
{
    match message {
        Message::User(_) => message.clone(),
        Message::ToolResult(result) => {
            let normalized_id = tool_call_id_map.get(&result.tool_call_id);
            match normalized_id {
                Some(normalized_id) if normalized_id != &result.tool_call_id => {
                    let mut result = result.clone();
                    result.tool_call_id = normalized_id.clone();
                    Message::ToolResult(result)
                }
                _ => message.clone(),
            }
        }
        Message::Assistant(assistant) => Message::Assistant(Box::new(transform_assistant(
            assistant,
            model,
            normalize_tool_call_id,
            tool_call_id_map,
        ))),
    }
}

fn transform_assistant<N>(
    assistant: &AssistantMessage,
    model: &Model,
    normalize_tool_call_id: Option<&N>,
    tool_call_id_map: &mut BTreeMap<String, String>,
) -> AssistantMessage
where
    N: Fn(&str, &Model, &AssistantMessage) -> String,
{
    let is_same_model =
        assistant.provider == model.provider && assistant.api == model.api && assistant.model == model.id;
    let mut content = Vec::new();
    for block in &assistant.content {
        match block {
            AssistantContent::Thinking(thinking) => {
                if let Some(transformed) = transform_thinking(thinking, is_same_model) {
                    content.push(transformed);
                }
            }
            AssistantContent::Text(text) => {
                content.push(AssistantContent::Text(if is_same_model {
                    text.clone()
                } else {
                    TextContent::new(&text.text)
                }));
            }
            AssistantContent::ToolCall(tool_call) => {
                content.push(AssistantContent::ToolCall(transform_tool_call(
                    tool_call,
                    assistant,
                    model,
                    is_same_model,
                    normalize_tool_call_id,
                    tool_call_id_map,
                )));
            }
        }
    }

    let mut transformed = assistant.clone();
    transformed.content = content;
    transformed
}

fn transform_thinking(thinking: &crate::types::ThinkingContent, is_same_model: bool) -> Option<AssistantContent> {
    if thinking.redacted == Some(true) {
        return is_same_model.then(|| AssistantContent::Thinking(thinking.clone()));
    }
    if is_same_model
        && thinking
            .thinking_signature
            .as_deref()
            .is_some_and(|signature| !signature.is_empty())
    {
        return Some(AssistantContent::Thinking(thinking.clone()));
    }
    if is_blank(&thinking.thinking) {
        return None;
    }
    if is_same_model {
        return Some(AssistantContent::Thinking(thinking.clone()));
    }
    Some(AssistantContent::Text(TextContent::new(&thinking.thinking)))
}

fn transform_tool_call<N>(
    tool_call: &ToolCall,
    assistant: &AssistantMessage,
    model: &Model,
    is_same_model: bool,
    normalize_tool_call_id: Option<&N>,
    tool_call_id_map: &mut BTreeMap<String, String>,
) -> ToolCall
where
    N: Fn(&str, &Model, &AssistantMessage) -> String,
{
    let mut normalized = tool_call.clone();
    if !is_same_model
        && normalized
            .thought_signature
            .as_deref()
            .is_some_and(|signature| !signature.is_empty())
    {
        normalized.thought_signature = None;
    }

    if let Some(normalize) = normalize_tool_call_id {
        let normalized_id = normalize(&tool_call.id, model, assistant);
        if normalized_id != tool_call.id {
            tool_call_id_map.insert(tool_call.id.clone(), normalized_id.clone());
            normalized.id = normalized_id;
        }
    }
    normalized
}

fn downgrade_unsupported_images(messages: &[Message], model: &Model) -> Vec<Message> {
    if model.input.contains(&Modality::Image) {
        return messages.to_vec();
    }
    messages
        .iter()
        .map(|message| match message {
            Message::User(user) => match &user.content {
                UserContent::Text(_) => message.clone(),
                UserContent::Blocks(blocks) => {
                    let mut user = user.clone();
                    user.content = UserContent::Blocks(replace_images_with_placeholder(
                        blocks,
                        NON_VISION_USER_IMAGE_PLACEHOLDER,
                    ));
                    Message::User(user)
                }
            },
            Message::ToolResult(result) => {
                let mut result = result.clone();
                result.content = replace_images_with_placeholder(&result.content, NON_VISION_TOOL_IMAGE_PLACEHOLDER);
                Message::ToolResult(result)
            }
            Message::Assistant(_) => message.clone(),
        })
        .collect()
}

fn replace_images_with_placeholder(content: &[TextOrImageContent], placeholder: &str) -> Vec<TextOrImageContent> {
    let mut result = Vec::new();
    let mut previous_was_placeholder = false;
    for block in content {
        match block {
            TextOrImageContent::Image(_) => {
                if !previous_was_placeholder {
                    result.push(TextOrImageContent::Text(TextContent::new(placeholder)));
                }
                previous_was_placeholder = true;
            }
            TextOrImageContent::Text(text) => {
                result.push(block.clone());
                previous_was_placeholder = text.text == placeholder;
            }
        }
    }
    result
}

#[derive(Default)]
struct PendingToolCalls {
    calls: Vec<ToolCall>,
    ids: BTreeSet<String>,
    existing_result_ids: BTreeSet<String>,
}

impl PendingToolCalls {
    fn insert_synthetic(&mut self, result: &mut Vec<Message>, now: f64) {
        for tool_call in std::mem::take(&mut self.calls) {
            if !self.existing_result_ids.contains(&tool_call.id) {
                result.push(Message::ToolResult(ToolResultMessage {
                    role: crate::types::ToolResultRole::Tag,
                    tool_call_id: tool_call.id,
                    tool_name: tool_call.name,
                    content: vec![TextOrImageContent::Text(TextContent::new("No result provided"))],
                    details: None,
                    is_error: true,
                    timestamp: now,
                }));
            }
        }
        self.ids.clear();
        self.existing_result_ids.clear();
    }

    fn track(&mut self, tool_calls: Vec<ToolCall>) {
        self.ids = tool_calls.iter().map(|tool_call| tool_call.id.clone()).collect();
        self.calls = tool_calls;
        self.existing_result_ids.clear();
    }
}

fn insert_synthetic_tool_results(transformed: Vec<Message>, now: f64) -> Vec<Message> {
    let mut result = Vec::new();
    let mut pending = PendingToolCalls::default();
    for message in transformed {
        match message {
            Message::Assistant(assistant) => {
                pending.insert_synthetic(&mut result, now);
                if matches!(assistant.stop_reason, StopReason::Error | StopReason::Aborted) {
                    continue;
                }
                let calls = tool_calls(&assistant);
                if !calls.is_empty() {
                    pending.track(calls);
                }
                result.push(Message::Assistant(assistant));
            }
            Message::ToolResult(tool_result) => {
                if pending.ids.contains(&tool_result.tool_call_id) {
                    pending.existing_result_ids.insert(tool_result.tool_call_id.clone());
                }
                result.push(Message::ToolResult(tool_result));
            }
            Message::User(user) => {
                pending.insert_synthetic(&mut result, now);
                result.push(Message::User(user));
            }
        }
    }
    pending.insert_synthetic(&mut result, now);
    result
}

fn tool_calls(assistant: &AssistantMessage) -> Vec<ToolCall> {
    assistant
        .content
        .iter()
        .filter_map(|block| match block {
            AssistantContent::ToolCall(tool_call) => Some(tool_call.clone()),
            _ => None,
        })
        .collect()
}

fn is_blank(value: impl Into<pi_js_compat::js_json::JsString>) -> bool {
    value.into().trim().is_empty()
}
