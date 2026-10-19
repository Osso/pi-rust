use pi_ai::types::{
    Message, ProviderNativeContext, TextContent, TextOrImageContent, UserContent, UserMessage, UserRole,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::agent::AgentMessage;
use pi_js_compat::number::js_number_value;

/// Role tag for shell-command execution messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BashExecutionRole {
    BashExecution,
}

impl BashExecutionRole {
    /// Returns the exact source role literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BashExecution => "bashExecution",
        }
    }
}

/// Role tag for branch-summary messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchSummaryRole {
    BranchSummary,
}

impl BranchSummaryRole {
    /// Returns the exact source role literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BranchSummary => "branchSummary",
        }
    }
}

/// Value-only record for a summary returned from a conversation branch.
#[derive(Debug)]
pub struct BranchSummaryMessage {
    pub role: BranchSummaryRole,
    pub summary: String,
    pub from_id: String,
    pub timestamp: f64,
}

/// Value-only record for shell-command execution results.
#[derive(Debug)]
pub struct BashExecutionMessage {
    pub role: BashExecutionRole,
    pub command: String,
    pub output: String,
    pub exit_code: Option<f64>,
    pub cancelled: bool,
    pub truncated: bool,
    pub full_output_path: Option<String>,
    pub timestamp: f64,
    pub exclude_from_context: Option<bool>,
}

/// Prefix framing a compacted conversation summary.
pub const COMPACTION_SUMMARY_PREFIX: &str =
    "The conversation history before this point was compacted into the following summary:\n\n<summary>\n";

/// Suffix framing a compacted conversation summary.
pub const COMPACTION_SUMMARY_SUFFIX: &str = "\n</summary>";

/// Prefix framing a summary returned from a conversation branch.
pub const BRANCH_SUMMARY_PREFIX: &str =
    "The following is a summary of a branch that this conversation came back from:\n\n<summary>\n";

/// Suffix framing a summary returned from a conversation branch.
pub const BRANCH_SUMMARY_SUFFIX: &str = "</summary>";

/// Options recorded on a compaction summary message.
#[derive(Debug, Clone, Default)]
pub struct CompactionSummaryOptions {
    pub duration_ms: Option<f64>,
    pub tokens_after: Option<f64>,
    pub kept_from_previous_context_tokens: Option<f64>,
    pub compacted_result_tokens: Option<f64>,
    pub provider_native: Option<ProviderNativeContext>,
}

/// `CustomMessage.content`: `string | (TextContent | ImageContent)[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CustomMessageContent {
    Text(String),
    Blocks(Vec<TextOrImageContent>),
}

fn insert_optional(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_owned(), value);
    }
}

/// Render a shell-execution message as the text inserted into LLM context.
pub fn bash_execution_to_text(message: &BashExecutionMessage) -> String {
    let mut text = format!("Ran `{}`\n", message.command);
    if message.output.is_empty() {
        text.push_str("(no output)");
    } else {
        text.push_str(&format!("```\n{}\n```", message.output));
    }
    if message.cancelled {
        text.push_str("\n\n(command cancelled)");
    } else if let Some(exit_code) = message.exit_code
        && exit_code != 0.0
    {
        text.push_str(&format!("\n\nCommand exited with code {exit_code}"));
    }
    if message.truncated
        && let Some(path) = message.full_output_path.as_deref()
        && !path.is_empty()
    {
        text.push_str(&format!("\n\n[Output truncated. Full output: {path}]"));
    }
    text
}

/// Map agent messages onto provider messages before an LLM request.
pub fn convert_to_llm(messages: &[AgentMessage]) -> Vec<Message> {
    messages.iter().filter_map(convert_message_to_llm).collect()
}

fn convert_message_to_llm(message: &AgentMessage) -> Option<Message> {
    match message {
        AgentMessage::User(user) => Some(Message::User(user.clone())),
        AgentMessage::Assistant(assistant) => Some(Message::Assistant(assistant.clone())),
        AgentMessage::ToolResult(result) => Some(Message::ToolResult(result.clone())),
        AgentMessage::Custom(object) => custom_message_to_llm(object),
    }
}

fn custom_message_to_llm(object: &Map<String, Value>) -> Option<Message> {
    match object.get("role").and_then(Value::as_str)? {
        "bashExecution" => bash_execution_message_to_llm(object),
        "custom" => Some(custom_content_message(object)),
        "branchSummary" => Some(summary_message(object, BRANCH_SUMMARY_PREFIX, BRANCH_SUMMARY_SUFFIX)),
        "compactionSummary" => Some(compaction_summary_message(object)),
        _ => None,
    }
}

fn bash_execution_message_to_llm(object: &Map<String, Value>) -> Option<Message> {
    if object.get("excludeFromContext").is_some_and(is_truthy) {
        return None;
    }
    let message = BashExecutionMessage {
        role: BashExecutionRole::BashExecution,
        command: string_field(object, "command"),
        output: string_field(object, "output"),
        exit_code: object.get("exitCode").and_then(Value::as_f64),
        cancelled: bool_field(object, "cancelled"),
        truncated: bool_field(object, "truncated"),
        full_output_path: object.get("fullOutputPath").and_then(Value::as_str).map(str::to_owned),
        timestamp: object.get("timestamp").and_then(Value::as_f64).unwrap_or(0.0),
        exclude_from_context: None,
    };
    Some(text_message(bash_execution_to_text(&message), &message))
}

fn custom_content_message(object: &Map<String, Value>) -> Message {
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: custom_content(object.get("content")),
        input_source: None,
        provider_native: None,
        timestamp: timestamp_field(object),
    })
}

fn summary_message(object: &Map<String, Value>, prefix: &str, suffix: &str) -> Message {
    let summary = string_field(object, "summary");
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(format!(
            "{prefix}{summary}{suffix}"
        )))]),
        input_source: None,
        provider_native: None,
        timestamp: timestamp_field(object),
    })
}

fn compaction_summary_message(object: &Map<String, Value>) -> Message {
    let summary = string_field(object, "summary");
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(format!(
            "{COMPACTION_SUMMARY_PREFIX}{summary}{COMPACTION_SUMMARY_SUFFIX}"
        )))]),
        input_source: None,
        provider_native: object
            .get("providerNative")
            .and_then(|value| serde_json::from_value::<ProviderNativeContext>(value.clone()).ok()),
        timestamp: timestamp_field(object),
    })
}

fn text_message(text: String, message: &BashExecutionMessage) -> Message {
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: message.timestamp as i64,
    })
}

/// `typeof content === "string" ? [{ type: "text", text: content }] : content`.
fn custom_content(content: Option<&Value>) -> UserContent {
    match content {
        Some(Value::String(text)) => {
            UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text.clone()))])
        }
        Some(Value::Array(blocks)) => UserContent::Blocks(blocks.iter().filter_map(content_block).collect()),
        _ => UserContent::Blocks(Vec::new()),
    }
}

/// Text/image blocks outside the typed boundary are dropped.
fn content_block(value: &Value) -> Option<TextOrImageContent> {
    serde_json::from_value(value.clone()).ok()
}

fn string_field(object: &Map<String, Value>, key: &str) -> String {
    object.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn bool_field(object: &Map<String, Value>, key: &str) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn timestamp_field(object: &Map<String, Value>) -> i64 {
    object.get("timestamp").and_then(Value::as_f64).unwrap_or(0.0) as i64
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0 && !value.is_nan()),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Build the custom `branchSummary` message for a branch summary entry.
pub fn create_branch_summary_message(summary: &str, from_id: &str, timestamp: i64) -> AgentMessage {
    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String("branchSummary".to_owned()));
    message.insert("summary".to_owned(), Value::String(summary.to_owned()));
    message.insert("fromId".to_owned(), Value::String(from_id.to_owned()));
    message.insert("timestamp".to_owned(), js_number_value(timestamp as f64));
    AgentMessage::Custom(message)
}

/// Build the custom `compactionSummary` message for a compaction entry.
pub fn create_compaction_summary_message(
    summary: &str,
    tokens_before: f64,
    timestamp: i64,
    options: &CompactionSummaryOptions,
) -> AgentMessage {
    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String("compactionSummary".to_owned()));
    message.insert("summary".to_owned(), Value::String(summary.to_owned()));
    message.insert("tokensBefore".to_owned(), js_number_value(tokens_before));
    insert_optional(&mut message, "tokensAfter", options.tokens_after.map(js_number_value));
    insert_optional(
        &mut message,
        "keptFromPreviousContextTokens",
        options.kept_from_previous_context_tokens.map(js_number_value),
    );
    insert_optional(
        &mut message,
        "compactedResultTokens",
        options.compacted_result_tokens.map(js_number_value),
    );
    insert_optional(&mut message, "durationMs", options.duration_ms.map(js_number_value));
    insert_optional(
        &mut message,
        "providerNative",
        options
            .provider_native
            .as_ref()
            .and_then(|value| serde_json::to_value(value).ok()),
    );
    message.insert("timestamp".to_owned(), js_number_value(timestamp as f64));
    AgentMessage::Custom(message)
}

/// Build the custom `custom` message for an application-defined message.
pub fn create_custom_message(
    custom_type: &str,
    content: CustomMessageContent,
    display: bool,
    details: Option<Value>,
    timestamp: i64,
) -> AgentMessage {
    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String("custom".to_owned()));
    message.insert("customType".to_owned(), Value::String(custom_type.to_owned()));
    message.insert(
        "content".to_owned(),
        serde_json::to_value(content).unwrap_or(Value::Null),
    );
    message.insert("display".to_owned(), Value::Bool(display));
    insert_optional(&mut message, "details", details);
    message.insert("timestamp".to_owned(), js_number_value(timestamp as f64));
    AgentMessage::Custom(message)
}
