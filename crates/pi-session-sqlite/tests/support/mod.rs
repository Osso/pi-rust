//! Shared test fixtures for the SQLite session backend.

#![allow(dead_code)]

pub mod oracle;
pub mod runner;

use std::path::Path;
use std::sync::Arc;

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::session::{Entry, EntryQuery, Session};
use pi_agent_core::harness::types::FileSystem;
use pi_ai::types::{
    AssistantContent, AssistantMessage, StopReason, TextContent, TextOrImageContent, UserContent, UserMessage, UserRole,
};
use pi_session_sqlite::{
    SqliteSessionCreateOptions, SqliteSessionListOptions, SqliteSessionMetadata, SqliteSessionRepository,
    SqliteSessionRepositoryOptions, SqliteWriterLeaseOptions,
};
use tempfile::TempDir;

pub fn user_message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    })
}

pub fn assistant_message(text: &str) -> AgentMessage {
    let mut message = AssistantMessage::empty("anthropic-messages", "anthropic", "claude-sonnet-4-5", 1);
    message.content = vec![AssistantContent::Text(TextContent::new(text))];
    message.stop_reason = StopReason::Stop;
    AgentMessage::Assistant(Box::new(message))
}

pub fn file_system(root: &Path) -> Arc<dyn FileSystem> {
    Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.to_string_lossy().into_owned(),
        ..NodeExecutionEnvOptions::default()
    }))
}

/// One repository bound to a fresh temporary directory.
pub struct Fixture {
    pub root: TempDir,
    pub repo: SqliteSessionRepository,
    pub database_path: String,
}

impl Fixture {
    pub fn new() -> Self {
        Self::with_lease(None)
    }

    pub fn with_lease(writer_lease: Option<SqliteWriterLeaseOptions>) -> Self {
        let root = TempDir::new().expect("temp dir");
        let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();
        let repo = SqliteSessionRepository::new(SqliteSessionRepositoryOptions {
            env: file_system(root.path()),
            database_path: database_path.clone(),
            writer_lease,
        })
        .expect("valid lease options");
        Self {
            root,
            repo,
            database_path,
        }
    }

    pub fn cwd(&self) -> String {
        self.root.path().to_string_lossy().into_owned()
    }

    pub async fn create(&self, id: &str) -> Session<SqliteSessionMetadata> {
        self.create_session(id, None).await
    }

    pub async fn create_with_metadata(
        &self,
        id: &str,
        metadata: serde_json::Map<String, serde_json::Value>,
    ) -> Session<SqliteSessionMetadata> {
        self.create_session(id, Some(metadata)).await
    }

    async fn create_session(
        &self,
        id: &str,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Session<SqliteSessionMetadata> {
        self.repo
            .create(SqliteSessionCreateOptions {
                id: Some(id.to_owned()),
                cwd: self.cwd(),
                metadata,
                ..SqliteSessionCreateOptions::default()
            })
            .await
            .expect("session created")
    }

    pub async fn list(&self) -> Vec<SqliteSessionMetadata> {
        self.repo
            .list(SqliteSessionListOptions::default())
            .await
            .expect("sessions listed")
    }

    pub fn open_connection(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(&self.database_path).expect("inspection connection")
    }
}

pub async fn entry_ids(entries: Result<Vec<Entry>, pi_agent_core::harness::session::SessionError>) -> Vec<String> {
    entries
        .expect("entries read")
        .into_iter()
        .map(|entry| entry.id().to_owned())
        .collect()
}

pub async fn all_entries(session: &Session<SqliteSessionMetadata>, query: EntryQuery) -> Vec<Entry> {
    session.find_entries(query).await.expect("entries read")
}
