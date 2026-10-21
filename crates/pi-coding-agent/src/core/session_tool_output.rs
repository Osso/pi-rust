//! Port of `packages/coding-agent/src/core/session-tool-output.ts`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_ai::utils::uuid::random_uuid;
use serde_json::Value;

use crate::core::session_manager::{FileEntry, KnownSessionEntry, SessionEntry, SessionMessageEntry};
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

fn utf8_bytes(text: &str) -> usize {
    text.len()
}

fn utf8_prefix(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn text_block(item: &TextOrImageContent) -> Option<&TextContent> {
    match item {
        TextOrImageContent::Text(text) => Some(text),
        TextOrImageContent::Image(_) => None,
    }
}

fn text_content_bytes(content: &[TextOrImageContent]) -> usize {
    content
        .iter()
        .filter_map(text_block)
        .map(|text| utf8_bytes(&text.text))
        .sum()
}

fn cap_text_content(content: &[TextOrImageContent]) -> Vec<TextOrImageContent> {
    let total_bytes = text_content_bytes(content);
    if total_bytes <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return content.to_vec();
    }

    let mut remaining_bytes =
        MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES as i64 - utf8_bytes(TOOL_RESULT_TRUNCATION_MARKER) as i64;
    content
        .iter()
        .map(|item| match item {
            TextOrImageContent::Image(_) => item.clone(),
            TextOrImageContent::Text(text) => {
                if remaining_bytes <= 0 {
                    return TextOrImageContent::Text(TextContent::new(""));
                }
                let item_bytes = utf8_bytes(&text.text) as i64;
                if item_bytes <= remaining_bytes {
                    remaining_bytes -= item_bytes;
                    return item.clone();
                }
                let prefix = utf8_prefix(&text.text, remaining_bytes as usize);
                remaining_bytes = 0;
                TextOrImageContent::Text(TextContent::new(format!("{prefix}{TOOL_RESULT_TRUNCATION_MARKER}")))
            }
        })
        .collect()
}

fn serialized_content_bytes(content: &[TextOrImageContent]) -> usize {
    serde_json::to_vec(content).map(|bytes| bytes.len()).unwrap_or(0)
}

fn cap_serialized_content(content: &[TextOrImageContent]) -> Vec<TextOrImageContent> {
    if serialized_content_bytes(content) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return content.to_vec();
    }

    let text = content
        .iter()
        .filter_map(text_block)
        .map(|text| text.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let marker = TOOL_RESULT_TRUNCATION_MARKER;
    let mut low = 0_usize;
    let mut high = utf8_bytes(&text);
    let mut best = marker.to_owned();
    while low <= high {
        let middle = (low + high) / 2;
        let candidate = format!("{}{}", utf8_prefix(&text, middle), marker);
        let blocks = vec![TextOrImageContent::Text(TextContent::new(candidate.clone()))];
        if serialized_content_bytes(&blocks) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
            best = candidate;
            low = middle + 1;
        } else {
            if middle == 0 {
                break;
            }
            high = middle - 1;
        }
    }
    vec![TextOrImageContent::Text(TextContent::new(best))]
}

fn cap_tool_result_content(content: &[TextOrImageContent]) -> Vec<TextOrImageContent> {
    cap_serialized_content(&cap_text_content(content))
}

fn serialized_entry_bytes(entry: &SessionMessageEntry) -> usize {
    serde_json::to_string(&KnownSessionEntry::Message(entry.clone()))
        .map(|json| json.len())
        .unwrap_or(0)
}

fn build_tool_result_candidate(
    entry: &SessionMessageEntry,
    content: Vec<TextOrImageContent>,
    details: Option<Value>,
) -> SessionMessageEntry {
    let mut candidate = entry.clone();
    if let AgentMessage::ToolResult(message) = candidate.message.as_mut() {
        message.content = content;
        message.details = details;
    }
    candidate
}

fn cap_tool_result_entry(entry: &SessionMessageEntry) -> SessionMessageEntry {
    let AgentMessage::ToolResult(message) = entry.message.as_ref() else {
        return entry.clone();
    };
    if serialized_entry_bytes(entry) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return entry.clone();
    }

    let initial_content = cap_tool_result_content(&message.content);
    let initial_entry = build_tool_result_candidate(entry, initial_content, message.details.clone());
    if serialized_entry_bytes(&initial_entry) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return initial_entry;
    }

    let text = message
        .content
        .iter()
        .filter_map(text_block)
        .map(|text| text.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let non_text_content: Vec<TextOrImageContent> = message
        .content
        .iter()
        .filter(|item| text_block(item).is_none())
        .cloned()
        .collect();
    let marker = TOOL_RESULT_TRUNCATION_MARKER;
    let message_variants = [message.details.clone(), None];
    for details in message_variants {
        let mut low = 0_usize;
        let mut high = utf8_bytes(&text);
        let mut best_content = non_text_content.clone();
        best_content.push(TextOrImageContent::Text(TextContent::new(marker)));
        let mut best = build_tool_result_candidate(entry, best_content, details.clone());
        while low <= high {
            let middle = (low + high) / 2;
            let mut content = non_text_content.clone();
            content.push(TextOrImageContent::Text(TextContent::new(format!(
                "{}{}",
                utf8_prefix(&text, middle),
                marker
            ))));
            let candidate = build_tool_result_candidate(entry, content, details.clone());
            if serialized_entry_bytes(&candidate) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
                best = candidate;
                low = middle + 1;
            } else {
                if middle == 0 {
                    break;
                }
                high = middle - 1;
            }
        }
        if serialized_entry_bytes(&best) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
            return best;
        }
    }

    let minimal_content = vec![TextOrImageContent::Text(TextContent::new(marker))];
    let mut minimal = entry.clone();
    if let AgentMessage::ToolResult(message) = minimal.message.as_mut() {
        message.tool_call_id = utf8_prefix(&message.tool_call_id, 128);
        message.tool_name = utf8_prefix(&message.tool_name, 128);
        message.content = minimal_content;
        message.details = None;
    }
    minimal
}

fn truncate_session_message_entry(entry: &SessionMessageEntry) -> (SessionMessageEntry, bool) {
    if !matches!(*entry.message, AgentMessage::ToolResult(_)) {
        return (entry.clone(), false);
    }
    if serialized_entry_bytes(entry) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return (entry.clone(), false);
    }
    (cap_tool_result_entry(entry), true)
}

/// Return a persistence-only copy of a session message entry with capped tool output.
pub fn truncate_tool_result_for_persistence(entry: &SessionEntry) -> (SessionEntry, bool) {
    let SessionEntry::Known(KnownSessionEntry::Message(message)) = entry else {
        return (entry.clone(), false);
    };
    let (truncated, changed) = truncate_session_message_entry(message);
    if changed {
        (SessionEntry::Known(KnownSessionEntry::Message(truncated)), true)
    } else {
        (entry.clone(), false)
    }
}

fn serialize_entry(entry: &SessionEntry) -> String {
    match entry {
        SessionEntry::Known(KnownSessionEntry::Message(message)) => {
            let (truncated, _) = truncate_session_message_entry(message);
            serde_json::to_string(&KnownSessionEntry::Message(truncated))
        }
        _ => serde_json::to_string(entry),
    }
    .map(|json| {
        let value = pi_js_compat::js_json::parse_lossless(&json).expect("typed entry serialization is valid JSON");
        pi_js_compat::js_json::stringify_lossless(&value)
    })
    .expect("session entry must serialize")
}

/// Serialize one session entry while applying persistence-only tool-result truncation.
pub fn serialize_session_entry(entry: &SessionEntry) -> String {
    serialize_entry(entry)
}

/// Serialize a header or entry while applying persistence-only tool-result truncation.
pub fn serialize_session_entry_for_persistence(entry: &FileEntry) -> String {
    match entry {
        FileEntry::Header(header) => serde_json::to_value(header)
            .map(|value| pi_js_compat::js_json::stringify(&value))
            .unwrap_or_default(),
        FileEntry::Entry(entry) => serialize_entry(entry),
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
        match serde_json::from_str::<Value>(line) {
            Ok(value) => entries.push(
                serde_json::from_value::<FileEntry>(value)
                    .unwrap_or_else(|_| FileEntry::Entry(SessionEntry::Unknown(serde_json::Map::new()))),
            ),
            Err(_) => return SessionFileParseResult::Malformed,
        }
    }
    let Some(FileEntry::Header(header)) = entries.first() else {
        return SessionFileParseResult::NonSession;
    };
    if header.id.is_empty() {
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
                FileEntry::Header(_) => entry.clone(),
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
