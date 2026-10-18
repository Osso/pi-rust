//! Port of `packages/session-backends/sqlite-node/src/sqlite/types.ts`.

use pi_agent_core::harness::session::SessionMetadataValue;
use pi_ai::types::JsonObject;
use serde::{Deserialize, Serialize};

/// Session metadata projected from one `sessions` row plus global facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqliteSessionMetadata {
    pub id: String,
    pub created_at: i64,
    pub cwd: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    /// Current session name projected from SQLite global facts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Opaque application-owned metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonObject>,
}

impl SessionMetadataValue for SqliteSessionMetadata {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Session creation options.
#[derive(Debug, Clone, Default)]
pub struct SqliteSessionCreateOptions {
    pub id: Option<String>,
    pub parent_session_id: Option<String>,
    pub cwd: String,
    pub metadata: Option<JsonObject>,
}

/// Session catalog filters.
#[derive(Debug, Clone, Default)]
pub struct SqliteSessionListOptions {
    pub cwd: Option<String>,
}
