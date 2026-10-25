//! Port of `packages/coding-agent/src/core/session-tool-output.ts`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use pi_ai::utils::random_uuid::random_uuid;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value, encode_node_utf8, stringify_lossless};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use pi_js_compat::node_fs::{
    node_file_write_error_record, node_fs_dest_error, node_fs_error, node_read_error, node_rm_error,
};

use crate::core::session_manager::{
    FileEntry, SessionEntry, SessionHeader, UnknownSessionEntry, parse_session_file_entry,
};
use crate::utils::iso_time;
use crate::utils::paths::node_join;

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
    pub backup_paths: Vec<JsString>,
    pub errors: Vec<JsString>,
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

fn minimal_tool_identity(value: Option<&Value>) -> Result<JsString, Box<NativeNodeError>> {
    let received = match value {
        Some(Value::String(text)) => return Ok(utf8_prefix(text, 128)),
        None => "undefined".to_owned(),
        Some(Value::Null) => "null".to_owned(),
        Some(Value::Bool(value)) => format!("type boolean ({value})"),
        Some(Value::Number(value)) => format!("type number ({})", pi_js_compat::number::js_number_to_string(*value)),
        Some(Value::Array(_) | Value::Object(_)) => {
            return Err(Box::new(NativeNodeError::new(
                "Error",
                "Non-string Buffer-like tool identity is not supported at the persistence fallback",
            )));
        }
    };
    Err(Box::new(NativeNodeError::from_node_constructor(
        NativeErrorClass::TypeError,
        "ERR_INVALID_ARG_TYPE",
        format!(
            "The first argument must be of type string or an instance of Buffer, ArrayBuffer, or Array or an Array-like Object. Received {received}"
        ),
    )))
}

fn cap_tool_result_entry(
    entry: &JsObject,
    message: &JsObject,
    content: &[Value],
) -> Result<JsObject, Box<NativeNodeError>> {
    let capped_content = cap_serialized_content(&cap_text_content(content));
    let initial = build_tool_result_candidate(entry, message, capped_content);
    if entry_bytes(&initial) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
        return Ok(initial);
    }
    let mut without_details = message.clone();
    without_details.shift_remove("details");
    for variant in [message, &without_details] {
        let candidate = fit_tool_result_candidate(entry, variant, content);
        if entry_bytes(&candidate) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES {
            return Ok(candidate);
        }
    }
    let mut minimal = JsObject::new();
    minimal.insert("role".into(), Value::String("toolResult".into()));
    for key in ["toolCallId", "toolName"] {
        minimal.insert(key.into(), Value::String(minimal_tool_identity(message.get(key))?));
    }
    let blocks = vec![text_value(TOOL_RESULT_TRUNCATION_MARKER.into())];
    minimal.insert("content".into(), Value::Array(blocks.clone()));
    for key in ["isError", "timestamp"] {
        if let Some(value) = message.get(key) {
            minimal.insert(key.into(), value.clone());
        }
    }
    Ok(build_tool_result_candidate(entry, &minimal, blocks))
}

/// Return a persistence-only copy; arbitrary raw entry/message fields remain authoritative.
pub fn truncate_tool_result_for_persistence(
    entry: &SessionEntry,
) -> Result<(SessionEntry, bool), Box<NativeNodeError>> {
    if entry.entry_type() != "message" {
        return Ok((entry.clone(), false));
    }
    let value = pi_js_compat::js_json::to_value(entry).expect("session entry serializes losslessly");
    let object = value.as_object().expect("session entry is an object");
    let raw_message = object.get("message");
    if matches!(raw_message, None | Some(Value::Null)) {
        let value = if raw_message.is_none() { "undefined" } else { "null" };
        return Err(Box::new(NativeNodeError::from_constructor(
            NativeErrorClass::TypeError,
            format!("Cannot read properties of {value} (reading 'role')"),
        )));
    }
    let Some(message) = raw_message.and_then(Value::as_object) else {
        return Ok((entry.clone(), false));
    };
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return Ok((entry.clone(), false));
    };
    if message.get("role").and_then(Value::as_string) != Some(&JsString::from("toolResult"))
        || entry_bytes(object) <= MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES
    {
        return Ok((entry.clone(), false));
    }
    Ok((
        SessionEntry::Unknown(UnknownSessionEntry(cap_tool_result_entry(object, message, content)?)),
        true,
    ))
}

/// Serialize one session entry while applying persistence-only tool-result truncation.
pub fn serialize_session_entry(entry: &SessionEntry) -> Result<String, Box<NativeNodeError>> {
    let (persisted, _) = truncate_tool_result_for_persistence(entry)?;
    Ok(serde_json::to_string(&persisted).expect("session entry serializes losslessly"))
}

/// Serialize a header or entry without projecting through scalar JSON.
pub fn serialize_session_entry_for_persistence(entry: &FileEntry) -> Result<String, Box<NativeNodeError>> {
    match entry {
        FileEntry::Header(header) => Ok(serde_json::to_string(header).expect("header serializes losslessly")),
        FileEntry::Entry(entry) => serialize_session_entry(entry),
        FileEntry::Json(value) => Ok(serde_json::to_string(value).expect("JSON value serializes losslessly")),
    }
}

fn list_jsonl_files(root: &JsString) -> Vec<JsString> {
    if !Path::new(&encode_node_utf8(root)).exists() {
        return Vec::new();
    }
    let mut files = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        // `readdirSync` throws for the whole directory; the scan skips it.
        let Ok(entries) = read_dir_entries(&directory) else {
            continue;
        };
        for entry in entries {
            // `readdirSync` decodes entry names as UTF-8.
            let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
            let path = node_join(directory.clone(), name.clone());
            let file_type = entry.file_type();
            if file_type.as_ref().is_ok_and(fs::FileType::is_dir) {
                pending.push(path);
            } else if file_type.as_ref().is_ok_and(fs::FileType::is_file) && name.ends_with(".jsonl") {
                files.push(path);
            }
        }
    }
    files
}

fn read_dir_entries(directory: &JsString) -> std::io::Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<_> = fs::read_dir(encode_node_utf8(directory))?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

struct ParsedSessionFile {
    lines: Vec<JsString>,
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

fn metadata_mtime_ms(metadata: &fs::Metadata) -> Result<f64, JsString> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(metadata.mtime() as f64 * 1000.0 + metadata.mtime_nsec() as f64 / 1_000_000.0)
    }
    #[cfg(not(unix))]
    {
        let modified = metadata.modified().map_err(|error| JsString::from(error.to_string()))?;
        Ok(match modified.duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs_f64() * 1000.0,
            Err(error) => -error.duration().as_secs_f64() * 1000.0,
        })
    }
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

fn stat_file(file_path: &JsString) -> Result<fs::Metadata, JsString> {
    fs::metadata(encode_node_utf8(file_path)).map_err(|error| node_fs_error(&error, "stat", file_path))
}

/// `typeof header.id === "string"`: an empty id still identifies a session.
fn has_string_id(header: &SessionHeader) -> bool {
    header.fields.get("id").and_then(Value::as_string).is_some()
}

fn read_session_file_for_migration(file_path: &JsString) -> Result<SessionFileParseResult, JsString> {
    let metadata = stat_file(file_path)?;
    let bytes = fs::read(encode_node_utf8(file_path)).map_err(|error| node_read_error(&error, file_path))?;
    let content = JsString::from(String::from_utf8_lossy(&bytes).as_ref());
    let lines: Vec<JsString> = content.split('\n').collect();
    let mut entries = Vec::new();
    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }
        match parse_session_file_entry(line.clone()) {
            Ok(entry) => entries.push(entry),
            Err(_) => return Ok(SessionFileParseResult::Malformed),
        }
    }
    let Some(FileEntry::Header(header)) = entries.first() else {
        return Ok(SessionFileParseResult::NonSession);
    };
    if !has_string_id(header) {
        return Ok(SessionFileParseResult::NonSession);
    }
    Ok(SessionFileParseResult::Session(ParsedSessionFile {
        lines,
        entries,
        size: metadata.len(),
        mtime_ms: metadata_mtime_ms(&metadata)?,
        mode: metadata_mode(&metadata),
    }))
}

fn build_rewritten_session_content(
    file_path: &JsString,
    lines: &[JsString],
    entries: &[FileEntry],
    changed_indexes: &[usize],
) -> Result<JsString, JsString> {
    let mut rewritten_lines = Vec::new();
    let mut entry_index = 0;
    for (line_index, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            rewritten_lines.push(line.clone());
            continue;
        }
        let Some(entry) = entries.get(entry_index) else {
            return Err(js_format!("Missing parsed entry for {}:{}", file_path, line_index + 1));
        };
        if changed_indexes.contains(&entry_index) {
            rewritten_lines.push(
                serialize_session_entry_for_persistence(entry)
                    .map_err(|error| error.message)?
                    .into(),
            );
        } else {
            rewritten_lines.push(line.clone());
        }
        entry_index += 1;
    }
    Ok(JsString::join(rewritten_lines, "\n"))
}

fn backup_path_for(file_path: &JsString, now_ms: i64) -> JsString {
    let timestamp = iso_time::format_iso_millis(now_ms).replace([':', '.'], "-");
    let suffix: String = random_uuid().chars().take(8).collect();
    js_format!("{}.tool-output-backup-{}-{}", file_path, timestamp, suffix)
}

/// `writeFileSync(tempPath, content, { flag: "wx", mode })`, `chmodSync`, then `renameSync`.
fn replace_with_temp_file(
    file_path: &JsString,
    temp_path: &JsString,
    content: &JsString,
    mode: u32,
) -> Result<(), JsString> {
    let encoded_temp = encode_node_utf8(temp_path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
    let mut file = options
        .open(&encoded_temp)
        .map_err(|error| node_fs_error(&error, "open", temp_path))?;
    file.write_all(encode_node_utf8(content).as_bytes())
        .map_err(|error| node_file_write_error_record(&error).message)?;
    drop(file);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&encoded_temp, fs::Permissions::from_mode(mode))
            .map_err(|error| node_fs_error(&error, "chmod", temp_path))?;
    }
    fs::rename(&encoded_temp, encode_node_utf8(file_path))
        .map_err(|error| node_fs_dest_error(&error, "rename", temp_path, file_path))
}

/// `rmSync(tempPath, { force: true })` in `finally`: a missing file is ignored,
/// any other failure replaces the write outcome.
fn remove_temp_file(temp_path: &JsString) -> Result<(), JsString> {
    match fs::remove_file(encode_node_utf8(temp_path)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(node_rm_error(&error, temp_path)),
        _ => Ok(()),
    }
}

fn write_session_file_atomically(
    file_path: &JsString,
    content: &JsString,
    parsed: &ParsedSessionFile,
    now_ms: i64,
) -> Result<JsString, JsString> {
    let current = stat_file(file_path)?;
    if current.len() != parsed.size || metadata_mtime_ms(&current)? != parsed.mtime_ms {
        return Err("session changed while scanning; rerun after it is idle".into());
    }

    let backup_path = backup_path_for(file_path, now_ms);
    let temp_path = js_format!("{}.tool-output-temp-{}", file_path, random_uuid());
    fs::copy(encode_node_utf8(file_path), encode_node_utf8(&backup_path))
        .map_err(|error| node_fs_dest_error(&error, "copyfile", file_path, &backup_path))?;
    let result = replace_with_temp_file(file_path, &temp_path, content, parsed.mode);
    remove_temp_file(&temp_path)?;
    result?;
    Ok(backup_path)
}

fn rewrite_session_file(
    file_path: &JsString,
    parsed: &ParsedSessionFile,
    entries: &[FileEntry],
    changed_indexes: &[usize],
    now_ms: i64,
) -> Result<JsString, JsString> {
    let content = build_rewritten_session_content(file_path, &parsed.lines, entries, changed_indexes)?;
    write_session_file_atomically(file_path, &content, parsed, now_ms)
}

fn cap_session_entries(entries: &[FileEntry]) -> Result<(Vec<FileEntry>, Vec<usize>), JsString> {
    let mut capped_entries = Vec::with_capacity(entries.len());
    let mut changed_indexes = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let capped = match entry {
            FileEntry::Entry(entry) => {
                let (truncated, changed) =
                    truncate_tool_result_for_persistence(entry).map_err(|error| error.message)?;
                if changed {
                    changed_indexes.push(index);
                }
                FileEntry::Entry(truncated)
            }
            FileEntry::Header(_) | FileEntry::Json(_) => entry.clone(),
        };
        capped_entries.push(capped);
    }
    Ok((capped_entries, changed_indexes))
}

/// One file of `migrateToolResultSessionFiles`; `Err` is the caught error message.
fn migrate_session_file(
    file_path: &JsString,
    report: &mut ToolResultSessionMigrationReport,
    now: &impl Fn() -> i64,
) -> Result<(), JsString> {
    let parsed = match read_session_file_for_migration(file_path)? {
        SessionFileParseResult::Session(parsed) => parsed,
        SessionFileParseResult::Malformed => {
            report.skipped_malformed_files += 1;
            report.errors.push(js_format!("{}: malformed JSONL", file_path));
            return Ok(());
        }
        SessionFileParseResult::NonSession => {
            report.skipped_non_session_files += 1;
            return Ok(());
        }
    };

    let (capped_entries, changed_indexes) = cap_session_entries(&parsed.entries)?;
    if changed_indexes.is_empty() {
        return Ok(());
    }

    let backup_path = rewrite_session_file(file_path, &parsed, &capped_entries, &changed_indexes, now())?;
    report.changed_files += 1;
    report.truncated_messages += changed_indexes.len();
    report.backup_paths.push(backup_path);
    Ok(())
}

/// Scan and atomically rewrite oversized tool-result content under an agent directory.
pub fn migrate_tool_result_session_files(
    agent_dir: &JsString,
    now: impl Fn() -> i64,
) -> ToolResultSessionMigrationReport {
    let mut report = ToolResultSessionMigrationReport::default();
    for file_path in list_jsonl_files(agent_dir) {
        report.scanned_files += 1;
        if let Err(message) = migrate_session_file(&file_path, &mut report, &now) {
            report.skipped_error_files += 1;
            report.errors.push(js_format!("{}: {}", file_path, message));
        }
    }
    report
}
