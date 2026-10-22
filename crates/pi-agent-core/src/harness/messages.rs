use pi_ai::types::{
    Message, ProviderNativeContext, TextContent, TextOrImageContent, UserContent, UserMessage, UserRole,
};
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value, is_truthy};
use serde::{Deserialize, Serialize};

use crate::agent::{AgentError, AgentMessage};

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
    pub summary: JsString,
    pub from_id: JsString,
    pub timestamp: f64,
}

/// Value-only record for shell-command execution results.
#[derive(Debug)]
pub struct BashExecutionMessage {
    pub role: BashExecutionRole,
    pub command: JsString,
    pub output: JsString,
    pub exit_code: Option<f64>,
    pub cancelled: bool,
    pub truncated: bool,
    pub full_output_path: Option<JsString>,
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
    pub provider_native: Option<Value>,
}

/// `CustomMessage.content`: `string | (TextContent | ImageContent)[]`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CustomMessageContent {
    Text(JsString),
    Blocks(Vec<TextOrImageContent>),
}

impl<'de> Deserialize<'de> for CustomMessageContent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        match Value::deserialize(d)? {
            Value::String(text) => Ok(Self::Text(text)),
            value @ Value::Array(_) => pi_js_compat::js_json::from_value(&value)
                .map(Self::Blocks)
                .map_err(D::Error::custom),
            _ => Err(D::Error::custom("expected custom text or block array")),
        }
    }
}

fn insert_optional(map: &mut JsObject, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.into(), value);
    }
}

/// Render a shell-execution message as the text inserted into LLM context.
pub fn bash_execution_to_text(message: &BashExecutionMessage) -> JsString {
    let mut text = JsString::join(
        [JsString::from("Ran `"), message.command.clone(), JsString::from("`\n")],
        "",
    );
    if message.output.is_empty() {
        text.push_str("(no output)");
    } else {
        text.push_str(JsString::join(
            [JsString::from("```\n"), message.output.clone(), JsString::from("\n```")],
            "",
        ));
    }

    if message.cancelled {
        text.push_str("\n\n(command cancelled)");
    } else if let Some(exit_code) = message.exit_code
        && exit_code != 0.0
    {
        text.push_str(&format!("\n\nCommand exited with code {exit_code}"));
    }
    if message.truncated
        && let Some(path) = message.full_output_path.as_ref()
        && !path.is_empty()
    {
        text.push_str(JsString::join(
            [
                JsString::from("\n\n[Output truncated. Full output: "),
                path.clone(),
                JsString::from("]"),
            ],
            "",
        ));
    }
    text
}

/// Map agent messages onto provider messages before an LLM request.
pub fn convert_to_llm(messages: &[AgentMessage]) -> Result<Vec<Message>, AgentError> {
    let converted = messages
        .iter()
        .map(convert_message_to_llm)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(converted.into_iter().flatten().collect())
}

fn convert_message_to_llm(message: &AgentMessage) -> Result<Option<Message>, AgentError> {
    match message {
        AgentMessage::User(user) => Ok(Some(Message::User(user.clone()))),
        AgentMessage::Assistant(assistant) => Ok(Some(Message::Assistant(assistant.clone()))),
        AgentMessage::ToolResult(result) => Ok(Some(Message::ToolResult(result.clone()))),
        AgentMessage::Custom(object) => custom_message_to_llm(object),
    }
}

fn custom_message_to_llm(object: &JsObject) -> Result<Option<Message>, AgentError> {
    let Some(role) = object.get("role").and_then(Value::as_string) else {
        return Ok(None);
    };
    let message = if role == "bashExecution" {
        bash_execution_message_to_llm(object)
    } else if role == "custom" {
        Some(custom_content_message(object))
    } else if role == "branchSummary" {
        Some(summary_message(object, BRANCH_SUMMARY_PREFIX, BRANCH_SUMMARY_SUFFIX))
    } else if role == "compactionSummary" {
        Some(compaction_summary_message(object)?)
    } else {
        None
    };
    Ok(message)
}

fn bash_execution_message_to_llm(object: &JsObject) -> Option<Message> {
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
        full_output_path: object.get("fullOutputPath").and_then(Value::as_string).cloned(),
        timestamp: object.get("timestamp").and_then(Value::as_f64).unwrap_or(0.0),
        exclude_from_context: None,
    };
    Some(text_message(bash_execution_to_text(&message), &message))
}

fn custom_content_message(object: &JsObject) -> Message {
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: custom_content(object.get("content")),
        input_source: None,
        provider_native: None,
        timestamp: timestamp_field(object),
    })
}

fn summary_message(object: &JsObject, prefix: &str, suffix: &str) -> Message {
    let summary = string_field(object, "summary");
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(JsString::join(
            [JsString::from(prefix), summary, JsString::from(suffix)],
            "",
        )))]),
        input_source: None,
        provider_native: None,
        timestamp: timestamp_field(object),
    })
}

fn compaction_summary_message(object: &JsObject) -> Result<Message, AgentError> {
    let summary = string_field(object, "summary");
    let provider_native = object
        .get("providerNative")
        .map(pi_js_compat::js_json::from_value::<ProviderNativeContext>)
        .transpose()
        .map_err(|error| AgentError::new(format!("Invalid provider-native context: {error}")))?;
    Ok(Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(JsString::join(
            [
                JsString::from(COMPACTION_SUMMARY_PREFIX),
                summary,
                JsString::from(COMPACTION_SUMMARY_SUFFIX),
            ],
            "",
        )))]),
        input_source: None,
        provider_native,
        timestamp: timestamp_field(object),
    }))
}

fn text_message(text: JsString, message: &BashExecutionMessage) -> Message {
    Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: message.timestamp,
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
    pi_js_compat::js_json::from_value(value).ok()
}

fn string_field(object: &JsObject, key: &str) -> JsString {
    object.get(key).and_then(Value::as_string).cloned().unwrap_or_default()
}

fn bool_field(object: &JsObject, key: &str) -> bool {
    object.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn timestamp_field(object: &JsObject) -> f64 {
    object.get("timestamp").and_then(Value::as_f64).unwrap_or(0.0)
}

/// Build the custom `branchSummary` message for a branch summary entry.
pub fn create_branch_summary_message(summary: impl Into<JsString>, from_id: &JsString, timestamp: i64) -> AgentMessage {
    let mut message = JsObject::new();
    message.insert("role".into(), Value::String("branchSummary".into()));
    message.insert("summary".into(), Value::String(summary.into()));
    message.insert("fromId".into(), Value::String(from_id.into()));
    message.insert("timestamp".into(), Value::Number(timestamp as f64));
    AgentMessage::Custom(message)
}

/// Build the custom `compactionSummary` message for a compaction entry.
pub fn create_compaction_summary_message(
    summary: impl Into<JsString>,
    tokens_before: f64,
    timestamp: i64,
    options: &CompactionSummaryOptions,
) -> AgentMessage {
    let mut message = JsObject::new();
    message.insert("role".into(), Value::String("compactionSummary".into()));
    message.insert("summary".into(), Value::String(summary.into()));
    message.insert("tokensBefore".into(), Value::Number(tokens_before));
    insert_optional(&mut message, "tokensAfter", options.tokens_after.map(Value::Number));
    insert_optional(
        &mut message,
        "keptFromPreviousContextTokens",
        options.kept_from_previous_context_tokens.map(Value::Number),
    );
    insert_optional(
        &mut message,
        "compactedResultTokens",
        options.compacted_result_tokens.map(Value::Number),
    );
    insert_optional(&mut message, "durationMs", options.duration_ms.map(Value::Number));
    insert_optional(&mut message, "providerNative", options.provider_native.clone());
    message.insert("timestamp".into(), Value::Number(timestamp as f64));
    AgentMessage::Custom(message)
}

/// Build the custom `custom` message for an application-defined message.
pub fn create_custom_message(
    custom_type: &JsString,
    content: CustomMessageContent,
    display: bool,
    details: Option<Value>,
    timestamp: i64,
) -> AgentMessage {
    let mut message = JsObject::new();
    message.insert("role".into(), Value::String("custom".into()));
    message.insert("customType".into(), Value::String(custom_type.into()));
    message.insert(
        "content".into(),
        pi_js_compat::js_json::to_value(&content).expect("custom content serializes losslessly"),
    );
    message.insert("display".into(), Value::Bool(display));
    insert_optional(&mut message, "details", details);
    message.insert("timestamp".into(), Value::Number(timestamp as f64));
    AgentMessage::Custom(message)
}
