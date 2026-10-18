//! Operation-script interpreter shared by the differential and cross-implementation tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::session::{
    BranchBounds, BranchQuery, Clock, Entry, EntryBody, EntryCursor, EntryOrder, EntryQuery, ForkOptions, LogItem,
    LogOptions, RecordBody, RecordQuery, Session, SessionError, SessionStats,
};
use pi_agent_core::search::SessionSearch;
use pi_ai::types::{
    AssistantContent, AssistantMessage, StopReason, TextContent, TextOrImageContent, UserContent, UserMessage, UserRole,
};
use pi_session_sqlite::{
    SqliteSessionCreateOptions, SqliteSessionListOptions, SqliteSessionMetadata, SqliteSessionRepository,
    SqliteSessionRepositoryOptions, SqliteSessionSearch, SqliteSessionSearchOptions, create_sqlite_session_search,
};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::file_system;

pub const FIXED_NOW: i64 = 1_700_000_000_000;

pub fn fixed_clock() -> Clock {
    Arc::new(|| FIXED_NOW)
}

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

pub fn usage_value() -> Value {
    json!({
        "input": 10,
        "output": 5,
        "cacheRead": 3,
        "cacheWrite": 2,
        "totalTokens": 20,
        "cost": {"input": 1, "output": 2, "cacheRead": 3, "cacheWrite": 4, "total": 10}
    })
}

pub fn script_entries() -> Vec<Value> {
    vec![
        json!({"type": "message", "id": "p-message", "message": user_message("payload")}),
        json!({"type": "message", "id": "p-terminate", "message": user_message("stop"), "terminate": true}),
        json!({"type": "model_change", "id": "p-model", "provider": "anthropic", "modelId": "claude-sonnet-4-5"}),
        json!({"type": "thinking_level_change", "id": "p-thinking", "thinkingLevel": "high"}),
        json!({"type": "active_tools_change", "id": "p-tools", "activeToolNames": ["read", "bash"]}),
        json!({"type": "custom", "id": "p-custom", "customType": "note", "data": {"nested": {"value": 1}}}),
        json!({"type": "custom", "id": "p-custom-bare", "customType": "bare"}),
        json!({"type": "compaction", "id": "p-compaction", "summary": "summary", "retainedTail": [user_message("kept")], "tokensBefore": 120, "details": {"reason": "threshold"}, "usage": usage_value()}),
        json!({"type": "compaction", "id": "p-compaction-facade", "summary": "summary", "tokensBefore": 120, "retainedTail": [user_message("kept")], "durationMs": 5, "details": {"reason": "threshold"}, "usage": usage_value()}),
        json!({"type": "branch_summary", "id": "p-branch", "fromId": "p-message", "summary": "branch", "usage": usage_value()}),
    ]
}

pub fn script() -> Vec<Value> {
    vec![
        json!({"op": "create", "id": "s1", "metadata": {"profile": "reviewer"}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "message", "id": "m1", "message": user_message("root")}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "message", "id": "m2", "message": assistant_message("answer")}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "message", "id": "m3", "message": user_message("terminating"), "terminate": true}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "model_change", "id": "e-model", "provider": "anthropic", "modelId": "claude-sonnet-4-5"}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "thinking_level_change", "id": "e-thinking", "thinkingLevel": "high"}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "active_tools_change", "id": "e-tools", "activeToolNames": ["read", "bash"]}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "custom", "id": "e-custom", "customType": "note", "data": {"nested": {"value": 1}}}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "custom", "id": "e-custom-nodata", "customType": "bare"}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "compaction", "id": "e-compaction", "summary": "summary", "tokensBefore": 120, "retainedTail": [user_message("kept")], "details": {"reason": "threshold"}, "usage": usage_value()}}),
        json!({"op": "append_entry", "session": "s1", "entry": {"type": "branch_summary", "id": "e-branch", "fromId": "m2", "summary": "branch", "usage": usage_value()}}),
        json!({"op": "append_record", "session": "s1", "record": {"type": "operation_started", "id": "r1", "lane": "main", "sourceLeafId": null, "intent": {"kind": "run", "originalPrompt": [], "initialMessages": []}}}),
        json!({"op": "append_record", "session": "s1", "record": {"type": "step_attempt", "id": "r2", "lane": "main", "runId": "r1", "step": "assistant", "attempt": 1, "resultEntryId": "m2"}}),
        json!({"op": "append_record", "session": "s1", "record": {"type": "tool_started", "id": "r3", "lane": "main", "runId": "r1", "assistantEntryId": "m2", "toolIndex": 0, "toolCallId": "call-1", "toolName": "example", "effectiveArgs": {"path": "a"}, "resultEntryId": "m2", "replay": "never"}}),
        json!({"op": "append_record", "session": "s1", "record": {"type": "queue_enqueued", "id": "r4", "lane": "main", "queue": "nextRun", "target": {"type": "message", "id": "queued", "message": user_message("queued")}}}),
        json!({"op": "append_record", "session": "s1", "record": {"type": "queue_cancelled", "id": "r5", "lane": "main", "entryId": "queued"}}),
        json!({"op": "append_record", "session": "s1", "record": {"type": "write_deferred", "id": "r6", "lane": "main", "runId": "r1", "target": {"type": "message", "id": "deferred", "message": user_message("deferred")}}}),
        json!({"op": "append_record", "session": "s1", "record": {"type": "usage", "id": "r7", "lane": "main", "cause": "assistant", "runId": "r1", "entryId": "m2", "attempt": 1, "stopReason": "stop", "usage": usage_value()}}),
        json!({"op": "append_record", "session": "s1", "record": {"type": "operation_finished", "id": "r8", "lane": "main", "runId": "r1", "outcome": "completed"}}),
        json!({"op": "create_lane", "session": "s1", "lane": "thread", "at": "m2"}),
        json!({"op": "append_entry", "session": "s1", "lane": "thread", "entry": {"type": "custom", "id": "t1", "customType": "note", "data": {"value": 2}}}),
        json!({"op": "move_lane", "session": "s1", "lane": "main", "to": "e-custom"}),
        json!({"op": "set_name", "session": "s1", "name": "Example"}),
        json!({"op": "set_label", "session": "s1", "entryId": "m1", "label": "checkpoint"}),
        json!({"op": "get_metadata", "session": "s1"}),
        json!({"op": "get_lanes", "session": "s1"}),
        json!({"op": "get_name", "session": "s1"}),
        json!({"op": "get_label", "session": "s1", "entryId": "m1"}),
        json!({"op": "get_label", "session": "s1", "entryId": "m2"}),
        json!({"op": "get_stats", "session": "s1"}),
        json!({"op": "get_entry", "session": "s1", "entryId": "e-custom"}),
        json!({"op": "get_entry", "session": "s1", "entryId": "missing"}),
        json!({"op": "find_entries", "session": "s1", "query": {}}),
        json!({"op": "find_entries", "session": "s1", "query": {"order": "oldestFirst"}}),
        json!({"op": "find_entries", "session": "s1", "query": {"order": "oldestFirst", "cursor": 3, "limit": 2}}),
        json!({"op": "find_entries", "session": "s1", "query": {"type": "custom"}}),
        json!({"op": "find_entries", "session": "s1", "query": {"customType": "note", "limit": 1}}),
        json!({"op": "find_entries", "session": "s1", "query": {"type": "message", "order": "oldestFirst"}}),
        json!({"op": "find_entries_on_branch", "session": "s1", "query": {"start": "m3", "order": "oldestFirst"}}),
        json!({"op": "find_entries_on_branch", "session": "s1", "query": {"start": "m3", "stopAtType": "compaction", "type": "message"}}),
        json!({"op": "find_entries_on_branch", "session": "s1", "query": {"start": "m3", "stopAtType": "custom", "order": "oldestFirst"}}),
        json!({"op": "find_entries_on_branch", "session": "s1", "query": {"start": "t1", "customType": "note", "limit": 1}}),
        json!({"op": "find_entries_on_branch", "session": "s1", "query": {"start": "missing"}}),
        json!({"op": "find_entries", "session": "s1", "query": {"limit": 0}}),
        json!({"op": "find_records", "session": "s1", "query": {}}),
        json!({"op": "find_records", "session": "s1", "query": {"lane": "main", "order": "oldestFirst"}}),
        json!({"op": "find_records", "session": "s1", "query": {"type": "step_attempt", "runId": "r1"}}),
        json!({"op": "find_records", "session": "s1", "query": {"type": "operation_started", "operationKind": "run"}}),
        json!({"op": "find_open_operations", "session": "s1", "lane": "main"}),
        json!({"op": "get_log", "session": "s1"}),
        json!({"op": "get_log", "session": "s1", "afterSeq": 5, "limit": 3}),
        json!({"op": "list", "cwd": true}),
        json!({"op": "fork", "session": "s1", "id": "f1", "options": {"entryId": "m3", "position": "at"}}),
        json!({"op": "find_entries", "session": "f1", "query": {"order": "oldestFirst"}}),
        json!({"op": "get_lanes", "session": "f1"}),
        json!({"op": "get_name", "session": "f1"}),
        json!({"op": "get_label", "session": "f1", "entryId": "m1"}),
        json!({"op": "get_stats", "session": "f1"}),
        json!({"op": "fork", "session": "s1", "id": "f2", "options": {"scope": "tree"}}),
        json!({"op": "find_entries", "session": "f2", "query": {"order": "oldestFirst"}}),
        json!({"op": "get_lanes", "session": "f2"}),
        json!({"op": "get_log", "session": "f2", "afterSeq": 10}),
        json!({"op": "repair_branch_cache", "session": "s1"}),
        json!({"op": "search", "text": "answer"}),
        json!({"op": "search", "text": "queued"}),
        json!({"op": "search", "text": "term", "options": {"entryTypes": ["message"]}}),
        json!({"op": "search", "text": "answer", "options": {"limit": 1}}),
        json!({"op": "search", "text": "  "}),
        json!({"op": "delete", "session": "f1"}),
        json!({"op": "get_metadata", "session": "f1"}),
        json!({"op": "close"}),
    ]
}

fn order(value: &Value) -> Option<EntryOrder> {
    match value.as_str() {
        Some("oldestFirst") => Some(EntryOrder::OldestFirst),
        Some("newestFirst") => Some(EntryOrder::NewestFirst),
        _ => None,
    }
}

fn entry_query(value: &Value) -> EntryQuery {
    EntryQuery {
        entry_type: value
            .get("type")
            .and_then(Value::as_str)
            .and_then(|name| serde_json::from_value(Value::String(name.to_owned())).ok()),
        custom_type: value.get("customType").and_then(Value::as_str).map(str::to_owned),
        order: order(value.get("order").unwrap_or(&Value::Null)),
        limit: value.get("limit").and_then(Value::as_u64).map(|limit| limit as usize),
        cursor: value
            .get("cursor")
            .and_then(Value::as_i64)
            .map(|after_seq| EntryCursor { after_seq }),
    }
}

fn record_query(value: &Value) -> RecordQuery {
    RecordQuery {
        lane: value.get("lane").and_then(Value::as_str).map(str::to_owned),
        record_type: value
            .get("type")
            .and_then(Value::as_str)
            .and_then(|name| serde_json::from_value(Value::String(name.to_owned())).ok()),
        run_id: value.get("runId").and_then(Value::as_str).map(str::to_owned),
        operation_kind: value
            .get("operationKind")
            .and_then(Value::as_str)
            .and_then(|name| serde_json::from_value(Value::String(name.to_owned())).ok()),
        after_seq: value.get("afterSeq").and_then(Value::as_i64),
        order: order(value.get("order").unwrap_or(&Value::Null)),
        limit: value.get("limit").and_then(Value::as_u64).map(|limit| limit as usize),
    }
}

fn fork_options(value: &Value) -> ForkOptions {
    if value.get("scope").and_then(Value::as_str) == Some("tree") {
        return ForkOptions::Tree;
    }
    let position = value.get("position").and_then(Value::as_str);
    match value.get("entryId").and_then(Value::as_str) {
        Some(entry_id) if position == Some("before") => ForkOptions::BranchBefore {
            entry_id: entry_id.to_owned(),
        },
        Some(entry_id) => ForkOptions::BranchAt {
            entry_id: entry_id.to_owned(),
        },
        None if position == Some("before") => ForkOptions::BranchDefaultBefore,
        None => ForkOptions::Branch,
    }
}

fn message_for(role: Option<&str>, text: &str) -> AgentMessage {
    if role == Some("assistant") {
        assistant_message(text)
    } else {
        user_message(text)
    }
}

/// Executes one scripted operation against the Rust backend.
pub struct RustRunner {
    root: PathBuf,
    _temp: Option<TempDir>,
    pub repo: SqliteSessionRepository,
    pub search: SqliteSessionSearch,
    pub database_path: String,
    sessions: HashMap<String, Session<SqliteSessionMetadata>>,
    metadata: HashMap<String, SqliteSessionMetadata>,
}

impl RustRunner {
    pub fn new() -> Self {
        let temp = TempDir::new().expect("temp dir");
        let mut runner = Self::at(temp.path());
        runner._temp = Some(temp);
        runner
    }

    /// Binds a runner to an existing directory (shared with the TypeScript oracle).
    pub fn at(root: &Path) -> Self {
        let database_path = root.join("sessions.sqlite").to_string_lossy().into_owned();
        let env = file_system(root);
        let repo = SqliteSessionRepository::with_clock(
            SqliteSessionRepositoryOptions {
                env: Arc::clone(&env),
                database_path: database_path.clone(),
                writer_lease: None,
            },
            fixed_clock(),
        )
        .expect("valid options");
        let search = create_sqlite_session_search(SqliteSessionSearchOptions {
            env,
            database_path: database_path.clone(),
        });
        Self {
            root: root.to_path_buf(),
            _temp: None,
            repo,
            search,
            database_path,
            sessions: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn cwd(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    fn session(&self, id: &str) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        self.sessions.get(id).cloned().ok_or_else(|| {
            SessionError::new(
                pi_agent_core::harness::session::SessionErrorCode::NotFound,
                format!("Unknown session {id}"),
            )
        })
    }

    fn stored_metadata(&self, id: &str) -> SqliteSessionMetadata {
        self.metadata
            .get(id)
            .unwrap_or_else(|| panic!("unknown metadata {id}"))
            .clone()
    }

    pub async fn run(&mut self, operation: &Value) -> Result<Value, SessionError> {
        let op = operation.get("op").and_then(Value::as_str).expect("op name");
        match op {
            "init" => Ok(Value::Null),
            "create" | "open" | "open_by_id" => self.run_open_op(operation).await,
            "fork" | "list" => self.run_fork_op(operation).await,
            "repair_branch_cache" | "delete" | "close" => self.run_lifecycle_op(operation).await,
            "append_message" | "append_entry" | "append_record" | "create_lane" | "move_lane" | "set_name"
            | "set_label" => self.run_session_write_op(operation).await,
            "get_metadata" | "get_lanes" | "get_name" | "get_label" | "get_stats" | "get_entry" => {
                self.run_session_read_op(operation).await
            }
            "find_entries" | "find_entries_on_branch" | "find_records" | "find_open_operations" | "get_log" => {
                self.run_query_op(operation).await
            }
            "search" => self.run_search_op(operation).await,
            other => panic!("unknown op {other}"),
        }
    }

    async fn run_open_op(&mut self, operation: &Value) -> Result<Value, SessionError> {
        let op = operation.get("op").and_then(Value::as_str).expect("op name");
        match op {
            "create" => {
                let id = operation.get("id").and_then(Value::as_str).expect("create id");
                let session = self
                    .repo
                    .create(SqliteSessionCreateOptions {
                        id: Some(id.to_owned()),
                        cwd: self.cwd(),
                        parent_session_id: operation
                            .get("parentSessionId")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        metadata: operation.get("metadata").and_then(Value::as_object).cloned(),
                    })
                    .await?;
                let metadata = session.get_metadata().await?;
                self.sessions.insert(id.to_owned(), session);
                self.metadata.insert(id.to_owned(), metadata.clone());
                Ok(serde_json::to_value(metadata).expect("metadata serializes"))
            }
            "open_by_id" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let listed = self.repo.list(SqliteSessionListOptions::default()).await?;
                let found = listed.into_iter().find(|metadata| metadata.id == id).ok_or_else(|| {
                    SessionError::new(
                        pi_agent_core::harness::session::SessionErrorCode::NotFound,
                        format!("Session not found: {id}"),
                    )
                })?;
                let session = self.repo.open(&found).await?;
                let metadata = session.get_metadata().await?;
                self.sessions.insert(id.to_owned(), session);
                self.metadata.insert(id.to_owned(), metadata.clone());
                Ok(serde_json::to_value(metadata).expect("metadata serializes"))
            }
            "open" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let metadata = self.stored_metadata(id);
                let session = self.repo.open(&metadata).await?;
                let metadata = session.get_metadata().await?;
                self.sessions.insert(id.to_owned(), session);
                self.metadata.insert(id.to_owned(), metadata.clone());
                Ok(serde_json::to_value(metadata).expect("metadata serializes"))
            }
            other => panic!("unknown op {other}"),
        }
    }

    async fn run_fork_op(&mut self, operation: &Value) -> Result<Value, SessionError> {
        let op = operation.get("op").and_then(Value::as_str).expect("op name");
        match op {
            "fork" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let fork_id = operation.get("id").and_then(Value::as_str).expect("fork id");
                let options = fork_options(operation.get("options").unwrap_or(&Value::Null));
                let session = self
                    .repo
                    .fork(
                        &self.stored_metadata(id),
                        options,
                        SqliteSessionCreateOptions {
                            id: Some(fork_id.to_owned()),
                            cwd: self.cwd(),
                            ..Default::default()
                        },
                    )
                    .await?;
                let metadata = session.get_metadata().await?;
                self.sessions.insert(fork_id.to_owned(), session);
                self.metadata.insert(fork_id.to_owned(), metadata.clone());
                Ok(serde_json::to_value(metadata).expect("metadata serializes"))
            }
            "list" => {
                let cwd = if operation.get("cwd").and_then(Value::as_bool) == Some(true) {
                    Some(self.cwd())
                } else {
                    None
                };
                let listed = self.repo.list(SqliteSessionListOptions { cwd }).await?;
                Ok(serde_json::to_value(listed).expect("metadata serializes"))
            }
            other => panic!("unknown op {other}"),
        }
    }

    async fn run_lifecycle_op(&mut self, operation: &Value) -> Result<Value, SessionError> {
        let op = operation.get("op").and_then(Value::as_str).expect("op name");
        match op {
            "repair_branch_cache" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                self.repo.repair_branch_cache(&self.stored_metadata(id)).await?;
                Ok(Value::Null)
            }
            "delete" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                self.repo.delete(&self.stored_metadata(id)).await?;
                Ok(Value::Null)
            }
            "close" => {
                self.repo.close().await?;
                Ok(Value::Null)
            }
            other => panic!("unknown op {other}"),
        }
    }

    async fn run_session_write_op(&mut self, operation: &Value) -> Result<Value, SessionError> {
        let op = operation.get("op").and_then(Value::as_str).expect("op name");
        match op {
            "append_message" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let text = operation.get("text").and_then(Value::as_str).expect("text");
                let role = operation.get("role").and_then(Value::as_str);
                let entry_id = self.session(id)?.append_message(message_for(role, text)).await?;
                Ok(Value::String(entry_id))
            }
            "append_entry" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let lane = operation.get("lane").and_then(Value::as_str).unwrap_or("main");
                let body: EntryBody =
                    serde_json::from_value(operation.get("entry").cloned().expect("entry")).expect("entry body");
                let entry = self.session(id)?.append_entry(body, lane).await?;
                Ok(serde_json::to_value(entry).expect("entry serializes"))
            }
            "append_record" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let body: RecordBody =
                    serde_json::from_value(operation.get("record").cloned().expect("record")).expect("record body");
                let record = self.session(id)?.append_record(body).await?;
                Ok(serde_json::to_value(record).expect("record serializes"))
            }
            "create_lane" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let lane = operation.get("lane").and_then(Value::as_str).expect("lane");
                let at = operation.get("at").and_then(Value::as_str);
                self.session(id)?.create_lane(lane, at).await?;
                Ok(Value::Null)
            }
            "move_lane" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let lane = operation.get("lane").and_then(Value::as_str).unwrap_or("main");
                let to = operation.get("to").and_then(Value::as_str);
                self.session(id)?.move_lane(lane, to).await?;
                Ok(Value::Null)
            }
            "set_name" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                self.session(id)?
                    .set_name(operation.get("name").and_then(Value::as_str))
                    .await?;
                Ok(Value::Null)
            }
            "set_label" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let entry_id = operation.get("entryId").and_then(Value::as_str).expect("entryId");
                self.session(id)?
                    .set_label(entry_id, operation.get("label").and_then(Value::as_str))
                    .await?;
                Ok(Value::Null)
            }
            other => panic!("unknown op {other}"),
        }
    }

    async fn run_session_read_op(&mut self, operation: &Value) -> Result<Value, SessionError> {
        let op = operation.get("op").and_then(Value::as_str).expect("op name");
        match op {
            "get_metadata" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let metadata = self.session(id)?.get_metadata().await?;
                Ok(serde_json::to_value(metadata).expect("metadata serializes"))
            }
            "get_lanes" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let lanes = self.session(id)?.get_lanes().await?;
                Ok(serde_json::to_value(lanes).expect("lanes serialize"))
            }
            "get_name" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let name = self.session(id)?.get_name().await?;
                Ok(name.map(Value::String).unwrap_or(Value::Null))
            }
            "get_label" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let entry_id = operation.get("entryId").and_then(Value::as_str).expect("entryId");
                let label = self.session(id)?.get_label(entry_id).await?;
                Ok(label.map(Value::String).unwrap_or(Value::Null))
            }
            "get_stats" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let stats: SessionStats = self.session(id)?.get_stats().await?;
                Ok(serde_json::to_value(stats).expect("stats serialize"))
            }
            "get_entry" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let entry_id = operation.get("entryId").and_then(Value::as_str).expect("entryId");
                let entry: Option<Entry> = self.session(id)?.get_entry(entry_id).await?;
                Ok(entry
                    .map(|entry| serde_json::to_value(entry).expect("entry serializes"))
                    .unwrap_or(Value::Null))
            }
            other => panic!("unknown op {other}"),
        }
    }

    async fn run_query_op(&mut self, operation: &Value) -> Result<Value, SessionError> {
        let op = operation.get("op").and_then(Value::as_str).expect("op name");
        match op {
            "find_entries" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let query = entry_query(operation.get("query").unwrap_or(&Value::Null));
                let entries = self.session(id)?.find_entries(query).await?;
                Ok(serde_json::to_value(entries).expect("entries serialize"))
            }
            "find_entries_on_branch" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let query = operation.get("query").unwrap_or(&Value::Null);
                let branch = BranchQuery {
                    entry: entry_query(query),
                    bounds: BranchBounds {
                        start: query.get("start").and_then(Value::as_str).map(str::to_owned),
                        stop_at_type: query
                            .get("stopAtType")
                            .and_then(Value::as_str)
                            .and_then(|name| serde_json::from_value(Value::String(name.to_owned())).ok()),
                        stop_at_id: query.get("stopAtId").and_then(Value::as_str).map(str::to_owned),
                    },
                };
                let entries = self.session(id)?.find_entries_on_branch(branch).await?;
                Ok(serde_json::to_value(entries).expect("entries serialize"))
            }
            "find_records" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let query = record_query(operation.get("query").unwrap_or(&Value::Null));
                let records = self.session(id)?.find_records(query).await?;
                Ok(serde_json::to_value(records).expect("records serialize"))
            }
            "find_open_operations" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let lane = operation.get("lane").and_then(Value::as_str).unwrap_or("main");
                let limit = operation
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|limit| limit as usize);
                let records = self.session(id)?.find_open_operations(lane, limit).await?;
                Ok(serde_json::to_value(records).expect("records serialize"))
            }
            "get_log" => {
                let id = operation.get("session").and_then(Value::as_str).expect("session");
                let items: Vec<LogItem> = self
                    .session(id)?
                    .get_log(LogOptions {
                        after_seq: operation.get("afterSeq").and_then(Value::as_i64),
                        limit: operation
                            .get("limit")
                            .and_then(Value::as_u64)
                            .map(|limit| limit as usize),
                    })
                    .await?;
                Ok(serde_json::to_value(items).expect("log serializes"))
            }
            other => panic!("unknown op {other}"),
        }
    }

    async fn run_search_op(&mut self, operation: &Value) -> Result<Value, SessionError> {
        let op = operation.get("op").and_then(Value::as_str).expect("op name");
        match op {
            "search" => {
                let text = operation.get("text").and_then(Value::as_str).expect("text");
                let options = operation.get("options").unwrap_or(&Value::Null);
                let entry_types = options.get("entryTypes").and_then(Value::as_array).map(|names| {
                    names
                        .iter()
                        .filter_map(|name| serde_json::from_value(name.clone()).ok())
                        .collect()
                });
                let hits = self
                    .search
                    .search(
                        text,
                        pi_agent_core::search::SessionSearchOptions {
                            entry_types,
                            limit: options.get("limit").and_then(Value::as_u64).map(|limit| limit as usize),
                            signal: None,
                        },
                    )
                    .await
                    .map_err(|error| match error {
                        pi_agent_core::search::SearchError::Session(error) => error,
                        other => SessionError::new(
                            pi_agent_core::harness::session::SessionErrorCode::Storage,
                            other.to_string(),
                        ),
                    })?;
                let hits: Vec<Value> = hits
                    .into_iter()
                    .map(|hit| {
                        json!({
                            "sessionId": hit.session_id,
                            "entryId": hit.entry_id,
                            "timestamp": hit.timestamp,
                            "score": hit.score,
                        })
                    })
                    .collect();
                Ok(Value::Array(hits))
            }
            other => panic!("unknown op {other}"),
        }
    }
}

/// Replaces volatile paths and drops the error `name` so responses compare structurally.
pub fn canonicalize(value: &Value, root: &str, database_path: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(database_path, "<db>").replace(root, "<root>")),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| canonicalize(item, root, database_path))
                .collect(),
        ),
        Value::Object(object) => {
            let mut canonical = serde_json::Map::new();
            for (key, item) in object {
                if key == "name" && object.contains_key("message") && object.contains_key("code") {
                    continue;
                }
                canonical.insert(key.clone(), canonicalize(item, root, database_path));
            }
            Value::Object(canonical)
        }
        other => other.clone(),
    }
}

pub fn sorted(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        Value::Object(object) => {
            let mut entries: Vec<(&String, &Value)> = object.iter().collect();
            entries.sort_by_key(|(key, _)| *key);
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), sorted(value)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

pub fn canonical_text(value: &Value) -> String {
    serde_json::to_string(&sorted(value)).expect("canonical value serializes")
}
