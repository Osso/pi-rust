//! Port of `packages/coding-agent/src/core/session-manager.ts`.
//!
//! Sessions are append-only trees stored as JSONL. Control-db metadata
//! (`setMetadataControlDbPath`, goal/model/sandbox accessors and
//! metadata-backed listing) is persisted through `session_control_db`.

use pi_agent_core::agent::AgentMessageIdentity;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::messages::CustomMessageContent;
use pi_agent_core::uuidv7;
use pi_ai::types::{ProviderNativeContext, TextOrImageContent, UserContent};
use pi_ai::utils::random_uuid::random_uuid;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::{get_agent_dir, get_sessions_dir};
use crate::core::permissions::presets::SandboxProfileName;
use crate::core::resident_session::is_resident_session;
use crate::core::session_control_db::{
    ResumeSessionMetadataOptions, SessionMetadata, WritableSessionMetadata, clear_session_sandbox_profile,
    list_resume_session_metadata, list_session_metadata, normalize_session_name, read_session_goal,
    read_session_metadata, read_session_name_state, read_session_sandbox_profile, write_session_goal,
    write_session_metadata, write_session_model, write_session_name, write_session_sandbox_profile,
    write_session_thinking_level,
};
use crate::core::session_tool_output::serialize_session_entry_for_persistence;
use crate::core::session_transcript::{create_session_transcript_read_stream, read_session_transcript_path};
use crate::utils::iso_time;
use crate::utils::paths::{
    PathInputOptions, node_basename, node_dirname, node_join, normalize_path, process_cwd, resolve_path,
};

/// Current persisted session schema version.
pub const CURRENT_SESSION_VERSION: f64 = 3.0;
/// Maximum UTF-8 byte length kept from pre-compaction messages.
pub const MAX_KEPT_PRE_COMPACTION_BYTES: usize = 20_000;

const SESSION_READ_BUFFER_SIZE: usize = 1024 * 1024;
const HEADER_READ_BUFFER_SIZE: usize = 4096;

/// Failure raised by session loading, mutation and persistence.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionManagerError {
    name: &'static str,
    message: JsString,
    config: Option<Box<crate::config::ConfigError>>,
    control_db: Option<Box<crate::core::sqlite::ControlDbError>>,
    native: Option<Box<pi_js_compat::node_error::NativeNodeError>>,
    callback: Option<Box<pi_js_compat::callback_error::CallbackError>>,
}

impl SessionManagerError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            name: "Error",
            message: message.into(),
            config: None,
            control_db: None,
            native: None,
            callback: None,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }
    pub(crate) fn config_error(&self) -> Option<&crate::config::ConfigError> {
        self.config.as_deref()
    }

    pub fn native_error(&self) -> Option<&pi_js_compat::node_error::NativeNodeError> {
        self.native
            .as_deref()
            .or_else(|| match self.callback.as_ref()?.rejection.as_deref()? {
                pi_js_compat::abort::AbortReason::Error(error)
                | pi_js_compat::abort::AbortReason::DomException(error) => Some(error),
                pi_js_compat::abort::AbortReason::Value(_) => None,
            })
            .or_else(|| {
                self.control_db
                    .as_deref()
                    .and_then(crate::core::sqlite::ControlDbError::native_error)
            })
            .or_else(|| self.config_error().and_then(crate::config::ConfigError::native_error))
    }

    pub fn raise_uncaught(self) -> ! {
        if let Some(callback) = &self.callback {
            pi_js_compat::callback_error::throw_callback_uncaught(callback.as_ref().clone());
        }
        if let Some(native) = self.native_error() {
            crate::utils::uncaught::throw_native_uncaught(native.clone());
        }
        match self.config {
            Some(error) => error.raise_uncaught(),
            None => crate::utils::uncaught::throw_uncaught(self.name, self.message),
        }
    }
}

impl From<SessionManagerError> for pi_ai::request::CallbackError {
    fn from(error: SessionManagerError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}

impl std::fmt::Display for SessionManagerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for SessionManagerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.callback
            .as_deref()
            .map(|error| error as &dyn std::error::Error)
            .or_else(|| self.native.as_deref().map(|error| error as &dyn std::error::Error))
            .or_else(|| self.control_db.as_deref().map(|error| error as &dyn std::error::Error))
            .or_else(|| self.config_error().map(|error| error as &dyn std::error::Error))
    }
}
impl From<pi_js_compat::callback_error::CallbackError> for SessionManagerError {
    fn from(error: pi_js_compat::callback_error::CallbackError) -> Self {
        let mut result = Self::new(error.message.clone());
        result.callback = Some(Box::new(error));
        result
    }
}

impl From<Box<pi_js_compat::node_error::NativeNodeError>> for SessionManagerError {
    fn from(error: Box<pi_js_compat::node_error::NativeNodeError>) -> Self {
        Self {
            name: error.constructor_class().name(),
            message: error.message.clone(),
            config: None,
            control_db: None,
            native: Some(error),
            callback: None,
        }
    }
}

impl From<crate::config::ConfigError> for SessionManagerError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self {
            name: error.label(),
            message: error.message().clone(),
            config: Some(Box::new(error)),
            control_db: None,
            native: None,
            callback: None,
        }
    }
}

impl From<pi_js_compat::date::InvalidTimeValue> for SessionManagerError {
    fn from(cause: pi_js_compat::date::InvalidTimeValue) -> Self {
        Self {
            name: cause.name(),
            message: cause.to_string().into(),
            config: None,
            control_db: None,
            native: None,
            callback: None,
        }
    }
}

fn filesystem_error(cause: std::io::Error, syscall: &str, path: &JsString) -> SessionManagerError {
    let record = pi_js_compat::node_fs::node_fs_error_record(&cause, syscall, path);
    Box::new(pi_js_compat::node_error::NativeNodeError::from(record)).into()
}

fn format_session_error(cause: impl std::fmt::Display) -> SessionManagerError {
    error(cause.to_string())
}

pub(super) fn control_db_error(cause: crate::core::sqlite::ControlDbError) -> SessionManagerError {
    SessionManagerError {
        name: cause
            .native_error()
            .map_or("Error", |error| error.constructor_class().name()),
        message: cause.message().clone(),
        config: None,
        control_db: Some(Box::new(cause)),
        native: None,
        callback: None,
    }
}

fn assert_unarchived_session_file(path: &JsString) -> Result<(), SessionManagerError> {
    if path.ends_with(".zst") {
        return Err(error(
            JsString::from("Archived session requires a control database restore before opening: ") + path,
        ));
    }
    Ok(())
}

fn error(message: impl Into<JsString>) -> SessionManagerError {
    SessionManagerError::new(message)
}

/// Opaque provider/model/thinking tags stay scalar under the approved metadata gap;
/// unpaired surrogates are rejected rather than replaced.
fn opaque_metadata(value: &JsString) -> Result<String, SessionManagerError> {
    value.to_utf8().map_err(|cause| error(cause.to_string()))
}

fn path_options() -> PathInputOptions {
    PathInputOptions::default()
}

fn current_cwd() -> Result<JsString, SessionManagerError> {
    process_cwd()
        .map_err(crate::config::ConfigError::from)
        .map_err(SessionManagerError::from)
}

fn resolve(input: impl Into<JsString>) -> Result<JsString, SessionManagerError> {
    resolve_path(input, current_cwd()?, &path_options()).map_err(|cause| error(cause.message().clone()))
}

fn normalize(input: impl Into<JsString>) -> Result<JsString, SessionManagerError> {
    normalize_path(input, &path_options()).map_err(|cause| error(cause.message().clone()))
}

// =============================================================================
// Wire records
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeaderTag {
    Session,
}

/// The parsed header object is the only stored authority. Mutations update it in place.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionHeader {
    pub fields: JsObject,
}

impl Serialize for SessionHeader {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        pi_js_compat::js_json::serialize_object(&self.fields, serializer)
    }
}

impl<'de> Deserialize<'de> for SessionHeader {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = pi_js_compat::js_json::deserialize_object(deserializer)?;
        if fields.get("type").and_then(JsValue::as_string) != Some(&JsString::from("session")) {
            return Err(serde::de::Error::custom("session header type must be session"));
        }
        Ok(Self { fields })
    }
}

fn empty_js_string() -> &'static JsString {
    static EMPTY: std::sync::OnceLock<JsString> = std::sync::OnceLock::new();
    EMPTY.get_or_init(JsString::default)
}

impl SessionHeader {
    pub fn new(id: JsString, timestamp: JsString, cwd: Option<JsString>, parent_session: Option<JsString>) -> Self {
        let mut fields = JsObject::new();
        fields.insert("type".into(), JsValue::String("session".into()));
        fields.insert("version".into(), JsValue::Number(CURRENT_SESSION_VERSION));
        fields.insert("id".into(), JsValue::String(id));
        fields.insert("timestamp".into(), JsValue::String(timestamp));
        if let Some(cwd) = cwd {
            fields.insert("cwd".into(), JsValue::String(cwd));
        }
        if let Some(parent) = parent_session {
            fields.insert("parentSession".into(), JsValue::String(parent));
        }
        Self { fields }
    }
    pub fn id(&self) -> &JsString {
        self.fields
            .get("id")
            .and_then(JsValue::as_string)
            .unwrap_or_else(|| empty_js_string())
    }
    pub fn timestamp(&self) -> &JsString {
        self.fields
            .get("timestamp")
            .and_then(JsValue::as_string)
            .unwrap_or_else(|| empty_js_string())
    }
    pub fn version(&self) -> Option<f64> {
        self.fields.get("version").and_then(JsValue::as_f64)
    }
    pub fn cwd(&self) -> Option<&JsString> {
        self.fields.get("cwd").and_then(JsValue::as_string)
    }
    pub fn parent_session(&self) -> Option<&JsString> {
        self.fields.get("parentSession").and_then(JsValue::as_string)
    }
    pub fn cwd_or_empty(&self) -> &JsString {
        self.cwd().unwrap_or_else(|| empty_js_string())
    }
    fn set_version(&mut self, version: f64) {
        self.fields.insert("version".into(), JsValue::Number(version));
    }
}

/// Options for creating a new session.
#[derive(Debug, Clone, Default)]
pub struct NewSessionOptions {
    pub id: Option<JsString>,
    pub parent_session: Option<JsString>,
    pub is_subagent: bool,
    pub subagent_name: Option<JsString>,
}

/// JSON stored at an untyped persistence boundary, retaining UTF-16 strings/keys.
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedJsonValue(pi_js_compat::js_json::JsValue);
impl PersistedJsonValue {
    pub(crate) fn is_null(&self) -> bool {
        matches!(self.0, JsValue::Null)
    }
}
impl Serialize for PersistedJsonValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize_lossless(&self.0, serializer)
    }
}
impl<'de> Deserialize<'de> for PersistedJsonValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsValue::deserialize(deserializer).map(Self)
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PersistedJsonObject(pi_js_compat::js_json::JsObject);
impl<'de> Deserialize<'de> for PersistedJsonObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        pi_js_compat::js_json::deserialize_object(deserializer).map(Self)
    }
}
fn serialize_lossless<S: serde::Serializer>(
    value: &pi_js_compat::js_json::JsValue,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let raw = serde_json::value::RawValue::from_string(pi_js_compat::js_json::stringify_lossless(value))
        .map_err(serde::ser::Error::custom)?;
    raw.serialize(serializer)
}
fn deserialize_receipt_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<PersistedJsonValue>, D::Error> {
    PersistedJsonValue::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessageEntry {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    pub message: Box<AgentMessage>,
    /// Raw persisted receipt value; None omits the key, Some(Null) preserves it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_receipt_value"
    )]
    pub mailbox_store_refs: Option<PersistedJsonValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingLevelChangeEntry {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    pub thinking_level: JsString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChangeEntry {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    pub provider: JsString,
    pub model_id: JsString,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompactionEntry {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    pub summary: JsString,
    pub first_kept_entry_id: JsString,
    pub tokens_before: f64,
    pub extra: PersistedJsonObject,
    pub duration_ms: Option<f64>,
    pub details: Option<Box<JsValue>>,
    pub provider_native: Option<Box<JsValue>>,
    pub from_hook: Option<bool>,
    /// Legacy v1 compaction field converted during migration.
    pub first_kept_entry_index: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompactionEntryFields {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    pub summary: JsString,
    pub first_kept_entry_id: JsString,
    #[serde(serialize_with = "js_number::serialize")]
    pub tokens_before: f64,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub duration_ms: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<Box<JsValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_native: Option<Box<JsValue>>,
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

impl<'de> Deserialize<'de> for CompactionEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let mut object = value
            .as_object()
            .cloned()
            .ok_or_else(|| serde::de::Error::custom("expected summary object"))?;
        let fields: CompactionEntryFields =
            pi_js_compat::js_json::from_value(&value).map_err(serde::de::Error::custom)?;
        for key in [
            "id",
            "parentId",
            "timestamp",
            "summary",
            "firstKeptEntryId",
            "tokensBefore",
            "durationMs",
            "details",
            "providerNative",
            "fromHook",
            "firstKeptEntryIndex",
        ] {
            object.shift_remove(key);
        }
        object.shift_remove("type");
        Ok(Self {
            id: fields.id,
            parent_id: fields.parent_id,
            timestamp: fields.timestamp,
            summary: fields.summary,
            first_kept_entry_id: fields.first_kept_entry_id,
            tokens_before: fields.tokens_before,
            duration_ms: fields.duration_ms,
            details: fields.details,
            provider_native: fields.provider_native,
            from_hook: fields.from_hook,
            first_kept_entry_index: fields.first_kept_entry_index,
            extra: PersistedJsonObject(object),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BranchSummaryEntry {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    pub from_id: JsString,
    pub summary: JsString,
    pub details: Option<JsValue>,
    pub extra: PersistedJsonObject,
    pub from_hook: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BranchSummaryEntryFields {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    pub from_id: JsString,
    pub summary: JsString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<JsValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_hook: Option<bool>,
}

impl<'de> Deserialize<'de> for BranchSummaryEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let mut object = value
            .as_object()
            .cloned()
            .ok_or_else(|| serde::de::Error::custom("expected summary object"))?;
        let fields: BranchSummaryEntryFields =
            pi_js_compat::js_json::from_value(&value).map_err(serde::de::Error::custom)?;
        for key in [
            "id",
            "parentId",
            "timestamp",
            "fromId",
            "summary",
            "details",
            "fromHook",
        ] {
            object.shift_remove(key);
        }
        object.shift_remove("type");
        Ok(Self {
            id: fields.id,
            parent_id: fields.parent_id,
            timestamp: fields.timestamp,
            from_id: fields.from_id,
            summary: fields.summary,
            details: fields.details,
            from_hook: fields.from_hook,
            extra: PersistedJsonObject(object),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEntry {
    pub custom_type: JsString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub data: Option<JsValue>,
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomMessageEntry {
    pub custom_type: JsString,
    pub content: CustomMessageContent,
    pub display: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<JsValue>,
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelEntry {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    pub target_id: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoEntry {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<JsString>,
}

fn insert_serialized<T: Serialize + ?Sized>(
    object: &mut pi_js_compat::js_json::JsObject,
    key: &str,
    value: &T,
) -> Result<(), String> {
    let json = serde_json::to_string(value).map_err(|e| e.to_string())?;
    let parsed = pi_js_compat::js_json::parse_lossless(&json).map_err(|e| e.to_string())?;
    object.insert(key.into(), parsed);
    Ok(())
}
impl CompactionEntry {
    fn lossless_object(&self) -> Result<pi_js_compat::js_json::JsObject, String> {
        let mut object = pi_js_compat::js_json::JsObject::new();
        insert_serialized(&mut object, "id", &self.id)?;
        insert_serialized(&mut object, "parentId", &self.parent_id)?;
        insert_serialized(&mut object, "timestamp", &self.timestamp)?;
        insert_serialized(&mut object, "summary", &self.summary)?;
        insert_serialized(&mut object, "firstKeptEntryId", &self.first_kept_entry_id)?;
        insert_serialized(&mut object, "tokensBefore", &self.tokens_before)?;
        object.extend(self.extra.0.clone());
        if let Some(value) = &self.duration_ms {
            insert_serialized(&mut object, "durationMs", value)?;
        }
        if let Some(value) = &self.details {
            insert_serialized(&mut object, "details", value)?;
        }
        if let Some(value) = &self.provider_native {
            insert_serialized(&mut object, "providerNative", value)?;
        }
        if let Some(value) = &self.from_hook {
            insert_serialized(&mut object, "fromHook", value)?;
        }
        if let Some(value) = &self.first_kept_entry_index {
            insert_serialized(&mut object, "firstKeptEntryIndex", value)?;
        }
        Ok(object)
    }
}
impl Serialize for CompactionEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let object = self.lossless_object().map_err(serde::ser::Error::custom)?;
        serialize_lossless(&pi_js_compat::js_json::JsValue::Object(object), serializer)
    }
}
impl BranchSummaryEntry {
    fn lossless_object(&self) -> Result<pi_js_compat::js_json::JsObject, String> {
        let mut object = pi_js_compat::js_json::JsObject::new();
        insert_serialized(&mut object, "id", &self.id)?;
        insert_serialized(&mut object, "parentId", &self.parent_id)?;
        insert_serialized(&mut object, "timestamp", &self.timestamp)?;
        insert_serialized(&mut object, "fromId", &self.from_id)?;
        insert_serialized(&mut object, "summary", &self.summary)?;
        if let Some(value) = &self.details {
            insert_serialized(&mut object, "details", value)?;
        }
        object.extend(self.extra.0.clone());
        if let Some(value) = &self.from_hook {
            insert_serialized(&mut object, "fromHook", value)?;
        }
        Ok(object)
    }
}
impl Serialize for BranchSummaryEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let object = self.lossless_object().map_err(serde::ser::Error::custom)?;
        serialize_lossless(&pi_js_compat::js_json::JsValue::Object(object), serializer)
    }
}

/// Closed set of known session entry shapes.
#[derive(Debug, Clone, PartialEq)]
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

impl<'de> Deserialize<'de> for KnownSessionEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let tag = value.get("type").and_then(JsValue::as_string);
        if tag == Some(&JsString::from("message")) {
            return pi_js_compat::js_json::from_value::<SessionMessageEntry>(&value)
                .map(Self::Message)
                .map_err(serde::de::Error::custom);
        }
        if tag == Some(&JsString::from("thinking_level_change")) {
            return pi_js_compat::js_json::from_value::<ThinkingLevelChangeEntry>(&value)
                .map(Self::ThinkingLevelChange)
                .map_err(serde::de::Error::custom);
        }
        if tag == Some(&JsString::from("model_change")) {
            return pi_js_compat::js_json::from_value::<ModelChangeEntry>(&value)
                .map(Self::ModelChange)
                .map_err(serde::de::Error::custom);
        }
        if tag == Some(&JsString::from("compaction")) {
            return pi_js_compat::js_json::from_value::<CompactionEntry>(&value)
                .map(Self::Compaction)
                .map_err(serde::de::Error::custom);
        }
        if tag == Some(&JsString::from("branch_summary")) {
            return pi_js_compat::js_json::from_value::<BranchSummaryEntry>(&value)
                .map(Self::BranchSummary)
                .map_err(serde::de::Error::custom);
        }
        if tag == Some(&JsString::from("custom")) {
            return pi_js_compat::js_json::from_value::<CustomEntry>(&value)
                .map(Self::Custom)
                .map_err(serde::de::Error::custom);
        }
        if tag == Some(&JsString::from("custom_message")) {
            return pi_js_compat::js_json::from_value::<CustomMessageEntry>(&value)
                .map(Self::CustomMessage)
                .map_err(serde::de::Error::custom);
        }
        if tag == Some(&JsString::from("label")) {
            return pi_js_compat::js_json::from_value::<LabelEntry>(&value)
                .map(Self::Label)
                .map_err(serde::de::Error::custom);
        }
        if tag == Some(&JsString::from("session_info")) {
            return pi_js_compat::js_json::from_value::<SessionInfoEntry>(&value)
                .map(Self::SessionInfo)
                .map_err(serde::de::Error::custom);
        }
        Err(serde::de::Error::custom("unknown session entry type"))
    }
}

impl Serialize for KnownSessionEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (tag, json) = match self {
            Self::Message(entry) => ("message", serde_json::to_string(entry)),
            Self::ThinkingLevelChange(entry) => ("thinking_level_change", serde_json::to_string(entry)),
            Self::ModelChange(entry) => ("model_change", serde_json::to_string(entry)),
            Self::Compaction(entry) => ("compaction", serde_json::to_string(entry)),
            Self::BranchSummary(entry) => ("branch_summary", serde_json::to_string(entry)),
            Self::Custom(entry) => ("custom", serde_json::to_string(entry)),
            Self::CustomMessage(entry) => ("custom_message", serde_json::to_string(entry)),
            Self::Label(entry) => ("label", serde_json::to_string(entry)),
            Self::SessionInfo(entry) => ("session_info", serde_json::to_string(entry)),
        };
        let json = json.map_err(serde::ser::Error::custom)?;
        let fields = pi_js_compat::js_json::parse_lossless(&json).map_err(serde::ser::Error::custom)?;
        let pi_js_compat::js_json::JsValue::Object(fields) = fields else {
            return Err(serde::ser::Error::custom("session entry must be an object"));
        };
        let mut object = pi_js_compat::js_json::JsObject::new();
        object.insert("type".into(), pi_js_compat::js_json::JsValue::String(tag.into()));
        object.extend(fields);
        serialize_lossless(&pi_js_compat::js_json::JsValue::Object(object), serializer)
    }
}

/// Imported object: exactly one editable UTF-16/JSON authority, including unknown fields.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnknownSessionEntry(pub JsObject);
impl From<Map<String, Value>> for UnknownSessionEntry {
    fn from(fields: Map<String, Value>) -> Self {
        Self(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), JsValue::from(value)))
                .collect(),
        )
    }
}
impl std::ops::Deref for UnknownSessionEntry {
    type Target = JsObject;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for UnknownSessionEntry {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl<'de> Deserialize<'de> for UnknownSessionEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        pi_js_compat::js_json::deserialize_object(deserializer).map(Self)
    }
}
impl Serialize for UnknownSessionEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        pi_js_compat::js_json::serialize_object(&self.0, serializer)
    }
}

/// One session entry. Unknown `type` values are preserved as raw JSON objects.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SessionEntry {
    Known(KnownSessionEntry),
    Unknown(UnknownSessionEntry),
}
impl<'de> Deserialize<'de> for SessionEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        UnknownSessionEntry::deserialize(deserializer).map(Self::Unknown)
    }
}

impl SessionEntry {
    /// A transient typed projection; imported objects remain the sole stored authority.
    /// Consumers read entries through this view, never by matching `Known` directly.
    pub fn known(&self) -> Option<Cow<'_, KnownSessionEntry>> {
        match self {
            Self::Known(entry) => Some(Cow::Borrowed(entry)),
            Self::Unknown(_) if self.entry_type() == "message" => self
                .as_message()
                .map(|entry| Cow::Owned(KnownSessionEntry::Message(entry.into_owned()))),
            Self::Unknown(object) => pi_js_compat::js_json::from_value(&JsValue::Object(object.0.clone()))
                .ok()
                .map(Cow::Owned),
        }
    }

    pub fn entry_type(&self) -> JsString {
        match self {
            Self::Known(KnownSessionEntry::Message(_)) => "message".into(),
            Self::Known(KnownSessionEntry::ThinkingLevelChange(_)) => "thinking_level_change".into(),
            Self::Known(KnownSessionEntry::ModelChange(_)) => "model_change".into(),
            Self::Known(KnownSessionEntry::Compaction(_)) => "compaction".into(),
            Self::Known(KnownSessionEntry::BranchSummary(_)) => "branch_summary".into(),
            Self::Known(KnownSessionEntry::Custom(_)) => "custom".into(),
            Self::Known(KnownSessionEntry::CustomMessage(_)) => "custom_message".into(),
            Self::Known(KnownSessionEntry::Label(_)) => "label".into(),
            Self::Known(KnownSessionEntry::SessionInfo(_)) => "session_info".into(),
            Self::Unknown(object) => object
                .get("type")
                .and_then(JsValue::as_string)
                .cloned()
                .unwrap_or_default(),
        }
    }

    pub fn id(&self) -> Option<&JsString> {
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
            Self::Unknown(object) => object.get("id").and_then(JsValue::as_string),
        }
    }

    pub fn parent_id(&self) -> Option<&JsString> {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => entry.parent_id.as_ref(),
            Self::Known(KnownSessionEntry::ThinkingLevelChange(entry)) => entry.parent_id.as_ref(),
            Self::Known(KnownSessionEntry::ModelChange(entry)) => entry.parent_id.as_ref(),
            Self::Known(KnownSessionEntry::Compaction(entry)) => entry.parent_id.as_ref(),
            Self::Known(KnownSessionEntry::BranchSummary(entry)) => entry.parent_id.as_ref(),
            Self::Known(KnownSessionEntry::Custom(entry)) => entry.parent_id.as_ref(),
            Self::Known(KnownSessionEntry::CustomMessage(entry)) => entry.parent_id.as_ref(),
            Self::Known(KnownSessionEntry::Label(entry)) => entry.parent_id.as_ref(),
            Self::Known(KnownSessionEntry::SessionInfo(entry)) => entry.parent_id.as_ref(),
            Self::Unknown(object) => object.get("parentId").and_then(JsValue::as_string),
        }
    }

    pub fn timestamp(&self) -> Option<&JsString> {
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
            Self::Unknown(object) => object.get("timestamp").and_then(JsValue::as_string),
        }
    }

    pub fn as_message(&self) -> Option<Cow<'_, SessionMessageEntry>> {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => Some(Cow::Borrowed(entry)),
            Self::Unknown(object) if self.entry_type() == "message" => Some(Cow::Owned(SessionMessageEntry {
                id: self.id()?.clone(),
                parent_id: self.parent_id().cloned(),
                timestamp: self.timestamp()?.clone(),
                message: Box::new(self.message()?.into_owned()),
                mailbox_store_refs: object.get("mailboxStoreRefs").cloned().map(PersistedJsonValue),
            })),
            _ => None,
        }
    }

    pub fn as_compaction(&self) -> Option<Cow<'_, CompactionEntry>> {
        match self {
            Self::Known(KnownSessionEntry::Compaction(entry)) => Some(Cow::Borrowed(entry)),
            Self::Unknown(object) if self.entry_type() == "compaction" => {
                pi_js_compat::js_json::from_value(&JsValue::Object(object.0.clone()))
                    .ok()
                    .map(Cow::Owned)
            }
            _ => None,
        }
    }

    pub fn as_custom_message(&self) -> Option<Cow<'_, CustomMessageEntry>> {
        match self {
            Self::Known(KnownSessionEntry::CustomMessage(entry)) => Some(Cow::Borrowed(entry)),
            Self::Unknown(object) if self.entry_type() == "custom_message" => {
                pi_js_compat::js_json::from_value(&JsValue::Object(object.0.clone()))
                    .ok()
                    .map(Cow::Owned)
            }
            _ => None,
        }
    }

    pub fn as_label(&self) -> Option<Cow<'_, LabelEntry>> {
        match self {
            Self::Known(KnownSessionEntry::Label(entry)) => Some(Cow::Borrowed(entry)),
            Self::Unknown(object) if self.entry_type() == "label" => {
                pi_js_compat::js_json::from_value(&JsValue::Object(object.0.clone()))
                    .ok()
                    .map(Cow::Owned)
            }
            _ => None,
        }
    }

    pub fn as_session_info(&self) -> Option<Cow<'_, SessionInfoEntry>> {
        match self {
            Self::Known(KnownSessionEntry::SessionInfo(entry)) => Some(Cow::Borrowed(entry)),
            Self::Unknown(object) if self.entry_type() == "session_info" => {
                pi_js_compat::js_json::from_value(&JsValue::Object(object.0.clone()))
                    .ok()
                    .map(Cow::Owned)
            }
            _ => None,
        }
    }

    /// Typed view of a `message` record's message, raw objects included.
    pub fn message(&self) -> Option<Cow<'_, AgentMessage>> {
        match self {
            Self::Known(KnownSessionEntry::Message(entry)) => Some(Cow::Borrowed(entry.message.as_ref())),
            Self::Unknown(object) if self.entry_type() == "message" => {
                let message = object.get("message")?.as_object()?;
                Some(Cow::Owned(project_agent_message(message)))
            }
            _ => None,
        }
    }

    fn object(&self) -> Cow<'_, JsObject> {
        match self {
            Self::Unknown(object) => Cow::Borrowed(&object.0),
            Self::Known(_) => {
                let value = pi_js_compat::js_json::to_value(self).expect("known record serializes losslessly");
                let JsValue::Object(object) = value else {
                    unreachable!("known record is an object")
                };
                Cow::Owned(object)
            }
        }
    }

    fn object_mut(&mut self) -> &mut JsObject {
        if let Self::Known(_) = self {
            let value = pi_js_compat::js_json::to_value(self).expect("known record serializes losslessly");
            let JsValue::Object(object) = value else {
                unreachable!("known record is an object")
            };
            *self = Self::Unknown(UnknownSessionEntry(object));
        }
        let Self::Unknown(object) = self else {
            unreachable!("record converted to raw authority")
        };
        &mut object.0
    }

    fn set_parent_id(&mut self, parent_id: Option<JsString>) {
        self.object_mut().insert(
            "parentId".into(),
            parent_id.map(JsValue::String).unwrap_or(JsValue::Null),
        );
    }

    fn set_id_parent(&mut self, id: JsString, parent_id: Option<JsString>) {
        let object = self.object_mut();
        object.insert("id".into(), JsValue::String(id));
        object.insert(
            "parentId".into(),
            parent_id.map(JsValue::String).unwrap_or(JsValue::Null),
        );
    }

    fn set_hook_role_to_custom(&mut self) {
        if self.entry_type() != "message" {
            return;
        }
        if let Some(message) = self.object_mut().get_mut("message").and_then(JsValue::as_object_mut)
            && message.get("role").and_then(JsValue::as_string) == Some(&JsString::from("hookMessage"))
        {
            message.insert("role".into(), JsValue::String("custom".into()));
        }
    }
}

/// Typed view of a persisted `message` object. Source messages are untyped JSON, so an
/// object that does not fit a `Message` role variant keeps its exact object as `Custom`.
fn project_agent_message(message: &JsObject) -> AgentMessage {
    pi_js_compat::js_json::from_value(&JsValue::Object(message.clone()))
        .unwrap_or_else(|_| AgentMessage::Custom(message.clone()))
}

/// Header or entry as read from a session file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FileEntry {
    Header(SessionHeader),
    Entry(SessionEntry),
    /// JSON.parse accepts non-record values; preserve them without a fabricated object.
    Json(PersistedJsonValue),
}
impl<'de> Deserialize<'de> for FileEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        match value {
            JsValue::Object(fields) => {
                if fields.get("type").and_then(JsValue::as_string) == Some(&JsString::from("session")) {
                    Ok(Self::Header(SessionHeader { fields }))
                } else {
                    Ok(Self::Entry(SessionEntry::Unknown(UnknownSessionEntry(fields))))
                }
            }
            value => Ok(Self::Json(PersistedJsonValue(value))),
        }
    }
}

impl FileEntry {
    pub fn entry(&self) -> Option<&SessionEntry> {
        match self {
            Self::Header(_) | Self::Json(_) => None,
            Self::Entry(entry) => Some(entry),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModel {
    pub provider: JsString,
    pub model_id: JsString,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionContext {
    pub messages: Vec<AgentMessage>,
    pub thinking_level: JsString,
    pub model: Option<SessionModel>,
}

/// Which leaf to walk from. `Last` maps `leafId === undefined`, `None` maps
/// `leafId === null`, and `Id` maps a concrete entry id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LeafSelection<'a> {
    #[default]
    Last,
    None,
    Id(&'a JsString),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTreeNode {
    pub entry: SessionEntry,
    pub children: Vec<SessionTreeNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_timestamp: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionInfo {
    pub path: JsString,
    pub id: JsString,
    pub cwd: JsString,
    pub name: Option<JsString>,
    pub is_archived: Option<bool>,
    pub parent_session_path: Option<JsString>,
    /// JavaScript Date time value: integral milliseconds or NaN for Invalid Date.
    pub created: f64,
    /// JavaScript Date time value: integral milliseconds or NaN for Invalid Date.
    pub modified: f64,
    pub message_count: usize,
    pub first_message: JsString,
}

/// Progress callback: `(loaded, total)`.
pub type SessionListProgress<'a> = &'a mut dyn FnMut(usize, usize);

/// Selection used by [`SessionManager::fork_from`] for the copied entry range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkEntrySelection {
    All,
    None,
    From(JsString),
}

/// Optional compaction metadata recorded by [`SessionManager::append_compaction`].
#[derive(Debug, Clone, Default)]
pub struct CompactionAppendOptions {
    pub details: Option<JsValue>,
    pub from_hook: Option<bool>,
    pub duration_ms: Option<f64>,
    pub provider_native: Option<ProviderNativeContext>,
}

// =============================================================================
// Pure helpers
// =============================================================================

fn generate_id(by_id: &HashMap<JsString, usize>) -> Result<JsString, SessionManagerError> {
    generate_id_with(|id| by_id.contains_key(id), &mut random_uuid).map_err(|error| Box::new(error).into())
}

fn generate_id_for_ids(ids: &HashMap<JsString, ()>) -> Result<JsString, SessionManagerError> {
    generate_id_with(|id| ids.contains_key(id), &mut random_uuid).map_err(|error| Box::new(error).into())
}

fn generate_id_with(
    contains: impl Fn(&JsString) -> bool,
    random: &mut impl FnMut() -> Result<String, pi_js_compat::node_error::NativeNodeError>,
) -> Result<JsString, pi_js_compat::node_error::NativeNodeError> {
    for _ in 0..100 {
        let candidate: JsString = random()?.chars().take(8).collect::<String>().into();
        if !contains(&candidate) {
            return Ok(candidate);
        }
    }
    Ok(random()?.into())
}

fn is_ascii_id_unit(unit: u16) -> bool {
    unit <= 127 && (unit as u8).is_ascii_alphanumeric()
}

/// Validate a caller-provided session id.
pub fn assert_valid_session_id(id: &JsString) -> Result<(), SessionManagerError> {
    let bytes = id.units();
    let valid = !bytes.is_empty()
        && is_ascii_id_unit(bytes[0])
        && is_ascii_id_unit(bytes[bytes.len() - 1])
        && bytes
            .iter()
            .all(|byte| is_ascii_id_unit(*byte) || matches!(*byte, 45 | 95 | 46));
    if valid {
        Ok(())
    } else {
        Err(error(
            "Session id must be non-empty, contain only alphanumeric characters, '-', '_', and '.', and start and end with an alphanumeric character",
        ))
    }
}

fn migrate_v1_to_v2(entries: &mut [FileEntry]) -> Result<(), SessionManagerError> {
    migrate_v1_to_v2_with(entries, random_uuid).map_err(|error| Box::new(error).into())
}

fn migrate_v1_to_v2_with(
    entries: &mut [FileEntry],
    mut random: impl FnMut() -> Result<String, pi_js_compat::node_error::NativeNodeError>,
) -> Result<(), pi_js_compat::node_error::NativeNodeError> {
    let mut ids = HashMap::new();
    let mut previous_id = None;
    for position in 0..entries.len() {
        let first_kept_index = match &mut entries[position] {
            FileEntry::Header(header) => {
                header.set_version(2.0);
                continue;
            }
            FileEntry::Json(_) => continue,
            FileEntry::Entry(entry) => {
                let id = generate_id_with(|id| ids.contains_key(id), &mut random)?;
                ids.insert(id.clone(), ());
                entry.set_id_parent(id.clone(), previous_id);
                previous_id = Some(id);
                if entry.entry_type() != "compaction" {
                    continue;
                }
                entry.object_mut().get("firstKeptEntryIndex").and_then(JsValue::as_f64)
            }
        };
        if let Some(index) = first_kept_index {
            migrate_first_kept_entry(entries, position, index);
        }
    }
    Ok(())
}

fn migrate_first_kept_entry(entries: &mut [FileEntry], position: usize, index: f64) {
    // JS array property access does not truncate fractional/negative indices.
    let target = (index >= 0.0 && index.fract() == 0.0 && index < entries.len() as f64)
        .then(|| entries[index as usize].entry())
        .flatten();
    let target_id = target.map(|entry| entry.id().cloned());
    let FileEntry::Entry(entry) = &mut entries[position] else {
        unreachable!("compaction entry")
    };
    let object = entry.object_mut();
    if let Some(target_id) = target_id {
        match target_id {
            Some(id) => {
                object.insert("firstKeptEntryId".into(), JsValue::String(id));
            }
            None => {
                object.shift_remove("firstKeptEntryId");
            }
        }
    }
    object.shift_remove("firstKeptEntryIndex");
}

fn migrate_v2_to_v3(entries: &mut [FileEntry]) {
    for entry in entries.iter_mut() {
        match entry {
            FileEntry::Json(_) => {}
            FileEntry::Header(header) => header.set_version(3.0),
            FileEntry::Entry(entry) => entry.set_hook_role_to_custom(),
        }
    }
}

/// Run all migrations to the current version. Returns whether anything changed.
pub fn migrate_to_current_version(entries: &mut [FileEntry]) -> Result<bool, SessionManagerError> {
    let version = entries
        .iter()
        .find_map(|entry| match entry {
            FileEntry::Header(header) => Some(header.version().unwrap_or(1.0)),
            FileEntry::Entry(_) | FileEntry::Json(_) => None,
        })
        .unwrap_or(1.0);

    if version >= CURRENT_SESSION_VERSION {
        return Ok(false);
    }
    if version < 2.0 {
        migrate_v1_to_v2(entries)?;
    }
    if version < 3.0 {
        migrate_v2_to_v3(entries);
    }
    Ok(true)
}

/// Migrate session entries in place (exported for tests).
pub fn migrate_session_entries(entries: &mut [FileEntry]) -> Result<(), SessionManagerError> {
    migrate_to_current_version(entries)?;
    Ok(())
}

/// Parse JSONL content, skipping malformed lines.
pub fn parse_session_entries(content: impl Into<JsString>) -> Vec<FileEntry> {
    content
        .into()
        .trim()
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| parse_session_file_entry(line).ok())
        .collect()
}

/// Latest compaction entry in the list.
pub fn get_latest_compaction_entry(entries: &[SessionEntry]) -> Option<&SessionEntry> {
    entries.iter().rev().find(|entry| entry.entry_type() == "compaction")
}

fn build_entry_index(entries: &[SessionEntry]) -> HashMap<JsString, usize> {
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
        LeafSelection::Id(id) if !id.is_empty() => index.get(id).and_then(|position| entries.get(*position)),
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
        let Some(parent_id) = current.parent_id().filter(|id| !id.is_empty()) else {
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

fn context_settings(path: &[&SessionEntry]) -> (JsString, Option<SessionModel>) {
    let mut thinking_level = "off".into();
    let mut model = None;
    for entry in path {
        let object = entry.object();
        if entry.entry_type() == "thinking_level_change" {
            if let Some(value) = object.get("thinkingLevel").and_then(JsValue::as_string) {
                thinking_level = value.clone();
            }
        } else if entry.entry_type() == "model_change" {
            model = context_model(&object, "modelId");
        } else if entry.entry_type() == "message"
            && let Some(message) = object.get("message").and_then(JsValue::as_object)
            && message.get("role").and_then(JsValue::as_string) == Some(&JsString::from("assistant"))
        {
            model = context_model(message, "model");
        }
    }
    (thinking_level, model)
}

fn context_model(object: &JsObject, model_key: &str) -> Option<SessionModel> {
    Some(SessionModel {
        provider: object.get("provider")?.as_string()?.clone(),
        model_id: object.get(model_key)?.as_string()?.clone(),
    })
}

fn record_timestamp_millis(timestamp: &JsString) -> Option<i64> {
    // A lone surrogate cannot occur in any valid Date.parse grammar.
    timestamp
        .to_utf8()
        .ok()
        .and_then(|text| pi_js_compat::date::date_parse_millis(&text))
}

fn context_projection_fields(
    entry_type: &JsString,
    object: &JsObject,
) -> Option<(&'static str, &'static [&'static str])> {
    let projection: (&str, &[&str]) = if entry_type == "custom_message" {
        ("custom", &["customType", "content", "display", "details"])
    } else if entry_type == "branch_summary" && object.get("summary").is_some_and(pi_js_compat::js_json::is_truthy) {
        ("branchSummary", &["summary", "fromId"])
    } else if entry_type == "compaction" {
        (
            "compactionSummary",
            &["summary", "tokensBefore", "durationMs", "providerNative"],
        )
    } else {
        return None;
    };
    Some(projection)
}

fn message_from_context_entry(entry: &SessionEntry) -> Option<AgentMessage> {
    if let Some(message) = entry.message() {
        return Some(message.into_owned());
    }
    let object = entry.object();
    let (role, fields) = context_projection_fields(&entry.entry_type(), &object)?;
    let mut message = JsObject::new();
    message.insert("role".into(), JsValue::String(role.into()));
    for key in fields {
        if let Some(value) = object.get(*key) {
            message.insert((*key).into(), value.clone());
        }
    }
    let timestamp = entry
        .timestamp()
        .and_then(record_timestamp_millis)
        .map(|value| JsValue::Number(value as f64))
        .unwrap_or(JsValue::Null);
    message.insert("timestamp".into(), timestamp);
    Some(AgentMessage::Custom(message))
}

/// Project a record without normalizing missing fields or interpreting provider-native tags.
pub fn session_entry_to_context_messages(entry: &SessionEntry) -> Vec<AgentMessage> {
    message_from_context_entry(entry).into_iter().collect()
}

fn json_bytes(messages: &[AgentMessage]) -> usize {
    serde_json::to_vec(messages).map(|bytes| bytes.len()).unwrap_or(0)
}

fn first_text_block(content: &[TextOrImageContent]) -> Option<&JsString> {
    content.iter().find_map(|block| match block {
        TextOrImageContent::Text(text) => Some(&text.text),
        TextOrImageContent::Image(_) => None,
    })
}

fn get_message_text(message: &AgentMessage) -> Option<JsString> {
    match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => Some(text.clone()),
            UserContent::Blocks(blocks) => first_text_block(blocks).cloned(),
        },
        AgentMessage::Assistant(assistant) => assistant.content.iter().find_map(|block| match block {
            pi_ai::types::AssistantContent::Text(text) => Some(text.text.clone()),
            _ => None,
        }),
        AgentMessage::ToolResult(result) => first_text_block(&result.content).cloned(),
        AgentMessage::Custom(object) => custom_message_text(object),
    }
}

fn custom_message_text(object: &pi_js_compat::js_json::JsObject) -> Option<JsString> {
    match object.get("content")? {
        JsValue::String(text) => Some(text.clone()),
        JsValue::Array(items) => items.iter().find_map(|item| {
            let object = item.as_object()?;
            if object.get("type").and_then(JsValue::as_string) != Some(&JsString::from("text")) {
                return None;
            }
            object.get("text").and_then(JsValue::as_string).cloned()
        }),
        _ => None,
    }
}

fn utf16_len(text: &JsString) -> usize {
    text.len()
}

/// JavaScript `text.slice(0, length)` retains exact UTF-16 units.
fn utf16_prefix(text: &JsString, length: usize) -> JsString {
    JsString::from_utf16(text.units()[..length.min(text.len())].to_vec())
}

fn clone_message_with_text(message: &AgentMessage, text: &JsString) -> Option<AgentMessage> {
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
                JsValue::String(value) => *value = text.to_owned(),
                JsValue::Array(items) => {
                    let item = items.iter_mut().find_map(|item| {
                        let object = item.as_object_mut()?;
                        (object.get("type").and_then(JsValue::as_string) == Some(&JsString::from("text")))
                            .then_some(object)
                    })?;
                    item.insert("text".into(), JsValue::String(text.to_owned()));
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

#[derive(Clone)]
struct ContextMessage {
    message: AgentMessage,
    entry_id: Option<JsString>,
}

fn project_context_message(entry: &SessionEntry) -> Option<ContextMessage> {
    Some(ContextMessage {
        message: message_from_context_entry(entry)?,
        entry_id: (entry.entry_type() == "message").then(|| entry.id().cloned()).flatten(),
    })
}

fn remove_leading_tool_results(messages: Vec<ContextMessage>) -> Vec<ContextMessage> {
    let Some(index) = messages
        .iter()
        .position(|message| message.message.role() != "toolResult")
    else {
        return Vec::new();
    };
    messages.into_iter().skip(index).collect()
}

fn build_capped_kept_messages(entries: &[&SessionEntry]) -> Vec<ContextMessage> {
    let mut kept: Vec<ContextMessage> = Vec::new();
    for entry in entries.iter().rev() {
        let Some(message) = project_context_message(entry) else {
            continue;
        };
        let newer: Vec<_> = kept.iter().map(|message| message.message.clone()).collect();
        let mut candidate = Vec::with_capacity(kept.len() + 1);
        candidate.push(message.message.clone());
        candidate.extend(newer.iter().cloned());
        if json_bytes(&candidate) <= MAX_KEPT_PRE_COMPACTION_BYTES {
            kept.insert(0, message);
            continue;
        }
        if let Some(truncated) = truncate_message_to_fit(&message.message, &newer) {
            // Frozen truncation clones the message, so it is not the stored entry object.
            kept.insert(
                0,
                ContextMessage {
                    message: truncated,
                    entry_id: None,
                },
            );
        }
        break;
    }
    remove_leading_tool_results(kept)
}

fn collect_kept_entries<'a>(
    path: &[&'a SessionEntry],
    first_kept_entry_id: &JsString,
    compaction_index: usize,
) -> Vec<&'a SessionEntry> {
    let mut kept = Vec::new();
    let mut found_first = false;
    for entry in path.iter().take(compaction_index) {
        if entry.id() == Some(first_kept_entry_id) {
            found_first = true;
        }
        if found_first {
            kept.push(*entry);
        }
    }
    kept
}

fn build_compacted_context_messages(path: &[&SessionEntry], compaction: &SessionEntry) -> Vec<ContextMessage> {
    let mut messages: Vec<_> = project_context_message(compaction).into_iter().collect();
    let compaction_index = path.iter().position(|entry| entry.id() == compaction.id()).unwrap_or(0);
    if let Some(first_kept) = compaction.object().get("firstKeptEntryId").and_then(JsValue::as_string) {
        let kept = collect_kept_entries(path, first_kept, compaction_index);
        messages.extend(build_capped_kept_messages(&kept));
    }
    for entry in path.iter().skip(compaction_index + 1) {
        messages.extend(project_context_message(entry));
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
    let first_kept = compaction
        .object()
        .get("firstKeptEntryId")
        .and_then(JsValue::as_string)
        .cloned();
    let mut found_first = false;
    for entry in path.iter().take(compaction_index) {
        if entry.id() == first_kept.as_ref() {
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
    build_session_context_projection(entries, leaf).0
}

fn build_session_context_projection(
    entries: &[SessionEntry],
    leaf: LeafSelection<'_>,
) -> (SessionContext, Vec<Option<JsString>>) {
    let path = build_session_path(entries, leaf);
    let (thinking_level, model) = context_settings(&path);
    let compaction = path
        .iter()
        .rev()
        .find(|entry| entry.entry_type() == "compaction")
        .copied();
    let projected: Vec<_> = match compaction {
        Some(compaction) => build_compacted_context_messages(&path, compaction),
        None => path.iter().filter_map(|entry| project_context_message(entry)).collect(),
    };
    let (messages, origins) = projected
        .into_iter()
        .map(|message| (message.message, message.entry_id))
        .unzip();
    (
        SessionContext {
            messages,
            thinking_level,
            model,
        },
        origins,
    )
}

/// Default session directory for a cwd.
pub fn get_default_session_dir_path(cwd: &JsString, agent_dir: &JsString) -> Result<JsString, SessionManagerError> {
    let resolved_cwd = resolve(cwd)?;
    let resolved_agent_dir = resolve(agent_dir)?;
    let trimmed = resolved_cwd
        .strip_prefix('/')
        .or_else(|| resolved_cwd.strip_prefix('\\'))
        .unwrap_or(resolved_cwd);
    let safe_path = JsString::from("--") + trimmed.replace("/", "-").replace("\\", "-").replace(":", "-") + "--";
    Ok(node_join(node_join(resolved_agent_dir, "sessions"), safe_path))
}

/// Default session directory for a cwd, created when missing.
pub fn get_default_session_dir(cwd: &JsString, agent_dir: Option<&JsString>) -> Result<JsString, SessionManagerError> {
    let agent_dir = match agent_dir {
        Some(directory) => directory.clone(),
        None => get_agent_dir()?,
    };
    let session_dir = get_default_session_dir_path(cwd, &agent_dir)?;
    fs::create_dir_all(encode_node_utf8(&session_dir)).map_err(format_session_error)?;
    Ok(session_dir)
}

fn parse_session_entry_line(line: &str) -> Option<FileEntry> {
    if line.trim().is_empty() {
        return None;
    }
    parse_session_file_entry(line).ok()
}

/// Parse JSONL without projecting message content through scalar-only JSON.
pub(crate) fn parse_session_file_entry(line: impl Into<JsString>) -> Result<FileEntry, SessionManagerError> {
    pi_js_compat::js_json::parse_lossless(line)
        .map_err(|cause| error(cause.into_message()))
        .and_then(|value| pi_js_compat::js_json::from_value(&value).map_err(format_session_error))
}

fn decode_utf8_lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Load every entry from a session file, skipping malformed lines.
pub fn load_entries_from_file(file_path: &JsString) -> Result<Vec<FileEntry>, SessionManagerError> {
    let source = read_session_transcript_path(&normalize(file_path)?);
    if !Path::new(&encode_node_utf8(&source)).exists() {
        return Ok(Vec::new());
    }
    super::session_transcript_spool::with_seekable_session_transcript(&source, |plain| {
        let file = File::open(encode_node_utf8(&plain)).map_err(|cause| filesystem_error(cause, "open", plain))?;
        let mut entries = Vec::new();
        for line in BufReader::with_capacity(SESSION_READ_BUFFER_SIZE, file).split(b'\n') {
            let line = line.map_err(|cause| filesystem_error(cause, "read", plain))?;
            if let Some(entry) = parse_session_entry_line(&decode_utf8_lossy(&line)) {
                entries.push(entry);
            }
        }
        if entries.first().is_some_and(|e| !matches!(e, FileEntry::Header(_))) {
            entries.clear();
        }
        Ok(entries)
    })
    .map_err(SessionManagerError::from)
}

pub struct LoadedSessionContextEntries {
    pub header: SessionHeader,
    pub entries: Vec<SessionEntry>,
}
/// Current context projection; source requires an existing actual filename here.
pub fn load_session_context_entries(file_path: &JsString) -> Result<LoadedSessionContextEntries, SessionManagerError> {
    let source = resolve(file_path)?;
    if !Path::new(&encode_node_utf8(&source)).exists() {
        return Err(error(JsString::from("Session file does not exist: ") + source));
    }
    let mut loaded = load_session_file_entries(&source)?.file_entries;
    if !matches!(loaded.first(), Some(FileEntry::Header(_))) {
        return Err(error(JsString::from("Session file has no valid header: ") + source));
    }
    migrate_to_current_version(&mut loaded)?;
    let FileEntry::Header(header) = loaded.remove(0) else {
        unreachable!("validated header");
    };
    let entries = loaded
        .into_iter()
        .filter_map(|e| match e {
            FileEntry::Entry(entry) => Some(entry),
            _ => None,
        })
        .collect::<Vec<_>>();
    Ok(LoadedSessionContextEntries {
        header,
        entries: build_context_entries(&entries, LeafSelection::Last),
    })
}

fn read_first_line(file_path: &JsString) -> Option<String> {
    super::session_transcript_spool::with_seekable_session_transcript(file_path, |plain| {
        Ok(read_plain_first_line(plain))
    })
    .ok()
    .flatten()
}
fn read_plain_first_line(file_path: &JsString) -> Option<String> {
    let mut file = create_session_transcript_read_stream(file_path).ok()?;
    let mut chunks: Vec<u8> = Vec::new();
    let mut buffer = [0_u8; HEADER_READ_BUFFER_SIZE];
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
            }
        }
    }
    if chunks.is_empty() {
        return None;
    }
    Some(decode_utf8_lossy(&chunks))
}

/// Read only the session header.
pub fn read_session_header(file_path: &JsString) -> Option<SessionHeader> {
    let line = read_first_line(file_path)?;
    let header: SessionHeader = serde_json::from_str(&line).ok()?;
    header.fields.get("id").and_then(JsValue::as_string)?;
    Some(header)
}

fn session_cwd_matches(cwd: Option<&JsString>, resolved_cwd: &JsString) -> bool {
    cwd.is_some_and(|cwd| !cwd.is_empty() && resolve(cwd).is_ok_and(|resolved| resolved == *resolved_cwd))
}

/// Find the most recently modified valid session in a directory.
pub fn find_most_recent_session(session_dir: &JsString, cwd: Option<&JsString>) -> Option<JsString> {
    let resolved_session_dir = normalize(session_dir).ok()?;
    let resolved_cwd = match cwd.filter(|cwd| !cwd.is_empty()) {
        Some(cwd) => Some(resolve(cwd).ok()?),
        None => None,
    };
    let mut entries = fs::read_dir(encode_node_utf8(&resolved_session_dir))
        .ok()?
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut files: Vec<(JsString, f64)> = Vec::new();
    for entry in entries {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.ends_with(".jsonl") {
            continue;
        }
        let path = node_join(&resolved_session_dir, file_name.as_str());
        let Some(header) = read_session_header(&path) else {
            continue;
        };
        if let Some(resolved_cwd) = resolved_cwd.as_ref()
            && !session_cwd_matches(header.cwd(), resolved_cwd)
        {
            continue;
        }
        let mtime = system_time_date(entry.metadata().ok()?.modified().ok()?);
        files.push((path, mtime));
    }
    files.sort_by(|left, right| right.1.total_cmp(&left.1));
    files.into_iter().next().map(|(path, _)| path)
}

// =============================================================================
// Reverse scan for current-version sessions
// =============================================================================

struct ReverseSessionScan {
    has_inactive_entries: bool,
    active_path: Vec<SessionEntry>,
    is_reading_leaf: bool,
    required_parent_id: Option<JsString>,
    latest_compaction: Option<(JsString, JsString)>,
    found_first_kept_entry: bool,
    preceding_cwd: Option<JsString>,
    allow_incomplete_trailing_entry: bool,
}

struct LoadedSessionEntries {
    entries: Vec<SessionEntry>,
    preceding_cwd: Option<JsString>,
}

struct LoadedCurrentVersionSession {
    file_entries: Vec<FileEntry>,
    preceding_cwd: Option<JsString>,
}

impl ReverseSessionScan {
    fn new() -> Self {
        Self {
            has_inactive_entries: false,
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
    file_path: &JsString,
) -> Result<Option<SessionEntry>, SessionManagerError> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    match parse_session_file_entry(line) {
        Ok(parsed) => {
            scan.allow_incomplete_trailing_entry = false;
            match parsed {
                FileEntry::Header(_) | FileEntry::Json(_) => Ok(None),
                FileEntry::Entry(entry) => Ok(Some(entry)),
            }
        }
        Err(_) if scan.allow_incomplete_trailing_entry => {
            scan.allow_incomplete_trailing_entry = false;
            Ok(None)
        }
        Err(_) => Err(error(
            JsString::from("Session file contains malformed JSONL entry: ") + file_path,
        )),
    }
}

fn cwd_from_changed_entry(entry: &SessionEntry) -> Option<JsString> {
    if entry.entry_type() != "custom_message" {
        return None;
    }
    let object = entry.object();
    if object.get("customType").and_then(JsValue::as_string) != Some(&JsString::from("cwd_changed")) {
        return None;
    }
    object.get("details")?.get("cwd")?.as_string().cloned()
}

fn scan_session_entry_reverse(scan: &mut ReverseSessionScan, entry: SessionEntry) -> bool {
    let entry_id = entry.id().unwrap_or_else(|| empty_js_string()).to_owned();
    let parent_id = entry.parent_id().cloned();

    let is_active_entry = scan.is_reading_leaf || scan.required_parent_id.as_ref() == Some(&entry_id);
    if !is_active_entry {
        scan.has_inactive_entries = true;
        return false;
    }

    scan.is_reading_leaf = false;
    if !scan.found_first_kept_entry {
        scan.active_path.push(entry.clone());
    }
    scan.required_parent_id = parent_id;
    if scan.latest_compaction.is_none()
        && entry.entry_type() == "compaction"
        && let Some(first_kept) = entry.object().get("firstKeptEntryId").and_then(JsValue::as_string)
    {
        scan.latest_compaction = Some((entry_id.clone(), first_kept.clone()));
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
    file_path: &JsString,
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

fn validate_complete_reverse_scan(scan: &ReverseSessionScan, file_path: &JsString) -> Result<(), SessionManagerError> {
    let has_unresolved_active_parent = !scan.active_path.is_empty() && scan.required_parent_id.is_some();
    if has_unresolved_active_parent && !scan.found_first_kept_entry {
        return Err(error(
            JsString::from("Session file contains a broken active parent chain: ") + file_path,
        ));
    }
    if let Some((id, first_kept)) = &scan.latest_compaction
        && !scan.found_first_kept_entry
    {
        return Err(error(
            JsString::from("Session compaction ")
                + id
                + " references missing firstKeptEntryId "
                + first_kept
                + ": "
                + file_path,
        ));
    }
    Ok(())
}

fn read_current_version_session_entries(
    file: &mut File,
    file_size: u64,
    file_path: &JsString,
) -> Result<Option<LoadedSessionEntries>, SessionManagerError> {
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
            return Ok(Some(loaded));
        }
        pending = remaining;
    }

    let first_line = decode_utf8_lossy(&pending);
    if let Some(entry) = parse_reverse_session_entry(&mut scan, &first_line, file_path)?
        && scan_session_entry_reverse(&mut scan, entry)
    {
        return Ok(Some(scan.loaded_active_slice()));
    }
    validate_complete_reverse_scan(&scan, file_path)?;
    if scan.found_first_kept_entry || !scan.has_inactive_entries {
        return Ok(Some(scan.loaded_active_slice()));
    }
    Ok(None)
}

fn load_current_version_entries_from_file(
    file_path: &JsString,
    header: SessionHeader,
) -> Result<LoadedCurrentVersionSession, SessionManagerError> {
    super::session_transcript_spool::with_seekable_session_transcript(file_path, |plain| {
        let mut file =
            File::open(encode_node_utf8(&plain)).map_err(|error| SessionManagerError::new(error.to_string()))?;
        let file_size = file
            .metadata()
            .map_err(|error| SessionManagerError::new(error.to_string()))?
            .len();
        let Some(loaded) = read_current_version_session_entries(&mut file, file_size, file_path)? else {
            return Ok(LoadedCurrentVersionSession {
                file_entries: load_entries_from_file(plain)?,
                preceding_cwd: None,
            });
        };
        let mut file_entries = vec![FileEntry::Header(header)];
        file_entries.extend(loaded.entries.into_iter().map(FileEntry::Entry));
        Ok(LoadedCurrentVersionSession {
            file_entries,
            preceding_cwd: loaded.preceding_cwd,
        })
    })
    .map_err(SessionManagerError::from)
}

fn load_session_file_entries(file_path: &JsString) -> Result<LoadedCurrentVersionSession, SessionManagerError> {
    super::session_transcript_spool::with_seekable_session_transcript(file_path, |plain| {
        let header = read_session_header(plain);
        if header.as_ref().and_then(|header| header.version()) == Some(CURRENT_SESSION_VERSION)
            && let Some(header) = header
        {
            return load_current_version_entries_from_file(plain, header)
                .map_err(pi_js_compat::callback_error::CallbackError::from);
        }
        Ok(LoadedCurrentVersionSession {
            file_entries: load_entries_from_file(plain)?,
            preceding_cwd: None,
        })
    })
    .map_err(SessionManagerError::from)
}

// =============================================================================
// Session info
// =============================================================================

#[derive(Default)]
struct SessionInfoAccumulator {
    message_count: usize,
    first_message: JsString,
    last_activity_time: Option<f64>,
}

fn message_input_source(message: &AgentMessage) -> Option<JsString> {
    match message {
        AgentMessage::User(user) => user.input_source.map(|source| match source {
            pi_ai::types::InputSource::Interactive => JsString::from("interactive"),
            pi_ai::types::InputSource::Rpc => JsString::from("rpc"),
            pi_ai::types::InputSource::Extension => JsString::from("extension"),
        }),
        AgentMessage::Custom(object) => object.get("inputSource").and_then(JsValue::as_string).cloned(),
        _ => None,
    }
}

fn extract_text_content(message: &AgentMessage) -> JsString {
    match message {
        AgentMessage::User(user) => match &user.content {
            UserContent::Text(text) => text.clone(),
            UserContent::Blocks(blocks) => text_blocks_joined(blocks),
        },
        AgentMessage::Assistant(assistant) => JsString::join(
            assistant.content.iter().filter_map(|block| match block {
                pi_ai::types::AssistantContent::Text(text) => Some(&text.text),
                _ => None,
            }),
            " ",
        ),
        AgentMessage::ToolResult(result) => text_blocks_joined(&result.content),
        AgentMessage::Custom(object) => match object.get("content") {
            Some(JsValue::String(text)) => text.clone(),
            Some(JsValue::Array(items)) => JsString::join(
                items.iter().filter_map(|item| {
                    let object = item.as_object()?;
                    if object.get("type").and_then(JsValue::as_string) != Some(&JsString::from("text")) {
                        return None;
                    }
                    object.get("text").and_then(JsValue::as_string)
                }),
                " ",
            ),
            _ => JsString::default(),
        },
    }
}

fn text_blocks_joined(blocks: &[TextOrImageContent]) -> JsString {
    JsString::join(
        blocks.iter().filter_map(|block| match block {
            TextOrImageContent::Text(text) => Some(&text.text),
            TextOrImageContent::Image(_) => None,
        }),
        " ",
    )
}

fn message_has_content(message: &AgentMessage) -> bool {
    match message {
        AgentMessage::User(_) | AgentMessage::Assistant(_) | AgentMessage::ToolResult(_) => true,
        AgentMessage::Custom(object) => object.contains_key("content"),
    }
}

fn message_activity_time(entry: &SessionMessageEntry) -> Option<f64> {
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
        && let Some(timestamp) = object.get("timestamp").and_then(JsValue::as_f64)
    {
        return Some(timestamp);
    }
    record_timestamp_millis(&entry.timestamp).map(|millis| millis as f64)
}

/// `affectsSessionInfo`: entries whose append refreshes control-db metadata.
fn affects_session_info(entry: &SessionEntry) -> bool {
    entry.entry_type() == "message"
}

fn apply_session_info_entry(accumulator: &mut SessionInfoAccumulator, entry: &SessionEntry) {
    let Some(message_entry) = entry.as_message() else {
        return;
    };
    accumulator.message_count += 1;

    if let Some(activity_time) = message_activity_time(&message_entry) {
        let previous = accumulator.last_activity_time.unwrap_or(0.0);
        accumulator.last_activity_time = Some(if previous.is_nan() || activity_time.is_nan() {
            f64::NAN
        } else {
            previous.max(activity_time)
        });
    }

    let role = message_entry.message.role();
    if role != "user"
        || message_input_source(&message_entry.message) == Some(JsString::from("extension"))
        || !accumulator.first_message.is_empty()
    {
        return;
    }
    let text = extract_text_content(&message_entry.message);
    if text.is_empty() {
        return;
    }
    accumulator.first_message = text;
}

fn new_date_value(value: Option<&JsValue>) -> Result<f64, SessionManagerError> {
    match value {
        None => Ok(f64::NAN),
        Some(JsValue::Null) => Ok(0.0),
        Some(JsValue::Bool(value)) => Ok(if *value { 1.0 } else { 0.0 }),
        Some(JsValue::Number(value)) => Ok(pi_js_compat::date::time_clip(*value)),
        Some(value) => {
            let text = pi_js_compat::js_json::to_js_string(value).map_err(|message| SessionManagerError {
                name: "TypeError",
                message: message.into(),
                config: None,
                control_db: None,
                native: None,
                callback: None,
            })?;
            Ok(pi_js_compat::date::date_parse_utf16(text.units()))
        }
    }
}

fn system_time_date(value: std::time::SystemTime) -> f64 {
    let millis = match value.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64() * 1000.0,
        Err(error) => -error.duration().as_secs_f64() * 1000.0,
    };
    // Node fs Stats rounds mtimeMs before constructing its Date property.
    pi_js_compat::date::time_clip((millis + 0.5).floor())
}

fn finish_session_info(
    file_path: &JsString,
    header: &SessionHeader,
    accumulator: SessionInfoAccumulator,
    fallback_modified: f64,
) -> Result<SessionInfo, SessionManagerError> {
    let cwd = header.cwd().cloned().unwrap_or_default();
    let header_time = record_timestamp_millis(header.timestamp());
    let modified = match accumulator.last_activity_time {
        Some(time) if time > 0.0 => pi_js_compat::date::time_clip(time),
        _ => header_time.map_or(fallback_modified, |time| time as f64),
    };
    Ok(SessionInfo {
        path: file_path.to_owned(),
        id: header.id().clone(),
        cwd,
        name: None,
        is_archived: None,
        parent_session_path: header.parent_session().cloned(),
        created: new_date_value(header.fields.get("timestamp"))?,
        modified,
        message_count: accumulator.message_count,
        first_message: if accumulator.first_message.is_empty() {
            "(no messages)".into()
        } else {
            accumulator.first_message
        },
    })
}

pub fn build_session_info(file_path: &JsString) -> Option<SessionInfo> {
    let metadata = fs::metadata(encode_node_utf8(file_path)).ok()?;
    let fallback_modified = system_time_date(metadata.modified().ok()?);
    let content = BufReader::new(create_session_transcript_read_stream(file_path).ok()?);
    let mut header: Option<SessionHeader> = None;
    let mut accumulator = SessionInfoAccumulator::default();
    for line in content.split(b'\n') {
        let line = decode_utf8_lossy(&line.ok()?);
        let line = line.strip_suffix('\r').unwrap_or(&line);
        let Some(entry) = parse_session_entry_line(line) else {
            continue;
        };
        match entry {
            FileEntry::Json(_) => {}
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
    finish_session_info(file_path, &header, accumulator, fallback_modified).ok()
}

fn list_sessions_from_dir(
    dir: &JsString,
    progress: Option<&mut dyn FnMut(usize, usize)>,
    progress_offset: usize,
    progress_total: Option<usize>,
) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let Ok(entries) = fs::read_dir(encode_node_utf8(dir)) else {
        return sessions;
    };
    let mut files: Vec<JsString> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(".jsonl").then(|| node_join(dir, name.as_str()))
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
    sort_session_infos(&mut sessions);
    sessions
}

fn sort_session_infos(sessions: &mut [SessionInfo]) {
    pi_js_compat::array_sort::js_array_sort(sessions, |left, right| right.modified - left.modified);
}

fn list_default_session_root(
    progress: Option<&mut dyn FnMut(usize, usize)>,
) -> Result<Vec<SessionInfo>, SessionManagerError> {
    let sessions_dir = get_sessions_dir()?;
    let Ok(entries) = fs::read_dir(encode_node_utf8(&sessions_dir)) else {
        return Ok(Vec::new());
    };
    let mut files: Vec<JsString> = Vec::new();
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
                files.push(dir.join(name).display().to_string().into());
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
    sort_session_infos(&mut sessions);
    Ok(sessions)
}

fn load_fork_source_entries(source_path: &JsString) -> Result<Vec<FileEntry>, SessionManagerError> {
    let entries = load_session_file_entries(source_path)?.file_entries;
    if entries.is_empty() {
        return Err(error(
            JsString::from("Cannot fork: source session file is empty or invalid: ") + source_path,
        ));
    }
    if !entries.iter().any(|entry| matches!(entry, FileEntry::Header(_))) {
        return Err(error(
            JsString::from("Cannot fork: source session has no header: ") + source_path,
        ));
    }
    Ok(entries)
}

fn fork_session_file_path(dir: &JsString, timestamp: &JsString, session_id: &JsString) -> JsString {
    let filename = timestamp.replace(":", "-").replace(".", "-") + "_" + session_id + ".jsonl";
    node_join(dir, filename)
}

fn write_new_session_header(path: &JsString, header: &SessionHeader) -> Result<(), SessionManagerError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(encode_node_utf8(path))
        .map_err(|cause| filesystem_error(cause, "open", path))?;
    let line = serde_json::to_string(header).map_err(|error| SessionManagerError::new(error.to_string()))?;
    writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))
}

fn write_fork_entries(
    path: &JsString,
    source_entries: Vec<FileEntry>,
    selection: Option<ForkEntrySelection>,
) -> Result<(), SessionManagerError> {
    let session_entries: Vec<SessionEntry> = source_entries
        .into_iter()
        .filter_map(|entry| match entry {
            FileEntry::Header(_) | FileEntry::Json(_) => None,
            FileEntry::Entry(entry) => Some(entry),
        })
        .collect();
    let entries_to_copy = select_fork_entries(session_entries, selection);
    let mut file = OpenOptions::new()
        .append(true)
        .open(encode_node_utf8(path))
        .map_err(|error| SessionManagerError::new(error.to_string()))?;
    for (index, entry) in entries_to_copy.into_iter().enumerate() {
        let mut entry = entry;
        if index == 0 {
            entry.set_parent_id(None);
        }
        let line = serialize_session_entry_for_persistence(&FileEntry::Entry(entry))?;
        writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))?;
    }
    Ok(())
}

fn select_fork_entries(entries: Vec<SessionEntry>, selection: Option<ForkEntrySelection>) -> Vec<SessionEntry> {
    match selection {
        Some(ForkEntrySelection::None) => Vec::new(),
        Some(ForkEntrySelection::From(id)) if !id.is_empty() => build_session_path(&entries, LeafSelection::Id(&id))
            .into_iter()
            .cloned()
            .collect(),
        Some(ForkEntrySelection::From(_) | ForkEntrySelection::All) | None => entries,
    }
}

fn build_session_tree(
    entries: &[SessionEntry],
    labels: &[(JsString, JsString)],
    label_timestamps: &[(JsString, JsString)],
) -> Vec<SessionTreeNode> {
    let mut nodes: Vec<Option<SessionTreeNode>> = entries
        .iter()
        .map(|entry| {
            let id = entry.id().unwrap_or_else(|| empty_js_string());
            Some(SessionTreeNode {
                entry: entry.clone(),
                children: Vec::new(),
                label: lookup_label(labels, id).cloned(),
                label_timestamp: lookup_label(label_timestamps, id).cloned(),
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

fn lookup_label<'a>(labels: &'a [(JsString, JsString)], id: &JsString) -> Option<&'a JsString> {
    labels.iter().find(|(target, _)| target == id).map(|(_, value)| value)
}

fn link_tree_positions(entries: &[SessionEntry]) -> (Vec<Vec<usize>>, Vec<usize>) {
    let index: HashMap<&JsString, usize> = entries
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
        children.sort_by(|left, right| {
            let left = left.entry.timestamp().and_then(record_timestamp_millis);
            let right = right.entry.timestamp().and_then(record_timestamp_millis);
            match (left, right) {
                (Some(left), Some(right)) => left.cmp(&right),
                // JS sort treats a NaN comparator result as equality, not epoch zero.
                _ => std::cmp::Ordering::Equal,
            }
        });
        if let Some(mut node) = nodes[*position].take() {
            node.children = children;
            nodes[*position] = Some(node);
        }
    }
}

fn rechain_path_without_labels(path: &[SessionEntry]) -> Vec<SessionEntry> {
    let mut rechain: Vec<SessionEntry> = Vec::new();
    let mut parent_id: Option<JsString> = None;
    for entry in path {
        if entry.as_label().is_some() {
            continue;
        }
        let mut entry = entry.clone();
        entry.set_id_parent(
            entry.id().unwrap_or_else(|| empty_js_string()).to_owned(),
            parent_id.clone(),
        );
        parent_id = entry.id().cloned();
        rechain.push(entry);
    }
    rechain
}

fn entry_id_set(entries: &[SessionEntry]) -> HashMap<JsString, ()> {
    entries
        .iter()
        .filter_map(|entry| entry.id().map(|id| (id.to_owned(), ())))
        .collect()
}

fn build_branch_label_entries(
    labels: &[(JsString, JsString)],
    label_timestamps: &[(JsString, JsString)],
    path_without_labels: &[SessionEntry],
    path_entry_ids: &mut HashMap<JsString, ()>,
) -> Result<Vec<SessionEntry>, SessionManagerError> {
    let labels_to_write: Vec<(JsString, JsString, JsString)> = labels
        .iter()
        .filter(|(target_id, _)| path_entry_ids.contains_key(target_id))
        .map(|(target_id, label)| {
            (
                target_id.clone(),
                label.clone(),
                lookup_label(label_timestamps, target_id).cloned().unwrap_or_default(),
            )
        })
        .collect();

    let mut parent_id = path_without_labels.last().and_then(|entry| entry.id().cloned());
    let mut label_entries = Vec::new();
    for (target_id, label, label_timestamp) in labels_to_write {
        let label_entry = SessionEntry::Known(KnownSessionEntry::Label(LabelEntry {
            id: generate_id_for_ids(path_entry_ids)?,
            parent_id: parent_id.clone(),
            timestamp: label_timestamp,
            target_id,
            label: Some(label),
        }));
        if let Some(id) = label_entry.id() {
            path_entry_ids.insert(id.to_owned(), ());
        }
        parent_id = label_entry.id().cloned();
        label_entries.push(label_entry);
    }
    Ok(label_entries)
}

// =============================================================================
// SessionManager
// =============================================================================

/// Persisted session settings read from control-db metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersistedSessionSettings {
    pub model: Option<PersistedSessionModel>,
    pub sandbox_profile: Option<crate::core::permissions::presets::SandboxProfileName>,
    pub thinking_level: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedSessionModel {
    pub provider: JsString,
    pub model_id: JsString,
}

/// Options for [`SessionManager::set_metadata_control_db_path`].
#[derive(Debug, Clone, Default)]
pub struct SetMetadataControlDbPathOptions {
    pub index_message_text: Option<bool>,
    pub cwd_override: Option<JsString>,
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
    session_id: JsString,
    session_file: Option<JsString>,
    session_dir: JsString,
    cwd: JsString,
    persist: bool,
    flushed: bool,
    file_entries: Vec<FileEntry>,
    by_id: HashMap<JsString, usize>,
    message_identities: HashMap<JsString, AgentMessageIdentity>,
    labels_by_id: Vec<(JsString, JsString)>,
    label_timestamps_by_id: Vec<(JsString, JsString)>,
    leaf_id: Option<JsString>,
    is_subagent: bool,
    subagent_name: Option<JsString>,
    metadata_control_db_path: Option<JsString>,
    index_message_text: bool,
    in_memory_goal_json: Option<JsString>,
    session_name: Option<JsString>,
    has_stored_session_name: bool,
}

impl crate::core::multi_agent_store::SessionPersistence for SessionManager {
    fn metadata_control_db_path(&self) -> Option<JsString> {
        self.get_metadata_control_db_path().cloned()
    }

    fn session_file(&self) -> Option<JsString> {
        self.get_session_file().cloned()
    }
}

impl SessionManager {
    fn new(
        cwd: &JsString,
        session_dir: &JsString,
        session_file: Option<&JsString>,
        persist: bool,
        options: Option<NewSessionOptions>,
    ) -> Result<Self, SessionManagerError> {
        let mut manager = Self {
            session_id: JsString::default(),
            session_file: None,
            session_dir: normalize(session_dir)?,
            cwd: resolve(cwd)?,
            persist,
            flushed: false,
            file_entries: Vec::new(),
            by_id: HashMap::new(),
            message_identities: HashMap::new(),
            labels_by_id: Vec::new(),
            label_timestamps_by_id: Vec::new(),
            leaf_id: None,
            is_subagent: false,
            subagent_name: None,
            metadata_control_db_path: None,
            index_message_text: true,
            in_memory_goal_json: None,
            session_name: None,
            has_stored_session_name: false,
        };
        if persist && !manager.session_dir.is_empty() && !Path::new(&encode_node_utf8(&manager.session_dir)).exists() {
            fs::create_dir_all(encode_node_utf8(&manager.session_dir))
                .map_err(|cause| filesystem_error(cause, "mkdir", &manager.session_dir))?;
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
        cwd: &JsString,
        session_dir: Option<&JsString>,
        options: Option<NewSessionOptions>,
    ) -> Result<Self, SessionManagerError> {
        let dir = match session_dir.filter(|dir| !dir.is_empty()) {
            Some(dir) => normalize(dir)?,
            None => get_default_session_dir(cwd, None)?,
        };
        Self::new(cwd, &dir, None, true, options)
    }

    /// Open a specific session file.
    pub fn open(
        path: &JsString,
        session_dir: Option<&JsString>,
        cwd_override: Option<&JsString>,
    ) -> Result<Self, SessionManagerError> {
        let resolved_path = read_session_transcript_path(&resolve(path)?);
        assert_unarchived_session_file(&resolved_path)?;
        let header = read_session_header(&resolved_path);
        let cwd = match cwd_override {
            Some(cwd) => cwd.to_owned(),
            None => header
                .as_ref()
                .and_then(|header| header.cwd().cloned())
                .map(Ok)
                .unwrap_or_else(current_cwd)?,
        };
        let dir = match session_dir.filter(|dir| !dir.is_empty()) {
            Some(dir) => normalize(dir)?,
            None => node_dirname(&resolved_path),
        };
        let mut manager = Self::new(&cwd, &dir, Some(&resolved_path), true, None)?;
        if let Some(cwd_override) = cwd_override.filter(|cwd| !cwd.is_empty()) {
            manager.cwd = resolve(cwd_override)?;
        }
        Ok(manager)
    }

    /// Inspect a transcript without restoration, migration writes or metadata mutation.
    pub fn open_read_only(path: &JsString, session_dir: Option<&JsString>) -> Result<Self, SessionManagerError> {
        let source = read_session_transcript_path(&resolve(path)?);
        let loaded = load_session_file_entries(&source)?;
        let Some(FileEntry::Header(header)) = loaded.file_entries.first() else {
            return Err(error(JsString::from("Invalid session file: ") + source));
        };
        let cwd = loaded
            .preceding_cwd
            .as_ref()
            .or(header.cwd())
            .cloned()
            .map(Ok)
            .unwrap_or_else(current_cwd)?;
        let id = header.id().clone();
        let dir = session_dir.cloned().unwrap_or_else(|| node_dirname(&source));
        let mut manager = Self::new(&cwd, &dir, None, false, None)?;
        manager.session_file = Some(source);
        manager.session_id = id;
        manager.file_entries = loaded.file_entries;
        migrate_to_current_version(&mut manager.file_entries)?;
        manager.build_index();
        Ok(manager)
    }

    /// Continue the most recent session, or create a new one.
    pub fn continue_recent(cwd: &JsString, session_dir: Option<&JsString>) -> Result<Self, SessionManagerError> {
        let dir = match session_dir.filter(|dir| !dir.is_empty()) {
            Some(dir) => normalize(dir)?,
            None => get_default_session_dir(cwd, None)?,
        };
        let filter_cwd = session_dir.is_some() && !uses_default_session_dir_for(cwd, &dir)?;
        let most_recent = find_most_recent_session(&dir, filter_cwd.then_some(cwd));
        match most_recent {
            Some(path) => Self::new(cwd, &dir, Some(&path), true, None),
            None => Self::new(cwd, &dir, None, true, None),
        }
    }

    /// Create an in-memory session without file persistence.
    pub fn in_memory(cwd: Option<&JsString>, options: Option<NewSessionOptions>) -> Result<Self, SessionManagerError> {
        Self::new(
            &cwd.cloned().map(Ok).unwrap_or_else(current_cwd)?,
            &JsString::default(),
            None,
            false,
            options,
        )
    }

    /// Fork a session into a target project directory.
    pub fn fork_from(
        source_path: &JsString,
        target_cwd: &JsString,
        session_dir: Option<&JsString>,
        options: Option<NewSessionOptions>,
        from_entry: Option<ForkEntrySelection>,
    ) -> Result<Self, SessionManagerError> {
        let resolved_source = resolve(source_path)?;
        let resolved_target_cwd = resolve(target_cwd)?;
        let source_entries = load_fork_source_entries(&resolved_source)?;
        let dir = match session_dir.filter(|dir| !dir.is_empty()) {
            Some(dir) => normalize(dir)?,
            None => get_default_session_dir(&resolved_target_cwd, None)?,
        };
        if !Path::new(&encode_node_utf8(&dir)).exists() {
            fs::create_dir_all(encode_node_utf8(&dir)).map_err(|cause| filesystem_error(cause, "mkdir", &dir))?;
        }

        let options = options.unwrap_or_default();
        if let Some(id) = options.id.as_ref() {
            assert_valid_session_id(id)?;
        }
        let new_session_id = options.id.clone().unwrap_or_else(|| uuidv7().into());
        let timestamp: JsString = iso_time::now_iso_string().into();
        let new_session_file = fork_session_file_path(&dir, &timestamp, &new_session_id);
        let header = SessionHeader::new(
            new_session_id,
            timestamp,
            Some(resolved_target_cwd.clone()),
            Some(resolved_source),
        );
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
    pub fn set_session_file(&mut self, session_file: &JsString) -> Result<(), SessionManagerError> {
        let source = read_session_transcript_path(&resolve(session_file)?);
        self.session_file = Some(source.clone());
        assert_unarchived_session_file(&source)?;
        self.reset_session_name_state();
        let Some(current) = self.session_file.clone() else {
            return Ok(());
        };
        if Path::new(&encode_node_utf8(&current)).exists() {
            self.load_existing_session_file(&current)?;
        } else {
            let explicit_path = self.session_file.clone();
            self.new_session(None)?;
            self.session_file = explicit_path;
        }
        Ok(())
    }

    fn load_existing_session_file(&mut self, path: &JsString) -> Result<(), SessionManagerError> {
        let loaded = load_session_file_entries(path)?;
        self.message_identities.clear();
        self.file_entries = loaded.file_entries;
        if self.file_entries.is_empty() {
            return self.recover_empty_session_file(path);
        }

        let Some(header) = self.header() else {
            return Err(error(JsString::from("Session file is not a valid pi session: ") + path));
        };
        let loaded_cwd = loaded.preceding_cwd.filter(|cwd| !cwd.is_empty());
        if let Some(loaded_cwd) = loaded_cwd {
            self.cwd = resolve(&loaded_cwd)?;
        }
        self.session_id = header.id().clone();
        if migrate_to_current_version(&mut self.file_entries)? {
            self.rewrite_file()?;
        }
        self.build_index();
        self.restore_persisted_metadata()?;
        self.flushed = true;
        Ok(())
    }

    fn recover_empty_session_file(&mut self, path: &JsString) -> Result<(), SessionManagerError> {
        let size = fs::metadata(encode_node_utf8(path))
            .map_err(|error| SessionManagerError::new(error.to_string()))?
            .len();
        if size > 0 {
            return Err(error(JsString::from("Session file is not a valid pi session: ") + path));
        }
        let explicit_path = path.to_owned();
        self.new_session(None)?;
        self.session_file = Some(explicit_path);
        self.rewrite_file()?;
        self.flushed = true;
        Ok(())
    }

    /// Start a new session, replacing in-memory state.
    pub fn new_session(&mut self, options: Option<NewSessionOptions>) -> Result<Option<JsString>, SessionManagerError> {
        let options = options.unwrap_or_default();
        if let Some(id) = options.id.as_ref() {
            assert_valid_session_id(id)?;
        }
        self.session_id = options.id.unwrap_or_else(|| uuidv7().into());
        self.is_subagent = options.is_subagent;
        self.subagent_name = options
            .subagent_name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
        self.reset_session_name_state();
        let timestamp: JsString = iso_time::now_iso_string().into();
        let header = SessionHeader::new(
            self.session_id.clone(),
            timestamp.clone(),
            Some(self.cwd.clone()),
            options.parent_session,
        );
        self.message_identities.clear();
        self.file_entries = vec![FileEntry::Header(header)];
        self.by_id.clear();
        self.labels_by_id.clear();
        self.leaf_id = None;
        self.flushed = false;

        if self.persist {
            let file_timestamp = timestamp.replace(":", "-").replace(".", "-");
            self.session_file = Some(node_join(
                &self.session_dir,
                file_timestamp + "_" + &self.session_id + ".jsonl",
            ));
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
            if entry.entry_type() == "message" {
                self.message_identities.entry(id.clone()).or_default();
            }
            self.leaf_id = Some(id.to_owned());
            if let Some(label) = entry.as_label() {
                label_updates.push((
                    label.target_id.clone(),
                    label.label.as_ref().filter(|label| !label.is_empty()).cloned(),
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

    fn set_label(&mut self, target_id: &JsString, label: &JsString, timestamp: &JsString) {
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

    fn remove_label(&mut self, target_id: &JsString) {
        self.labels_by_id.retain(|(id, _)| id != target_id);
        self.label_timestamps_by_id.retain(|(id, _)| id != target_id);
    }

    fn label(&self, target_id: &JsString) -> Option<&JsString> {
        self.labels_by_id
            .iter()
            .find(|(id, _)| id == target_id)
            .map(|(_, label)| label)
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

    fn write_entries(&self, file_path: &JsString, create_new: bool) -> Result<(), SessionManagerError> {
        let mut options = OpenOptions::new();
        options.write(true);
        if create_new {
            options.create_new(true);
        } else {
            options.create(true).truncate(true);
        }
        let mut file = options
            .open(encode_node_utf8(file_path))
            .map_err(|cause| filesystem_error(cause, "open", file_path))?;
        for entry in &self.file_entries {
            let line = serialize_session_entry_for_persistence(entry)?;
            writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        Ok(())
    }

    fn has_assistant(&self) -> bool {
        self.file_entries.iter().any(|entry| match entry {
            FileEntry::Header(_) | FileEntry::Json(_) => false,
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
                let line = serialize_session_entry_for_persistence(&FileEntry::Entry(entry.clone()))?;
                let mut file = OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(encode_node_utf8(&session_file))
                    .map_err(|cause| filesystem_error(cause, "open", &session_file))?;
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
            let line = serialize_session_entry_for_persistence(&FileEntry::Entry(entry.clone()))?;
            let mut file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(encode_node_utf8(&session_file))
                .map_err(|cause| filesystem_error(cause, "open", &session_file))?;
            writeln!(file, "{line}").map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        Ok(())
    }

    fn append_entry(&mut self, entry: SessionEntry) -> Result<JsString, SessionManagerError> {
        let id = entry.id().unwrap_or_else(|| empty_js_string()).to_owned();
        if entry.entry_type() == "message" {
            self.message_identities.insert(id.clone(), AgentMessageIdentity::new());
        }
        self.file_entries.push(FileEntry::Entry(entry.clone()));
        let position = self.file_entries.len() - 1;
        self.by_id.insert(id.clone(), position);
        self.leaf_id = Some(id.clone());
        self.persist_entry(&entry)?;
        if affects_session_info(&entry) {
            self.write_metadata_snapshot()?;
        }
        Ok(id)
    }

    fn timestamp(&self) -> JsString {
        iso_time::now_iso_string().into()
    }

    pub fn is_persisted(&self) -> bool {
        self.persist
    }

    pub fn get_cwd(&self) -> &JsString {
        &self.cwd
    }

    pub fn get_session_dir(&self) -> &JsString {
        &self.session_dir
    }

    pub fn uses_default_session_dir(&self) -> Result<bool, SessionManagerError> {
        let agent_dir = get_agent_dir()?;
        Ok(get_default_session_dir_path(&self.cwd, &agent_dir)? == self.session_dir)
    }

    pub fn get_session_id(&self) -> &JsString {
        &self.session_id
    }

    pub fn get_session_file(&self) -> Option<&JsString> {
        self.session_file.as_ref()
    }

    pub fn get_metadata_control_db_path(&self) -> Option<&JsString> {
        self.metadata_control_db_path.as_ref()
    }

    /// The control DB and session file when both are set; TS tests each with
    /// truthiness, so empty strings count as unset.
    fn metadata_target(&self) -> Option<(&JsString, &JsString)> {
        let control_db_path = self.metadata_control_db_path.as_ref().filter(|path| !path.is_empty())?;
        let session_file = self.session_file.as_ref().filter(|path| !path.is_empty())?;
        Some((control_db_path, session_file))
    }

    /// `getSessionGoalJson`.
    pub fn get_session_goal_json(&self) -> Result<Option<JsString>, SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(self.in_memory_goal_json.clone());
        };
        read_session_goal(control_db_path, session_file).map_err(control_db_error)
    }

    /// `getSessionGoalJsonForSession`.
    pub fn get_session_goal_json_for_session(
        &self,
        session_file: &JsString,
    ) -> Result<Option<JsString>, SessionManagerError> {
        let Some(control_db_path) = self.metadata_control_db_path.as_ref().filter(|path| !path.is_empty()) else {
            return Ok(None);
        };
        read_session_goal(control_db_path, read_session_transcript_path(&resolve(session_file)?))
            .map_err(control_db_error)
    }

    /// `setSessionGoalJson`.
    pub fn set_session_goal_json(&mut self, goal_json: &JsString) -> Result<(), SessionManagerError> {
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
                Some(PersistedSessionModel {
                    provider: provider.into(),
                    model_id: model_id.into(),
                })
            }
            _ => None,
        };
        Ok(Some(PersistedSessionSettings {
            model,
            sandbox_profile,
            thinking_level: metadata.thinking_level.map(Into::into),
        }))
    }

    /// `setSessionModel`.
    pub fn set_session_model(&mut self, provider: &JsString, model_id: &JsString) -> Result<(), SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(());
        };
        self.write_metadata_snapshot()?;
        write_session_model(
            control_db_path,
            session_file,
            &opaque_metadata(provider)?,
            &opaque_metadata(model_id)?,
        )
        .map_err(control_db_error)
    }

    /// `setSessionThinkingLevel`.
    pub fn set_session_thinking_level(&mut self, thinking_level: &JsString) -> Result<(), SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(());
        };
        self.write_metadata_snapshot()?;
        write_session_thinking_level(control_db_path, session_file, &opaque_metadata(thinking_level)?)
            .map_err(control_db_error)
    }

    /// `setSessionSandboxProfile`. Returns `false` for an unpersisted session.
    pub fn set_session_sandbox_profile(
        &mut self,
        profile: Option<SandboxProfileName>,
    ) -> Result<bool, SessionManagerError> {
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(false);
        };
        let (control_db_path, session_file) = (control_db_path.clone(), session_file.clone());
        self.persist_for_recovery()?;
        self.write_metadata_snapshot()?;
        match profile {
            Some(profile) => write_session_sandbox_profile(&control_db_path, &session_file, &self.session_id, profile)
                .map_err(|error| SessionManagerError::new(error.to_string()))?,
            None => clear_session_sandbox_profile(&control_db_path, &session_file, &self.session_id)
                .map_err(|error| SessionManagerError::new(error.to_string()))?,
        }
        Ok(true)
    }

    /// `setMetadataControlDbPath`.
    pub fn set_metadata_control_db_path(
        &mut self,
        control_db_path: Option<&JsString>,
        options: SetMetadataControlDbPathOptions,
    ) -> Result<(), SessionManagerError> {
        self.metadata_control_db_path = control_db_path.cloned();
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
        let control_db_path = control_db_path.to_owned();
        let session_file = session_file.to_owned();
        let name_state = read_session_name_state(&control_db_path, &session_file).map_err(control_db_error)?;
        self.session_name = name_state.name.map(JsString::from);
        self.has_stored_session_name = name_state.has_stored_value;
        let Some(metadata) = read_session_metadata(&control_db_path, &session_file).map_err(control_db_error)? else {
            return Ok(());
        };
        self.cwd = resolve(metadata.cwd)?;
        if metadata.is_subagent != Some(true) {
            return Ok(());
        }
        self.is_subagent = true;
        self.subagent_name = metadata.subagent_name.map(JsString::from);
        Ok(())
    }

    fn reset_session_name_state(&mut self) {
        self.session_name = None;
        self.has_stored_session_name = false;
    }

    /// `setSessionName`: set or clear the display name.
    pub fn set_session_name(&mut self, name: Option<&JsString>) -> Result<(), SessionManagerError> {
        self.session_name = normalize_session_name(name);
        self.has_stored_session_name = true;
        let Some((control_db_path, session_file)) = self.metadata_target() else {
            return Ok(());
        };
        let control_db_path = control_db_path.to_owned();
        let session_file = session_file.to_owned();
        self.write_metadata_snapshot()?;
        write_session_name(&control_db_path, &session_file, self.session_name.as_ref()).map_err(control_db_error)
    }

    /// `getSessionName`.
    pub fn get_session_name(&self) -> Option<JsString> {
        self.session_name.clone()
    }

    /// `hasSessionNameState`: whether a name was ever stored (including an explicit clear).
    pub fn has_session_name_state(&self) -> bool {
        self.has_stored_session_name
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
        let info = finish_session_info(
            session_file,
            header,
            accumulator,
            pi_js_compat::date::time_clip(pi_js_compat::date::js_now_millis()),
        )?;
        let metadata = WritableSessionMetadata {
            session_path: info.path.clone(),
            id: info.id.clone(),
            cwd: self.cwd.clone(),
            name: None,
            parent_session_path: info.parent_session_path.clone(),
            archived_at: None,
            is_archived: info.is_archived,
            is_subagent: Some(self.is_subagent),
            subagent_name: self.subagent_name.clone(),
            model_provider: None,
            model_id: None,
            thinking_level: None,
            created_at: pi_js_compat::date::to_iso_string_value(info.created)?.into(),
            modified_at: pi_js_compat::date::to_iso_string_value(info.modified)?.into(),
            message_count: i64::try_from(info.message_count).unwrap_or(i64::MAX),
            first_message: info.first_message.clone(),
        };
        write_session_metadata(control_db_path, &metadata).map_err(control_db_error)
    }

    pub fn is_subagent_session(&self) -> bool {
        self.is_subagent
    }

    pub fn get_subagent_name(&self) -> Option<&JsString> {
        self.subagent_name.as_ref()
    }

    pub fn relocate(&mut self, target_cwd: &JsString, agent_dir: Option<&JsString>) -> Result<(), SessionManagerError> {
        let resolved_target_cwd = resolve(target_cwd)?;
        let agent_dir = match agent_dir {
            Some(directory) => directory.clone(),
            None => get_agent_dir()?,
        };
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
                    node_join(&target_session_dir, node_basename(old))
                } else {
                    old.clone()
                }
            })
        };
        let should_copy = destination.is_some() && old_session_file.is_some() && destination != old_session_file;
        if should_copy
            && let Some(destination) = destination.as_ref()
            && Path::new(&encode_node_utf8(destination)).exists()
        {
            return Err(error(
                JsString::from("Cannot relocate session: destination already exists: ") + destination,
            ));
        }
        if should_copy
            && let (Some(old), Some(destination)) = (old_session_file.as_ref(), destination.as_ref())
            && Path::new(&encode_node_utf8(old)).exists()
        {
            let content =
                fs::read(encode_node_utf8(old)).map_err(|error| SessionManagerError::new(error.to_string()))?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(encode_node_utf8(destination))
                .map_err(|error| SessionManagerError::new(error.to_string()))?;
            file.write_all(&content)
                .map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        if let Some(control_db_path) = self.metadata_control_db_path.as_ref().filter(|path| !path.is_empty())
            && let (Some(old), Some(destination)) = (old_session_file.as_ref(), destination.as_ref())
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
            && let Some(old) = old_session_file.as_ref()
            && Path::new(&encode_node_utf8(old)).exists()
        {
            fs::remove_file(encode_node_utf8(&old)).map_err(|error| SessionManagerError::new(error.to_string()))?;
        }
        self.write_metadata_snapshot()
    }

    pub fn get_leaf_id(&self) -> Option<&JsString> {
        self.leaf_id.as_ref()
    }

    pub fn get_leaf_entry(&self) -> Option<&SessionEntry> {
        let id = self.leaf_id.as_ref().filter(|id| !id.is_empty())?;
        self.get_entry(id)
    }

    pub fn get_entry(&self, id: &JsString) -> Option<&SessionEntry> {
        let position = *self.by_id.get(id)?;
        match self.file_entries.get(position)? {
            FileEntry::Entry(entry) => Some(entry),
            FileEntry::Header(_) | FileEntry::Json(_) => None,
        }
    }

    pub fn get_children(&self, parent_id: &JsString) -> Vec<SessionEntry> {
        self.file_entries
            .iter()
            .filter_map(|entry| match entry {
                FileEntry::Entry(entry) if entry.parent_id() == Some(parent_id) => Some(entry.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn get_label(&self, id: &JsString) -> Option<&JsString> {
        self.label(id)
    }

    pub fn append_message(&mut self, message: AgentMessage) -> Result<JsString, SessionManagerError> {
        self.append_message_with_mailbox_store_refs(message, None)
    }

    pub fn append_message_with_mailbox_store_refs(
        &mut self,
        message: AgentMessage,
        mailbox_store_refs: Option<&[super::session_control_db::RuntimeMailboxStoreRef]>,
    ) -> Result<JsString, SessionManagerError> {
        let mailbox_store_refs = mailbox_store_refs
            .map(|refs| {
                let json = serde_json::to_string(refs).map_err(format_session_error)?;
                pi_js_compat::js_json::parse_lossless(&json)
                    .map(PersistedJsonValue)
                    .map_err(format_session_error)
            })
            .transpose()?;
        let entry = SessionEntry::Known(KnownSessionEntry::Message(SessionMessageEntry {
            mailbox_store_refs,
            id: generate_id(&self.by_id)?,
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
            message: Box::new(message),
        }));
        self.append_entry(entry)
    }

    pub fn append_thinking_level_change(&mut self, thinking_level: &JsString) -> Result<JsString, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
            id: generate_id(&self.by_id)?,
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
            thinking_level: thinking_level.to_owned(),
        }));
        self.append_entry(entry)
    }

    pub fn append_model_change(
        &mut self,
        provider: &JsString,
        model_id: &JsString,
    ) -> Result<JsString, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::ModelChange(ModelChangeEntry {
            id: generate_id(&self.by_id)?,
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
        summary: &JsString,
        first_kept_entry_id: &JsString,
        tokens_before: f64,
        options: CompactionAppendOptions,
    ) -> Result<JsString, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::Compaction(CompactionEntry {
            id: generate_id(&self.by_id)?,
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
            summary: summary.to_owned(),
            first_kept_entry_id: first_kept_entry_id.to_owned(),
            tokens_before,
            extra: PersistedJsonObject::default(),
            duration_ms: options.duration_ms,
            details: options.details.map(Box::new),
            provider_native: options.provider_native.map(|value| {
                Box::new(pi_js_compat::js_json::to_value(&value).expect("provider context serializes losslessly"))
            }),
            from_hook: options.from_hook,
            first_kept_entry_index: None,
        }));
        self.append_entry(entry)
    }

    /// Update a compaction summary in place and rewrite the file.
    pub fn update_compaction_summary(
        &mut self,
        entry_id: &JsString,
        summary: &JsString,
    ) -> Result<(), SessionManagerError> {
        let position = *self
            .by_id
            .get(entry_id)
            .ok_or_else(|| error(JsString::from("Entry ") + entry_id + " not found"))?;
        let is_compaction = self.file_entries[position]
            .entry()
            .is_some_and(|entry| entry.entry_type() == "compaction");
        if !is_compaction {
            return Err(error(JsString::from("Entry ") + entry_id + " is not a compaction"));
        }
        let trimmed = summary.trim();
        if trimmed.is_empty() {
            return Err(error("Compaction summary must be non-empty"));
        }
        let previous = self.file_entries[position].clone();
        let FileEntry::Entry(entry) = &mut self.file_entries[position] else {
            return Err(error(JsString::from("Entry ") + entry_id + " is not a compaction"));
        };
        if entry.entry_type() != "compaction" {
            return Err(error(JsString::from("Entry ") + entry_id + " is not a compaction"));
        }
        let object = entry.object_mut();
        object.insert("summary".into(), JsValue::String(trimmed));
        if object
            .get("providerNative")
            .is_some_and(pi_js_compat::js_json::is_truthy)
        {
            object.shift_remove("details");
            object.shift_remove("providerNative");
        }
        if let Err(rewrite_error) = self.rewrite_file() {
            self.file_entries[position] = previous;
            return Err(rewrite_error);
        }
        Ok(())
    }

    pub fn append_custom_entry(
        &mut self,
        custom_type: &JsString,
        data: Option<JsValue>,
    ) -> Result<JsString, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::Custom(CustomEntry {
            custom_type: custom_type.to_owned(),
            data,
            id: generate_id(&self.by_id)?,
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
        }));
        self.append_entry(entry)
    }

    pub fn append_custom_message_entry(
        &mut self,
        custom_type: &JsString,
        content: CustomMessageContent,
        display: bool,
        details: Option<JsValue>,
    ) -> Result<JsString, SessionManagerError> {
        let entry = SessionEntry::Known(KnownSessionEntry::CustomMessage(CustomMessageEntry {
            custom_type: custom_type.to_owned(),
            content,
            display,
            details,
            id: generate_id(&self.by_id)?,
            parent_id: self.leaf_id.clone(),
            timestamp: self.timestamp(),
        }));
        self.append_entry(entry)
    }

    pub fn append_label_change(
        &mut self,
        target_id: &JsString,
        label: Option<&JsString>,
    ) -> Result<JsString, SessionManagerError> {
        if !self.by_id.contains_key(target_id) {
            return Err(error(JsString::from("Entry ") + target_id + " not found"));
        }
        let timestamp = self.timestamp();
        let entry = SessionEntry::Known(KnownSessionEntry::Label(LabelEntry {
            id: generate_id(&self.by_id)?,
            parent_id: self.leaf_id.clone(),
            timestamp: timestamp.clone(),
            target_id: target_id.to_owned(),
            label: label.cloned(),
        }));
        let id = self.append_entry(entry)?;
        match label {
            Some(label) if !label.is_empty() => self.set_label(target_id, label, &timestamp),
            _ => self.remove_label(target_id),
        }
        Ok(id)
    }

    /// Walk from an entry to the root, returning the path in root-first order.
    pub fn get_branch(&self, from_id: Option<&JsString>) -> Vec<SessionEntry> {
        let start_id = from_id.or(self.leaf_id.as_ref()).filter(|id| !id.is_empty());
        let Some(mut current) = start_id.and_then(|id| self.get_entry(id)) else {
            return Vec::new();
        };
        let mut path = Vec::new();
        let mut seen: HashMap<JsString, ()> = HashMap::new();
        loop {
            path.push(current.clone());
            let Some(parent_id) = current.parent_id().filter(|id| !id.is_empty()) else {
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
        let leaf = match self.leaf_id.as_ref() {
            Some(id) => LeafSelection::Id(id),
            None => LeafSelection::None,
        };
        build_context_entries(&entries, leaf)
    }

    pub fn build_session_context(&self) -> SessionContext {
        let entries = self.get_entries();
        let leaf = match self.leaf_id.as_ref() {
            Some(id) => LeafSelection::Id(id),
            None => LeafSelection::None,
        };
        build_session_context(&entries, leaf)
    }

    /// Associates the entry with the exact message object persisted by an Agent listener.
    pub fn bind_message_identity(
        &mut self,
        entry_id: &JsString,
        identity: AgentMessageIdentity,
    ) -> Result<(), SessionManagerError> {
        if self
            .get_entry(entry_id)
            .is_none_or(|entry| entry.entry_type() != "message")
        {
            return Err(error(JsString::from("Message entry not found: ") + entry_id));
        }
        self.message_identities.insert(entry_id.clone(), identity);
        Ok(())
    }

    pub fn message_entry_id(&self, identity: &AgentMessageIdentity) -> Option<&JsString> {
        self.message_identities
            .iter()
            .find_map(|(id, candidate)| (candidate == identity && self.by_id.contains_key(id)).then_some(id))
    }

    /// Stored messages retain their reference; generated context objects get fresh identities.
    pub fn build_session_context_with_identities(&self) -> (SessionContext, Vec<AgentMessageIdentity>) {
        let entries = self.get_entries();
        let leaf = self.leaf_id.as_ref().map_or(LeafSelection::None, LeafSelection::Id);
        let (context, origins) = build_session_context_projection(&entries, leaf);
        let identities = origins
            .into_iter()
            .map(|origin| match origin {
                Some(id) => self
                    .message_identities
                    .get(&id)
                    .expect("stored message identity")
                    .clone(),
                None => AgentMessageIdentity::new(),
            })
            .collect();
        (context, identities)
    }

    pub fn get_header(&self) -> Option<&SessionHeader> {
        self.file_entries.iter().find_map(|entry| match entry {
            FileEntry::Header(header) => Some(header),
            FileEntry::Entry(_) | FileEntry::Json(_) => None,
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
                FileEntry::Header(_) | FileEntry::Json(_) => None,
            })
            .collect()
    }

    pub fn get_tree(&self) -> Vec<SessionTreeNode> {
        build_session_tree(&self.get_entries(), &self.labels_by_id, &self.label_timestamps_by_id)
    }

    pub fn branch(&mut self, branch_from_id: &JsString) -> Result<(), SessionManagerError> {
        if !self.by_id.contains_key(branch_from_id) {
            return Err(error(JsString::from("Entry ") + branch_from_id + " not found"));
        }
        self.leaf_id = Some(branch_from_id.to_owned());
        Ok(())
    }

    pub fn reset_leaf(&mut self) {
        self.leaf_id = None;
    }

    pub fn branch_with_summary(
        &mut self,
        branch_from_id: Option<&JsString>,
        summary: &JsString,
        details: Option<JsValue>,
        from_hook: Option<bool>,
    ) -> Result<JsString, SessionManagerError> {
        if let Some(branch_from_id) = branch_from_id
            && !self.by_id.contains_key(branch_from_id)
        {
            return Err(error(JsString::from("Entry ") + branch_from_id + " not found"));
        }
        self.leaf_id = branch_from_id.cloned();
        let entry = SessionEntry::Known(KnownSessionEntry::BranchSummary(BranchSummaryEntry {
            id: generate_id(&self.by_id)?,
            parent_id: branch_from_id.cloned(),
            timestamp: self.timestamp(),
            from_id: branch_from_id.cloned().unwrap_or_else(|| "root".into()),
            summary: summary.to_owned(),
            details,
            extra: PersistedJsonObject::default(),
            from_hook,
        }));
        self.append_entry(entry)
    }

    /// Create a new session file containing only the path to `leaf_id`.
    pub fn create_branched_session(&mut self, leaf_id: &JsString) -> Result<Option<JsString>, SessionManagerError> {
        let previous_session_file = self.session_file.clone();
        let path = self.get_branch(Some(leaf_id));
        if path.is_empty() {
            return Err(error(JsString::from("Entry ") + leaf_id + " not found"));
        }
        let path_without_labels = rechain_path_without_labels(&path);

        let new_session_id: JsString = uuidv7().into();
        let timestamp = self.timestamp();
        let new_session_file = fork_session_file_path(&self.session_dir, &timestamp, &new_session_id);
        let header = SessionHeader::new(
            new_session_id.clone(),
            timestamp,
            Some(self.cwd.clone()),
            if self.persist { previous_session_file } else { None },
        );

        let mut path_entry_ids = entry_id_set(&path_without_labels);
        let label_entries = build_branch_label_entries(
            &self.labels_by_id,
            &self.label_timestamps_by_id,
            &path_without_labels,
            &mut path_entry_ids,
        )?;
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
        new_session_id: JsString,
    ) {
        let mut file_entries = vec![FileEntry::Header(header)];
        file_entries.extend(path_without_labels.into_iter().map(FileEntry::Entry));
        file_entries.extend(label_entries.into_iter().map(FileEntry::Entry));
        self.file_entries = file_entries;
        self.session_id = new_session_id;
        self.reset_session_name_state();
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

    /// Synchronize the recipient transcript before the caller commits mailbox delivery.
    /// This does not claim or acknowledge any mailbox row.
    pub fn fsync(&self) -> Result<(), SessionManagerError> {
        let path = self
            .session_file
            .as_ref()
            .ok_or_else(|| error("Cannot fsync an in-memory session"))?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(encode_node_utf8(path))
            .and_then(|file| file.sync_all())
            .map_err(format_session_error)
    }

    /// List sessions for a directory, preferring control-db metadata when present.
    pub fn list(
        cwd: &JsString,
        session_dir: Option<&JsString>,
        progress: Option<&mut dyn FnMut(usize, usize)>,
        control_db_path: Option<&JsString>,
    ) -> Result<Vec<SessionInfo>, SessionManagerError> {
        let dir = match session_dir.filter(|dir| !dir.is_empty()) {
            Some(dir) => normalize(dir)?,
            None => get_default_session_dir_path(cwd, &get_agent_dir()?)?,
        };
        if let Some(sessions) = list_metadata_sessions(
            control_db_path,
            MetadataScope {
                cwd: Some(cwd),
                dir: Some(&dir),
                archived: false,
            },
        )? {
            return Ok(sessions);
        }
        let filter_cwd = session_dir.is_some() && !uses_default_session_dir_for(cwd, &dir)?;
        let resolved_cwd = resolve(cwd)?;
        let mut sessions = list_sessions_from_dir(&dir, progress, 0, None);
        if filter_cwd {
            sessions.retain(|session| session_cwd_matches(Some(&session.cwd), &resolved_cwd));
        }
        cache_session_metadata(control_db_path, &sessions)?;
        let mut sessions = exclude_known_subagent_sessions(control_db_path, sessions)?;
        sort_session_infos(&mut sessions);
        Ok(sessions)
    }

    /// List archived sessions across all project directories.
    pub fn list_archived(
        session_dir: Option<&JsString>,
        control_db_path: Option<&JsString>,
    ) -> Result<Vec<SessionInfo>, SessionManagerError> {
        if control_db_path.is_none_or(JsString::is_empty) {
            return Ok(Vec::new());
        }
        let dir = session_dir.filter(|dir| !dir.is_empty()).map(normalize).transpose()?;
        Ok(list_metadata_sessions(
            control_db_path,
            MetadataScope {
                cwd: None,
                dir: dir.as_ref(),
                archived: true,
            },
        )?
        .unwrap_or_default())
    }

    /// List sessions across all project directories (or one custom directory).
    pub fn list_all(
        session_dir: Option<&JsString>,
        progress: Option<&mut dyn FnMut(usize, usize)>,
        control_db_path: Option<&JsString>,
    ) -> Result<Vec<SessionInfo>, SessionManagerError> {
        if let Some(dir) = session_dir.map(normalize).transpose()?.filter(|dir| !dir.is_empty()) {
            if let Some(sessions) = list_metadata_sessions(
                control_db_path,
                MetadataScope {
                    cwd: None,
                    dir: Some(&dir),
                    archived: false,
                },
            )? {
                return Ok(sessions);
            }
            let sessions = list_sessions_from_dir(&dir, progress, 0, None);
            cache_session_metadata(control_db_path, &sessions)?;
            let mut sessions = exclude_known_subagent_sessions(control_db_path, sessions)?;
            sort_session_infos(&mut sessions);
            return Ok(sessions);
        }
        if let Some(sessions) = list_metadata_sessions(
            control_db_path,
            MetadataScope {
                cwd: None,
                dir: None,
                archived: false,
            },
        )? {
            return Ok(sessions);
        }
        let sessions = list_default_session_root(progress)?;
        cache_session_metadata(control_db_path, &sessions)?;
        exclude_known_subagent_sessions(control_db_path, sessions)
    }
}

#[derive(Clone, Copy)]
struct MetadataScope<'a> {
    cwd: Option<&'a JsString>,
    dir: Option<&'a JsString>,
    archived: bool,
}

fn session_info_from_metadata(metadata: &SessionMetadata) -> SessionInfo {
    SessionInfo {
        path: metadata.session_path.clone().into(),
        id: metadata.id.clone().into(),
        cwd: metadata.cwd.clone().into(),
        name: metadata.name.clone().map(JsString::from),
        is_archived: Some(metadata.archived_at.is_some()),
        parent_session_path: metadata.parent_session_path.clone().map(Into::into),
        created: pi_js_compat::date::date_parse(&metadata.created_at),
        modified: pi_js_compat::date::date_parse(&metadata.modified_at),
        message_count: usize::try_from(metadata.message_count).unwrap_or(0),
        first_message: metadata.first_message.clone().into(),
    }
}

fn list_metadata_sessions(
    control_db_path: Option<&JsString>,
    scope: MetadataScope<'_>,
) -> Result<Option<Vec<SessionInfo>>, SessionManagerError> {
    let Some(control_db_path) = control_db_path.filter(|path| !path.is_empty()) else {
        return Ok(None);
    };
    let cwd = scope.cwd.filter(|cwd| !cwd.is_empty()).map(resolve).transpose()?;
    let session_path_prefix = scope
        .dir
        .filter(|dir| !dir.is_empty())
        .map(|dir| {
            let mut prefix = normalize(dir)?;
            if !prefix.ends_with(std::path::MAIN_SEPARATOR) {
                prefix.push_str(std::path::MAIN_SEPARATOR);
            }
            Ok::<_, SessionManagerError>(prefix)
        })
        .transpose()?;
    let options = ResumeSessionMetadataOptions {
        archived: scope.archived,
        cwd: cwd.clone(),
        session_path_prefix: session_path_prefix.clone(),
    };
    Ok(list_resume_session_metadata(control_db_path, &options)
        .map_err(control_db_error)?
        .map(|metadata| {
            metadata
                .iter()
                .map(session_info_from_metadata)
                .filter(|session| !is_resident_session(session))
                .collect()
        }))
}

fn exclude_known_subagent_sessions(
    control_db_path: Option<&JsString>,
    sessions: Vec<SessionInfo>,
) -> Result<Vec<SessionInfo>, SessionManagerError> {
    let sessions: Vec<_> = sessions
        .into_iter()
        .filter(|session| !is_resident_session(session))
        .collect();
    let Some(control_db_path) = control_db_path.filter(|path| !path.is_empty()) else {
        return Ok(sessions);
    };
    let mut subagent_paths: Vec<JsString> = Vec::new();
    for metadata in list_session_metadata(control_db_path).map_err(control_db_error)? {
        if metadata.is_subagent == Some(true) {
            subagent_paths.push(normalize(metadata.session_path)?);
        }
    }
    if subagent_paths.is_empty() {
        return Ok(sessions);
    }
    let mut filtered = Vec::new();
    for session in sessions {
        if !subagent_paths.contains(&normalize(&session.path)?) {
            filtered.push(session);
        }
    }
    Ok(filtered)
}

fn cache_session_metadata(
    control_db_path: Option<&JsString>,
    sessions: &[SessionInfo],
) -> Result<(), SessionManagerError> {
    let Some(control_db_path) = control_db_path.filter(|path| !path.is_empty()) else {
        return Ok(());
    };
    for session in sessions {
        write_session_metadata(
            control_db_path,
            &WritableSessionMetadata {
                session_path: session.path.clone(),
                id: session.id.clone(),
                cwd: session.cwd.clone(),
                name: session.name.clone(),
                parent_session_path: session.parent_session_path.clone(),
                archived_at: None,
                is_archived: session.is_archived,
                is_subagent: None,
                subagent_name: None,
                model_provider: None,
                model_id: None,
                thinking_level: None,
                created_at: pi_js_compat::date::to_iso_string_value(session.created)?.into(),
                modified_at: pi_js_compat::date::to_iso_string_value(session.modified)?.into(),
                message_count: i64::try_from(session.message_count).unwrap_or(i64::MAX),
                first_message: session.first_message.clone(),
            },
        )
        .map_err(control_db_error)?;
    }
    Ok(())
}

fn uses_default_session_dir_for(cwd: &JsString, dir: &JsString) -> Result<bool, SessionManagerError> {
    let agent_dir = get_agent_dir()?;
    Ok(get_default_session_dir_path(cwd, &agent_dir)? == *dir)
}

#[cfg(test)]
mod record_identity_tests {
    use super::*;
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }

    #[test]
    fn migration_retries_generated_id_collision_and_preserves_raw_payload() {
        let content = concat!(
            "{\"type\":\"session\",\"version\":1,\"id\":\"s\",\"timestamp\":\"t\"}\n",
            "{\"type\":\"custom\",\"customType\":\"first\",\"data\":{\"\\ud800\":\"\\udfff\"}}\n",
            "{\"type\":\"custom\",\"customType\":\"second\",\"data\":null}"
        );
        let mut entries = parse_session_entries(content);
        let mut samples = [
            "aaaaaaaa-0000-4000-8000-000000000001",
            "aaaaaaaa-0000-4000-8000-000000000002",
            "bbbbbbbb-0000-4000-8000-000000000003",
        ]
        .into_iter();
        migrate_v1_to_v2_with(&mut entries, || {
            Ok(samples.next().expect("bounded collision samples").to_owned())
        })
        .unwrap();
        migrate_v2_to_v3(&mut entries);
        let request = serde_json::json!({"id": 93000, "operation":"migrate_collision", "content":content});
        let mut frozen = oracle::TsOracle::spawn(
            "lossless-session-records-oracle.mjs",
            "packages/coding-agent/src/core/session-manager.ts",
        );
        let frozen = frozen.request(&request);
        assert_eq!(frozen["ok"], true, "{frozen:?}");
        let duplicate = pi_js_compat::js_json::parse_lossless(frozen["output"][2].as_str().unwrap()).unwrap();
        assert_eq!(duplicate.get("id"), Some(&JsValue::String("aaaaaaaa".into())));
        let mut delta = oracle::TsOracle::spawn_with_environment(
            "lossless-session-records-oracle.mjs",
            "packages/coding-agent/src/core/session-manager.ts",
            &[("PI_APPROVED_DELTA", "1".into())],
        );
        let expected = delta.request(&request);
        assert_eq!(expected["ok"], true, "{expected:?}");
        let actual: Vec<String> = entries
            .iter()
            .map(|entry| serde_json::to_string(entry).unwrap())
            .collect();
        assert_eq!(serde_json::json!(actual), expected["output"]);
        assert_eq!(entries[1].entry().unwrap().id(), Some(&JsString::from("aaaaaaaa")));
        assert_eq!(entries[2].entry().unwrap().id(), Some(&JsString::from("bbbbbbbb")));
        assert_eq!(
            entries[2].entry().unwrap().parent_id(),
            Some(&JsString::from("aaaaaaaa"))
        );
        let value = pi_js_compat::js_json::to_value(&entries[1]).unwrap();
        let data = value.get("data").unwrap().as_object().unwrap();
        assert_eq!(
            data.get(&JsString::from_utf16(vec![0xd800])),
            Some(&JsValue::String(JsString::from_utf16(vec![0xdfff])))
        );
    }
}
