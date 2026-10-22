//! Port of `packages/coding-agent/src/core/session-tool-output.ts`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use pi_ai::utils::uuid::random_uuid;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value, encode_node_utf8, stringify_lossless};

use crate::core::session_manager::{FileEntry, SessionEntry, UnknownSessionEntry, parse_session_file_entry};
use crate::utils::iso_time;

/// Maximum UTF-8 byte length persisted for a tool result's content.
pub const MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES: usize = 1_048_576;

/// Marker appended when persisted tool-result content is truncated.
pub const TOOL_RESULT_TRUNCATION_MARKER: &str = "\n[tool output truncated at 1 MiB]";

/// Outcome of [`migrate_tool_result_session_files`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolResultSessionMigrationReport {
    pub scanned_files: usize,
    pub changed_files: usize,
    pub truncated_messages: usize,
    pub skipped_malformed_files: usize,
    pub skipped_non_session_files: usize,
    pub skipped_error_files: usize,
    pub backup_paths: Vec<String>,
    pub errors: Vec<String>,
}

fn utf8_bytes(text: impl Into<JsString>) -> usize {
    encode_node_utf8(&text.into()).len()
}

fn utf8_prefix(text: impl Into<JsString>, max_bytes: usize) -> JsString {
    let text = text.into();
    let encoded = encode_node_utf8(&text);
    if encoded.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !encoded.is_char_boundary(end) {
        end -= 1;
    }
    encoded[..end].into()
}

fn text_block(item: &Value) -> Option<&JsString> {
    (item.get("type").and_then(Value::as_string) == Some(&JsString::from("text")))
        .then(|| item.get("text").and_then(Value::as_string))
        .flatten()
}

fn text_value(text: JsString) -> Value {
    let mut object = JsObject::new();
    object.insert("type".into(), Value::String("text".into()));
    object.insert("text".into(), Value::String(text));
    Value::Object(object)
}

fn serialized_content_bytes(content: &[Value]) -> usize {
    stringify_lossless(&Value::Array(content.to_vec())).len()
}

fn cap_text_content(content: &[Value]) -> Vec<Value> {
    let total_bytes: usize = content.iter().filter_map(text_block).map(utf8_bytes).sum();
    if total_bytes <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return content.to_vec();
    }
    let mut remaining_bytes = MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES - utf8_bytes(TOOL_RESULT_TRUNCATION_MARKER);
    content
        .iter()
        .map(|item| {
            let Some(text) = text_block(item) else {
                return item.clone();
            };
            let item_bytes = utf8_bytes(text);
            if item_bytes <= remaining_bytes {
                remaining_bytes -= item_bytes;
                return item.clone();
            }
            let replacement = if remaining_bytes == 0 {
                JsString::default()
            } else {
                utf8_prefix(text, remaining_bytes) + TOOL_RESULT_TRUNCATION_MARKER
            };
            remaining_bytes = 0;
            let mut item = item.clone();
            item.as_object_mut()
                .expect("text block is an object")
                .insert("text".into(), Value::String(replacement));
            item
        })
        .collect()
}

fn cap_serialized_content(content: &[Value]) -> Vec<Value> {
    if serialized_content_bytes(content) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return content.to_vec();
    }
    let text = JsString::join(content.iter().filter_map(text_block), "\n");
    let mut low = 0;
    let mut high = utf8_bytes(&text);
    let mut best = JsString::from(TOOL_RESULT_TRUNCATION_MARKER);
    while low <= high {
        let middle = (low + high) / 2;
        let candidate = utf8_prefix(&text, middle) + TOOL_RESULT_TRUNCATION_MARKER;
        if serialized_content_bytes(&[text_value(candidate.clone())]) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
            best = candidate;
            low = middle + 1;
        } else {
            if middle == 0 {
                break;
            }
            high = middle - 1;
        }
    }
    vec![text_value(best)]
}

fn build_tool_result_candidate(entry: &JsObject, message: &JsObject, content: Vec<Value>) -> JsObject {
    let mut candidate = entry.clone();
    let mut message = message.clone();
    message.insert("content".into(), Value::Array(content));
    candidate.insert("message".into(), Value::Object(message));
    candidate
}

fn entry_bytes(entry: &JsObject) -> usize {
    stringify_lossless(&Value::Object(entry.clone())).len()
}

fn fit_tool_result_candidate(entry: &JsObject, message: &JsObject, content: &[Value]) -> JsObject {
    let text = JsString::join(content.iter().filter_map(text_block), "\n");
    let non_text: Vec<Value> = content
        .iter()
        .filter(|item| text_block(item).is_none())
        .cloned()
        .collect();
    let mut low = 0;
    let mut high = utf8_bytes(&text);
    let mut initial = non_text.clone();
    initial.push(text_value(TOOL_RESULT_TRUNCATION_MARKER.into()));
    let mut best = build_tool_result_candidate(entry, message, initial);
    while low <= high {
        let middle = (low + high) / 2;
        let mut blocks = non_text.clone();
        blocks.push(text_value(utf8_prefix(&text, middle) + TOOL_RESULT_TRUNCATION_MARKER));
        let candidate = build_tool_result_candidate(entry, message, blocks);
        if entry_bytes(&candidate) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
            best = candidate;
            low = middle + 1;
        } else {
            if middle == 0 {
                break;
            }
            high = middle - 1;
        }
    }
    best
}

fn cap_tool_result_entry(entry: &JsObject, message: &JsObject, content: &[Value]) -> JsObject {
    let capped_content = cap_serialized_content(&cap_text_content(content));
    let initial = build_tool_result_candidate(entry, message, capped_content);
    if entry_bytes(&initial) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return initial;
    }
    let mut without_details = message.clone();
    without_details.shift_remove("details");
    for variant in [message, &without_details] {
        let candidate = fit_tool_result_candidate(entry, variant, content);
        if entry_bytes(&candidate) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
            return candidate;
        }
    }
    let mut minimal = JsObject::new();
    minimal.insert("role".into(), Value::String("toolResult".into()));
    for key in ["toolCallId", "toolName"] {
        if let Some(text) = message.get(key).and_then(Value::as_string) {
            minimal.insert(key.into(), Value::String(utf8_prefix(text, 128)));
        }
    }
    let blocks = vec![text_value(TOOL_RESULT_TRUNCATION_MARKER.into())];
    minimal.insert("content".into(), Value::Array(blocks.clone()));
    for key in ["isError", "timestamp"] {
        if let Some(value) = message.get(key) {
            minimal.insert(key.into(), value.clone());
        }
    }
    build_tool_result_candidate(entry, &minimal, blocks)
}

/// Return a persistence-only copy; arbitrary raw entry/message fields remain authoritative.
pub fn truncate_tool_result_for_persistence(entry: &SessionEntry) -> (SessionEntry, bool) {
    if entry.entry_type() != "message" {
        return (entry.clone(), false);
    }
    let value = pi_js_compat::js_json::to_value(entry).expect("session entry serializes losslessly");
    let object = value.as_object().expect("session entry is an object");
    let Some(message) = object.get("message").and_then(Value::as_object) else {
        return (entry.clone(), false);
    };
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return (entry.clone(), false);
    };
    if message.get("role").and_then(Value::as_string) != Some(&JsString::from("toolResult"))
        || entry_bytes(object) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES
    {
        return (entry.clone(), false);
    }
    (
        SessionEntry::Unknown(UnknownSessionEntry(cap_tool_result_entry(object, message, content))),
        true,
    )
}

/// Serialize one session entry while applying persistence-only tool-result truncation.
pub fn serialize_session_entry(entry: &SessionEntry) -> String {
    let (persisted, _) = truncate_tool_result_for_persistence(entry);
    serde_json::to_string(&persisted).expect("session entry serializes losslessly")
}

/// Serialize a header or entry without projecting through scalar JSON.
pub fn serialize_session_entry_for_persistence(entry: &FileEntry) -> String {
    match entry {
        FileEntry::Header(header) => serde_json::to_string(header).expect("header serializes losslessly"),
        FileEntry::Entry(entry) => serialize_session_entry(entry),
        FileEntry::Json(value) => serde_json::to_string(value).expect("JSON value serializes losslessly"),
    }
}

fn list_jsonl_files(root: &str) -> Vec<String> {
    if !Path::new(root).exists() {
        return Vec::new();
    }
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::from(root)];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path);
            } else if entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry.file_name().to_string_lossy().ends_with(".jsonl")
            {
                files.push(path.display().to_string());
            }
        }
    }
    files
}

struct ParsedSessionFile {
    lines: Vec<String>,
    entries: Vec<FileEntry>,
    size: u64,
    mtime_ms: f64,
    mode: u32,
}

enum SessionFileParseResult {
    Session(ParsedSessionFile),
    Malformed,
    NonSession,
}

fn metadata_mtime_ms(metadata: &fs::Metadata) -> f64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn metadata_mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        0
    }
}

fn read_session_file_for_migration(file_path: &str) -> SessionFileParseResult {
    let Ok(metadata) = fs::metadata(file_path) else {
        return SessionFileParseResult::Malformed;
    };
    let Ok(bytes) = fs::read(file_path) else {
        return SessionFileParseResult::Malformed;
    };
    let content = String::from_utf8_lossy(&bytes);
    let lines: Vec<String> = content.split('\n').map(str::to_owned).collect();
    let mut entries = Vec::new();
    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }
        match parse_session_file_entry(line) {
            Ok(entry) => entries.push(entry),
            Err(_) => return SessionFileParseResult::Malformed,
        }
    }
    let Some(FileEntry::Header(header)) = entries.first() else {
        return SessionFileParseResult::NonSession;
    };
    if header.id().is_empty() {
        return SessionFileParseResult::NonSession;
    }
    SessionFileParseResult::Session(ParsedSessionFile {
        lines,
        entries,
        size: metadata.len(),
        mtime_ms: metadata_mtime_ms(&metadata),
        mode: metadata_mode(&metadata),
    })
}

fn build_rewritten_session_content(
    file_path: &str,
    lines: &[String],
    entries: &[FileEntry],
    changed_indexes: &[usize],
) -> Result<String, String> {
    let mut rewritten_lines = Vec::new();
    let mut entry_index = 0;
    for (line_index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            rewritten_lines.push(line.clone());
            continue;
        }
        let Some(entry) = entries.get(entry_index) else {
            return Err(format!("Missing parsed entry for {file_path}:{}", line_index + 1));
        };
        if changed_indexes.contains(&entry_index) {
            rewritten_lines.push(serialize_session_entry_for_persistence(entry));
        } else {
            rewritten_lines.push(line.clone());
        }
        entry_index += 1;
    }
    Ok(rewritten_lines.join("\n"))
}

fn backup_path_for(file_path: &str, now_ms: i64) -> String {
    let timestamp = iso_time::format_iso_millis(now_ms).replace([':', '.'], "-");
    let suffix: String = random_uuid().chars().take(8).collect();
    format!("{file_path}.tool-output-backup-{timestamp}-{suffix}")
}

fn write_session_file_atomically(
    file_path: &str,
    content: &str,
    parsed: &ParsedSessionFile,
    now_ms: i64,
) -> Result<String, String> {
    let current = fs::metadata(file_path).map_err(|error| error.to_string())?;
    if current.len() != parsed.size || metadata_mtime_ms(&current) != parsed.mtime_ms {
        return Err("session changed while scanning; rerun after it is idle".to_owned());
    }

    let backup_path = backup_path_for(file_path, now_ms);
    let temp_path = format!("{file_path}.tool-output-temp-{}", random_uuid());
    fs::copy(file_path, &backup_path).map_err(|error| error.to_string())?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| error.to_string())?;
        file.write_all(content.as_bytes()).map_err(|error| error.to_string())?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp_path, fs::Permissions::from_mode(parsed.mode))
                .map_err(|error| error.to_string())?;
        }
        fs::rename(&temp_path, file_path).map_err(|error| error.to_string())
    })();
    let _ = fs::remove_file(&temp_path);
    result?;
    Ok(backup_path)
}

fn rewrite_session_file(
    file_path: &str,
    parsed: &ParsedSessionFile,
    entries: &[FileEntry],
    changed_indexes: &[usize],
    now_ms: i64,
) -> Result<String, String> {
    let content = build_rewritten_session_content(file_path, &parsed.lines, entries, changed_indexes)?;
    write_session_file_atomically(file_path, &content, parsed, now_ms)
}

/// Scan and atomically rewrite oversized tool-result content under an agent directory.
pub fn migrate_tool_result_session_files(agent_dir: &str, now: impl Fn() -> i64) -> ToolResultSessionMigrationReport {
    let mut report = ToolResultSessionMigrationReport::default();
    for file_path in list_jsonl_files(agent_dir) {
        report.scanned_files += 1;
        let parsed = match read_session_file_for_migration(&file_path) {
            SessionFileParseResult::Session(parsed) => parsed,
            SessionFileParseResult::Malformed => {
                report.skipped_malformed_files += 1;
                report.errors.push(format!("{file_path}: malformed JSONL"));
                continue;
            }
            SessionFileParseResult::NonSession => {
                report.skipped_non_session_files += 1;
                continue;
            }
        };

        let mut capped_entries = Vec::with_capacity(parsed.entries.len());
        let mut changed_indexes = Vec::new();
        for (index, entry) in parsed.entries.iter().enumerate() {
            let capped = match entry {
                FileEntry::Entry(entry) => {
                    let (truncated, changed) = truncate_tool_result_for_persistence(entry);
                    if changed {
                        changed_indexes.push(index);
                    }
                    FileEntry::Entry(truncated)
                }
                FileEntry::Header(_) | FileEntry::Json(_) => entry.clone(),
            };
            capped_entries.push(capped);
        }
        if changed_indexes.is_empty() {
            continue;
        }

        match rewrite_session_file(&file_path, &parsed, &capped_entries, &changed_indexes, now()) {
            Ok(backup_path) => {
                report.changed_files += 1;
                report.truncated_messages += changed_indexes.len();
                report.backup_paths.push(backup_path);
            }
            Err(message) => {
                report.skipped_error_files += 1;
                report.errors.push(format!("{file_path}: {message}"));
            }
        }
    }
    report
}
