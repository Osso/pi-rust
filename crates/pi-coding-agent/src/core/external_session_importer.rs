//! Port of `packages/coding-agent/src/core/external-session-importer.ts`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{
    Api, AssistantMessage, AssistantRole, ProviderId, StopReason, TextContent, TextOrImageContent, Usage, UserContent,
    UserMessage, UserRole,
};
use pi_ai::utils::uuid::random_uuid;
use serde_json::Value;

use crate::core::session_manager::{CURRENT_SESSION_VERSION, FileEntry, SessionHeader, SessionManagerError};
use crate::core::session_tool_output::serialize_session_entry_for_persistence;
use crate::utils::iso_time;
use crate::utils::paths::{PathInputOptions, resolve_path};

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

    fn parse(value: &str) -> Option<Self> {
        match value {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSessionAlias {
    pub provider: ExternalSessionProvider,
    pub id: String,
}

#[derive(Debug, Clone, Default)]
pub struct ExternalSessionImportOptions {
    pub home_dir: Option<String>,
    pub session_dir: Option<String>,
    pub fallback_cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSessionImportResult {
    pub path: String,
    pub cwd: String,
    pub id: String,
    pub provider: ExternalSessionProvider,
}

struct SourceSession {
    path: String,
    id: String,
    cwd: String,
    messages: Vec<AgentMessage>,
}

pub fn is_external_session_alias(session_arg: &str) -> bool {
    parse_external_session_alias(session_arg).is_some()
}

pub fn import_external_session_alias(
    session_arg: &str,
    options: &ExternalSessionImportOptions,
) -> Result<Option<ExternalSessionImportResult>, SessionManagerError> {
    let Some(alias) = parse_external_session_alias(session_arg) else {
        return Ok(None);
    };
    let home_dir = options.home_dir.clone().unwrap_or_else(home_directory);
    let fallback_cwd = options.fallback_cwd.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    });
    let Some(source) = find_source_session(&alias, &home_dir, &fallback_cwd) else {
        return Ok(None);
    };

    let session_id = format!("{}-{}", alias.provider.as_str(), source.id);
    let destination_dir = match options.session_dir.as_deref() {
        Some(dir) => resolve(dir)?,
        None => crate::core::session_manager::get_default_session_dir(&source.cwd, None)?,
    };
    let destination_path = PathBuf::from(&destination_dir)
        .join(format!("{}.jsonl", sanitize_file_name(&session_id)))
        .display()
        .to_string();
    if !Path::new(&destination_path).exists() {
        write_imported_session(&destination_path, &session_id, &source)?;
    }

    Ok(Some(ExternalSessionImportResult {
        path: destination_path,
        cwd: source.cwd,
        id: session_id,
        provider: alias.provider,
    }))
}

fn resolve(input: &str) -> Result<String, SessionManagerError> {
    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    resolve_path(input, &cwd, &PathInputOptions::default()).map_err(|error| SessionManagerError::new(error.to_string()))
}

fn home_directory() -> String {
    pi_js_compat::process_env::get("HOME").unwrap_or_default()
}

fn parse_external_session_alias(session_arg: &str) -> Option<ExternalSessionAlias> {
    let separator_index = session_arg.find('/')?;
    let provider = ExternalSessionProvider::parse(&session_arg[..separator_index])?;
    let id = &session_arg[separator_index + 1..];
    if id.is_empty() || id.contains('/') {
        return None;
    }
    Some(ExternalSessionAlias {
        provider,
        id: id.to_owned(),
    })
}

fn find_source_session(alias: &ExternalSessionAlias, home_dir: &str, fallback_cwd: &str) -> Option<SourceSession> {
    let root = match alias.provider {
        ExternalSessionProvider::Codex => PathBuf::from(home_dir).join(".codex").join("sessions"),
        ExternalSessionProvider::Claude => PathBuf::from(home_dir).join(".claude").join("projects"),
    };
    for file_path in list_jsonl_files(&root.display().to_string()) {
        if !file_name_matches_alias(&file_path, alias) {
            continue;
        }
        let source = match alias.provider {
            ExternalSessionProvider::Codex => read_codex_session(&file_path, fallback_cwd),
            ExternalSessionProvider::Claude => read_claude_session(&file_path, fallback_cwd),
        };
        if let Some(source) = source
            && (source.id == alias.id || source.id.starts_with(&alias.id))
        {
            return Some(source);
        }
    }
    None
}

fn file_name_matches_alias(path: &str, alias: &ExternalSessionAlias) -> bool {
    let file_name = Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    file_name == alias.id || file_name.ends_with(&format!("-{}", alias.id)) || file_name.contains(&alias.id)
}

fn list_jsonl_files(root: &str) -> Vec<String> {
    if !Path::new(root).exists() {
        return Vec::new();
    }
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::from(root)];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
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

fn read_json_lines(path: &str) -> Vec<Value> {
    let Ok(bytes) = fs::read(path) else {
        return Vec::new();
    };
    let content = String::from_utf8_lossy(&bytes);
    content
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn read_codex_session(path: &str, fallback_cwd: &str) -> Option<SourceSession> {
    let lines = read_json_lines(path);
    let meta = lines
        .iter()
        .find(|line| line.get("type").and_then(Value::as_str) == Some("session_meta"));
    let payload = meta.and_then(|line| line.get("payload")).and_then(Value::as_object);
    let id = payload
        .and_then(|payload| string_or_undefined(payload.get("id")))
        .or_else(|| find_id_in_codex_file_name(path))?;
    let cwd = payload
        .and_then(|payload| string_or_undefined(payload.get("cwd")))
        .unwrap_or_else(|| fallback_cwd.to_owned());
    let messages = lines.iter().flat_map(codex_line_to_message).collect();
    Some(SourceSession {
        path: path.to_owned(),
        id,
        cwd,
        messages,
    })
}

fn read_claude_session(path: &str, fallback_cwd: &str) -> Option<SourceSession> {
    let lines = read_json_lines(path);
    let file_id = Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id = lines
        .iter()
        .filter_map(|line| string_or_undefined(line.get("sessionId")))
        .next()
        .unwrap_or(file_id);
    let cwd = lines
        .iter()
        .filter_map(|line| string_or_undefined(line.get("cwd")))
        .next()
        .unwrap_or_else(|| fallback_cwd.to_owned());
    let messages = lines.iter().flat_map(claude_line_to_message).collect();
    Some(SourceSession {
        path: path.to_owned(),
        id,
        cwd,
        messages,
    })
}

fn codex_line_to_message(line: &Value) -> Vec<AgentMessage> {
    if line.get("type").and_then(Value::as_str) != Some("response_item") {
        return Vec::new();
    }
    let Some(payload) = line.get("payload").and_then(Value::as_object) else {
        return Vec::new();
    };
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return Vec::new();
    }
    message_like_to_agent_messages(payload, string_or_undefined(line.get("timestamp")).as_deref(), "codex")
}

fn claude_line_to_message(line: &Value) -> Vec<AgentMessage> {
    let Some(message) = line.get("message").and_then(Value::as_object) else {
        return Vec::new();
    };
    message_like_to_agent_messages(message, string_or_undefined(line.get("timestamp")).as_deref(), "claude")
}

fn message_like_to_agent_messages(
    message: &serde_json::Map<String, Value>,
    timestamp: Option<&str>,
    provider: &str,
) -> Vec<AgentMessage> {
    let content = normalize_text_content(message.get("content"));
    if content.is_empty() {
        return Vec::new();
    }
    let unix_timestamp = timestamp
        .and_then(pi_js_compat::date::date_parse_millis)
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis() as i64)
                .unwrap_or(0)
        });
    match message.get("role").and_then(Value::as_str) {
        Some("user") => vec![AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(content),
            input_source: None,
            provider_native: None,
            timestamp: unix_timestamp,
        })],
        Some("assistant") => vec![AgentMessage::Assistant(Box::new(AssistantMessage {
            role: AssistantRole::Tag,
            content: content
                .into_iter()
                .filter_map(|block| match block {
                    TextOrImageContent::Text(text) => Some(pi_ai::types::AssistantContent::Text(text)),
                    TextOrImageContent::Image(_) => None,
                })
                .collect(),
            api: provider.to_owned() as Api,
            provider: provider.to_owned() as ProviderId,
            model: "imported-session".to_owned(),
            response_model: None,
            response_id: None,
            image_generation_result: None,
            diagnostics: None,
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            deferred: None,
            error_message: None,
            raw_stop_reason: None,
            end_turn: None,
            timestamp: unix_timestamp,
        }))],
        _ => Vec::new(),
    }
}

fn normalize_text_content(content: Option<&Value>) -> Vec<TextOrImageContent> {
    match content {
        Some(Value::String(text)) => vec![TextOrImageContent::Text(TextContent::new(text.clone()))],
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| {
                let object = block.as_object()?;
                let block_type = object.get("type").and_then(Value::as_str)?;
                if !matches!(block_type, "text" | "input_text" | "output_text") {
                    return None;
                }
                let text = object.get("text").and_then(Value::as_str)?;
                Some(TextOrImageContent::Text(TextContent::new(text)))
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn write_imported_session(path: &str, session_id: &str, source: &SourceSession) -> Result<(), SessionManagerError> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|error| SessionManagerError::new(error.to_string()))?;
    }
    let header = SessionHeader {
        header_type: crate::core::session_manager::HeaderTag::Session,
        version: Some(CURRENT_SESSION_VERSION),
        id: session_id.to_owned(),
        timestamp: iso_time::now_iso_string(),
        cwd: Some(source.cwd.clone()),
        parent_session: Some(source.path.clone()),
        original_key_order: None,
    };
    let mut entries: Vec<FileEntry> = vec![FileEntry::Header(header)];
    let mut parent_id: Option<String> = None;
    for message in &source.messages {
        let id: String = random_uuid().chars().take(8).collect();
        let timestamp = message
            .timestamp()
            .map(iso_time::format_iso_millis)
            .unwrap_or_else(iso_time::now_iso_string);
        entries.push(FileEntry::Entry(crate::core::session_manager::SessionEntry::Known(
            crate::core::session_manager::KnownSessionEntry::Message(
                crate::core::session_manager::SessionMessageEntry {
                    mailbox_store_refs: None,
                    id: id.clone(),
                    parent_id: parent_id.clone(),
                    timestamp,
                    message: Box::new(message.clone()),
                },
            ),
        )));
        parent_id = Some(id);
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|error| SessionManagerError::new(error.to_string()))?;
    for entry in &entries {
        let line = serialize_session_entry_for_persistence(entry);
        writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))?;
    }
    Ok(())
}

fn find_id_in_codex_file_name(path: &str) -> Option<String> {
    let file_name = Path::new(path).file_name()?.to_string_lossy().into_owned();
    let stem = file_name.strip_suffix(".jsonl")?;
    let all_allowed = !stem.is_empty()
        && stem
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'));
    if all_allowed {
        return Some(stem.to_owned());
    }
    if stem.len() >= 36 && is_uuid_like(&stem[stem.len() - 36..]) {
        return Some(stem[stem.len() - 36..].to_owned());
    }
    None
}

fn is_uuid_like(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, length)| group.len() == length && group.chars().all(|c| c.is_ascii_hexdigit()))
}

fn sanitize_file_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn string_or_undefined(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
