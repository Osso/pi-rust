//! Port of `packages/agent/src/harness/compaction/utils.ts`.

use indexmap::IndexSet;
use pi_ai::types::{AssistantContent, Message, TextOrImageContent, UserContent};
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};

use crate::agent::AgentMessage;

const TOOL_RESULT_MAX_CHARS: usize = 2000;

/// File paths touched by a session branch or compaction range.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOperations {
    /// Files read but not necessarily modified.
    pub read: IndexSet<JsString>,
    /// Files written by full-file write operations.
    pub written: IndexSet<JsString>,
    /// Files modified by edit operations.
    pub edited: IndexSet<JsString>,
}

/// Sorted read-only and modified file lists derived from [`FileOperations`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileLists {
    pub read_files: Vec<JsString>,
    pub modified_files: Vec<JsString>,
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
        let Some(path) = call.arguments.get("path").and_then(Value::as_string) else {
            continue;
        };
        match call.name.as_str() {
            "read" => {
                file_ops.read.insert(path.clone());
            }
            "write" => {
                file_ops.written.insert(path.clone());
            }
            "edit" => {
                file_ops.edited.insert(path.clone());
            }
            _ => {}
        }
    }
}

/// Compute sorted read-only and modified file lists from accumulated operations.
pub fn compute_file_lists(file_ops: &FileOperations) -> FileLists {
    let modified: IndexSet<&JsString> = file_ops.edited.iter().chain(&file_ops.written).collect();
    let mut read_files: Vec<JsString> = file_ops
        .read
        .iter()
        .filter(|path| !modified.contains(path))
        .cloned()
        .collect();
    let mut modified_files: Vec<JsString> = modified.into_iter().cloned().collect();
    sort_utf16(&mut read_files);
    sort_utf16(&mut modified_files);
    FileLists {
        read_files,
        modified_files,
    }
}

/// `Array.prototype.sort()` default comparison orders UTF-16 code units.
fn sort_utf16(values: &mut [JsString]) {
    values.sort();
}

/// Formats read-only and modified file lists as summary metadata sections.
pub fn format_file_operations(read_files: &[JsString], modified_files: &[JsString]) -> JsString {
    let mut sections = Vec::new();

    if !read_files.is_empty() {
        sections.push(format_section("read-files", read_files));
    }
    if !modified_files.is_empty() {
        sections.push(format_section("modified-files", modified_files));
    }

    if sections.is_empty() {
        JsString::default()
    } else {
        JsString::from("\n\n") + JsString::join(sections, "\n\n")
    }
}

fn format_section(tag: &str, entries: &[JsString]) -> JsString {
    JsString::from(format!("<{tag}>\n")) + JsString::join(entries, "\n") + JsString::from(format!("\n</{tag}>"))
}

/// Serialize LLM messages to plain text for summarization prompts.
pub fn serialize_conversation(messages: &[Message]) -> JsString {
    serialize_conversation_with_arguments(messages, &safe_json_stringify)
}

/// [`serialize_conversation`] with a caller-supplied tool-argument serializer.
///
/// The coding-agent copy of this module truncates tool arguments; the agent copy
/// serializes them in full. Both share this body.
pub fn serialize_conversation_with_arguments(
    messages: &[Message],
    serialize_argument: &dyn Fn(&Value) -> JsString,
) -> JsString {
    let mut parts = Vec::new();

    for message in messages {
        match message {
            Message::User(user) => {
                let content = match &user.content {
                    UserContent::Text(text) => text.clone(),
                    UserContent::Blocks(blocks) => text_blocks(blocks),
                };
                if !content.is_empty() {
                    parts.push(JsString::join([JsString::from("[User]: "), content], ""));
                }
            }
            Message::Assistant(assistant) => {
                serialize_assistant_message(&assistant.content, serialize_argument, &mut parts);
            }
            Message::ToolResult(result) => {
                let content = text_blocks(&result.content);
                if !content.is_empty() {
                    parts.push(JsString::join(
                        [
                            JsString::from("[Tool result]: "),
                            serialize_tool_result_for_summary(&content, result.details.as_ref()),
                        ],
                        "",
                    ));
                }
            }
        }
    }

    JsString::join(parts, "\n\n")
}

fn text_blocks(blocks: &[TextOrImageContent]) -> JsString {
    JsString::join(
        blocks.iter().filter_map(|block| match block {
            TextOrImageContent::Text(text) => Some(&text.text),
            TextOrImageContent::Image(_) => None,
        }),
        "",
    )
}

fn serialize_assistant_message(
    content: &[AssistantContent],
    serialize_argument: &dyn Fn(&Value) -> JsString,
    parts: &mut Vec<JsString>,
) {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in content {
        match block {
            AssistantContent::Text(text) => text_parts.push(text.text.clone()),
            AssistantContent::ToolCall(call) => {
                tool_calls.push(
                    JsString::from(&call.name)
                        + "("
                        + serialize_tool_arguments(&call.arguments, serialize_argument)
                        + ")",
                );
            }
            AssistantContent::Thinking(_) => {}
        }
    }

    if !text_parts.is_empty() {
        parts.push(JsString::join(
            [JsString::from("[Assistant]: "), JsString::join(text_parts, "\n")],
            "",
        ));
    }
    if !tool_calls.is_empty() {
        parts.push(JsString::from("[Assistant tool calls]: ") + JsString::join(tool_calls, "; "));
    }
}

fn serialize_tool_arguments(arguments: &JsObject, serialize_argument: &dyn Fn(&Value) -> JsString) -> JsString {
    JsString::join(
        pi_js_compat::js_json::object_entries_lossless(arguments)
            .into_iter()
            .map(|(key, value)| key.clone() + "=" + serialize_argument(value)),
        ", ",
    )
}

fn safe_json_stringify(value: &Value) -> JsString {
    pi_js_compat::js_json::stringify_lossless(value).into()
}

fn serialize_tool_result_for_summary(content: &JsString, details: Option<&Value>) -> JsString {
    let serialized = truncate_for_summary(content, TOOL_RESULT_MAX_CHARS);
    let Some(full_output_path) = extract_full_output_path(details) else {
        return serialized;
    };
    if serialized.contains(full_output_path) {
        return serialized;
    }
    if content.len() <= TOOL_RESULT_MAX_CHARS && !has_tool_output_truncation(details) {
        return serialized;
    }
    JsString::join(
        [
            serialized,
            JsString::from("\n\n[Output truncated. Full output: "),
            full_output_path.clone(),
            JsString::from("]"),
        ],
        "",
    )
}

/// Truncate text to a maximum UTF-16 length for summarization.
pub fn truncate_for_summary(text: impl Into<JsString>, max_chars: usize) -> JsString {
    let text = text.into();
    if text.len() <= max_chars {
        return text;
    }
    let truncated_chars = text.len() - max_chars;
    JsString::join(
        [
            JsString::from_utf16(text.units()[..max_chars].to_vec()),
            JsString::from(format!("\n\n[... {truncated_chars} more characters truncated]")),
        ],
        "",
    )
}

fn extract_full_output_path(details: Option<&Value>) -> Option<&JsString> {
    details?
        .as_object()?
        .get("fullOutputPath")?
        .as_string()
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
