//! Frozen JSONL v3 metadata and repository options.

use super::super::types::SessionMetadataValue;
use crate::harness::types::FileSystem;
use pi_js_compat::js_json::JsString;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonlSessionMetadata {
    pub id: JsString,
    pub created_at: JsString,
    pub cwd: JsString,
    pub path: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_path: Option<JsString>,
}
impl SessionMetadataValue for JsonlSessionMetadata {
    fn id(&self) -> &JsString {
        &self.id
    }
    fn created_at(&self) -> &JsString {
        &self.created_at
    }
}

pub type JsonlSessionRepoFileSystem = dyn FileSystem;
#[derive(Clone)]
pub struct JsonlSessionRepoOptions {
    pub fs: Arc<dyn FileSystem>,
    pub sessions_root: JsString,
}
#[derive(Debug, Clone)]
pub struct JsonlSessionCreateOptions {
    pub cwd: JsString,
    pub id: Option<JsString>,
    pub parent_session_path: Option<JsString>,
}
#[derive(Debug, Clone, Default)]
pub struct JsonlSessionListOptions {
    pub cwd: Option<JsString>,
}
#[derive(Debug, Clone)]
pub struct JsonlSessionStorageCreateOptions {
    pub cwd: JsString,
    pub session_id: JsString,
    pub parent_session_path: Option<JsString>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionHeader {
    #[serde(rename = "type")]
    pub kind: JsString,
    pub version: u8,
    pub id: JsString,
    pub timestamp: JsString,
    pub cwd: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session: Option<JsString>,
}
impl SessionHeader {
    pub fn metadata(&self, path: &JsString) -> JsonlSessionMetadata {
        JsonlSessionMetadata {
            id: self.id.clone(),
            created_at: self.timestamp.clone(),
            cwd: self.cwd.clone(),
            path: path.clone(),
            parent_session_path: self.parent_session.clone(),
        }
    }
}
