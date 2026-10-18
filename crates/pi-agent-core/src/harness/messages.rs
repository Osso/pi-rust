use pi_ai::types::{
    Message, ProviderNativeContext, TextContent, TextOrImageContent, UserContent, UserMessage, UserRole,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::agent::AgentMessage;

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

/// Typed view of a stored `custom` agent message.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomMessagePayload {
    custom_type: String,
    content: CustomMessageContent,
    display: bool,
    #[serde(default)]
    details: Option<Value>,
    timestamp: f64,
}

/// Typed view of a stored `bashExecution` agent message.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BashExecutionPayload {
    command: String,
    output: String,
    #[serde(default)]
    exit_code: Option<f64>,
    cancelled: bool,
    truncated: bool,
    #[serde(default)]
    full_output_path: Option<String>,
    timestamp: f64,
    #[serde(default)]
    exclude_from_context: Option<bool>,
}

/// Typed view of a stored `branchSummary` agent message.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BranchSummaryPayload {
    summary: String,
    timestamp: f64,
}

/// Typed view of a stored `compactionSummary` agent message.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompactionSummaryPayload {
    summary: String,
    #[serde(default)]
    provider_native: Option<ProviderNativeContext>,
    timestamp: f64,
}

fn json_number(value: f64) -> Value {
    if !value.is_finite() {
        return Value::Null;
    }
    if value.fract() == 0.0 && value.abs() < 1e21 {
        return serde_json::Number::from_i128(value as i128)
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn insert_optional(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_owned(), value);
    }
}

/// Build the custom `branchSummary` message for a branch summary entry.
pub fn create_branch_summary_message(summary: &str, from_id: &str, timestamp: i64) -> AgentMessage {
    let mut message = Map::new();
    message.insert("role".to_owned(), Value::String("branchSummary".to_owned()));
    message.insert("summary".to_owned(), Value::String(summary.to_owned()));
    message.insert("fromId".to_owned(), Value::String(from_id.to_owned()));
    message.insert("timestamp".to_owned(), json_number(timestamp as f64));
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
    message.insert("tokensBefore".to_owned(), json_number(tokens_before));
    insert_optional(&mut message, "tokensAfter", options.tokens_after.map(json_number));
    insert_optional(
        &mut message,
        "keptFromPreviousContextTokens",
        options.kept_from_previous_context_tokens.map(json_number),
    );
    insert_optional(
        &mut message,
        "compactedResultTokens",
        options.compacted_result_tokens.map(json_number),
    );
    insert_optional(&mut message, "durationMs", options.duration_ms.map(json_number));
    insert_optional(
        &mut message,
        "providerNative",
        options
            .provider_native
            .as_ref()
            .and_then(|value| serde_json::to_value(value).ok()),
    );
    message.insert("timestamp".to_owned(), json_number(timestamp as f64));
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
    message.insert("timestamp".to_owned(), json_number(timestamp as f64));
    AgentMessage::Custom(message)
}

/// Render a shell-execution message as model-visible text.
pub fn bash_execution_to_text(message: &BashExecutionMessage) -> String {
    let mut text = format!("Ran `{}`\n", message.command);
    if !message.output.is_empty() {
        text.push_str(&format!("```\n{}\n```", message.output));
    } else {
        text.push_str("(no output)");
    }
    if message.cancelled {
        text.push_str("\n\n(command cancelled)");
    } else if let Some(exit_code) = message.exit_code
        && exit_code != 0.0
    {
        text.push_str(&format!("\n\nCommand exited with code {}", js_number(exit_code)));
    }
    if message.truncated
        && let Some(path) = message.full_output_path.as_deref()
        && !path.is_empty()
    {
        text.push_str(&format!("\n\n[Output truncated. Full output: {path}]"));
    }
    text
}

/// Convert harness messages into provider messages.
///
/// Bash messages excluded from context and unknown custom roles are dropped. Invalid custom payloads
/// are rejected instead of being silently coerced.
pub fn convert_to_llm(messages: &[AgentMessage]) -> Result<Vec<Message>, serde_json::Error> {
    let mut converted = Vec::new();
    for message in messages {
        if let Some(message) = convert_message(message)? {
            converted.push(message);
        }
    }
    Ok(converted)
}

fn convert_message(message: &AgentMessage) -> Result<Option<Message>, serde_json::Error> {
    let AgentMessage::Custom(payload) = message else {
        return Ok(match message {
            AgentMessage::User(user) => Some(Message::User(user.clone())),
            AgentMessage::Assistant(assistant) => Some(Message::Assistant(assistant.clone())),
            AgentMessage::ToolResult(result) => Some(Message::ToolResult(result.clone())),
            AgentMessage::Custom(_) => unreachable!(),
        });
    };

    let value = Value::Object(payload.clone());
    match payload.get("role").and_then(Value::as_str) {
        Some("bashExecution") => {
            let bash: BashExecutionPayload = serde_json::from_value(value)?;
            if bash.exclude_from_context == Some(true) {
                return Ok(None);
            }
            Ok(Some(text_message(
                bash_execution_to_text(&BashExecutionMessage {
                    role: BashExecutionRole::BashExecution,
                    command: bash.command,
                    output: bash.output,
                    exit_code: bash.exit_code,
                    cancelled: bash.cancelled,
                    truncated: bash.truncated,
                    full_output_path: bash.full_output_path,
                    timestamp: bash.timestamp,
                    exclude_from_context: bash.exclude_from_context,
                }),
                bash.timestamp,
            )))
        }
        Some("custom") => {
            let custom: CustomMessagePayload = serde_json::from_value(value)?;
            Ok(Some(Message::User(UserMessage {
                role: UserRole::Tag,
                content: match custom.content {
                    CustomMessageContent::Text(text) => {
                        UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))])
                    }
                    CustomMessageContent::Blocks(blocks) => UserContent::Blocks(blocks),
                },
                input_source: None,
                provider_native: None,
                timestamp: timestamp_millis(custom.timestamp),
            })))
        }
        Some("branchSummary") => {
            let branch: BranchSummaryPayload = serde_json::from_value(value)?;
            Ok(Some(text_message(
                format!("{BRANCH_SUMMARY_PREFIX}{}{BRANCH_SUMMARY_SUFFIX}", branch.summary),
                branch.timestamp,
            )))
        }
        Some("compactionSummary") => {
            let compaction: CompactionSummaryPayload = serde_json::from_value(value)?;
            Ok(Some(Message::User(UserMessage {
                role: UserRole::Tag,
                content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(format!(
                    "{COMPACTION_SUMMARY_PREFIX}{}{COMPACTION_SUMMARY_SUFFIX}",
                    compaction.summary
                )))]),
                input_source: None,
                provider_native: compaction.provider_native,
                timestamp: timestamp_millis(compaction.timestamp),
            })))
        }
        _ => Ok(None),
    }
}

fn text_message(text: String, timestamp: f64) -> Message {
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: timestamp_millis(timestamp),
    })
}

fn timestamp_millis(value: f64) -> i64 {
    value as i64
}

fn js_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e21 {
        return format!("{}", value as i128);
    }
    value.to_string()
}
