//! Port of `packages/coding-agent/src/core/external-session-importer.ts`.

use std::fs;
use std::path::Path;

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, TextOrImageContent, Usage, UserContent,
    UserMessage, UserRole,
};
use pi_ai::utils::random_uuid::random_uuid;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_fs::{node_fs_error, node_read_error};

use crate::core::session_manager::{
    FileEntry, KnownSessionEntry, SessionEntry, SessionHeader, SessionManagerError, SessionMessageEntry,
    get_default_session_dir,
};
use crate::core::session_tool_output::serialize_session_entry_for_persistence;
use crate::utils::iso_time;
use crate::utils::paths::{
    PathInputOptions, home_directory, node_basename, node_dirname, node_join, process_cwd, resolve_path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalSessionProvider {
    Codex,
    Claude,
}

impl ExternalSessionProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }

    fn parse(value: &JsString) -> Option<Self> {
        if *value == "codex" {
            Some(Self::Codex)
        } else if *value == "claude" {
            Some(Self::Claude)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSessionAlias {
    pub provider: ExternalSessionProvider,
    pub id: JsString,
}

#[derive(Debug, Clone, Default)]
pub struct ExternalSessionImportOptions {
    pub home_dir: Option<JsString>,
    pub session_dir: Option<JsString>,
    pub fallback_cwd: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSessionImportResult {
    pub path: JsString,
    pub cwd: JsString,
    pub id: JsString,
    pub provider: ExternalSessionProvider,
}

/// A converted message, or the `NaN` timestamp that makes `toISOString` throw on write.
type ImportedMessage = Result<AgentMessage, InvalidTimestamp>;

#[derive(Debug, Clone, Copy)]
struct InvalidTimestamp;

struct SourceSession {
    path: JsString,
    id: JsString,
    cwd: JsString,
    messages: Vec<ImportedMessage>,
}

pub fn is_external_session_alias(session_arg: &JsString) -> bool {
    parse_external_session_alias(session_arg).is_some()
}

pub fn import_external_session_alias(
    session_arg: &JsString,
    options: &ExternalSessionImportOptions,
) -> Result<Option<ExternalSessionImportResult>, SessionManagerError> {
    let Some(alias) = parse_external_session_alias(session_arg) else {
        return Ok(None);
    };
    let home_dir = options
        .home_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(home_directory)
        .map_err(|error| SessionManagerError::new(error.message))?;
    let fallback_cwd = match &options.fallback_cwd {
        Some(cwd) => cwd.clone(),
        None => process_cwd().map_err(path_error)?,
    };
    let Some(source) = find_source_session(&alias, &home_dir, &fallback_cwd)? else {
        return Ok(None);
    };

    let session_id = js_format!("{}-{}", alias.provider.as_str(), source.id);
    let destination_dir = match options.session_dir.as_ref().filter(|dir| !dir.is_empty()) {
        Some(dir) => {
            resolve_path(dir, process_cwd().map_err(path_error)?, &PathInputOptions::default()).map_err(path_error)?
        }
        None => get_default_session_dir(&source.cwd, None)?,
    };
    let destination_path = node_join(destination_dir, js_format!("{}.jsonl", sanitize_file_name(&session_id)));
    if !Path::new(&encode_node_utf8(&destination_path)).exists() {
        write_imported_session(&destination_path, &session_id, &source)?;
    }

    Ok(Some(ExternalSessionImportResult {
        path: destination_path,
        cwd: source.cwd,
        id: session_id,
        provider: alias.provider,
    }))
}

fn path_error(error: crate::utils::paths::PathError) -> SessionManagerError {
    SessionManagerError::new(error.message().clone())
}

fn parse_external_session_alias(session_arg: &JsString) -> Option<ExternalSessionAlias> {
    let separator_index = session_arg.find("/")?;
    let provider = ExternalSessionProvider::parse(&session_arg.slice(0, separator_index))?;
    let id = session_arg.slice(separator_index + 1, session_arg.len());
    if id.is_empty() || id.contains("/") {
        return None;
    }
    Some(ExternalSessionAlias { provider, id })
}

fn find_source_session(
    alias: &ExternalSessionAlias,
    home_dir: &JsString,
    fallback_cwd: &JsString,
) -> Result<Option<SourceSession>, SessionManagerError> {
    let root = match alias.provider {
        ExternalSessionProvider::Codex => node_join(node_join(home_dir, ".codex"), "sessions"),
        ExternalSessionProvider::Claude => node_join(node_join(home_dir, ".claude"), "projects"),
    };
    for file_path in list_jsonl_files(&root)? {
        if !file_name_matches_alias(&file_path, alias) {
            continue;
        }
        let source = match alias.provider {
            ExternalSessionProvider::Codex => read_codex_session(&file_path, fallback_cwd)?,
            ExternalSessionProvider::Claude => Some(read_claude_session(&file_path, fallback_cwd)?),
        };
        if let Some(source) = source
            && (source.id == alias.id || source.id.starts_with(&alias.id))
        {
            return Ok(Some(source));
        }
    }
    Ok(None)
}

/// `basename(path, ".jsonl")`.
fn jsonl_basename(path: &JsString) -> JsString {
    let name = node_basename(path);
    if name == ".jsonl" {
        return JsString::default();
    }
    name.strip_suffix(".jsonl").unwrap_or(name)
}

fn file_name_matches_alias(path: &JsString, alias: &ExternalSessionAlias) -> bool {
    let file_name = jsonl_basename(path);
    file_name == alias.id || file_name.ends_with(js_format!("-{}", alias.id)) || file_name.contains(&alias.id)
}

fn list_jsonl_files(root: &JsString) -> Result<Vec<JsString>, SessionManagerError> {
    if !Path::new(&encode_node_utf8(root)).exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(dir) = pending.pop() {
        let encoded_dir = encode_node_utf8(&dir);
        let entries = fs::read_dir(&encoded_dir)
            .map_err(|error| SessionManagerError::new(node_fs_error(&error, "scandir", &dir)))?;
        for entry in entries {
            let entry = entry.map_err(|error| SessionManagerError::new(node_fs_error(&error, "scandir", &dir)))?;
            let file_type = entry
                .file_type()
                .map_err(|error| SessionManagerError::new(node_fs_error(&error, "scandir", &dir)))?;
            // `readdirSync` decodes names as UTF-8 with replacement.
            let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
            let path = node_join(&dir, &name);
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() && name.ends_with(".jsonl") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn read_codex_session(path: &JsString, fallback_cwd: &JsString) -> Result<Option<SourceSession>, SessionManagerError> {
    let lines = read_json_lines(path)?;
    let mut meta = None;
    for line in &lines {
        if read_property(line, "type")?.and_then(JsValue::as_string) == Some(&JsString::from("session_meta")) {
            meta = Some(line);
            break;
        }
    }
    let payload = meta.and_then(|line| line.get("payload"));
    let Some(id) = payload
        .and_then(|payload| string_or_undefined(payload.get("id")))
        .or_else(|| find_id_in_codex_file_name(path))
    else {
        return Ok(None);
    };
    let cwd = payload
        .and_then(|payload| string_or_undefined(payload.get("cwd")))
        .unwrap_or_else(|| fallback_cwd.clone());
    let mut messages = Vec::new();
    for line in &lines {
        messages.extend(codex_line_to_message(line)?);
    }
    Ok(Some(SourceSession {
        path: path.clone(),
        id,
        cwd,
        messages,
    }))
}

fn read_claude_session(path: &JsString, fallback_cwd: &JsString) -> Result<SourceSession, SessionManagerError> {
    let lines = read_json_lines(path)?;
    let file_id = jsonl_basename(path);
    let session_ids = lines
        .iter()
        .map(|line| Ok(string_or_undefined(read_property(line, "sessionId")?)))
        .collect::<Result<Vec<_>, SessionManagerError>>()?;
    let cwds = lines
        .iter()
        .map(|line| Ok(string_or_undefined(read_property(line, "cwd")?)))
        .collect::<Result<Vec<_>, SessionManagerError>>()?;
    let id = session_ids.into_iter().flatten().next().unwrap_or(file_id);
    let cwd = cwds
        .into_iter()
        .flatten()
        .next()
        .unwrap_or_else(|| fallback_cwd.clone());
    let mut messages = Vec::new();
    for line in &lines {
        messages.extend(claude_line_to_message(line)?);
    }
    Ok(SourceSession {
        path: path.clone(),
        id,
        cwd,
        messages,
    })
}

fn read_json_lines(path: &JsString) -> Result<Vec<JsValue>, SessionManagerError> {
    let encoded_path = encode_node_utf8(path);
    let bytes = fs::read(&encoded_path).map_err(|error| SessionManagerError::new(node_read_error(&error, path)))?;
    // `readFileSync(path, "utf8")` decodes with replacement.
    let content = JsString::from(String::from_utf8_lossy(&bytes).as_ref());
    Ok(content
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        // `parseJsonLine` drops unparsable lines in its `catch`.
        .filter_map(|line| js_json::parse_lossless(line).ok())
        .collect())
}

/// Property read on a parsed JSON line; `null` throws like JavaScript member access.
fn read_property<'a>(line: &'a JsValue, key: &str) -> Result<Option<&'a JsValue>, SessionManagerError> {
    if matches!(line, JsValue::Null) {
        return Err(SessionManagerError::new(format!(
            "Cannot read properties of null (reading '{key}')"
        )));
    }
    Ok(line.get(key))
}

fn codex_line_to_message(line: &JsValue) -> Result<Vec<ImportedMessage>, SessionManagerError> {
    if read_property(line, "type")?.and_then(JsValue::as_string) != Some(&JsString::from("response_item")) {
        return Ok(Vec::new());
    }
    let Some(payload) = line.get("payload").filter(|payload| is_record(payload)) else {
        return Ok(Vec::new());
    };
    if payload.get("type").and_then(JsValue::as_string) != Some(&JsString::from("message")) {
        return Ok(Vec::new());
    }
    Ok(message_like_to_agent_messages(
        payload,
        string_or_undefined(line.get("timestamp")).as_ref(),
        "codex",
    ))
}

fn claude_line_to_message(line: &JsValue) -> Result<Vec<ImportedMessage>, SessionManagerError> {
    let Some(message) = read_property(line, "message")?.filter(|message| is_record(message)) else {
        return Ok(Vec::new());
    };
    Ok(message_like_to_agent_messages(
        message,
        string_or_undefined(line.get("timestamp")).as_ref(),
        "claude",
    ))
}

/// `new Date(timestamp).getTime()`, or `Date.now()` without a timestamp.
fn unix_timestamp(timestamp: Option<&JsString>) -> Result<f64, InvalidTimestamp> {
    let Some(timestamp) = timestamp else {
        return Ok(pi_js_compat::date::js_now_millis());
    };
    timestamp
        .to_utf8()
        .ok()
        .and_then(|text| pi_js_compat::date::date_parse_millis(&text).map(|millis| millis as f64))
        .ok_or(InvalidTimestamp)
}

fn message_like_to_agent_messages(
    message: &JsValue,
    timestamp: Option<&JsString>,
    provider: &str,
) -> Vec<ImportedMessage> {
    let content = normalize_text_content(message.get("content"));
    if content.is_empty() {
        return Vec::new();
    }
    let role = message.get("role").and_then(JsValue::as_string);
    let message = if role == Some(&JsString::from("user")) {
        unix_timestamp(timestamp).map(|timestamp| user_message(content, timestamp))
    } else if role == Some(&JsString::from("assistant")) {
        unix_timestamp(timestamp).map(|timestamp| assistant_message(content, timestamp, provider))
    } else {
        return Vec::new();
    };
    vec![message]
}

fn user_message(content: Vec<TextContent>, timestamp: f64) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(content.into_iter().map(TextOrImageContent::Text).collect()),
        input_source: None,
        provider_native: None,
        timestamp,
    })
}

fn assistant_message(content: Vec<TextContent>, timestamp: f64, provider: &str) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: content.into_iter().map(AssistantContent::Text).collect(),
        api: provider.to_owned(),
        provider: provider.to_owned(),
        model: "imported-session".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp,
    }))
}

fn normalize_text_content(content: Option<&JsValue>) -> Vec<TextContent> {
    match content {
        Some(JsValue::String(text)) => vec![TextContent::new(text.clone())],
        Some(JsValue::Array(blocks)) => blocks
            .iter()
            .filter(|block| is_record(block))
            .filter(|block| {
                block
                    .get("type")
                    .and_then(JsValue::as_string)
                    .is_some_and(is_text_block_type)
            })
            .filter_map(|block| block.get("text").and_then(JsValue::as_string))
            .map(|text| TextContent::new(text.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

fn is_text_block_type(block_type: &JsString) -> bool {
    *block_type == "text" || *block_type == "input_text" || *block_type == "output_text"
}

fn write_imported_session(
    path: &JsString,
    session_id: &JsString,
    source: &SourceSession,
) -> Result<(), SessionManagerError> {
    let directory = encode_node_utf8(&node_dirname(path));
    fs::create_dir_all(&directory)
        .map_err(|error| SessionManagerError::new(node_fs_error(&error, "mkdir", &node_dirname(path))))?;
    let header = SessionHeader::new(
        session_id.clone(),
        iso_time::now_iso_string().into(),
        Some(source.cwd.clone()),
        Some(source.path.clone()),
    );
    let mut entries = vec![FileEntry::Header(header)];
    let mut parent_id: Option<JsString> = None;
    for message in &source.messages {
        let id: JsString = random_uuid()
            .map_err(Box::new)?
            .chars()
            .take(8)
            .collect::<String>()
            .into();
        let message = message
            .as_ref()
            .map_err(|_| SessionManagerError::new("Invalid time value"))?;
        let timestamp = match message.timestamp() {
            Some(timestamp) => {
                let millis = pi_js_compat::date::time_clip_millis(timestamp)
                    .ok_or_else(|| SessionManagerError::new("Invalid time value"))?;
                iso_time::format_iso_millis(millis)
            }
            None => iso_time::now_iso_string(),
        };
        entries.push(FileEntry::Entry(SessionEntry::Known(KnownSessionEntry::Message(
            SessionMessageEntry {
                mailbox_store_refs: None,
                id: id.clone(),
                parent_id: parent_id.clone(),
                timestamp: timestamp.into(),
                message: Box::new(message.clone()),
            },
        ))));
        parent_id = Some(id);
    }

    let lines: Vec<String> = entries
        .iter()
        .map(serialize_session_entry_for_persistence)
        .collect::<Result<_, _>>()?;
    let encoded_path = encode_node_utf8(path);
    fs::write(&encoded_path, format!("{}\n", lines.join("\n")))
        .map_err(|error| SessionManagerError::new(node_fs_error(&error, "open", path)))
}

fn is_codex_file_name_unit(unit: u16) -> bool {
    u8::try_from(unit).is_ok_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// `/(<uuid>|[A-Za-z0-9._-]+)\.jsonl$/.exec(basename(path))?.[1]`.
///
/// UUID characters are a subset of the second alternative, so the leftmost match is
/// the longest allowed run that ends at the `.jsonl` suffix.
fn find_id_in_codex_file_name(path: &JsString) -> Option<JsString> {
    let name = node_basename(path);
    let stem = name.strip_suffix(".jsonl")?;
    let start = stem
        .units()
        .iter()
        .rposition(|unit| !is_codex_file_name_unit(*unit))
        .map_or(0, |index| index + 1);
    (start < stem.len()).then(|| stem.slice(start, stem.len()))
}

/// `value.replace(/[^A-Za-z0-9._-]/g, "-")`, applied per UTF-16 code unit.
fn sanitize_file_name(value: &JsString) -> JsString {
    JsString::from_utf16(
        value
            .units()
            .iter()
            .map(|unit| {
                if is_codex_file_name_unit(*unit) {
                    *unit
                } else {
                    u16::from(b'-')
                }
            })
            .collect(),
    )
}

fn string_or_undefined(value: Option<&JsValue>) -> Option<JsString> {
    value
        .and_then(JsValue::as_string)
        .filter(|text| !text.is_empty())
        .cloned()
}

/// `typeof value === "object" && value !== null`.
fn is_record(value: &JsValue) -> bool {
    matches!(value, JsValue::Object(_) | JsValue::Array(_))
}

#[cfg(test)]
mod native_error_tests {
    use super::*;
    #[test]
    fn missing_import_file_error_keeps_original_raw_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = JsString::from(directory.path().to_str().expect("utf-8 test path"))
            + "/"
            + JsString::from_utf16(vec![0xd800]);
        let error = read_json_lines(&path).unwrap_err();
        // Node fs errors report the UTF-8-repaired path (pi_js_compat::node_fs format_fs_error_record).
        assert!(error.message().contains(JsString::from(encode_node_utf8(&path))));
    }
}
