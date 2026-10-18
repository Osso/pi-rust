//! Port of `packages/agent/src/harness/compaction/utils.ts`.

use indexmap::IndexSet;
use pi_ai::types::{AssistantContent, Message, TextOrImageContent, UserContent};
use serde_json::Value;

use crate::agent::AgentMessage;

const TOOL_RESULT_MAX_CHARS: usize = 2000;

/// File paths touched by a session branch or compaction range.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileOperations {
    /// Files read but not necessarily modified.
    pub read: IndexSet<String>,
    /// Files written by full-file write operations.
    pub written: IndexSet<String>,
    /// Files modified by edit operations.
    pub edited: IndexSet<String>,
}

/// Sorted read-only and modified file lists derived from [`FileOperations`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileLists {
    pub read_files: Vec<String>,
    pub modified_files: Vec<String>,
}

/// Create an empty file-operation accumulator.
pub fn create_file_ops() -> FileOperations {
    FileOperations::default()
}

/// Add file operations from assistant tool calls to an accumulator.
pub fn extract_file_ops_from_message(message: &AgentMessage, file_ops: &mut FileOperations) {
    let Some(assistant) = message.as_assistant() else {
        return;
    };
    for block in &assistant.content {
        let AssistantContent::ToolCall(call) = block else {
            continue;
        };
        let Some(path) = call.arguments.get("path").and_then(Value::as_str) else {
            continue;
        };
        match call.name.as_str() {
            "read" => {
                file_ops.read.insert(path.to_owned());
            }
            "write" => {
                file_ops.written.insert(path.to_owned());
            }
            "edit" => {
                file_ops.edited.insert(path.to_owned());
            }
            _ => {}
        }
    }
}

/// Compute sorted read-only and modified file lists from accumulated operations.
pub fn compute_file_lists(file_ops: &FileOperations) -> FileLists {
    let modified: IndexSet<&String> = file_ops.edited.iter().chain(&file_ops.written).collect();
    let mut read_files: Vec<String> = file_ops
        .read
        .iter()
        .filter(|path| !modified.contains(path))
        .cloned()
        .collect();
    let mut modified_files: Vec<String> = modified.into_iter().cloned().collect();
    sort_utf16(&mut read_files);
    sort_utf16(&mut modified_files);
    FileLists {
        read_files,
        modified_files,
    }
}

/// `Array.prototype.sort()` default comparison orders UTF-16 code units.
fn sort_utf16(values: &mut [String]) {
    values.sort_by(|left, right| left.encode_utf16().cmp(right.encode_utf16()));
}

/// Formats read-only and modified file lists as summary metadata sections.
pub fn format_file_operations(read_files: &[String], modified_files: &[String]) -> String {
    let mut sections = Vec::new();

    if !read_files.is_empty() {
        sections.push(format_section("read-files", read_files));
    }
    if !modified_files.is_empty() {
        sections.push(format_section("modified-files", modified_files));
    }

    if sections.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", sections.join("\n\n"))
    }
}

fn format_section(tag: &str, entries: &[String]) -> String {
    format!("<{tag}>\n{}\n</{tag}>", entries.join("\n"))
}

/// Serialize LLM messages to plain text for summarization prompts.
pub fn serialize_conversation(messages: &[Message]) -> String {
    let mut parts = Vec::new();

    for message in messages {
        match message {
            Message::User(user) => {
                let content = match &user.content {
                    UserContent::Text(text) => text.clone(),
                    UserContent::Blocks(blocks) => text_blocks(blocks),
                };
                if !content.is_empty() {
                    parts.push(format!("[User]: {content}"));
                }
            }
            Message::Assistant(assistant) => {
                serialize_assistant_message(&assistant.content, &mut parts);
            }
            Message::ToolResult(result) => {
                let content = text_blocks(&result.content);
                if !content.is_empty() {
                    parts.push(format!(
                        "[Tool result]: {}",
                        serialize_tool_result_for_summary(&content, result.details.as_ref())
                    ));
                }
            }
        }
    }

    parts.join("\n\n")
}

fn text_blocks(blocks: &[TextOrImageContent]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            TextOrImageContent::Text(text) => Some(text.text.as_str()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn serialize_assistant_message(content: &[AssistantContent], parts: &mut Vec<String>) {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in content {
        match block {
            AssistantContent::Text(text) => text_parts.push(text.text.clone()),
            AssistantContent::ToolCall(call) => {
                tool_calls.push(format!("{}({})", call.name, serialize_tool_arguments(&call.arguments)));
            }
            AssistantContent::Thinking(_) => {}
        }
    }

    if !text_parts.is_empty() {
        parts.push(format!("[Assistant]: {}", text_parts.join("\n")));
    }
    if !tool_calls.is_empty() {
        parts.push(format!("[Assistant tool calls]: {}", tool_calls.join("; ")));
    }
}

fn serialize_tool_arguments(arguments: &serde_json::Map<String, Value>) -> String {
    arguments
        .iter()
        .map(|(key, value)| format!("{key}={}", safe_json_stringify(value)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn safe_json_stringify(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "[unserializable]".to_owned())
}

fn serialize_tool_result_for_summary(content: &str, details: Option<&Value>) -> String {
    let serialized = truncate_for_summary(content, TOOL_RESULT_MAX_CHARS);
    let Some(full_output_path) = extract_full_output_path(details) else {
        return serialized;
    };
    if serialized.contains(full_output_path) {
        return serialized;
    }
    if js_len(content) <= TOOL_RESULT_MAX_CHARS && !has_tool_output_truncation(details) {
        return serialized;
    }
    format!("{serialized}\n\n[Output truncated. Full output: {full_output_path}]")
}

fn truncate_for_summary(text: &str, max_chars: usize) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() <= max_chars {
        return text.to_owned();
    }
    let truncated_chars = units.len() - max_chars;
    format!(
        "{}\n\n[... {truncated_chars} more characters truncated]",
        String::from_utf16_lossy(&units[..max_chars])
    )
}

fn extract_full_output_path(details: Option<&Value>) -> Option<&str> {
    details?
        .as_object()?
        .get("fullOutputPath")?
        .as_str()
        .filter(|path| !path.is_empty())
}

fn has_tool_output_truncation(details: Option<&Value>) -> bool {
    details
        .and_then(Value::as_object)
        .and_then(|details| details.get("truncation"))
        .and_then(Value::as_object)
        .and_then(|truncation| truncation.get("truncated"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// JavaScript `String.length`: UTF-16 code units, not Unicode scalar values.
fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}
