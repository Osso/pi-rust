//! Port of `packages/coding-agent/src/core/session-manager.ts`.
//!
//! Sessions are append-only trees stored as JSONL. Control-db metadata
//! (`setMetadataControlDbPath`, goal/model/sandbox accessors and
//! metadata-backed listing) is persisted through `session_control_db`.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::messages::{
    CompactionSummaryOptions, CustomMessageContent, create_branch_summary_message, create_compaction_summary_message,
    create_custom_message,
};
use pi_ai::types::{ProviderNativeContext, TextOrImageContent, Usage, UserContent};
use pi_ai::utils::uuid::{random_uuid, uuidv7};
use pi_js_compat::number as js_number;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::{get_agent_dir, get_sessions_dir};
use crate::core::session_control_db::{
    WritableSessionMetadata, read_session_goal, read_session_metadata, read_session_sandbox_profile,
    write_session_goal, write_session_metadata, write_session_model, write_session_thinking_level,
};
use crate::core::session_tool_output::serialize_session_entry_for_persistence;
use crate::utils::iso_time;
use crate::utils::paths::{PathInputOptions, normalize_path, resolve_path};

/// Current persisted session schema version.
pub const CURRENT_SESSION_VERSION: f64 = 3.0;
/// Maximum UTF-8 byte length kept from pre-compaction messages.
pub const MAX_KEPT_PRE_COMPACTION_BYTES: usize = 20_000;

const SESSION_READ_BUFFER_SIZE: usize = 1024 * 1024;
const HEADER_READ_BUFFER_SIZE: usize = 4096;

/// Failure raised by session loading, mutation and persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionManagerError {
    message: String,
}

impl SessionManagerError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SessionManagerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SessionManagerError {}

fn control_db_error(cause: impl std::fmt::Display) -> SessionManagerError {
    error(cause.to_string())
}

fn error(message: impl Into<String>) -> SessionManagerError {
    SessionManagerError::new(message)
}

fn path_options() -> PathInputOptions {
    PathInputOptions::default()
}

fn process_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

fn resolve(input: &str) -> Result<String, SessionManagerError> {
    resolve_path(input, &process_cwd(), &path_options()).map_err(|error| SessionManagerError::new(error.to_string()))
}

fn normalize(input: &str) -> Result<String, SessionManagerError> {
    normalize_path(input, &path_options()).map_err(|error| SessionManagerError::new(error.to_string()))
}

// =============================================================================
// Wire records
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeaderTag {
    Session,
}

/// Session header (`type: "session"`).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionHeader {
    pub header_type: HeaderTag,
    pub version: Option<f64>,
    pub id: String,
    pub timestamp: String,
    pub cwd: Option<String>,
    pub parent_session: Option<String>,
    /// Original JSON key order when this header was parsed. `None` for headers
    /// created in memory; parsed headers keep TypeScript's insertion order when
    /// rewritten after a migration.
    pub original_key_order: Option<Vec<String>>,
}

const HEADER_FIELD_ORDER: [&str; 6] = ["type", "version", "id", "timestamp", "cwd", "parentSession"];

struct JsNumber(f64);

impl Serialize for JsNumber {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        js_number::serialize(&self.0, serializer)
    }
}

fn header_value(header: &SessionHeader, field: &str) -> Option<Value> {
    match field {
        "type" => Some(Value::String("session".to_owned())),
        "version" => header
            .version
            .map(|version| serde_json::to_value(JsNumber(version)).unwrap_or(Value::Null)),
        "id" => Some(Value::String(header.id.clone())),
        "timestamp" => Some(Value::String(header.timestamp.clone())),
        "cwd" => header.cwd.clone().map(Value::String),
        "parentSession" => header.parent_session.clone().map(Value::String),
        _ => None,
    }
}

impl Serialize for SessionHeader {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut object = Map::new();
        let original_order = self.original_key_order.as_deref().unwrap_or(&[]);
        for field in original_order {
            if !HEADER_FIELD_ORDER.contains(&field.as_str()) {
                continue;
            }
            // A key present in the source file keeps its explicit `null` on rewrite.
            let value = header_value(self, field).unwrap_or(Value::Null);
            object.insert(field.clone(), value);
        }
        for field in HEADER_FIELD_ORDER {
            if original_order.iter().any(|existing| existing == field) {
                continue;
            }
            if let Some(value) = header_value(self, field) {
                object.insert(field.to_owned(), value);
            }
        }
        object.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SessionHeader {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as DeError;

        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("session header must be an object"))?;
        if object.get("type").and_then(Value::as_str) != Some("session") {
            return Err(D::Error::custom("session header type must be \"session\""));
        }
        let version = match object.get("version") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_f64()
                    .ok_or_else(|| D::Error::custom("session header version must be a number"))?,
            ),
        };
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom("session header id must be a string"))?
            .to_owned();
        let timestamp = object
            .get("timestamp")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom("session header timestamp must be a string"))?
            .to_owned();
        let cwd = match object.get("cwd") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| D::Error::custom("session header cwd must be a string"))?
                    .to_owned(),
            ),
        };
        let parent_session = match object.get("parentSession") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| D::Error::custom("session header parentSession must be a string"))?
                    .to_owned(),
            ),
        };
        Ok(Self {
            header_type: HeaderTag::Session,
            version,
            id,
            timestamp,
            cwd,
            parent_session,
            original_key_order: Some(object.keys().cloned().collect()),
        })
    }
}

impl SessionHeader {
    pub fn cwd_or_empty(&self) -> &str {
        self.cwd.as_deref().unwrap_or("")
    }
}

/// Options for creating a new session.
#[derive(Debug, Clone, Default)]
pub struct NewSessionOptions {
    pub id: Option<String>,
    pub parent_session: Option<String>,
    pub is_subagent: bool,
    pub subagent_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessageEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub message: Box<AgentMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingLevelChangeEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub thinking_level: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChangeEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub summary: String,
    pub first_kept_entry_id: String,
    #[serde(serialize_with = "js_number::serialize")]
    pub tokens_before: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Box<Usage>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub duration_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Box<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_native: Option<Box<ProviderNativeContext>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_hook: Option<bool>,
    /// Legacy v1 compaction field converted during migration.
    #[serde(
        rename = "firstKeptEntryIndex",
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub first_kept_entry_index: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummaryEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub from_id: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Box<Usage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_hook: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEntry {
    pub custom_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomMessageEntry {
    pub custom_type: String,
    pub content: CustomMessageContent,
    pub display: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Closed set of known session entry shapes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KnownSessionEntry {
    Message(SessionMessageEntry),
    ThinkingLevelChange(ThinkingLevelChangeEntry),
    ModelChange(ModelChangeEntry),
    Compaction(CompactionEntry),
    BranchSummary(BranchSummaryEntry),
    Custom(CustomEntry),
    CustomMessage(CustomMessageEntry),
    Label(LabelEntry),
    SessionInfo(SessionInfoEntry),
}

/// One session entry. Unknown `type` values are preserved as raw JSON objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SessionEntry {
    Known(KnownSessionEntry),
    Unknown(Map<String, Value>),
}

impl SessionEntry {
    fn from_value(value: Value) -> Self {
        match serde_json::from_value::<KnownSessionEntry>(value.clone()) {
            Ok(entry) => Self::Known(entry),
            Err(_) => match value {
                Value::Object(object) => Self::Unknown(object),
                other => {
                    let mut object = Map::new();
                    object.insert("value".to_owned(), other);
                    Self::Unknown(object)
                }
            },
        }
    }

    pub fn entry_type(&self) -> &str {
        match self {
            Self::Known(KnownSessionEntry::Message(_)) => "message",
            Self::Known(KnownSessionEntry::ThinkingLevelChange(_)) => "thinking_level_change",
            Self::Known(KnownSessionEntry::ModelChange(_)) => "model_change",
            Self::Known(KnownSessionEntry::Compaction(_)) => "compaction",
            Self::Known(KnownSessionEntry::BranchSummary(_)) => "branch_summary",
            Self::Known(KnownSessionEntry::Custom(_)) => "custom",
            Self::Known(KnownSessionEntry::CustomMessage(_)) => "custom_message",
            Self::Known(KnownSessionEntry::Label(_)) => "label",
            Self::Known(KnownSessionEntry::SessionInfo(_)) => "session_info",
            Self::Unknown(object) => object.get("type").and_then(Value::as_str).unwrap_or_default(),
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => Some(&entry.id),
            Self::Known(KnownSessionEntry::ThinkingLevelChange(entry)) => Some(&entry.id),
            Self::Known(KnownSessionEntry::ModelChange(entry)) => Some(&entry.id),
            Self::Known(KnownSessionEntry::Compaction(entry)) => Some(&entry.id),
            Self::Known(KnownSessionEntry::BranchSummary(entry)) => Some(&entry.id),
            Self::Known(KnownSessionEntry::Custom(entry)) => Some(&entry.id),
            Self::Known(KnownSessionEntry::CustomMessage(entry)) => Some(&entry.id),
            Self::Known(KnownSessionEntry::Label(entry)) => Some(&entry.id),
            Self::Known(KnownSessionEntry::SessionInfo(entry)) => Some(&entry.id),
            Self::Unknown(object) => object.get("id").and_then(Value::as_str),
        }
    }

    pub fn parent_id(&self) -> Option<&str> {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => entry.parent_id.as_deref(),
            Self::Known(KnownSessionEntry::ThinkingLevelChange(entry)) => entry.parent_id.as_deref(),
            Self::Known(KnownSessionEntry::ModelChange(entry)) => entry.parent_id.as_deref(),
            Self::Known(KnownSessionEntry::Compaction(entry)) => entry.parent_id.as_deref(),
            Self::Known(KnownSessionEntry::BranchSummary(entry)) => entry.parent_id.as_deref(),
            Self::Known(KnownSessionEntry::Custom(entry)) => entry.parent_id.as_deref(),
            Self::Known(KnownSessionEntry::CustomMessage(entry)) => entry.parent_id.as_deref(),
            Self::Known(KnownSessionEntry::Label(entry)) => entry.parent_id.as_deref(),
            Self::Known(KnownSessionEntry::SessionInfo(entry)) => entry.parent_id.as_deref(),
            Self::Unknown(object) => object.get("parentId").and_then(Value::as_str),
        }
    }

    pub fn timestamp(&self) -> Option<&str> {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => Some(&entry.timestamp),
            Self::Known(KnownSessionEntry::ThinkingLevelChange(entry)) => Some(&entry.timestamp),
            Self::Known(KnownSessionEntry::ModelChange(entry)) => Some(&entry.timestamp),
            Self::Known(KnownSessionEntry::Compaction(entry)) => Some(&entry.timestamp),
            Self::Known(KnownSessionEntry::BranchSummary(entry)) => Some(&entry.timestamp),
            Self::Known(KnownSessionEntry::Custom(entry)) => Some(&entry.timestamp),
            Self::Known(KnownSessionEntry::CustomMessage(entry)) => Some(&entry.timestamp),
            Self::Known(KnownSessionEntry::Label(entry)) => Some(&entry.timestamp),
            Self::Known(KnownSessionEntry::SessionInfo(entry)) => Some(&entry.timestamp),
            Self::Unknown(object) => object.get("timestamp").and_then(Value::as_str),
        }
    }

    pub fn as_message(&self) -> Option<&SessionMessageEntry> {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => Some(entry),
            _ => None,
        }
    }

    pub fn as_compaction(&self) -> Option<&CompactionEntry> {
        match self {
            Self::Known(KnownSessionEntry::Compaction(entry)) => Some(entry),
            _ => None,
        }
    }

    pub fn as_custom_message(&self) -> Option<&CustomMessageEntry> {
        match self {
            Self::Known(KnownSessionEntry::CustomMessage(entry)) => Some(entry),
            _ => None,
        }
    }

    pub fn as_label(&self) -> Option<&LabelEntry> {
        match self {
            Self::Known(KnownSessionEntry::Label(entry)) => Some(entry),
            _ => None,
        }
    }

    pub fn as_session_info(&self) -> Option<&SessionInfoEntry> {
        match self {
            Self::Known(KnownSessionEntry::SessionInfo(entry)) => Some(entry),
            _ => None,
        }
    }

    fn message(&self) -> Option<&AgentMessage> {
        self.as_message().map(|entry| entry.message.as_ref())
    }

    fn set_id_parent(&mut self, id: String, parent_id: Option<String>) {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Known(KnownSessionEntry::ThinkingLevelChange(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Known(KnownSessionEntry::ModelChange(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Known(KnownSessionEntry::Compaction(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Known(KnownSessionEntry::BranchSummary(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Known(KnownSessionEntry::Custom(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Known(KnownSessionEntry::CustomMessage(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Known(KnownSessionEntry::Label(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Known(KnownSessionEntry::SessionInfo(entry)) => {
                entry.id = id;
                entry.parent_id = parent_id;
            }
            Self::Unknown(object) => {
                object.insert("id".to_owned(), Value::String(id));
                object.insert(
                    "parentId".to_owned(),
                    parent_id.map(Value::String).unwrap_or(Value::Null),
                );
            }
        }
    }

    fn set_hook_role_to_custom(&mut self) {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => {
                if let AgentMessage::Custom(object) = entry.message.as_mut()
                    && object.get("role").and_then(Value::as_str) == Some("hookMessage")
                {
                    object.insert("role".to_owned(), Value::String("custom".to_owned()));
                }
            }
            Self::Unknown(object) => {
                if let Some(message) = object.get_mut("message").and_then(Value::as_object_mut)
                    && message.get("role").and_then(Value::as_str) == Some("hookMessage")
                {
                    message.insert("role".to_owned(), Value::String("custom".to_owned()));
                }
            }
            _ => {}
        }
    }
}

/// Header or entry as read from a session file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileEntry {
    Header(SessionHeader),
    Entry(SessionEntry),
}

impl FileEntry {
    pub fn entry(&self) -> Option<&SessionEntry> {
        match self {
            Self::Header(_) => None,
            Self::Entry(entry) => Some(entry),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModel {
    pub provider: String,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionContext {
    pub messages: Vec<AgentMessage>,
    pub thinking_level: String,
    pub model: Option<SessionModel>,
}

/// Which leaf to walk from. `Last` maps `leafId === undefined`, `None` maps
/// `leafId === null`, and `Id` maps a concrete entry id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeafSelection<'a> {
    #[default]
    Last,
    None,
    Id(&'a str),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTreeNode {
    pub entry: SessionEntry,
    pub children: Vec<SessionTreeNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub path: String,
    pub id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub is_archived: Option<bool>,
    pub parent_session_path: Option<String>,
    /// Unix milliseconds.
    pub created: i64,
    /// Unix milliseconds.
    pub modified: i64,
    pub message_count: usize,
    pub first_message: String,
    pub all_messages_text: String,
}

/// Progress callback: `(loaded, total)`.
pub type SessionListProgress<'a> = &'a mut dyn FnMut(usize, usize);

/// Selection used by [`SessionManager::fork_from`] for the copied entry range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkEntrySelection {
    All,
    None,
    From(String),
}

/// Optional compaction metadata recorded by [`SessionManager::append_compaction`].
#[derive(Debug, Clone, Default)]
pub struct CompactionAppendOptions {
    pub details: Option<Value>,
    pub from_hook: Option<bool>,
    pub usage: Option<Usage>,
    pub duration_ms: Option<f64>,
    pub provider_native: Option<ProviderNativeContext>,
}

// =============================================================================
// Pure helpers
// =============================================================================

fn generate_id(by_id: &HashMap<String, usize>) -> String {
    for _ in 0..100 {
        let candidate: String = random_uuid().chars().take(8).collect();
        if !by_id.contains_key(&candidate) {
            return candidate;
        }
    }
    random_uuid()
}

fn generate_id_for_ids(ids: &HashMap<String, ()>) -> String {
    for _ in 0..100 {
        let candidate: String = random_uuid().chars().take(8).collect();
        if !ids.contains_key(&candidate) {
            return candidate;
        }
    }
    random_uuid()
}

/// Validate a caller-provided session id.
pub fn assert_valid_session_id(id: &str) -> Result<(), SessionManagerError> {
    let bytes = id.as_bytes();
    let valid = !bytes.is_empty()
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(error(
            "Session id must be non-empty, contain only alphanumeric characters, '-', '_', and '.', and start and end with an alphanumeric character",
        ))
    }
}

fn migrate_v1_to_v2(entries: &mut [FileEntry]) {
    let mut ids: HashMap<String, ()> = HashMap::new();
    let mut previous_id: Option<String> = None;

    for entry in entries.iter_mut() {
        match entry {
            FileEntry::Header(header) => {
                header.version = Some(2.0);
            }
            FileEntry::Entry(entry) => {
                let id = generate_id_for_ids(&ids);
                ids.insert(id.clone(), ());
                entry.set_id_parent(id.clone(), previous_id.clone());
                previous_id = Some(id);
            }
        }
    }

    let entry_ids: Vec<Option<String>> = entries
        .iter()
        .map(|entry| match entry {
            FileEntry::Header(_) => None,
            FileEntry::Entry(entry) => entry.id().map(str::to_owned),
        })
        .collect();
    for entry in entries.iter_mut() {
        let FileEntry::Entry(entry) = entry else {
            continue;
        };
        match entry {
            SessionEntry::Known(KnownSessionEntry::Compaction(compaction)) => {
                if let Some(index) = compaction.first_kept_entry_index {
                    if let Some(Some(id)) = entry_ids.get(index as usize) {
                        compaction.first_kept_entry_id = id.clone();
                    }
                    compaction.first_kept_entry_index = None;
                }
            }
            SessionEntry::Unknown(object) => {
                if let Some(index) = object.get("firstKeptEntryIndex").and_then(Value::as_f64) {
                    if let Some(Some(id)) = entry_ids.get(index as usize) {
                        object.insert("firstKeptEntryId".to_owned(), Value::String(id.clone()));
                    }
                    object.shift_remove("firstKeptEntryIndex");
                }
            }
            _ => {}
        }
    }
}

fn migrate_v2_to_v3(entries: &mut [FileEntry]) {
    for entry in entries.iter_mut() {
        match entry {
            FileEntry::Header(header) => header.version = Some(3.0),
            FileEntry::Entry(entry) => entry.set_hook_role_to_custom(),
        }
    }
}

/// Run all migrations to the current version. Returns whether anything changed.
pub fn migrate_to_current_version(entries: &mut [FileEntry]) -> bool {
    let version = entries
        .iter()
        .find_map(|entry| match entry {
            FileEntry::Header(header) => Some(header.version.unwrap_or(1.0)),
            FileEntry::Entry(_) => None,
        })
        .unwrap_or(1.0);

    if version >= CURRENT_SESSION_VERSION {
        return false;
    }
    if version < 2.0 {
        migrate_v1_to_v2(entries);
    }
    if version < 3.0 {
        migrate_v2_to_v3(entries);
    }
    true
}

/// Migrate session entries in place (exported for tests).
pub fn migrate_session_entries(entries: &mut [FileEntry]) {
    migrate_to_current_version(entries);
}

/// Parse JSONL content, skipping malformed lines.
pub fn parse_session_entries(content: &str) -> Vec<FileEntry> {
    content
        .trim()
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(parse_file_entry_value)
        .collect()
}

fn parse_file_entry_value(value: Value) -> FileEntry {
    let is_header = value.get("type").and_then(Value::as_str) == Some("session");
    if is_header && let Ok(header) = serde_json::from_value::<SessionHeader>(value.clone()) {
        return FileEntry::Header(header);
    }
    FileEntry::Entry(SessionEntry::from_value(value))
}

/// Latest compaction entry in the list.
pub fn get_latest_compaction_entry(entries: &[SessionEntry]) -> Option<&SessionEntry> {
    entries.iter().rev().find(|entry| entry.entry_type() == "compaction")
}

fn build_entry_index(entries: &[SessionEntry]) -> HashMap<String, usize> {
    let mut index = HashMap::new();
    for (position, entry) in entries.iter().enumerate() {
        if let Some(id) = entry.id() {
            index.insert(id.to_owned(), position);
        }
    }
    index
}

fn build_session_path<'a>(entries: &'a [SessionEntry], leaf: LeafSelection<'_>) -> Vec<&'a SessionEntry> {
    if leaf == LeafSelection::None {
        return Vec::new();
    }
    let index = build_entry_index(entries);
    let leaf_entry = match leaf {
        LeafSelection::Id(id) => index.get(id).and_then(|position| entries.get(*position)),
        _ => None,
    }
    .or_else(|| entries.last());
    let Some(mut current) = leaf_entry else {
        return Vec::new();
    };

    let mut path = Vec::new();
    let mut seen = HashMap::new();
    loop {
        path.push(current);
        let Some(parent_id) = current.parent_id() else {
            break;
        };
        let Some(position) = index.get(parent_id) else {
            break;
        };
        let Some(parent) = entries.get(*position) else {
            break;
        };
        if seen.insert(parent_id.to_owned(), ()).is_some() {
            break;
        }
        current = parent;
    }
    path.reverse();
    path
}

fn context_settings(path: &[&SessionEntry]) -> (String, Option<SessionModel>) {
    let mut thinking_level = "off".to_owned();
    let mut model = None;
    for entry in path {
        match entry {
            SessionEntry::Known(KnownSessionEntry::ThinkingLevelChange(change)) => {
                thinking_level = change.thinking_level.clone();
            }
            SessionEntry::Known(KnownSessionEntry::ModelChange(change)) => {
                model = Some(SessionModel {
                    provider: change.provider.clone(),
                    model_id: change.model_id.clone(),
                });
            }
            SessionEntry::Known(KnownSessionEntry::Message(message)) => {
                if let AgentMessage::Assistant(assistant) = message.message.as_ref() {
                    model = Some(SessionModel {
                        provider: assistant.provider.clone(),
                        model_id: assistant.model.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    (thinking_level, model)
}

fn message_from_context_entry(entry: &SessionEntry) -> Option<AgentMessage> {
    match entry {
        SessionEntry::Known(KnownSessionEntry::Message(message)) => Some(message.message.as_ref().clone()),
        SessionEntry::Known(KnownSessionEntry::CustomMessage(message)) => {
            let timestamp = pi_js_compat::date::date_parse_millis(&message.timestamp).unwrap_or(0);
            Some(create_custom_message(
                &message.custom_type,
                message.content.clone(),
                message.display,
                message.details.clone(),
                timestamp,
            ))
        }
        SessionEntry::Known(KnownSessionEntry::BranchSummary(summary)) if !summary.summary.is_empty() => {
            let timestamp = pi_js_compat::date::date_parse_millis(&summary.timestamp).unwrap_or(0);
            Some(create_branch_summary_message(
                &summary.summary,
                &summary.from_id,
                timestamp,
            ))
        }
        SessionEntry::Known(KnownSessionEntry::Compaction(compaction)) => {
            let timestamp = pi_js_compat::date::date_parse_millis(&compaction.timestamp).unwrap_or(0);
            Some(create_compaction_summary_message(
                &compaction.summary,
                compaction.tokens_before,
                timestamp,
                &CompactionSummaryOptions {
                    duration_ms: compaction.duration_ms,
                    provider_native: compaction.provider_native.as_deref().cloned(),
                    ..CompactionSummaryOptions::default()
                },
            ))
        }
        _ => None,
    }
}

fn requires_content(role: &str) -> bool {
    matches!(role, "user" | "assistant" | "toolResult" | "custom")
}

/// Project one session entry into LLM/runtime messages.
pub fn session_entry_to_context_messages(entry: &SessionEntry) -> Vec<AgentMessage> {
    let Some(message) = message_from_context_entry(entry) else {
        return Vec::new();
    };
    if requires_content(message.role())
        && let AgentMessage::Custom(object) = &message
        && object.get("content").is_none_or(Value::is_null)
    {
        let mut object = object.clone();
        object.insert("content".to_owned(), Value::Array(Vec::new()));
        return vec![AgentMessage::Custom(object)];
    }
    vec![message]
}

fn json_bytes(messages: &[AgentMessage]) -> usize {
    serde_json::to_vec(messages).map(|bytes| bytes.len()).unwrap_or(0)
}

fn first_text_block(content: &[TextOrImageContent]) -> Option<&str> {
    content.iter().find_map(|block| match block {
        TextOrImageContent::Text(text) => Some(text.text.as_str()),
        TextOrImageContent::Image(_) => None,
    })
}

fn get_message_text(message: &AgentMessage) -> Option<String> {
    match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => Some(text.clone()),
            UserContent::Blocks(blocks) => first_text_block(blocks).map(str::to_owned),
        },
        AgentMessage::Assistant(assistant) => assistant.content.iter().find_map(|block| match block {
            pi_ai::types::AssistantContent::Text(text) => Some(text.text.clone()),
            _ => None,
        }),
        AgentMessage::ToolResult(result) => first_text_block(&result.content).map(str::to_owned),
        AgentMessage::Custom(object) => custom_message_text(object),
    }
}

fn custom_message_text(object: &Map<String, Value>) -> Option<String> {
    match object.get("content")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => items.iter().find_map(|item| {
            let object = item.as_object()?;
            if object.get("type").and_then(Value::as_str) != Some("text") {
                return None;
            }
            object.get("text").and_then(Value::as_str).map(str::to_owned)
        }),
        _ => None,
    }
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `text.slice(0, length)` for a UTF-16 length, restricted to whole Rust chars.
fn utf16_prefix(text: &str, length: usize) -> String {
    let mut units = 0;
    let mut end = 0;
    for (index, character) in text.char_indices() {
        let character_units = character.len_utf16();
        if units + character_units > length {
            break;
        }
        units += character_units;
        end = index + character.len_utf8();
    }
    text[..end].to_owned()
}

fn clone_message_with_text(message: &AgentMessage, text: &str) -> Option<AgentMessage> {
    match message {
        AgentMessage::User(user) => {
            let mut clone = user.clone();
            match &mut clone.content {
                UserContent::Text(content) => *content = text.to_owned(),
                UserContent::Blocks(blocks) => {
                    let block = blocks.iter_mut().find_map(|block| match block {
                        TextOrImageContent::Text(text) => Some(text),
                        TextOrImageContent::Image(_) => None,
                    })?;
                    block.text = text.to_owned();
                }
            }
            Some(AgentMessage::User(clone))
        }
        AgentMessage::Assistant(assistant) => {
            let mut clone = assistant.clone();
            let block = clone.content.iter_mut().find_map(|block| match block {
                pi_ai::types::AssistantContent::Text(text) => Some(text),
                _ => None,
            })?;
            block.text = text.to_owned();
            Some(AgentMessage::Assistant(clone))
        }
        AgentMessage::ToolResult(result) => {
            let mut clone = result.clone();
            let block = clone.content.iter_mut().find_map(|block| match block {
                TextOrImageContent::Text(text) => Some(text),
                TextOrImageContent::Image(_) => None,
            })?;
            block.text = text.to_owned();
            Some(AgentMessage::ToolResult(clone))
        }
        AgentMessage::Custom(object) => {
            let mut clone = object.clone();
            let content = clone.get_mut("content")?;
            match content {
                Value::String(value) => *value = text.to_owned(),
                Value::Array(items) => {
                    let item = items.iter_mut().find_map(|item| {
                        let object = item.as_object_mut()?;
                        (object.get("type").and_then(Value::as_str) == Some("text")).then_some(object)
                    })?;
                    item.insert("text".to_owned(), Value::String(text.to_owned()));
                }
                _ => return None,
            }
            Some(AgentMessage::Custom(clone))
        }
    }
}

fn truncate_message_to_fit(message: &AgentMessage, newer: &[AgentMessage]) -> Option<AgentMessage> {
    let original_text = get_message_text(message)?;
    let mut best = None;
    let mut low = 0;
    let mut high = utf16_len(&original_text);
    while low <= high {
        let middle = (low + high) / 2;
        let candidate = clone_message_with_text(message, &utf16_prefix(&original_text, middle));
        let fits = candidate.as_ref().is_some_and(|candidate| {
            let mut messages = Vec::with_capacity(newer.len() + 1);
            messages.push(candidate.clone());
            messages.extend_from_slice(newer);
            json_bytes(&messages) <= MAX_KEPT_PRE_COMPACTION_BYTES
        });
        if fits {
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

fn remove_leading_tool_results(messages: Vec<AgentMessage>) -> Vec<AgentMessage> {
    let Some(index) = messages.iter().position(|message| message.role() != "toolResult") else {
        return Vec::new();
    };
    messages.into_iter().skip(index).collect()
}

fn build_capped_kept_messages(entries: &[&SessionEntry]) -> Vec<AgentMessage> {
    let mut kept: Vec<AgentMessage> = Vec::new();
    for entry in entries.iter().rev() {
        let Some(message) = message_from_context_entry(entry) else {
            continue;
        };
        let mut candidate = Vec::with_capacity(kept.len() + 1);
        candidate.push(message.clone());
        candidate.extend(kept.iter().cloned());
        if json_bytes(&candidate) <= MAX_KEPT_PRE_COMPACTION_BYTES {
            kept.insert(0, message);
            continue;
        }
        if let Some(truncated) = truncate_message_to_fit(&message, &kept) {
            kept.insert(0, truncated);
        }
        break;
    }
    remove_leading_tool_results(kept)
}

fn collect_kept_entries<'a>(
    path: &[&'a SessionEntry],
    compaction: &CompactionEntry,
    compaction_index: usize,
) -> Vec<&'a SessionEntry> {
    let mut kept = Vec::new();
    let mut found_first = false;
    for entry in path.iter().take(compaction_index) {
        if entry.id() == Some(compaction.first_kept_entry_id.as_str()) {
            found_first = true;
        }
        if found_first {
            kept.push(*entry);
        }
    }
    kept
}

fn build_compacted_context_messages(path: &[&SessionEntry], compaction: &SessionEntry) -> Vec<AgentMessage> {
    let mut messages = session_entry_to_context_messages(compaction);
    let compaction_index = path.iter().position(|entry| entry.id() == compaction.id()).unwrap_or(0);
    if let SessionEntry::Known(KnownSessionEntry::Compaction(data)) = compaction {
        let kept = collect_kept_entries(path, data, compaction_index);
        messages.extend(build_capped_kept_messages(&kept));
    }
    for entry in path.iter().skip(compaction_index + 1) {
        messages.extend(session_entry_to_context_messages(entry));
    }
    messages
}

/// Build the active, compaction-aware session entry list.
pub fn build_context_entries(entries: &[SessionEntry], leaf: LeafSelection<'_>) -> Vec<SessionEntry> {
    let path = build_session_path(entries, leaf);
    let compaction = path.iter().rev().find(|entry| entry.entry_type() == "compaction");
    let Some(compaction) = compaction else {
        return path.into_iter().cloned().collect();
    };
    let Some(compaction_index) = path.iter().position(|entry| entry.id() == compaction.id()) else {
        return path.into_iter().cloned().collect();
    };

    let mut context_entries = vec![(*compaction).clone()];
    let first_kept = compaction.as_compaction().map(|data| data.first_kept_entry_id.as_str());
    let mut found_first = false;
    for entry in path.iter().take(compaction_index) {
        if Some(entry.id().unwrap_or_default()) == first_kept {
            found_first = true;
        }
        if found_first {
            context_entries.push((*entry).clone());
        }
    }
    context_entries.extend(path.iter().skip(compaction_index + 1).map(|entry| (*entry).clone()));
    context_entries
}

/// Build the resolved LLM context for a session.
pub fn build_session_context(entries: &[SessionEntry], leaf: LeafSelection<'_>) -> SessionContext {
    let path = build_session_path(entries, leaf);
    let (thinking_level, model) = context_settings(&path);
    let compaction = path
        .iter()
        .rev()
        .find(|entry| entry.entry_type() == "compaction")
        .copied();
    let messages = match compaction {
        Some(compaction) => build_compacted_context_messages(&path, compaction),
        None => path
            .iter()
            .flat_map(|entry| session_entry_to_context_messages(entry))
            .collect(),
    };
    SessionContext {
        messages,
        thinking_level,
        model,
    }
}

/// Default session directory for a cwd.
pub fn get_default_session_dir_path(cwd: &str, agent_dir: &str) -> Result<String, SessionManagerError> {
    let resolved_cwd = resolve(cwd)?;
    let resolved_agent_dir = resolve(agent_dir)?;
    let trimmed = resolved_cwd
        .strip_prefix('/')
        .or_else(|| resolved_cwd.strip_prefix('\\'))
        .unwrap_or(&resolved_cwd);
    let safe_path = format!("--{}--", trimmed.replace(['/', '\\', ':'], "-"));
    Ok(PathBuf::from(resolved_agent_dir)
        .join("sessions")
        .join(safe_path)
        .display()
        .to_string())
}

/// Default session directory for a cwd, created when missing.
pub fn get_default_session_dir(cwd: &str, agent_dir: Option<&str>) -> Result<String, SessionManagerError> {
    let agent_dir = agent_dir
        .map(str::to_owned)
        .unwrap_or_else(|| get_agent_dir().display().to_string());
    let session_dir = get_default_session_dir_path(cwd, &agent_dir)?;
    if !Path::new(&session_dir).exists() {
        fs::create_dir_all(&session_dir).map_err(|error| SessionManagerError::new(error.to_string()))?;
    }
    Ok(session_dir)
}

fn parse_session_entry_line(line: &str) -> Option<FileEntry> {
    if line.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<Value>(line).ok().map(parse_file_entry_value)
}

fn decode_utf8_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Load every entry from a session file, skipping malformed lines.
pub fn load_entries_from_file(file_path: &str) -> Vec<FileEntry> {
    let Ok(resolved_path) = normalize(file_path) else {
        return Vec::new();
    };
    let Ok(bytes) = fs::read(&resolved_path) else {
        return Vec::new();
    };

    let content = decode_utf8_lossy(&bytes);
    let entries: Vec<FileEntry> = content.split('\n').filter_map(parse_session_entry_line).collect();

    let Some(first) = entries.first() else {
        return entries;
    };
    match first {
        FileEntry::Header(header) if !header.id.is_empty() => entries,
        _ => Vec::new(),
    }
}

fn read_first_line(file_path: &str) -> Option<String> {
    let mut file = File::open(file_path).ok()?;
    let mut chunks: Vec<u8> = Vec::new();
    let mut buffer = [0_u8; HEADER_READ_BUFFER_SIZE];
    let mut position = 0_u64;
    loop {
        let bytes_read = file.read(&mut buffer).ok()?;
        if bytes_read == 0 {
            break;
        }
        let bytes = &buffer[..bytes_read];
        match bytes.iter().position(|byte| *byte == b'\n') {
            Some(index) => {
                chunks.extend_from_slice(&bytes[..index]);
                break;
            }
            None => {
                chunks.extend_from_slice(bytes);
                position += bytes_read as u64;
                let _ = file.seek(SeekFrom::Start(position));
            }
        }
    }
    if chunks.is_empty() {
        return None;
    }
    Some(decode_utf8_lossy(&chunks))
}

/// Read only the session header.
pub fn read_session_header(file_path: &str) -> Option<SessionHeader> {
    let first_line = read_first_line(file_path)?;
    let value: Value = serde_json::from_str(&first_line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session") {
        return None;
    }
    value.get("id").and_then(Value::as_str)?;
    let mut header: SessionHeader = serde_json::from_value(value.clone()).ok()?;
    header.original_key_order = value.as_object().map(|object| object.keys().cloned().collect());
    Some(header)
}

fn session_cwd_matches(cwd: Option<&str>, resolved_cwd: &str) -> bool {
    cwd.is_some_and(|cwd| !cwd.is_empty() && resolve(cwd).is_ok_and(|resolved| resolved == resolved_cwd))
}

/// Find the most recently modified valid session in a directory.
pub fn find_most_recent_session(session_dir: &str, cwd: Option<&str>) -> Option<String> {
    let resolved_session_dir = normalize(session_dir).ok()?;
    let resolved_cwd = match cwd {
        Some(cwd) => Some(resolve(cwd).ok()?),
        None => None,
    };
    let entries = fs::read_dir(&resolved_session_dir).ok()?;
    let mut files: Vec<(String, f64)> = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.ends_with(".jsonl") {
            continue;
        }
        let path = PathBuf::from(&resolved_session_dir)
            .join(&file_name)
            .display()
            .to_string();
        let Some(header) = read_session_header(&path) else {
            continue;
        };
        if let Some(resolved_cwd) = resolved_cwd.as_deref()
            && !session_cwd_matches(header.cwd.as_deref(), resolved_cwd)
        {
            continue;
        }
        let mtime = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        files.push((path, mtime));
    }
    files.sort_by(|left, right| right.1.total_cmp(&left.1));
    files.into_iter().next().map(|(path, _)| path)
}

// =============================================================================
// Reverse scan for current-version sessions
// =============================================================================

struct ReverseSessionScan {
    all_entries: Vec<SessionEntry>,
    active_path: Vec<SessionEntry>,
    is_reading_leaf: bool,
    required_parent_id: Option<String>,
    latest_compaction: Option<(String, String)>,
    found_first_kept_entry: bool,
    preceding_cwd: Option<String>,
    allow_incomplete_trailing_entry: bool,
}

struct LoadedSessionEntries {
    entries: Vec<SessionEntry>,
    preceding_cwd: Option<String>,
}

struct LoadedCurrentVersionSession {
    file_entries: Vec<FileEntry>,
    preceding_cwd: Option<String>,
}

impl ReverseSessionScan {
    fn new() -> Self {
        Self {
            all_entries: Vec::new(),
            active_path: Vec::new(),
            is_reading_leaf: true,
            required_parent_id: None,
            latest_compaction: None,
            found_first_kept_entry: false,
            preceding_cwd: None,
            allow_incomplete_trailing_entry: true,
        }
    }

    fn loaded_active_slice(&mut self) -> LoadedSessionEntries {
        let mut entries = std::mem::take(&mut self.active_path);
        entries.reverse();
        LoadedSessionEntries {
            entries,
            preceding_cwd: self.preceding_cwd.clone(),
        }
    }
}

fn parse_reverse_session_entry(
    scan: &mut ReverseSessionScan,
    line: &str,
    file_path: &str,
) -> Result<Option<SessionEntry>, SessionManagerError> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Value>(line) {
        Ok(value) => {
            scan.allow_incomplete_trailing_entry = false;
            if value.get("type").and_then(Value::as_str) == Some("session") {
                return Ok(None);
            }
            Ok(Some(SessionEntry::from_value(value)))
        }
        Err(_) if scan.allow_incomplete_trailing_entry => {
            scan.allow_incomplete_trailing_entry = false;
            Ok(None)
        }
        Err(_) => Err(error(format!(
            "Session file contains malformed JSONL entry: {file_path}"
        ))),
    }
}

fn cwd_from_changed_entry(entry: &SessionEntry) -> Option<String> {
    let custom = entry.as_custom_message()?;
    if custom.custom_type != "cwd_changed" {
        return None;
    }
    let details = custom.details.as_ref()?.as_object()?;
    details.get("cwd").and_then(Value::as_str).map(str::to_owned)
}

fn scan_session_entry_reverse(scan: &mut ReverseSessionScan, entry: SessionEntry) -> bool {
    let entry_id = entry.id().unwrap_or_default().to_owned();
    let parent_id = entry.parent_id().map(str::to_owned);
    scan.all_entries.push(entry.clone());

    let is_active_entry = scan.is_reading_leaf || scan.required_parent_id.as_deref() == Some(entry_id.as_str());
    if !is_active_entry {
        return false;
    }

    scan.is_reading_leaf = false;
    if !scan.found_first_kept_entry {
        scan.active_path.push(entry.clone());
    }
    scan.required_parent_id = parent_id;
    if scan.latest_compaction.is_none()
        && entry.entry_type() == "compaction"
        && let Some(compaction) = entry.as_compaction()
    {
        scan.latest_compaction = Some((compaction.id.clone(), compaction.first_kept_entry_id.clone()));
    }
    if !scan.found_first_kept_entry
        && scan
            .latest_compaction
            .as_ref()
            .is_some_and(|(_, first_kept)| first_kept == &entry_id)
    {
        scan.found_first_kept_entry = true;
        return false;
    }
    if !scan.found_first_kept_entry {
        return false;
    }

    scan.preceding_cwd = cwd_from_changed_entry(&entry);
    scan.preceding_cwd.is_some()
}

fn scan_complete_reverse_lines(
    scan: &mut ReverseSessionScan,
    pending: &[u8],
    file_path: &str,
) -> Result<(Vec<u8>, Option<LoadedSessionEntries>), SessionManagerError> {
    let mut line_end = pending.len();
    while let Some(newline_index) = pending[..line_end].iter().rposition(|byte| *byte == b'\n') {
        let line = decode_utf8_lossy(&pending[newline_index + 1..line_end]);
        if let Some(entry) = parse_reverse_session_entry(scan, &line, file_path)?
            && scan_session_entry_reverse(scan, entry)
        {
            return Ok((Vec::new(), Some(scan.loaded_active_slice())));
        }
        line_end = newline_index;
        if line_end == 0 {
            break;
        }
    }
    Ok((pending[..line_end].to_vec(), None))
}

fn validate_complete_reverse_scan(scan: &ReverseSessionScan, file_path: &str) -> Result<(), SessionManagerError> {
    let has_unresolved_active_parent = !scan.active_path.is_empty() && scan.required_parent_id.is_some();
    if has_unresolved_active_parent && !scan.found_first_kept_entry {
        return Err(error(format!(
            "Session file contains a broken active parent chain: {file_path}"
        )));
    }
    if let Some((id, first_kept)) = &scan.latest_compaction
        && !scan.found_first_kept_entry
    {
        return Err(error(format!(
            "Session compaction {id} references missing firstKeptEntryId {first_kept}: {file_path}"
        )));
    }
    Ok(())
}

fn read_current_version_session_entries(
    file: &mut File,
    file_size: u64,
    file_path: &str,
) -> Result<LoadedSessionEntries, SessionManagerError> {
    let mut scan = ReverseSessionScan::new();
    let mut position = file_size;
    let mut pending: Vec<u8> = Vec::new();

    while position > 0 {
        let bytes_to_read = (SESSION_READ_BUFFER_SIZE as u64).min(position) as usize;
        position -= bytes_to_read as u64;
        file.seek(SeekFrom::Start(position))
            .map_err(|error| SessionManagerError::new(error.to_string()))?;
        let mut buffer = vec![0_u8; bytes_to_read];
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|error| SessionManagerError::new(error.to_string()))?;
        buffer.truncate(bytes_read);
        buffer.extend_from_slice(&pending);
        pending = buffer;

        let (remaining, loaded) = scan_complete_reverse_lines(&mut scan, &pending, file_path)?;
        if let Some(loaded) = loaded {
            return Ok(loaded);
        }
        pending = remaining;
    }

    let first_line = decode_utf8_lossy(&pending);
    if let Some(entry) = parse_reverse_session_entry(&mut scan, &first_line, file_path)?
        && scan_session_entry_reverse(&mut scan, entry)
    {
        return Ok(scan.loaded_active_slice());
    }
    validate_complete_reverse_scan(&scan, file_path)?;
    if scan.found_first_kept_entry {
        return Ok(scan.loaded_active_slice());
    }
    let mut entries = std::mem::take(&mut scan.all_entries);
    entries.reverse();
    Ok(LoadedSessionEntries {
        entries,
        preceding_cwd: None,
    })
}

fn load_current_version_entries_from_file(
    file_path: &str,
    header: SessionHeader,
) -> Result<LoadedCurrentVersionSession, SessionManagerError> {
    let mut file = File::open(file_path).map_err(|error| SessionManagerError::new(error.to_string()))?;
    let file_size = file
        .metadata()
        .map_err(|error| SessionManagerError::new(error.to_string()))?
        .len();
    let loaded = read_current_version_session_entries(&mut file, file_size, file_path)?;
    let mut file_entries = vec![FileEntry::Header(header)];
    file_entries.extend(loaded.entries.into_iter().map(FileEntry::Entry));
    Ok(LoadedCurrentVersionSession {
        file_entries,
        preceding_cwd: loaded.preceding_cwd,
    })
}

fn load_session_file_entries(file_path: &str) -> Result<LoadedCurrentVersionSession, SessionManagerError> {
    let header = read_session_header(file_path);
    if header.as_ref().and_then(|header| header.version) == Some(CURRENT_SESSION_VERSION)
        && let Some(header) = header
    {
        return load_current_version_entries_from_file(file_path, header);
    }
    Ok(LoadedCurrentVersionSession {
        file_entries: load_entries_from_file(file_path),
        preceding_cwd: None,
    })
}

// =============================================================================
// Session info
// =============================================================================

#[derive(Default)]
struct SessionInfoAccumulator {
    message_count: usize,
    first_message: String,
    all_messages_text: String,
    name: Option<String>,
    last_activity_time: Option<i64>,
}

fn message_input_source(message: &AgentMessage) -> Option<&str> {
    match message {
        AgentMessage::User(user) => user.input_source.map(|source| match source {
            pi_ai::types::InputSource::Interactive => "interactive",
            pi_ai::types::InputSource::Rpc => "rpc",
            pi_ai::types::InputSource::Extension => "extension",
        }),
        AgentMessage::Custom(object) => object.get("inputSource").and_then(Value::as_str),
        _ => None,
    }
}

fn extract_text_content(message: &AgentMessage) -> String {
    match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => text_blocks_joined(blocks),
        },
        AgentMessage::Assistant(assistant) => assistant
            .content
            .iter()
            .filter_map(|block| match block {
                pi_ai::types::AssistantContent::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
        AgentMessage::ToolResult(result) => text_blocks_joined(&result.content),
        AgentMessage::Custom(object) => match object.get("content") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|item| {
                    let object = item.as_object()?;
                    (object.get("type").and_then(Value::as_str) == Some("text"))
                        .then(|| object.get("text").and_then(Value::as_str))
                        .flatten()
                })
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        },
    }
}

fn text_blocks_joined(blocks: &[TextOrImageContent]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            TextOrImageContent::Text(text) => Some(text.text.as_str()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn message_has_content(message: &AgentMessage) -> bool {
    match message {
        AgentMessage::User(_) | AgentMessage::Assistant(_) | AgentMessage::ToolResult(_) => true,
        AgentMessage::Custom(object) => object.contains_key("content"),
    }
}

fn message_activity_time(entry: &SessionMessageEntry) -> Option<i64> {
    if !message_has_content(&entry.message) {
        return None;
    }
    let role = entry.message.role();
    if role != "user" && role != "assistant" {
        return None;
    }
    if let Some(timestamp) = entry.message.timestamp() {
        return Some(timestamp);
    }
    if let AgentMessage::Custom(object) = entry.message.as_ref()
        && let Some(timestamp) = object.get("timestamp").and_then(Value::as_f64)
    {
        return Some(timestamp as i64);
    }
    pi_js_compat::date::date_parse_millis(&entry.timestamp)
}

fn apply_session_info_entry(accumulator: &mut SessionInfoAccumulator, entry: &SessionEntry) {
    if let Some(info) = entry.as_session_info() {
        accumulator.name = info
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
    }
    let Some(message_entry) = entry.as_message() else {
        return;
    };
    accumulator.message_count += 1;

    if let Some(activity_time) = message_activity_time(message_entry) {
        accumulator.last_activity_time = Some(accumulator.last_activity_time.unwrap_or(0).max(activity_time));
    }

    let role = message_entry.message.role();
    if role != "user" && role != "assistant" {
        return;
    }
    let text = extract_text_content(&message_entry.message);
    if text.is_empty() {
        return;
    }
    accumulator.all_messages_text = if accumulator.all_messages_text.is_empty() {
        text.clone()
    } else {
        format!("{} {}", accumulator.all_messages_text, text)
    };
    let is_user_prompt = role == "user" && message_input_source(&message_entry.message) != Some("extension");
    if accumulator.first_message.is_empty() && is_user_prompt {
        accumulator.first_message = text;
    }
}

fn finish_session_info(
    file_path: &str,
    header: &SessionHeader,
    accumulator: SessionInfoAccumulator,
    fallback_modified: i64,
) -> SessionInfo {
    let cwd = header.cwd.clone().unwrap_or_default();
    let header_time = pi_js_compat::date::date_parse_millis(&header.timestamp);
    let modified = match accumulator.last_activity_time {
        Some(time) if time > 0 => time,
        _ => header_time.unwrap_or(fallback_modified),
    };
    SessionInfo {
        path: file_path.to_owned(),
        id: header.id.clone(),
        cwd,
        name: accumulator.name,
        is_archived: None,
        parent_session_path: header.parent_session.clone(),
        created: header_time.unwrap_or(0),
        modified,
        message_count: accumulator.message_count,
        first_message: if accumulator.first_message.is_empty() {
            "(no messages)".to_owned()
        } else {
            accumulator.first_message
        },
        all_messages_text: accumulator.all_messages_text,
    }
}

fn build_session_info(file_path: &str) -> Option<SessionInfo> {
    let metadata = fs::metadata(file_path).ok()?;
    let fallback_modified = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0);
    let content = fs::read(file_path).ok()?;
    let content = decode_utf8_lossy(&content);
    let mut header: Option<SessionHeader> = None;
    let mut accumulator = SessionInfoAccumulator::default();
    for line in content.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let Some(entry) = parse_session_entry_line(line) else {
            continue;
        };
        match entry {
            FileEntry::Header(found) => {
                if header.is_none() {
                    header = Some(found);
                    continue;
                }
                // A later `session` line is not a SessionEntry in the source.
            }
            FileEntry::Entry(entry) => {
                header.as_ref()?;
                apply_session_info_entry(&mut accumulator, &entry);
            }
        }
    }
    let header = header?;
    Some(finish_session_info(file_path, &header, accumulator, fallback_modified))
}

fn list_sessions_from_dir(
    dir: &str,
    progress: Option<&mut dyn FnMut(usize, usize)>,
    progress_offset: usize,
    progress_total: Option<usize>,
) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return sessions;
    };
    let mut files: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(".jsonl")
                .then(|| PathBuf::from(dir).join(name).display().to_string())
        })
        .collect();
    files.sort();
    let total = progress_total.unwrap_or(files.len());
    let mut loaded = 0;
    let mut progress = progress;
    for file in files {
        loaded += 1;
        if let Some(progress) = progress.as_deref_mut() {
            progress(progress_offset + loaded, total);
        }
        if let Some(info) = build_session_info(&file) {
            sessions.push(info);
        }
    }
    sessions
}

fn list_default_session_root(progress: Option<&mut dyn FnMut(usize, usize)>) -> Vec<SessionInfo> {
    let sessions_dir = get_sessions_dir();
    let Ok(entries) = fs::read_dir(&sessions_dir) else {
        return Vec::new();
    };
    let mut files: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let dir = entry.path();
        let Ok(dir_entries) = fs::read_dir(&dir) else {
            continue;
        };
        for file in dir_entries.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if name.ends_with(".jsonl") {
                files.push(dir.join(name).display().to_string());
            }
        }
    }
    files.sort();
    let total = files.len();
    let mut loaded = 0;
    let mut sessions = Vec::new();
    let mut progress = progress;
    for file in files {
        loaded += 1;
        if let Some(progress) = progress.as_deref_mut() {
            progress(loaded, total);
        }
        if let Some(info) = build_session_info(&file) {
            sessions.push(info);
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.modified));
    sessions
}

fn load_fork_source_entries(source_path: &str) -> Result<Vec<FileEntry>, SessionManagerError> {
    let source_header = read_session_header(source_path);
    let entries = match source_header.as_ref().and_then(|header| header.version) {
        Some(version) if version == CURRENT_SESSION_VERSION => {
            let header = source_header.clone().expect("header was checked");
            load_current_version_entries_from_file(source_path, header)?.file_entries
        }
        _ => load_entries_from_file(source_path),
    };
    if entries.is_empty() {
        return Err(error(format!(
            "Cannot fork: source session file is empty or invalid: {source_path}"
        )));
    }
    if !entries.iter().any(|entry| matches!(entry, FileEntry::Header(_))) {
        return Err(error(format!(
            "Cannot fork: source session has no header: {source_path}"
        )));
    }
    Ok(entries)
}

fn fork_session_file_path(dir: &str, timestamp: &str, session_id: &str) -> String {
    let file_timestamp = timestamp.replace([':', '.'], "-");
    PathBuf::from(dir)
        .join(format!("{file_timestamp}_{session_id}.jsonl"))
        .display()
        .to_string()
}

fn write_new_session_header(path: &str, header: &SessionHeader) -> Result<(), SessionManagerError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| SessionManagerError::new(error.to_string()))?;
    let line = serde_json::to_string(header).map_err(|error| SessionManagerError::new(error.to_string()))?;
    writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))
}

fn write_fork_entries(
    path: &str,
    source_entries: Vec<FileEntry>,
    selection: Option<ForkEntrySelection>,
) -> Result<(), SessionManagerError> {
    let session_entries: Vec<SessionEntry> = source_entries
        .into_iter()
        .filter_map(|entry| match entry {
            FileEntry::Header(_) => None,
            FileEntry::Entry(entry) => Some(entry),
        })
        .collect();
    let entries_to_copy = select_fork_entries(session_entries, selection);
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| SessionManagerError::new(error.to_string()))?;
    for (index, entry) in entries_to_copy.into_iter().enumerate() {
        let mut entry = entry;
        if index == 0 {
            entry.set_id_parent(entry.id().unwrap_or_default().to_owned(), None);
        }
        let line = serialize_session_entry_for_persistence(&FileEntry::Entry(entry));
        writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))?;
    }
    Ok(())
}

fn select_fork_entries(entries: Vec<SessionEntry>, selection: Option<ForkEntrySelection>) -> Vec<SessionEntry> {
    match selection {
        Some(ForkEntrySelection::None) => Vec::new(),
        Some(ForkEntrySelection::From(id)) => build_session_path(&entries, LeafSelection::Id(&id))
            .into_iter()
            .cloned()
            .collect(),
        Some(ForkEntrySelection::All) | None => entries,
    }
}

fn build_session_tree(
    entries: &[SessionEntry],
    labels: &[(String, String)],
    label_timestamps: &[(String, String)],
) -> Vec<SessionTreeNode> {
    let mut nodes: Vec<Option<SessionTreeNode>> = entries
        .iter()
        .map(|entry| {
            let id = entry.id().unwrap_or_default();
            Some(SessionTreeNode {
                entry: entry.clone(),
                children: Vec::new(),
                label: lookup_label(labels, id).map(str::to_owned),
                label_timestamp: lookup_label(label_timestamps, id).map(str::to_owned),
            })
        })
        .collect();
    let (child_positions, root_positions) = link_tree_positions(entries);
    attach_children_in_bottom_up_order(&mut nodes, &child_positions, &root_positions);

    root_positions
        .into_iter()
        .filter_map(|position| nodes[position].take())
        .collect()
}

fn lookup_label<'a>(labels: &'a [(String, String)], id: &str) -> Option<&'a str> {
    labels
        .iter()
        .find(|(target, _)| target == id)
        .map(|(_, value)| value.as_str())
}

fn link_tree_positions(entries: &[SessionEntry]) -> (Vec<Vec<usize>>, Vec<usize>) {
    let index: HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(position, entry)| entry.id().map(|id| (id, position)))
        .collect();
    let mut child_positions: Vec<Vec<usize>> = vec![Vec::new(); entries.len()];
    let mut root_positions = Vec::new();
    for (position, entry) in entries.iter().enumerate() {
        match entry.parent_id() {
            None => root_positions.push(position),
            Some(parent_id) if Some(parent_id) == entry.id() => root_positions.push(position),
            Some(parent_id) => match index.get(parent_id) {
                Some(parent_position) => child_positions[*parent_position].push(position),
                None => root_positions.push(position),
            },
        }
    }
    (child_positions, root_positions)
}

fn attach_children_in_bottom_up_order(
    nodes: &mut [Option<SessionTreeNode>],
    child_positions: &[Vec<usize>],
    root_positions: &[usize],
) {
    let mut order = root_positions.to_vec();
    let mut cursor = 0;
    while cursor < order.len() {
        let position = order[cursor];
        order.extend(child_positions[position].iter().copied());
        cursor += 1;
    }
    for position in order.iter().rev() {
        let mut children: Vec<SessionTreeNode> = child_positions[*position]
            .iter()
            .filter_map(|child| nodes[*child].take())
            .collect();
        children.sort_by_key(|node| {
            node.entry
                .timestamp()
                .and_then(pi_js_compat::date::date_parse_millis)
                .unwrap_or(0)
        });
        if let Some(mut node) = nodes[*position].take() {
            node.children = children;
            nodes[*position] = Some(node);
        }
    }
}

fn rechain_path_without_labels(path: &[SessionEntry]) -> Vec<SessionEntry> {
    let mut rechain: Vec<SessionEntry> = Vec::new();
    let mut parent_id: Option<String> = None;
    for entry in path {
        if entry.as_label().is_some() {
            continue;
        }
        let mut entry = entry.clone();
        entry.set_id_parent(entry.id().unwrap_or_default().to_owned(), parent_id.clone());
        parent_id = entry.id().map(str::to_owned);
        rechain.push(entry);
    }
    rechain
}

fn entry_id_set(entries: &[SessionEntry]) -> HashMap<String, ()> {
    entries
        .iter()
        .filter_map(|entry| entry.id().map(|id| (id.to_owned(), ())))
        .collect()
}

fn build_branch_label_entries(
    labels: &[(String, String)],
    label_timestamps: &[(String, String)],
    path_without_labels: &[SessionEntry],
    path_entry_ids: &mut HashMap<String, ()>,
) -> Vec<SessionEntry> {
    let labels_to_write: Vec<(String, String, String)> = labels
        .iter()
        .filter(|(target_id, _)| path_entry_ids.contains_key(target_id))
        .map(|(target_id, label)| {
            (
                target_id.clone(),
                label.clone(),
                lookup_label(label_timestamps, target_id).unwrap_or_default().to_owned(),
            )
        })
        .collect();

    let mut parent_id = path_without_labels
        .last()
        .and_then(|entry| entry.id().map(str::to_owned));
    let mut label_entries = Vec::new();
    for (target_id, label, label_timestamp) in labels_to_write {
        let label_entry = SessionEntry::Known(KnownSessionEntry::Label(LabelEntry {
            id: generate_id_for_ids(path_entry_ids),
            parent_id: parent_id.clone(),
            timestamp: label_timestamp,
            target_id,
            label: Some(label),
        }));
        if let Some(id) = label_entry.id() {
            path_entry_ids.insert(id.to_owned(), ());
        }
        parent_id = label_entry.id().map(str::to_owned);
        label_entries.push(label_entry);
    }
    label_entries
}

// =============================================================================
// SessionManager
// =============================================================================

/// Persisted session settings read from control-db metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersistedSessionSettings {
    pub model: Option<PersistedSessionModel>,
    pub sandbox_profile: Option<crate::core::permissions::presets::SandboxProfileName>,
    pub thinking_level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedSessionModel {
    pub provider: String,
    pub model_id: String,
}

/// Options for [`SessionManager::set_metadata_control_db_path`].
#[derive(Debug, Clone, Default)]
pub struct SetMetadataControlDbPathOptions {
    pub index_message_text: Option<bool>,
    pub cwd_override: Option<String>,
}

/// Handle shared by `AgentSession` and `ExtensionContext`, mirroring the single
/// `SessionManager` object the source passes to both.
pub type SharedSessionManager = Arc<Mutex<SessionManager>>;

/// Locks a shared session handle, recovering from a poisoned mutex.
pub fn lock_session_manager(handle: &SharedSessionManager) -> MutexGuard<'_, SessionManager> {
    handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Manages conversation sessions as append-only trees stored in JSONL files.
#[derive(Clone)]
pub struct SessionManager {
    session_id: String,
    session_file: Option<String>,
    session_dir: String,
    cwd: String,
    persist: bool,
    flushed: bool,
    file_entries: Vec<FileEntry>,
    by_id: HashMap<String, usize>,
    labels_by_id: Vec<(String, String)>,
    label_timestamps_by_id: Vec<(String, String)>,
    leaf_id: Option<String>,
    is_subagent: bool,
    subagent_name: Option<String>,
    metadata_control_db_path: Option<String>,
    index_message_text: bool,
    in_memory_goal_json: Option<String>,
}

impl crate::core::multi_agent_store::SessionPersistence for SessionManager {
    fn metadata_control_db_path(&self) -> Option<String> {
        self.get_metadata_control_db_path().map(str::to_owned)
    }

    fn session_file(&self) -> Option<String> {
        self.get_session_file().map(str::to_owned)
    }
}

impl SessionManager {
    fn new(
        cwd: &str,
        session_dir: &str,
        session_file: Option<&str>,
        persist: bool,
        options: Option<NewSessionOptions>,
    ) -> Result<Self, SessionManagerError> {
        let mut manager = Self {
            session_id: String::new(),
            session_file: None,
            session_dir: normalize(session_dir)?,
            cwd: resolve(cwd)?,
            persist,
            flushed: false,
            file_entries: Vec::new(),
            by_id: HashMap::new(),
            labels_by_id: Vec::new(),
            label_timestamps_by_id: Vec::new(),
            leaf_id: None,
            is_subagent: false,
            subagent_name: None,
            metadata_control_db_path: None,
            index_message_text: true,
            in_memory_goal_json: None,
        };
        if persist && !manager.session_dir.is_empty() && !Path::new(&manager.session_dir).exists() {
            fs::create_dir_all(&manager.session_dir).map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        match session_file {
            Some(path) => manager.set_session_file(path)?,
            None => {
                manager.new_session(options)?;
            }
        }
        Ok(manager)
    }

    /// Create a persisted session, creating the session directory when needed.
    pub fn create(
        cwd: &str,
        session_dir: Option<&str>,
        options: Option<NewSessionOptions>,
    ) -> Result<Self, SessionManagerError> {
        let dir = match session_dir {
            Some(dir) => normalize(dir)?,
            None => get_default_session_dir(cwd, None)?,
        };
        Self::new(cwd, &dir, None, true, options)
    }

    /// Open a specific session file.
    pub fn open(
        path: &str,
        session_dir: Option<&str>,
        cwd_override: Option<&str>,
    ) -> Result<Self, SessionManagerError> {
        let resolved_path = resolve(path)?;
        let header = read_session_header(&resolved_path);
        let cwd = match cwd_override {
            Some(cwd) => cwd.to_owned(),
            None => header
                .as_ref()
                .and_then(|header| header.cwd.clone())
                .unwrap_or_else(process_cwd),
        };
        let dir = match session_dir {
            Some(dir) => normalize(dir)?,
            None => Path::new(&resolved_path)
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_default(),
        };
        let mut manager = Self::new(&cwd, &dir, Some(&resolved_path), true, None)?;
        if let Some(cwd_override) = cwd_override {
            manager.cwd = resolve(cwd_override)?;
        }
        Ok(manager)
    }

    /// Continue the most recent session, or create a new one.
    pub fn continue_recent(cwd: &str, session_dir: Option<&str>) -> Result<Self, SessionManagerError> {
        let dir = match session_dir {
            Some(dir) => normalize(dir)?,
            None => get_default_session_dir(cwd, None)?,
        };
        let filter_cwd = session_dir.is_some() && !uses_default_session_dir_for(cwd, &dir);
        let most_recent = find_most_recent_session(&dir, filter_cwd.then_some(cwd));
        match most_recent {
            Some(path) => Self::new(cwd, &dir, Some(&path), true, None),
            None => Self::new(cwd, &dir, None, true, None),
        }
    }

    /// Create an in-memory session without file persistence.
    pub fn in_memory(cwd: Option<&str>, options: Option<NewSessionOptions>) -> Result<Self, SessionManagerError> {
        Self::new(cwd.unwrap_or(&process_cwd()), "", None, false, options)
    }

    /// Fork a session into a target project directory.
    pub fn fork_from(
        source_path: &str,
        target_cwd: &str,
        session_dir: Option<&str>,
        options: Option<NewSessionOptions>,
        from_entry: Option<ForkEntrySelection>,
    ) -> Result<Self, SessionManagerError> {
        let resolved_source = resolve(source_path)?;
        let resolved_target_cwd = resolve(target_cwd)?;
        let source_entries = load_fork_source_entries(&resolved_source)?;
        let dir = match session_dir {
            Some(dir) => normalize(dir)?,
            None => get_default_session_dir(&resolved_target_cwd, None)?,
        };
        fs::create_dir_all(&dir).map_err(|error| SessionManagerError::new(error.to_string()))?;

        let options = options.unwrap_or_default();
        if let Some(id) = options.id.as_deref() {
            assert_valid_session_id(id)?;
        }
        let new_session_id = options.id.clone().unwrap_or_else(uuidv7);
        let timestamp = iso_time::now_iso_string();
        let new_session_file = fork_session_file_path(&dir, &timestamp, &new_session_id);
        let header = SessionHeader {
            header_type: HeaderTag::Session,
            version: Some(CURRENT_SESSION_VERSION),
            id: new_session_id,
            timestamp,
            cwd: Some(resolved_target_cwd.clone()),
            parent_session: Some(resolved_source),
            original_key_order: None,
        };
        write_new_session_header(&new_session_file, &header)?;
        write_fork_entries(&new_session_file, source_entries, from_entry)?;

        let mut manager = Self::new(&resolved_target_cwd, &dir, Some(&new_session_file), true, None)?;
        manager.is_subagent = options.is_subagent;
        manager.subagent_name = options
            .subagent_name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
        Ok(manager)
    }

    /// Switch to a different session file (resume and branching).
    pub fn set_session_file(&mut self, session_file: &str) -> Result<(), SessionManagerError> {
        self.session_file = Some(resolve(session_file)?);
        let Some(current) = self.session_file.clone() else {
            return Ok(());
        };
        if Path::new(&current).exists() {
            self.load_existing_session_file(&current)?;
        } else {
            let explicit_path = self.session_file.clone();
            self.new_session(None)?;
            self.session_file = explicit_path;
        }
        Ok(())
    }

    fn load_existing_session_file(&mut self, path: &str) -> Result<(), SessionManagerError> {
        let loaded = load_session_file_entries(path)?;
        self.file_entries = loaded.file_entries;
        if self.file_entries.is_empty() {
            return self.recover_empty_session_file(path);
        }

        let Some(header) = self.header() else {
            return Err(error(format!("Session file is not a valid pi session: {path}")));
        };
        let loaded_cwd = loaded.preceding_cwd.clone().or_else(|| header.cwd.clone());
        if let Some(loaded_cwd) = loaded_cwd {
            self.cwd = resolve(&loaded_cwd)?;
        }
        self.session_id = header.id.clone();
        if migrate_to_current_version(&mut self.file_entries) {
            self.rewrite_file()?;
        }
        self.build_index();
        self.flushed = true;
        Ok(())
    }

    fn recover_empty_session_file(&mut self, path: &str) -> Result<(), SessionManagerError> {
        let size = fs::metadata(path)
            .map_err(|error| SessionManagerError::new(error.to_string()))?
            .len();
        if size > 0 {
            return Err(error(format!("Session file is not a valid pi session: {path}")));
        }
        let explicit_path = path.to_owned();
        self.new_session(None)?;
        self.session_file = Some(explicit_path);
        self.rewrite_file()?;
        self.flushed = true;
        Ok(())
    }

    /// Start a new session, replacing in-memory state.
    pub fn new_session(&mut self, options: Option<NewSessionOptions>) -> Result<Option<String>, SessionManagerError> {
        let options = options.unwrap_or_default();
        if let Some(id) = options.id.as_deref() {
            assert_valid_session_id(id)?;
        }
        self.session_id = options.id.unwrap_or_else(uuidv7);
        self.is_subagent = options.is_subagent;
        self.subagent_name = options
            .subagent_name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
        let timestamp = iso_time::now_iso_string();
        let header = SessionHeader {
            header_type: HeaderTag::Session,
            version: Some(CURRENT_SESSION_VERSION),
            id: self.session_id.clone(),
            timestamp: timestamp.clone(),
            cwd: Some(self.cwd.clone()),
            parent_session: options.parent_session,
            original_key_order: None,
        };
        self.file_entries = vec![FileEntry::Header(header)];
        self.by_id.clear();
        self.labels_by_id.clear();
        self.leaf_id = None;
        self.flushed = false;

        if self.persist {
            let file_timestamp = timestamp.replace([':', '.'], "-");
            self.session_file = Some(
                PathBuf::from(&self.session_dir)
                    .join(format!("{file_timestamp}_{}.jsonl", self.session_id))
                    .display()
                    .to_string(),
            );
        }
        Ok(self.session_file.clone())
    }

    fn build_index(&mut self) {
        self.by_id.clear();
        self.labels_by_id.clear();
        self.label_timestamps_by_id.clear();
        self.leaf_id = None;
        let mut label_updates = Vec::new();
        for (position, entry) in self.file_entries.iter().enumerate() {
            let FileEntry::Entry(entry) = entry else {
                continue;
            };
            let Some(id) = entry.id() else {
                continue;
            };
            self.by_id.insert(id.to_owned(), position);
            self.leaf_id = Some(id.to_owned());
            if let Some(label) = entry.as_label() {
                label_updates.push((
                    label.target_id.clone(),
                    label
                        .label
                        .as_deref()
                        .filter(|label| !label.is_empty())
                        .map(str::to_owned),
                    label.timestamp.clone(),
                ));
            }
        }
        for (target_id, label, timestamp) in label_updates {
            match label {
                Some(label) => self.set_label(&target_id, &label, &timestamp),
                None => self.remove_label(&target_id),
            }
        }
    }

    fn set_label(&mut self, target_id: &str, label: &str, timestamp: &str) {
        if let Some(existing) = self.labels_by_id.iter_mut().find(|(id, _)| id == target_id) {
            existing.1 = label.to_owned();
        } else {
            self.labels_by_id.push((target_id.to_owned(), label.to_owned()));
        }
        if let Some(existing) = self.label_timestamps_by_id.iter_mut().find(|(id, _)| id == target_id) {
            existing.1 = timestamp.to_owned();
        } else {
            self.label_timestamps_by_id
                .push((target_id.to_owned(), timestamp.to_owned()));
        }
    }

    fn remove_label(&mut self, target_id: &str) {
        self.labels_by_id.retain(|(id, _)| id != target_id);
        self.label_timestamps_by_id.retain(|(id, _)| id != target_id);
    }

    fn label(&self, target_id: &str) -> Option<&str> {
        self.labels_by_id
            .iter()
            .find(|(id, _)| id == target_id)
            .map(|(_, label)| label.as_str())
    }

    fn rewrite_file(&mut self) -> Result<(), SessionManagerError> {
        if !self.persist {
            return Ok(());
        }
        let Some(session_file) = self.session_file.clone() else {
            return Ok(());
        };
        self.write_entries(&session_file, false)
    }

    fn write_entries(&self, file_path: &str, create_new: bool) -> Result<(), SessionManagerError> {
        let mut options = OpenOptions::new();
        options.write(true);
        if create_new {
            options.create_new(true);
        } else {
            options.create(true).truncate(true);
        }
        let mut file = options
            .open(file_path)
            .map_err(|error| SessionManagerError::new(error.to_string()))?;
        for entry in &self.file_entries {
            let line = serialize_session_entry_for_persistence(entry);
            writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        Ok(())
    }

    fn has_assistant(&self) -> bool {
        self.file_entries.iter().any(|entry| match entry {
            FileEntry::Header(_) => false,
            FileEntry::Entry(entry) => entry.message().is_some_and(|message| message.role() == "assistant"),
        })
    }

    fn persist_entry(&mut self, entry: &SessionEntry) -> Result<(), SessionManagerError> {
        if !self.persist {
            return Ok(());
        }
        let Some(session_file) = self.session_file.clone() else {
            return Ok(());
        };
        if !self.has_assistant() {
            if self.flushed {
                let line = serialize_session_entry_for_persistence(&FileEntry::Entry(entry.clone()));
                let mut file = OpenOptions::new()
                    .append(true)
                    .open(&session_file)
                    .map_err(|error| SessionManagerError::new(error.to_string()))?;
                writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))?;
            } else {
                self.flushed = false;
            }
            return Ok(());
        }
        if !self.flushed {
            self.write_entries(&session_file, true)?;
            self.flushed = true;
        } else {
            let line = serialize_session_entry_for_persistence(&FileEntry::Entry(entry.clone()));
            let mut file = OpenOptions::new()
                .append(true)
                .open(&session_file)
                .map_err(|error| SessionManagerError::new(error.to_string()))?;
            writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        Ok(())
    }

    fn append_entry(&mut self, entry: SessionEntry) -> Result<String, SessionManagerError> {
        let id = entry.id().unwrap_or_default().to_owned();
        self.file_entries.push(FileEntry::Entry(entry.clone()));
        let position = self.file_entries.len() - 1;
        self.by_id.insert(id.clone(), position);
        self.leaf_id = Some(id.clone());
        self.persist_entry(&entry)?;
        if matches!(entry.entry_type(), "message" | "session_info") {
            self.write_metadata_snapshot()?;
        }
        Ok(id)
    }

    fn timestamp(&self) -> String {
        iso_time::now_iso_string()
    }

    pub fn is_persisted(&self) -> bool {
        self.persist
    }

    pub fn get_cwd(&self) -> &str {
        &self.cwd
    }

    pub fn get_session_dir(&self) -> &str {
        &self.session_dir
    }

    pub fn uses_default_session_dir(&self) -> bool {
        let agent_dir = get_agent_dir().display().to_string();
        get_default_session_dir_path(&self.cwd, &agent_dir).is_ok_and(|path| path == self.session_dir)
    }

    pub fn get_session_id(&self) -> &str {
        &self.session_id
    }

    pub fn get_session_file(&self) -> Option<&str> {
        self.session_file.as_deref()
    }

    pub fn get_metadata_control_db_path(&self) -> Option<&str> {
        self.metadata_control_db_path.as_deref()
    }

    /// The control DB and session file when both are set; TS tests each with
    /// truthiness, so empty strings count as unset.
    fn metadata_target(&self) -> Option<(&str, &str)> {
        let control_db_path = self
            .metadata_control_db_path
            .as_deref()
            .filter(|path| !path.is_empty())?;
        let session_file = self.session_file.as_deref().filter(|path| !path.is_empty())?;
        Some((control_db_path, session_file))
    }

    /// `getSessionGoalJson`.
    pub fn get_session_goal_json(&self) -> Result<Option<String>, SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(self.in_memory_goal_json.clone());
        };
        read_session_goal(control_db_path, session_file).map_err(control_db_error)
    }

    /// `getSessionGoalJsonForSession`.
    pub fn get_session_goal_json_for_session(&self, session_file: &str) -> Result<Option<String>, SessionManagerError> {
        let Some(control_db_path) = self.metadata_control_db_path.as_deref().filter(|path| !path.is_empty()) else {
            return Ok(None);
        };
        read_session_goal(control_db_path, &resolve(session_file)?).map_err(control_db_error)
    }

    /// `setSessionGoalJson`.
    pub fn set_session_goal_json(&mut self, goal_json: &str) -> Result<(), SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            self.in_memory_goal_json = Some(goal_json.to_owned());
            return Ok(());
        };
        self.write_metadata_snapshot()?;
        write_session_goal(control_db_path, session_file, Some(goal_json)).map_err(control_db_error)
    }

    /// `clearSessionGoalJson`.
    pub fn clear_session_goal_json(&mut self) -> Result<(), SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            self.in_memory_goal_json = None;
            return Ok(());
        };
        self.write_metadata_snapshot()?;
        write_session_goal(control_db_path, session_file, None).map_err(control_db_error)
    }

    /// `readPersistedSessionSettings`.
    pub fn read_persisted_session_settings(&self) -> Result<Option<PersistedSessionSettings>, SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(None);
        };
        let sandbox_profile =
            read_session_sandbox_profile(control_db_path, session_file, &self.session_id).map_err(control_db_error)?;
        let Some(metadata) = read_session_metadata(control_db_path, session_file).map_err(control_db_error)? else {
            return Ok(None);
        };
        let model = match (metadata.model_provider, metadata.model_id) {
            (Some(provider), Some(model_id)) if !provider.is_empty() && !model_id.is_empty() => {
                Some(PersistedSessionModel { provider, model_id })
            }
            _ => None,
        };
        Ok(Some(PersistedSessionSettings {
            model,
            sandbox_profile,
            thinking_level: metadata.thinking_level,
        }))
    }

    /// `setSessionModel`.
    pub fn set_session_model(&mut self, provider: &str, model_id: &str) -> Result<(), SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(());
        };
        self.write_metadata_snapshot()?;
        write_session_model(control_db_path, session_file, provider, model_id).map_err(control_db_error)
    }

    /// `setSessionThinkingLevel`.
    pub fn set_session_thinking_level(&mut self, thinking_level: &str) -> Result<(), SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(());
        };
        self.write_metadata_snapshot()?;
        write_session_thinking_level(control_db_path, session_file, thinking_level).map_err(control_db_error)
    }

    /// `setMetadataControlDbPath`.
    pub fn set_metadata_control_db_path(
        &mut self,
        control_db_path: Option<&str>,
        options: SetMetadataControlDbPathOptions,
    ) -> Result<(), SessionManagerError> {
        self.metadata_control_db_path = control_db_path.map(str::to_owned);
        if let Some(index_message_text) = options.index_message_text {
            self.index_message_text = index_message_text;
        }
        self.restore_persisted_metadata()?;
        if let Some(cwd_override) = options.cwd_override.filter(|cwd| !cwd.is_empty()) {
            self.cwd = resolve(&cwd_override)?;
        }
        self.write_metadata_snapshot()
    }

    fn restore_persisted_metadata(&mut self) -> Result<(), SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(());
        };
        let Some(metadata) = read_session_metadata(control_db_path, session_file).map_err(control_db_error)? else {
            return Ok(());
        };
        self.cwd = resolve(&metadata.cwd)?;
        if metadata.is_subagent != Some(true) {
            return Ok(());
        }
        self.is_subagent = true;
        self.subagent_name = metadata.subagent_name;
        Ok(())
    }

    fn write_metadata_snapshot(&self) -> Result<(), SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target().filter(|_| self.persist) else {
            return Ok(());
        };
        let Some(FileEntry::Header(header)) = self.file_entries.first() else {
            return Ok(());
        };
        let mut accumulator = SessionInfoAccumulator::default();
        for entry in self.file_entries.iter().skip(1) {
            if let FileEntry::Entry(entry) = entry {
                apply_session_info_entry(&mut accumulator, entry);
            }
        }
        let info = finish_session_info(session_file, header, accumulator, pi_ai::request::now_millis());
        let metadata = WritableSessionMetadata {
            session_path: info.path,
            id: info.id,
            cwd: self.cwd.clone(),
            name: info.name,
            parent_session_path: info.parent_session_path,
            archived_at: None,
            is_archived: info.is_archived,
            is_subagent: Some(self.is_subagent),
            subagent_name: self.subagent_name.clone(),
            model_provider: None,
            model_id: None,
            thinking_level: None,
            created_at: iso_time::format_iso_millis(info.created),
            modified_at: iso_time::format_iso_millis(info.modified),
            message_count: i64::try_from(info.message_count).unwrap_or(i64::MAX),
            first_message: info.first_message,
            all_messages_text: info.all_messages_text,
            index_message_text: Some(self.index_message_text),
        };
        write_session_metadata(control_db_path, &metadata).map_err(control_db_error)
    }

    pub fn is_subagent_session(&self) -> bool {
        self.is_subagent
    }

    pub fn get_subagent_name(&self) -> Option<&str> {
        self.subagent_name.as_deref()
    }

    /// Move a persisted session to another cwd's default session directory.
    pub fn relocate(&mut self, target_cwd: &str, agent_dir: Option<&str>) -> Result<(), SessionManagerError> {
        let resolved_target_cwd = resolve(target_cwd)?;
        let agent_dir = agent_dir
            .map(str::to_owned)
            .unwrap_or_else(|| get_agent_dir().display().to_string());
        let header = self
            .header()
            .ok_or_else(|| error("Cannot relocate session without a session header"))?;
        let _ = header;

        let old_session_file = self.session_file.clone();
        let should_move_to_target_default_dir =
            self.persist && self.session_dir == get_default_session_dir_path(&self.cwd, &agent_dir)?;
        let target_session_dir = if should_move_to_target_default_dir {
            get_default_session_dir(&resolved_target_cwd, Some(&agent_dir))?
        } else {
            self.session_dir.clone()
        };
        let destination = if !self.persist {
            None
        } else {
            old_session_file.as_ref().map(|old| {
                if should_move_to_target_default_dir {
                    PathBuf::from(&target_session_dir)
                        .join(Path::new(old).file_name().unwrap_or_default())
                        .display()
                        .to_string()
                } else {
                    old.clone()
                }
            })
        };
        let should_copy = destination.is_some() && old_session_file.is_some() && destination != old_session_file;
        if should_copy
            && let Some(destination) = destination.as_deref()
            && Path::new(destination).exists()
        {
            return Err(error(format!(
                "Cannot relocate session: destination already exists: {destination}"
            )));
        }
        if should_copy
            && let (Some(old), Some(destination)) = (old_session_file.as_deref(), destination.as_deref())
            && Path::new(old).exists()
        {
            let content = fs::read(old).map_err(|error| SessionManagerError::new(error.to_string()))?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)
                .map_err(|error| SessionManagerError::new(error.to_string()))?;
            file.write_all(&content)
                .map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        if let Some(control_db_path) = self.metadata_control_db_path.as_deref().filter(|path| !path.is_empty())
            && let (Some(old), Some(destination)) = (old_session_file.as_deref(), destination.as_deref())
        {
            crate::core::session_control_db::relocate_session_control_data(control_db_path, old, destination)
                .map_err(|error| SessionManagerError::new(error.to_string()))?;
        }

        self.cwd = resolved_target_cwd;
        self.session_dir = target_session_dir;
        if let Some(destination) = destination {
            self.session_file = Some(destination);
        }
        if should_copy
            && let Some(old) = old_session_file.as_deref()
            && Path::new(old).exists()
        {
            fs::remove_file(old).map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        self.write_metadata_snapshot()
    }

    pub fn get_leaf_id(&self) -> Option<&str> {
        self.leaf_id.as_deref()
    }

    pub fn get_leaf_entry(&self) -> Option<&SessionEntry> {
        let id = self.leaf_id.as_deref()?;
        self.get_entry(id)
    }

    pub fn get_entry(&self, id: &str) -> Option<&SessionEntry> {
        let position = *self.by_id.get(id)?;
        match self.file_entries.get(position)? {
            FileEntry::Entry(entry) => Some(entry),
            FileEntry::Header(_) => None,
        }
    }

    pub fn get_children(&self, parent_id: &str) -> Vec<SessionEntry> {
        self.file_entries
            .iter()
            .filter_map(|entry| match entry {
                FileEntry::Entry(entry) if entry.parent_id() == Some(parent_id) => Some(entry.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn get_label(&self, id: &str) -> Option<&str> {
        self.label(id)
    }

    pub fn append_message(&mut self, message: AgentMessage) -> Result<String, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
            id: generate_id(&self.by_id),
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
            message: Box::new(message),
        }));
        self.append_entry(entry)
    }

    pub fn append_thinking_level_change(&mut self, thinking_level: &str) -> Result<String, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
            id: generate_id(&self.by_id),
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
            thinking_level: thinking_level.to_owned(),
        }));
        self.append_entry(entry)
    }

    pub fn append_model_change(&mut self, provider: &str, model_id: &str) -> Result<String, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::ModelChange(ModelChangeEntry {
            id: generate_id(&self.by_id),
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
            provider: provider.to_owned(),
            model_id: model_id.to_owned(),
        }));
        self.append_entry(entry)
    }

    /// Append a compaction summary entry.
    pub fn append_compaction(
        &mut self,
        summary: &str,
        first_kept_entry_id: &str,
        tokens_before: f64,
        options: CompactionAppendOptions,
    ) -> Result<String, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::Compaction(CompactionEntry {
            id: generate_id(&self.by_id),
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
            summary: summary.to_owned(),
            first_kept_entry_id: first_kept_entry_id.to_owned(),
            tokens_before,
            usage: options.usage.map(Box::new),
            duration_ms: options.duration_ms,
            details: options.details.map(Box::new),
            provider_native: options.provider_native.map(Box::new),
            from_hook: options.from_hook,
            first_kept_entry_index: None,
        }));
        self.append_entry(entry)
    }

    /// Update a compaction summary in place and rewrite the file.
    pub fn update_compaction_summary(&mut self, entry_id: &str, summary: &str) -> Result<(), SessionManagerError> {
        let position = *self
            .by_id
            .get(entry_id)
            .ok_or_else(|| error(format!("Entry {entry_id} not found")))?;
        let trimmed = summary.trim();
        if trimmed.is_empty() {
            return Err(error("Compaction summary must be non-empty"));
        }
        let previous = match self.file_entries.get(position) {
            Some(FileEntry::Entry(SessionEntry::Known(KnownSessionEntry::Compaction(compaction)))) => {
                compaction.clone()
            }
            _ => return Err(error(format!("Entry {entry_id} is not a compaction"))),
        };
        let FileEntry::Entry(SessionEntry::Known(KnownSessionEntry::Compaction(entry))) =
            &mut self.file_entries[position]
        else {
            return Err(error(format!("Entry {entry_id} is not a compaction")));
        };
        entry.summary = trimmed.to_owned();
        if entry.provider_native.is_some() {
            entry.details = None;
            entry.provider_native = None;
        }
        if let Err(rewrite_error) = self.rewrite_file() {
            if let FileEntry::Entry(SessionEntry::Known(KnownSessionEntry::Compaction(entry))) =
                &mut self.file_entries[position]
            {
                entry.summary = previous.summary;
                entry.details = previous.details;
                entry.provider_native = previous.provider_native;
            }
            return Err(rewrite_error);
        }
        Ok(())
    }

    pub fn append_custom_entry(
        &mut self,
        custom_type: &str,
        data: Option<Value>,
    ) -> Result<String, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::Custom(CustomEntry {
            custom_type: custom_type.to_owned(),
            data,
            id: generate_id(&self.by_id),
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
        }));
        self.append_entry(entry)
    }

    pub fn append_session_info(&mut self, name: &str) -> Result<String, SessionManagerError> {
        let sanitized = sanitize_session_name(name);
        let entry = SessionEntry::Known(KnownSessionEntry::SessionInfo(SessionInfoEntry {
            id: generate_id(&self.by_id),
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
            name: Some(sanitized),
        }));
        self.append_entry(entry)
    }

    pub fn get_session_name(&self) -> Option<String> {
        self.get_entries()
            .iter()
            .rev()
            .find_map(|entry| entry.as_session_info())
            .and_then(|info| info.name.as_deref().map(str::trim))
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
    }

    pub fn append_custom_message_entry(
        &mut self,
        custom_type: &str,
        content: CustomMessageContent,
        display: bool,
        details: Option<Value>,
    ) -> Result<String, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::CustomMessage(CustomMessageEntry {
            custom_type: custom_type.to_owned(),
            content,
            display,
            details,
            id: generate_id(&self.by_id),
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
        }));
        self.append_entry(entry)
    }

    pub fn append_label_change(&mut self, target_id: &str, label: Option<&str>) -> Result<String, SessionManagerError> {
        if !self.by_id.contains_key(target_id) {
            return Err(error(format!("Entry {target_id} not found")));
        }
        let timestamp = self.timestamp();
        let entry = SessionEntry::Known(KnownSessionEntry::Label(LabelEntry {
            id: generate_id(&self.by_id),
            parent_id: self.leaf_id.clone(),
            timestamp: timestamp.clone(),
            target_id: target_id.to_owned(),
            label: label.map(str::to_owned),
        }));
        let id = self.append_entry(entry)?;
        match label {
            Some(label) if !label.is_empty() => self.set_label(target_id, label, &timestamp),
            _ => self.remove_label(target_id),
        }
        Ok(id)
    }

    /// Walk from an entry to the root, returning the path in root-first order.
    pub fn get_branch(&self, from_id: Option<&str>) -> Vec<SessionEntry> {
        let start_id = from_id.or(self.leaf_id.as_deref());
        let Some(mut current) = start_id.and_then(|id| self.get_entry(id)) else {
            return Vec::new();
        };
        let mut path = Vec::new();
        let mut seen: HashMap<String, ()> = HashMap::new();
        loop {
            path.push(current.clone());
            let Some(parent_id) = current.parent_id() else {
                break;
            };
            let Some(parent) = self.get_entry(parent_id) else {
                break;
            };
            if seen.insert(parent_id.to_owned(), ()).is_some() {
                break;
            }
            current = parent;
        }
        path.reverse();
        path
    }

    pub fn build_context_entries(&self) -> Vec<SessionEntry> {
        let entries = self.get_entries();
        let leaf = match self.leaf_id.as_deref() {
            Some(id) => LeafSelection::Id(id),
            None => LeafSelection::None,
        };
        build_context_entries(&entries, leaf)
    }

    pub fn build_session_context(&self) -> SessionContext {
        let entries = self.get_entries();
        let leaf = match self.leaf_id.as_deref() {
            Some(id) => LeafSelection::Id(id),
            None => LeafSelection::None,
        };
        build_session_context(&entries, leaf)
    }

    pub fn get_header(&self) -> Option<&SessionHeader> {
        self.file_entries.iter().find_map(|entry| match entry {
            FileEntry::Header(header) => Some(header),
            FileEntry::Entry(_) => None,
        })
    }

    fn header(&self) -> Option<SessionHeader> {
        self.get_header().cloned()
    }

    pub fn get_entries(&self) -> Vec<SessionEntry> {
        self.file_entries
            .iter()
            .filter_map(|entry| match entry {
                FileEntry::Entry(entry) => Some(entry.clone()),
                FileEntry::Header(_) => None,
            })
            .collect()
    }

    pub fn get_tree(&self) -> Vec<SessionTreeNode> {
        build_session_tree(&self.get_entries(), &self.labels_by_id, &self.label_timestamps_by_id)
    }

    pub fn branch(&mut self, branch_from_id: &str) -> Result<(), SessionManagerError> {
        if !self.by_id.contains_key(branch_from_id) {
            return Err(error(format!("Entry {branch_from_id} not found")));
        }
        self.leaf_id = Some(branch_from_id.to_owned());
        Ok(())
    }

    pub fn reset_leaf(&mut self) {
        self.leaf_id = None;
    }

    pub fn branch_with_summary(
        &mut self,
        branch_from_id: Option<&str>,
        summary: &str,
        details: Option<Value>,
        from_hook: Option<bool>,
        usage: Option<Usage>,
    ) -> Result<String, SessionManagerError> {
        if let Some(branch_from_id) = branch_from_id
            && !self.by_id.contains_key(branch_from_id)
        {
            return Err(error(format!("Entry {branch_from_id} not found")));
        }
        self.leaf_id = branch_from_id.map(str::to_owned);
        let entry = SessionEntry::Known(KnownSessionEntry::BranchSummary(BranchSummaryEntry {
            id: generate_id(&self.by_id),
            parent_id: branch_from_id.map(str::to_owned),
            timestamp: self.timestamp(),
            from_id: branch_from_id.unwrap_or("root").to_owned(),
            summary: summary.to_owned(),
            details,
            usage: usage.map(Box::new),
            from_hook,
        }));
        self.append_entry(entry)
    }

    /// Create a new session file containing only the path to `leaf_id`.
    pub fn create_branched_session(&mut self, leaf_id: &str) -> Result<Option<String>, SessionManagerError> {
        let previous_session_file = self.session_file.clone();
        let path = self.get_branch(Some(leaf_id));
        if path.is_empty() {
            return Err(error(format!("Entry {leaf_id} not found")));
        }
        let path_without_labels = rechain_path_without_labels(&path);

        let new_session_id = uuidv7();
        let timestamp = self.timestamp();
        let new_session_file = fork_session_file_path(&self.session_dir, &timestamp, &new_session_id);
        let header = SessionHeader {
            header_type: HeaderTag::Session,
            version: Some(CURRENT_SESSION_VERSION),
            id: new_session_id.clone(),
            timestamp,
            cwd: Some(self.cwd.clone()),
            parent_session: if self.persist { previous_session_file } else { None },
            original_key_order: None,
        };

        let mut path_entry_ids = entry_id_set(&path_without_labels);
        let label_entries = build_branch_label_entries(
            &self.labels_by_id,
            &self.label_timestamps_by_id,
            &path_without_labels,
            &mut path_entry_ids,
        );
        self.install_branched_entries(header, path_without_labels, label_entries, new_session_id);

        if !self.persist {
            return Ok(None);
        }
        self.session_file = Some(new_session_file.clone());
        if self.has_assistant() {
            self.rewrite_file()?;
            self.flushed = true;
        } else {
            self.flushed = false;
        }
        Ok(Some(new_session_file))
    }

    fn install_branched_entries(
        &mut self,
        header: SessionHeader,
        path_without_labels: Vec<SessionEntry>,
        label_entries: Vec<SessionEntry>,
        new_session_id: String,
    ) {
        let mut file_entries = vec![FileEntry::Header(header)];
        file_entries.extend(path_without_labels.into_iter().map(FileEntry::Entry));
        file_entries.extend(label_entries.into_iter().map(FileEntry::Entry));
        self.file_entries = file_entries;
        self.session_id = new_session_id;
        self.build_index();
    }

    /// Persist the current session before its first assistant response.
    pub fn persist_for_recovery(&mut self) -> Result<(), SessionManagerError> {
        if !self.persist || self.session_file.is_none() || self.flushed {
            return Ok(());
        }
        let Some(session_file) = self.session_file.clone() else {
            return Ok(());
        };
        self.write_entries(&session_file, true)?;
        self.flushed = true;
        Ok(())
    }

    /// List sessions for a directory. Metadata-backed listing is blocked on
    /// `ca-control-db`; the file scan is used in every case.
    pub fn list(
        cwd: &str,
        session_dir: Option<&str>,
        progress: Option<&mut dyn FnMut(usize, usize)>,
        _control_db_path: Option<&str>,
    ) -> Result<Vec<SessionInfo>, SessionManagerError> {
        let dir = match session_dir {
            Some(dir) => normalize(dir)?,
            None => get_default_session_dir(cwd, None)?,
        };
        let filter_cwd = session_dir.is_some() && !uses_default_session_dir_for(cwd, &dir);
        let resolved_cwd = resolve(cwd)?;
        let mut sessions = list_sessions_from_dir(&dir, progress, 0, None);
        if filter_cwd {
            sessions.retain(|session| session_cwd_matches(Some(&session.cwd), &resolved_cwd));
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.modified));
        Ok(sessions)
    }

    /// List archived sessions. Requires the blocked control-db metadata store.
    pub fn list_archived(_session_dir: Option<&str>, _control_db_path: Option<&str>) -> Vec<SessionInfo> {
        Vec::new()
    }

    /// List sessions across all project directories (or one custom directory).
    pub fn list_all(
        session_dir: Option<&str>,
        progress: Option<&mut dyn FnMut(usize, usize)>,
        _control_db_path: Option<&str>,
    ) -> Result<Vec<SessionInfo>, SessionManagerError> {
        if let Some(dir) = session_dir {
            let dir = normalize(dir)?;
            let mut sessions = match progress {
                Some(callback) => list_sessions_from_dir(&dir, Some(callback), 0, None),
                None => list_sessions_from_dir(&dir, None, 0, None),
            };
            sessions.sort_by_key(|session| std::cmp::Reverse(session.modified));
            return Ok(sessions);
        }
        Ok(match progress {
            Some(callback) => list_default_session_root(Some(callback)),
            None => list_default_session_root(None),
        })
    }
}

fn uses_default_session_dir_for(cwd: &str, dir: &str) -> bool {
    let agent_dir = get_agent_dir().display().to_string();
    get_default_session_dir_path(cwd, &agent_dir).is_ok_and(|path| path == dir)
}

fn sanitize_session_name(name: &str) -> String {
    let mut result = String::new();
    let mut pending_space = false;
    for character in name.chars() {
        if character == '\r' || character == '\n' {
            pending_space = true;
            continue;
        }
        if pending_space {
            result.push(' ');
            pending_space = false;
        }
        result.push(character);
    }
    result.trim().to_owned()
}
