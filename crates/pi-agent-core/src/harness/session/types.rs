//! Native v3 tree contracts from agent-core/harness/types.ts.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use pi_ai::request::BoxFuture;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, from_value, to_value};
use serde::{Deserialize, Serialize};

use crate::agent::AgentMessage;
use crate::harness::messages::CustomMessageContent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTreeEntryBase {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub timestamp: JsString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum KnownSessionEntry {
    Message {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        message: AgentMessage,
    },
    ThinkingLevelChange {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        thinking_level: JsString,
    },
    ModelChange {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        provider: JsString,
        model_id: JsString,
    },
    ActiveToolsChange {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        active_tool_names: Vec<JsString>,
    },
    Compaction {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        summary: JsString,
        first_kept_entry_id: JsString,
        tokens_before: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<f64>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_present"
        )]
        details: Option<JsValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_hook: Option<bool>,
    },
    BranchSummary {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        from_id: JsString,
        summary: JsString,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_present"
        )]
        details: Option<JsValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_hook: Option<bool>,
    },
    Custom {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        custom_type: JsString,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_present"
        )]
        data: Option<JsValue>,
    },
    CustomMessage {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        custom_type: JsString,
        content: CustomMessageContent,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_present"
        )]
        details: Option<JsValue>,
        display: bool,
    },
    Label {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        target_id: JsString,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<JsString>,
    },
    SessionInfo {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<JsString>,
    },
    Leaf {
        #[serde(flatten)]
        base: SessionTreeEntryBase,
        target_id: Option<JsString>,
    },
}

fn deserialize_present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<JsValue>, D::Error> {
    JsValue::deserialize(deserializer).map(Some)
}

/// Raw JSON stays authoritative: unknown types, fields and payloads survive storage round trips.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionTreeEntry {
    raw: JsObject,
    known: Option<KnownSessionEntry>,
}

impl SessionTreeEntry {
    pub fn from_value(value: JsValue) -> Result<Self, SessionError> {
        validate_entry_fields(&value, false)
            .map_err(|message| SessionError::new(SessionErrorCode::InvalidEntry, message))?;
        let JsValue::Object(raw) = value else {
            unreachable!("entry validation requires object");
        };
        let known = from_value(&JsValue::Object(raw.clone())).ok();
        Ok(Self { raw, known })
    }

    pub fn from_known(entry: KnownSessionEntry) -> Self {
        let JsValue::Object(raw) = to_value(&entry).expect("known tree entries serialize natively") else {
            unreachable!("tagged tree entries serialize as objects");
        };
        Self {
            raw,
            known: Some(entry),
        }
    }

    pub fn known(&self) -> Option<&KnownSessionEntry> {
        self.known.as_ref()
    }
    pub fn as_message(&self) -> Option<&AgentMessage> {
        match self.known() {
            Some(KnownSessionEntry::Message { message, .. }) => Some(message),
            _ => None,
        }
    }
    pub fn message(&self) -> Option<&AgentMessage> {
        self.as_message()
    }
    pub fn value(&self) -> JsValue {
        JsValue::Object(self.raw.clone())
    }
    pub fn object(&self) -> &JsObject {
        &self.raw
    }
    pub fn id(&self) -> &JsString {
        self.raw["id"].as_string().expect("validated id")
    }
    pub fn kind(&self) -> &JsString {
        self.raw["type"].as_string().expect("validated type")
    }
    pub fn timestamp(&self) -> &JsString {
        self.raw["timestamp"].as_string().expect("validated timestamp")
    }
    pub fn parent_id(&self) -> Option<&JsString> {
        self.raw["parentId"].as_string()
    }
    pub fn leaf_id_after(&self) -> Option<JsString> {
        if self.kind() == "leaf" {
            self.raw["targetId"].as_string().cloned()
        } else {
            Some(self.id().clone())
        }
    }
}

impl Serialize for SessionTreeEntry {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        JsValue::Object(self.raw.clone()).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for SessionTreeEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_value(JsValue::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Frozen parseEntryLine validates only these common fields, not known payload shapes.
pub(crate) fn validate_entry(value: &JsValue) -> Result<(), &'static str> {
    validate_entry_fields(value, true)
}

fn validate_entry_fields(value: &JsValue, require_nonempty_fields: bool) -> Result<(), &'static str> {
    let object = match value {
        JsValue::Object(object) => object,
        JsValue::Array(_) => return Err("is missing entry type"),
        _ => return Err("is not a valid session entry"),
    };
    if object.get("type").and_then(JsValue::as_string).is_none() {
        return Err("is missing entry type");
    }
    if object
        .get("id")
        .and_then(JsValue::as_string)
        .is_none_or(|id| require_nonempty_fields && id.is_empty())
    {
        return Err("is missing entry id");
    }
    if !matches!(object.get("parentId"), Some(JsValue::Null | JsValue::String(_))) {
        return Err("has invalid parentId");
    }
    if object
        .get("timestamp")
        .and_then(JsValue::as_string)
        .is_none_or(|timestamp| require_nonempty_fields && timestamp.is_empty())
    {
        return Err("is missing timestamp");
    }
    if object
        .get("type")
        .and_then(JsValue::as_string)
        .is_some_and(|kind| kind == "leaf")
        && !matches!(object.get("targetId"), Some(JsValue::Null | JsValue::String(_)))
    {
        return Err("has invalid targetId");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub id: JsString,
    pub created_at: JsString,
}

pub trait SessionMetadataValue: Clone + Send + Sync + 'static {
    fn id(&self) -> &JsString;
    fn created_at(&self) -> &JsString;
}
impl SessionMetadataValue for SessionMetadata {
    fn id(&self) -> &JsString {
        &self.id
    }
    fn created_at(&self) -> &JsString {
        &self.created_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionErrorCode {
    NotFound,
    InvalidSession,
    InvalidEntry,
    InvalidForkTarget,
    Storage,
    Unknown,
}
impl SessionErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::InvalidSession => "invalid_session",
            Self::InvalidEntry => "invalid_entry",
            Self::InvalidForkTarget => "invalid_fork_target",
            Self::Storage => "storage",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Debug)]
pub struct SessionError {
    pub code: SessionErrorCode,
    pub message: JsString,
    pub cause: Option<Box<dyn Error + Send + Sync>>,
}
impl SessionError {
    pub fn new(code: SessionErrorCode, message: impl Into<JsString>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: None,
        }
    }
    pub fn with_cause(
        code: SessionErrorCode,
        message: impl Into<JsString>,
        cause: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            cause: Some(Box::new(cause)),
        }
    }
    pub const fn code(&self) -> SessionErrorCode {
        self.code
    }
    pub fn message(&self) -> &JsString {
        &self.message
    }
    pub fn missing(code: SessionErrorCode, id: &JsString) -> Self {
        Self::new(code, js_format!("Entry {} not found", id))
    }
}
impl Display for SessionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}
impl Error for SessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref().map(|cause| cause as &dyn Error)
    }
}

pub trait SessionStorage<M: SessionMetadataValue>: Send + Sync {
    fn get_metadata(&self) -> BoxFuture<'_, Result<M, SessionError>>;
    fn get_leaf_id(&self) -> BoxFuture<'_, Result<Option<JsString>, SessionError>>;
    fn set_leaf_id<'a>(&'a self, leaf: Option<&'a JsString>) -> BoxFuture<'a, Result<(), SessionError>>;
    fn create_entry_id(&self) -> BoxFuture<'_, Result<JsString, SessionError>>;
    fn append_entry(&self, entry: SessionTreeEntry) -> BoxFuture<'_, Result<(), SessionError>>;
    fn get_entry<'a>(&'a self, id: &'a JsString) -> BoxFuture<'a, Result<Option<SessionTreeEntry>, SessionError>>;
    fn find_entries<'a>(&'a self, kind: &'a JsString) -> BoxFuture<'a, Result<Vec<SessionTreeEntry>, SessionError>>;
    fn get_label<'a>(&'a self, id: &'a JsString) -> BoxFuture<'a, Result<Option<JsString>, SessionError>>;
    fn get_path_to_root<'a>(
        &'a self,
        leaf: Option<&'a JsString>,
    ) -> BoxFuture<'a, Result<Vec<SessionTreeEntry>, SessionError>>;
    fn get_entries(&self) -> BoxFuture<'_, Result<Vec<SessionTreeEntry>, SessionError>>;
}

pub type Clock = Arc<dyn Fn() -> JsString + Send + Sync>;
pub type IdGenerator = Arc<dyn Fn() -> JsString + Send + Sync>;
pub fn system_clock() -> Clock {
    Arc::new(|| pi_js_compat::date::to_iso_string(pi_js_compat::date::js_now_millis() as i64).into())
}
pub fn uuid_generator() -> IdGenerator {
    Arc::new(|| super::uuid::uuidv7().into())
}

#[derive(Debug, Clone, Default)]
pub struct SessionCreateOptions {
    pub id: Option<JsString>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForkPosition {
    #[default]
    Before,
    At,
}
#[derive(Debug, Clone, Default)]
pub struct SessionForkOptions {
    pub entry_id: Option<JsString>,
    pub position: ForkPosition,
    pub id: Option<JsString>,
}
