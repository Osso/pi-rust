//! Port of `packages/session-backends/sqlite-node/src/sqlite/repo.ts`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::harness::session::{
    Clock, Entry, EntryBody, EntryCursor, EntryOrder, EntryQuery, EntryType, FactItem, ForkOptions, IdGenerator,
    LanePointer, LaneRecord, LogItem, LogOptions, OptionalValue, RecordBody, RecordQuery, RecordType, Session,
    SessionError, SessionErrorCode, SessionStats, SessionStorage, StorageBranchQuery, UuidV7IdGenerator, system_clock,
};
use pi_agent_core::harness::types::{CreateDirOptions, FileError, FileErrorCode, FileSystem};
use pi_ai::request::BoxFuture;
use pi_ai::types::Usage;
use serde_json::Value;

use crate::error::SqliteError;
use crate::sqlite::branch_cache::{
    append_entry_to_branch_cache, build_cached_branch, delete_branch_cache, rebuild_branch_cache,
};
use crate::sqlite::connection::{Database, configure_sqlite_database, open_database, transaction};
use crate::sqlite::migrations::apply_migrations;
use crate::sqlite::storage::branch_entries::{
    CachedBranchEntryRow, CachedBranchQuery, query_cached_branch_rows, read_cached_branch,
};
use crate::sqlite::storage::branch_tips::read_branch_tip_ids;
use crate::sqlite::storage::entries::{
    EntryRow, EntryRowsQuery, NewEntryRow, delete_entry_rows, entry_payload, id_exists_in_entries, insert_entry_row,
    read_entry_row, read_entry_rows,
};
use crate::sqlite::storage::facts::{
    FactRow, append_fact, delete_fact_rows, read_fact_rows, read_latest_fact, read_latest_label_facts,
};
use crate::sqlite::storage::lanes::{
    LaneMoveRow, create_initial_lane, create_lane, delete_lane_rows, finish_lane_operation, move_lane, read_lane,
    read_lane_head, read_lane_move_rows, read_lanes, set_lane_leaf, start_lane_operation,
};
use crate::sqlite::storage::records::{
    NewRecordRow, RecordRow, RecordRowsQuery, append_record_row, delete_record_rows, id_exists_in_records,
    read_open_operation_rows, read_record_rows,
};
use crate::sqlite::storage::session_sequences::{
    advance_sequence, create_sequence, delete_sequence, get_next_sequence, set_next_sequence,
};
use crate::sqlite::storage::session_stats::{
    add_usage_to_stats, create_stats, delete_stats, increment_message_count, read_stats,
};
use crate::sqlite::storage::sessions::{
    NewSessionRow, SessionRow, decode_session_metadata, delete_session_row, insert_session_row, read_session_row,
    read_session_rows, session_exists,
};
use crate::sqlite::storage::writer_leases::{
    WriterLease, acquire_writer_lease, delete_writer_lease, release_writer_lease, renew_writer_lease,
};
use crate::sqlite::types::{SqliteSessionCreateOptions, SqliteSessionListOptions, SqliteSessionMetadata};

/// Writer lease timing.
#[derive(Debug, Clone, Default)]
pub struct SqliteWriterLeaseOptions {
    /// Time without a successful heartbeat before another writer may take over. Default: 30 seconds.
    pub ttl_ms: Option<i64>,
    /// Idle heartbeat cadence. Default: 10 seconds. Must be less than `ttl_ms`.
    pub heartbeat_interval_ms: Option<i64>,
}

/// Invalid lease timing, the Rust mapping of the source `RangeError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteWriterLeaseOptionsError {
    TtlNotPositive,
    HeartbeatNotPositiveOrNotLessThanTtl,
}

impl std::fmt::Display for SqliteWriterLeaseOptionsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TtlNotPositive => formatter.write_str("writerLease.ttlMs must be positive"),
            Self::HeartbeatNotPositiveOrNotLessThanTtl => {
                formatter.write_str("writerLease.heartbeatIntervalMs must be positive and less than ttlMs")
            }
        }
    }
}

impl std::error::Error for SqliteWriterLeaseOptionsError {}

/// Repository options.
#[derive(Clone)]
pub struct SqliteSessionRepositoryOptions {
    pub env: Arc<dyn FileSystem>,
    pub database_path: String,
    pub writer_lease: Option<SqliteWriterLeaseOptions>,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedWriterLeaseOptions {
    ttl_ms: i64,
    heartbeat_interval_ms: i64,
}

fn resolve_writer_lease_options(
    options: Option<&SqliteWriterLeaseOptions>,
) -> Result<ResolvedWriterLeaseOptions, SqliteWriterLeaseOptionsError> {
    let ttl_ms = options.and_then(|options| options.ttl_ms).unwrap_or(30_000);
    let heartbeat_interval_ms = options
        .and_then(|options| options.heartbeat_interval_ms)
        .unwrap_or(10_000);
    if ttl_ms <= 0 {
        return Err(SqliteWriterLeaseOptionsError::TtlNotPositive);
    }
    if heartbeat_interval_ms <= 0 || heartbeat_interval_ms >= ttl_ms {
        return Err(SqliteWriterLeaseOptionsError::HeartbeatNotPositiveOrNotLessThanTtl);
    }
    Ok(ResolvedWriterLeaseOptions {
        ttl_ms,
        heartbeat_interval_ms,
    })
}

fn active_writer_error(session_id: &str) -> SessionError {
    SessionError::new(
        SessionErrorCode::Storage,
        format!("SQLite session {session_id} already has an active writer"),
    )
}

fn lost_writer_error(session_id: &str) -> SessionError {
    SessionError::new(
        SessionErrorCode::Storage,
        format!("SQLite session {session_id} writer lease was lost"),
    )
}

fn claim_writer_lease(
    db: &rusqlite::Connection,
    session_id: &str,
    options: ResolvedWriterLeaseOptions,
    now: i64,
) -> Result<WriterLease, SqliteError> {
    let owner_id = pi_ai::utils::uuid::uuidv7();
    match acquire_writer_lease(db, session_id, &owner_id, now, now + options.ttl_ms)? {
        Some(lease) => Ok(lease),
        None => Err(SqliteError::Session(active_writer_error(session_id))),
    }
}

fn result_or_throw<T>(result: Result<T, FileError>, message: &str) -> Result<T, SessionError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let code = if error.code == FileErrorCode::NotFound {
                SessionErrorCode::NotFound
            } else {
                SessionErrorCode::Storage
            };
            Err(SessionError::new(code, format!("{message}: {}", error.message)))
        }
    }
}

fn get_parent_path(path: &str) -> String {
    let normalized = path.trim_end_matches(['/', '\\']);
    match normalized.rfind(['/', '\\']) {
        None => ".".to_owned(),
        Some(0) => normalized[..1].to_owned(),
        Some(index) => normalized[..index].to_owned(),
    }
}

fn entry_row_from_cached(row: &CachedBranchEntryRow) -> EntryRow {
    EntryRow {
        session_id: row.session_id.clone(),
        seq: row.entry_seq,
        id: row.id.clone(),
        parent_id: row.parent_id.clone(),
        entry_type: row.entry_type.clone(),
        timestamp: row.timestamp,
        payload: row.payload.clone(),
    }
}

fn invalid_entry_error(row: &EntryRow) -> SqliteError {
    SqliteError::Session(SessionError::new(
        SessionErrorCode::InvalidEntry,
        format!(
            "Invalid SQLite session entry {}: failed to decode entry {}",
            row.id, row.id
        ),
    ))
}

fn invalid_record_error(seq: i64) -> SqliteError {
    SqliteError::Session(SessionError::new(
        SessionErrorCode::Storage,
        format!("Invalid SQLite session record at sequence {seq}: failed to decode payload"),
    ))
}

fn decode_entry(row: &EntryRow) -> Result<Entry, SqliteError> {
    let payload: Value = serde_json::from_str(&row.payload).map_err(|_| invalid_entry_error(row))?;
    let Value::Object(object) = payload else {
        return Err(invalid_entry_error(row));
    };
    let body = decode_entry_body(row, &object)?;
    Ok(Entry {
        body,
        parent_id: row.parent_id.clone(),
        seq: row.seq,
        timestamp: row.timestamp,
    })
}

fn decode_entry_body(row: &EntryRow, payload: &serde_json::Map<String, Value>) -> Result<EntryBody, SqliteError> {
    let id = row.id.clone();
    match row.entry_type.as_str() {
        "message" => {
            let message = payload
                .get("message")
                .filter(|value| value.is_object())
                .ok_or_else(|| invalid_entry_error(row))?;
            let message = serde_json::from_value(message.clone()).map_err(|_| invalid_entry_error(row))?;
            let terminate = match payload.get("terminate") {
                Some(Value::Bool(true)) => Some(true),
                _ => None,
            };
            Ok(EntryBody::Message { id, message, terminate })
        }
        "model_change" => Ok(EntryBody::ModelChange {
            id,
            provider: string_field(payload, "provider", row)?,
            model_id: string_field(payload, "modelId", row)?,
        }),
        "thinking_level_change" => Ok(EntryBody::ThinkingLevelChange {
            id,
            thinking_level: string_field(payload, "thinkingLevel", row)?,
        }),
        "active_tools_change" => {
            let names = payload
                .get("activeToolNames")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid_entry_error(row))?;
            let mut active_tool_names = Vec::with_capacity(names.len());
            for name in names {
                active_tool_names.push(name.as_str().ok_or_else(|| invalid_entry_error(row))?.to_owned());
            }
            Ok(EntryBody::ActiveToolsChange { id, active_tool_names })
        }
        "compaction" => {
            let summary = string_field(payload, "summary", row)?;
            let retained_tail = payload
                .get("retainedTail")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid_entry_error(row))?;
            let retained_tail =
                serde_json::from_value(Value::Array(retained_tail.clone())).map_err(|_| invalid_entry_error(row))?;
            let tokens_before = payload
                .get("tokensBefore")
                .and_then(Value::as_f64)
                .ok_or_else(|| invalid_entry_error(row))?;
            Ok(EntryBody::Compaction {
                id,
                summary,
                tokens_before,
                retained_tail,
                duration_ms: payload.get("durationMs").and_then(Value::as_f64),
                details: optional_value(payload, "details"),
                usage: optional_usage(payload, row)?,
            })
        }
        "branch_summary" => Ok(EntryBody::BranchSummary {
            id,
            from_id: string_field(payload, "fromId", row)?,
            summary: string_field(payload, "summary", row)?,
            details: optional_value(payload, "details"),
            usage: optional_usage(payload, row)?,
        }),
        "custom" => Ok(EntryBody::Custom {
            id,
            custom_type: string_field(payload, "customType", row)?,
            data: optional_value(payload, "data"),
        }),
        _ => Err(invalid_entry_error(row)),
    }
}

fn string_field(payload: &serde_json::Map<String, Value>, name: &str, row: &EntryRow) -> Result<String, SqliteError> {
    payload
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid_entry_error(row))
}

fn optional_value(payload: &serde_json::Map<String, Value>, name: &str) -> OptionalValue {
    match payload.get(name) {
        Some(value) => OptionalValue::Present(value.clone()),
        None => OptionalValue::Missing,
    }
}

fn optional_usage(payload: &serde_json::Map<String, Value>, row: &EntryRow) -> Result<Option<Usage>, SqliteError> {
    match payload.get("usage") {
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|_| invalid_entry_error(row)),
        None => Ok(None),
    }
}

fn record_run_id(record: &RecordBody) -> Option<String> {
    record.run_id().map(str::to_owned)
}

fn record_op_kind(record: &RecordBody) -> Option<String> {
    match record {
        RecordBody::OperationStarted { intent, .. } => Some(intent.kind().as_str().to_owned()),
        _ => None,
    }
}

fn decode_record(row: &RecordRow) -> Result<LaneRecord, SqliteError> {
    let payload: Value = serde_json::from_str(&row.payload).map_err(|_| invalid_record_error(row.seq))?;
    let Value::Object(mut object) = payload else {
        return Err(invalid_record_error(row.seq));
    };
    object.insert("seq".to_owned(), Value::from(row.seq));
    object.insert("timestamp".to_owned(), Value::from(row.timestamp));
    serde_json::from_value(Value::Object(object)).map_err(|_| invalid_record_error(row.seq))
}

fn validate_cached_branch_rows(rows: &[CachedBranchEntryRow], query: &StorageBranchQuery) -> Result<(), SqliteError> {
    if rows.is_empty() || query.entry.entry_type.is_some() || query.entry.custom_type.is_some() {
        return Ok(());
    }
    let mut path = rows.to_vec();
    path.sort_by_key(|row| row.entry_seq);
    let should_include_root = query.stop_at_id.is_none()
        && query.stop_at_type.is_none()
        && query.entry.cursor.is_none()
        && (query.entry.order == Some(EntryOrder::OldestFirst) || query.entry.limit.is_none());
    if should_include_root && path[0].parent_id.is_some() {
        return Err(missing_entry_error(path[0].parent_id.as_deref()));
    }
    for index in 1..path.len() {
        if path[index].parent_id.as_deref() != Some(path[index - 1].id.as_str()) {
            return Err(missing_entry_error(path[index].parent_id.as_deref()));
        }
    }
    Ok(())
}

fn missing_entry_error(id: Option<&str>) -> SqliteError {
    SqliteError::Session(SessionError::new(
        SessionErrorCode::InvalidEntry,
        format!("Entry {} not found", id.unwrap_or_default()),
    ))
}

fn matches_entry_query(entry: &Entry, query: &EntryQuery) -> bool {
    let type_matches = query.entry_type.is_none() || query.entry_type == Some(entry.entry_type());
    let custom_matches = query.custom_type.is_none()
        || (entry.entry_type() == EntryType::Custom && entry.body.custom_type() == query.custom_type.as_deref());
    let cursor_matches = match query.cursor {
        None => true,
        Some(EntryCursor { after_seq }) => {
            if query.order == Some(EntryOrder::OldestFirst) {
                entry.seq > after_seq
            } else {
                entry.seq < after_seq
            }
        }
    };
    type_matches && custom_matches && cursor_matches
}

fn assert_unused_id(db: &rusqlite::Connection, session_id: &str, id: &str) -> Result<(), SqliteError> {
    if id_exists_in_entries(db, session_id, id)? || id_exists_in_records(db, session_id, id)? {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::AlreadyExists,
            format!("ID already exists: {id}"),
        )));
    }
    Ok(())
}

fn require_session_row(db: &rusqlite::Connection, session_id: &str) -> Result<SessionRow, SqliteError> {
    match read_session_row(db, session_id)? {
        Some(row) => Ok(row),
        None => Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::NotFound,
            format!("Session not found: {session_id}"),
        ))),
    }
}

fn parse_json_string(value: Option<&str>, session_id: &str, field: &str) -> Result<Option<String>, SqliteError> {
    let Some(value) = value else {
        return Ok(None);
    };
    serde_json::from_str::<String>(value).map(Some).map_err(|_| {
        SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Invalid SQLite session {session_id}: {field} must be a string"),
        ))
    })
}

enum PendingLogRow {
    Entry(EntryRow),
    Record(RecordRow),
    Lane(LaneMoveRow),
    Fact(FactRow),
}

impl PendingLogRow {
    fn decode(self, session_id: &str) -> Result<LogItem, SqliteError> {
        match self {
            Self::Entry(row) => Ok(LogItem::Entry {
                seq: row.seq,
                entry: decode_entry(&row)?,
            }),
            Self::Record(row) => Ok(LogItem::Record {
                seq: row.seq,
                record: decode_record(&row)?,
            }),
            Self::Lane(row) => Ok(LogItem::Lane {
                seq: row.seq,
                lane: row.lane,
                leaf_id: row.leaf_id,
            }),
            Self::Fact(row) => {
                let fact = if row.kind == "name" {
                    FactItem::Name {
                        name: parse_json_string(row.value.as_deref(), session_id, "name")?,
                    }
                } else {
                    FactItem::Label {
                        target_id: row.key.unwrap_or_default(),
                        label: parse_json_string(row.value.as_deref(), session_id, "label")?,
                    }
                };
                Ok(LogItem::Fact { seq: row.seq, fact })
            }
        }
    }
}

/// One claimed session writer.
pub struct SqliteSessionStorage {
    db: Arc<Database>,
    metadata: SqliteSessionMetadata,
    lease: Mutex<WriterLease>,
    lease_options: ResolvedWriterLeaseOptions,
    on_release: Box<dyn Fn() + Send + Sync>,
    write_lock: Mutex<()>,
    heartbeat: Mutex<Option<tokio::task::JoinHandle<()>>>,
    lease_error: Mutex<Option<SessionError>>,
    closing: AtomicBool,
    release_lock: tokio::sync::Mutex<bool>,
    clock: Clock,
}

impl SqliteSessionStorage {
    fn new(
        db: Arc<Database>,
        metadata: SqliteSessionMetadata,
        lease: WriterLease,
        lease_options: ResolvedWriterLeaseOptions,
        clock: Clock,
        on_release: Box<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            db,
            metadata,
            lease: Mutex::new(lease),
            lease_options,
            on_release,
            write_lock: Mutex::new(()),
            heartbeat: Mutex::new(None),
            lease_error: Mutex::new(None),
            closing: AtomicBool::new(false),
            release_lock: tokio::sync::Mutex::new(false),
            clock,
        }
    }

    pub fn is_for_session(&self, session_id: &str) -> bool {
        self.metadata.id == session_id
    }

    pub fn metadata(&self) -> &SqliteSessionMetadata {
        &self.metadata
    }

    /// Starts the idle lease heartbeat. Called once the storage is owned by an `Arc`.
    fn start_heartbeat(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        let interval = Duration::from_millis(self.lease_options.heartbeat_interval_ms as u64);
        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(storage) = weak.upgrade() else {
                    return;
                };
                storage.heartbeat_once();
                if storage.closing.load(Ordering::SeqCst) {
                    return;
                }
            }
        });
        *self.heartbeat.lock().expect("heartbeat mutex") = Some(handle);
    }

    fn stop_heartbeat(&self) {
        let handle = self.heartbeat.lock().expect("heartbeat mutex").take();
        if let Some(handle) = handle {
            handle.abort();
        }
    }

    fn heartbeat_once(&self) {
        if self.closing.load(Ordering::SeqCst) {
            return;
        }
        let Ok(_guard) = self.write_lock.lock() else {
            return;
        };
        if self.closing.load(Ordering::SeqCst) || self.lease_error.lock().expect("lease error mutex").is_some() {
            return;
        }
        let now = (self.clock)();
        let renewed = self.db.with_connection(|connection| {
            transaction(connection, |connection| {
                let mut lease = self.lease.lock().expect("lease mutex");
                let renewed = renew_writer_lease(
                    connection,
                    &self.metadata.id,
                    &mut lease,
                    now,
                    now + self.lease_options.ttl_ms,
                )?;
                if !renewed {
                    *self.lease_error.lock().expect("lease error mutex") = Some(lost_writer_error(&self.metadata.id));
                }
                Ok(())
            })
        });
        // A transient heartbeat failure is retried; every write still verifies ownership transactionally.
        let _ = renewed;
    }

    fn enqueue_write<T>(
        &self,
        operation: impl FnOnce(&rusqlite::Connection) -> Result<T, SqliteError>,
    ) -> Result<T, SqliteError> {
        if self.closing.load(Ordering::SeqCst) {
            return Err(SqliteError::Session(SessionError::new(
                SessionErrorCode::Storage,
                format!("SQLite session {} is closed", self.metadata.id),
            )));
        }
        let _guard = self.write_lock.lock().expect("write mutex");
        if let Some(error) = self.lease_error.lock().expect("lease error mutex").clone() {
            return Err(SqliteError::Session(error));
        }
        let now = (self.clock)();
        self.db.with_connection(|connection| {
            transaction(connection, |connection| {
                let mut lease = self.lease.lock().expect("lease mutex");
                let renewed = renew_writer_lease(
                    connection,
                    &self.metadata.id,
                    &mut lease,
                    now,
                    now + self.lease_options.ttl_ms,
                )?;
                drop(lease);
                if !renewed {
                    let error = lost_writer_error(&self.metadata.id);
                    *self.lease_error.lock().expect("lease error mutex") = Some(error.clone());
                    self.stop_heartbeat();
                    return Err(SqliteError::Session(error));
                }
                operation(connection)
            })
        })
    }

    /// Releases the writer claim. Idempotent.
    pub async fn release(&self) -> Result<(), SessionError> {
        let mut released = self.release_lock.lock().await;
        if *released {
            return Ok(());
        }
        *released = true;
        self.closing.store(true, Ordering::SeqCst);
        self.stop_heartbeat();
        let result = {
            let _guard = self.write_lock.lock().expect("write mutex");
            self.db.with_connection(|connection| {
                transaction(connection, |connection| {
                    let lease = self.lease.lock().expect("lease mutex").clone();
                    release_writer_lease(connection, &self.metadata.id, &lease)
                })
            })
        };
        (self.on_release)();
        result.map_err(SqliteError::into_session)
    }
}

impl SessionStorage<SqliteSessionMetadata> for SqliteSessionStorage {
    fn get_metadata<'a>(&'a self) -> BoxFuture<'a, Result<SqliteSessionMetadata, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| {
                    let row = require_session_row(connection, &self.metadata.id)?;
                    decode_session_metadata(&row, &self.metadata.path)
                })
                .map_err(SqliteError::into_session)
        })
    }

    fn get_lanes<'a>(&'a self) -> BoxFuture<'a, Result<Vec<LanePointer>, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| {
                    read_lanes(connection, &self.metadata.id).map(|rows| {
                        rows.into_iter()
                            .map(|row| LanePointer {
                                lane: row.lane,
                                leaf_id: row.leaf_id,
                            })
                            .collect()
                    })
                })
                .map_err(SqliteError::into_session)
        })
    }

    fn create_lane<'a>(&'a self, lane: &'a str, at: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            self.enqueue_write(|connection| {
                if read_lane(connection, &self.metadata.id, lane)?.is_some() {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::AlreadyExists,
                        format!("Lane already exists: {lane}"),
                    )));
                }
                if let Some(at) = at
                    && read_entry_row(connection, &self.metadata.id, at)?.is_none()
                {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::NotFound,
                        format!("Entry not found: {at}"),
                    )));
                }
                let seq = get_next_sequence(connection, &self.metadata.id)?;
                create_lane(connection, &self.metadata.id, seq, lane, at)?;
                advance_sequence(connection, &self.metadata.id, seq)
            })
            .map_err(SqliteError::into_session)
        })
    }

    fn move_lane<'a>(&'a self, lane: &'a str, to: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            self.enqueue_write(|connection| {
                if read_lane(connection, &self.metadata.id, lane)?.is_none() {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::InvalidLane,
                        format!("Lane not found: {lane}"),
                    )));
                }
                if let Some(to) = to
                    && read_entry_row(connection, &self.metadata.id, to)?.is_none()
                {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::NotFound,
                        format!("Entry not found: {to}"),
                    )));
                }
                let seq = get_next_sequence(connection, &self.metadata.id)?;
                move_lane(connection, &self.metadata.id, seq, lane, to)?;
                advance_sequence(connection, &self.metadata.id, seq)
            })
            .map_err(SqliteError::into_session)
        })
    }

    fn append_entry<'a>(&'a self, entry: EntryBody, lane: &'a str) -> BoxFuture<'a, Result<Entry, SessionError>> {
        Box::pin(async move {
            self.enqueue_write(|connection| {
                let parent_id = read_lane_head(connection, &self.metadata.id, lane)?;
                assert_unused_id(connection, &self.metadata.id, entry.id())?;
                let seq = get_next_sequence(connection, &self.metadata.id)?;
                let committed = Entry {
                    body: entry,
                    parent_id,
                    seq,
                    timestamp: (self.clock)(),
                };
                let payload = entry_payload(&committed.body);
                insert_entry_row(
                    connection,
                    &self.metadata.id,
                    &NewEntryRow {
                        seq,
                        id: committed.id().to_owned(),
                        parent_id: committed.parent_id.clone(),
                        entry_type: committed.entry_type().as_str().to_owned(),
                        timestamp: committed.timestamp,
                        payload: Value::Object(payload).to_string(),
                    },
                )?;
                set_lane_leaf(connection, &self.metadata.id, lane, Some(committed.id()))?;
                append_entry_to_branch_cache(
                    connection,
                    &self.metadata.id,
                    committed.id(),
                    seq,
                    committed.entry_type().as_str(),
                    committed.body.custom_type(),
                    committed.parent_id.as_deref(),
                )?;
                if committed.entry_type() == EntryType::Message {
                    increment_message_count(connection, &self.metadata.id)?;
                }
                advance_sequence(connection, &self.metadata.id, seq)?;
                Ok(committed)
            })
            .map_err(SqliteError::into_session)
        })
    }

    fn append_record<'a>(&'a self, record: RecordBody) -> BoxFuture<'a, Result<LaneRecord, SessionError>> {
        Box::pin(async move {
            self.enqueue_write(|connection| {
                let lane = record.lane().to_owned();
                if read_lane(connection, &self.metadata.id, &lane)?.is_none() {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::InvalidLane,
                        format!("Lane not found: {lane}"),
                    )));
                }
                assert_unused_id(connection, &self.metadata.id, record.id())?;
                let seq = get_next_sequence(connection, &self.metadata.id)?;
                let committed = LaneRecord {
                    body: record,
                    seq,
                    timestamp: (self.clock)(),
                };
                if committed.record_type() == RecordType::OperationStarted {
                    start_lane_operation(connection, &self.metadata.id, &lane, committed.id())?;
                }
                let payload = serde_json::to_value(&committed.body).expect("records serialize");
                append_record_row(
                    connection,
                    &self.metadata.id,
                    &NewRecordRow {
                        seq,
                        id: committed.id().to_owned(),
                        lane: lane.clone(),
                        run_id: record_run_id(&committed.body),
                        record_type: committed.record_type().as_str().to_owned(),
                        op_kind: record_op_kind(&committed.body),
                        timestamp: committed.timestamp,
                        payload: payload.to_string(),
                    },
                )?;
                if committed.record_type() == RecordType::OperationFinished {
                    let run_id = committed.body.run_id().unwrap_or_default().to_owned();
                    finish_lane_operation(connection, &self.metadata.id, &lane, &run_id)?;
                }
                if let Some(usage) = committed.body.usage() {
                    add_usage_to_stats(connection, &self.metadata.id, usage)?;
                }
                advance_sequence(connection, &self.metadata.id, seq)?;
                Ok(committed)
            })
            .map_err(SqliteError::into_session)
        })
    }

    fn get_entry<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<Entry>, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| match read_entry_row(connection, &self.metadata.id, id)? {
                    Some(row) => decode_entry(&row).map(Some),
                    None => Ok(None),
                })
                .map_err(SqliteError::into_session)
        })
    }

    fn find_entries<'a>(&'a self, query: EntryQuery) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>> {
        Box::pin(async move {
            let sql_type = match (&query.entry_type, &query.custom_type) {
                (Some(entry_type), _) => Some(entry_type.as_str().to_owned()),
                (None, Some(_)) => Some(EntryType::Custom.as_str().to_owned()),
                (None, None) => None,
            };
            let sql_limit = if query.custom_type.is_none() { query.limit } else { None };
            let entries = self
                .db
                .with_connection(|connection| {
                    let rows = read_entry_rows(
                        connection,
                        &self.metadata.id,
                        &EntryRowsQuery {
                            cursor_after_seq: query.cursor.map(|cursor| cursor.after_seq),
                            limit: sql_limit,
                            order: query.order,
                            entry_type: sql_type,
                            after_seq: None,
                        },
                    )?;
                    let mut entries = Vec::with_capacity(rows.len());
                    for row in &rows {
                        let entry = decode_entry(row)?;
                        if matches_entry_query(&entry, &query) {
                            entries.push(entry);
                        }
                    }
                    Ok(entries)
                })
                .map_err(SqliteError::into_session)?;
            Ok(match query.limit {
                Some(limit) => entries.into_iter().take(limit).collect(),
                None => entries,
            })
        })
    }

    fn find_entries_on_branch<'a>(
        &'a self,
        query: StorageBranchQuery,
    ) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>> {
        Box::pin(async move {
            let entries = self
                .db
                .with_connection(|connection| {
                    let Some(cached) = read_cached_branch(connection, &self.metadata.id, &query.start)? else {
                        if read_entry_row(connection, &self.metadata.id, &query.start)?.is_none() {
                            return Err(SqliteError::Session(SessionError::new(
                                SessionErrorCode::NotFound,
                                format!("Entry not found: {}", query.start),
                            )));
                        }
                        return Err(SqliteError::Session(SessionError::new(
                            SessionErrorCode::InvalidEntry,
                            format!("Branch cache missing entry {}", query.start),
                        )));
                    };
                    let rows = query_cached_branch_rows(
                        connection,
                        &self.metadata.id,
                        cached,
                        &CachedBranchQuery {
                            entry_type: query.entry.entry_type.map(|entry_type| entry_type.as_str().to_owned()),
                            custom_type: query.entry.custom_type.clone(),
                            stop_at_type: query.stop_at_type.map(|entry_type| entry_type.as_str().to_owned()),
                            stop_at_id: query.stop_at_id.clone(),
                            cursor_after_seq: query.entry.cursor.map(|cursor| cursor.after_seq),
                            order: query.entry.order,
                            limit: query.entry.limit,
                        },
                    )?;
                    validate_cached_branch_rows(&rows, &query)?;
                    let mut entries = Vec::with_capacity(rows.len());
                    for row in &rows {
                        let entry = decode_entry(&entry_row_from_cached(row))?;
                        if matches_entry_query(&entry, &query.entry) {
                            entries.push(entry);
                        }
                    }
                    Ok(entries)
                })
                .map_err(SqliteError::into_session)?;
            Ok(match query.entry.limit {
                Some(limit) => entries.into_iter().take(limit).collect(),
                None => entries,
            })
        })
    }

    fn find_records<'a>(&'a self, query: RecordQuery) -> BoxFuture<'a, Result<Vec<LaneRecord>, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| {
                    let rows = read_record_rows(
                        connection,
                        &self.metadata.id,
                        &RecordRowsQuery {
                            lane: query.lane.clone(),
                            record_type: query.record_type.map(|record_type| record_type.as_str().to_owned()),
                            run_id: query.run_id.clone(),
                            operation_kind: query.operation_kind.map(|kind| kind.as_str().to_owned()),
                            after_seq: query.after_seq,
                            order: query.order,
                            limit: query.limit,
                        },
                    )?;
                    rows.iter().map(decode_record).collect()
                })
                .map_err(SqliteError::into_session)
        })
    }

    fn find_open_operations<'a>(
        &'a self,
        lane: &'a str,
        _limit: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<LaneRecord>, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| {
                    let rows = read_open_operation_rows(connection, &self.metadata.id, lane)?;
                    let mut records = Vec::with_capacity(rows.len());
                    for row in &rows {
                        let record = decode_record(row)?;
                        if record.record_type() != RecordType::OperationStarted {
                            return Err(SqliteError::Session(SessionError::new(
                                SessionErrorCode::Storage,
                                "Expected operation_started record",
                            )));
                        }
                        records.push(record);
                    }
                    Ok(records)
                })
                .map_err(SqliteError::into_session)
        })
    }

    fn get_log<'a>(&'a self, options: LogOptions) -> BoxFuture<'a, Result<Vec<LogItem>, SessionError>> {
        Box::pin(async move {
            let items = self
                .db
                .with_connection(|connection| {
                    let after_seq = options.after_seq.unwrap_or(0);
                    let limit = options.limit;
                    let entry_rows = read_entry_rows(
                        connection,
                        &self.metadata.id,
                        &EntryRowsQuery {
                            after_seq: Some(after_seq),
                            order: Some(EntryOrder::OldestFirst),
                            limit,
                            ..EntryRowsQuery::default()
                        },
                    )?;
                    let record_rows = read_record_rows(
                        connection,
                        &self.metadata.id,
                        &RecordRowsQuery {
                            after_seq: Some(after_seq),
                            order: Some(EntryOrder::OldestFirst),
                            limit,
                            ..RecordRowsQuery::default()
                        },
                    )?;
                    let lane_rows = read_lane_move_rows(connection, &self.metadata.id, Some(after_seq), limit)?;
                    let fact_rows = read_fact_rows(connection, &self.metadata.id, Some(after_seq), limit)?;

                    // Rows are decoded only after the combined log is truncated, like the source's
                    // lazy `decode` closures.
                    let mut items: Vec<(i64, PendingLogRow)> = Vec::new();
                    items.extend(entry_rows.into_iter().map(|row| (row.seq, PendingLogRow::Entry(row))));
                    items.extend(record_rows.into_iter().map(|row| (row.seq, PendingLogRow::Record(row))));
                    items.extend(lane_rows.into_iter().map(|row| (row.seq, PendingLogRow::Lane(row))));
                    items.extend(fact_rows.into_iter().map(|row| (row.seq, PendingLogRow::Fact(row))));
                    items.sort_by_key(|(seq, _)| *seq);
                    if let Some(limit) = limit {
                        items.truncate(limit);
                    }
                    items
                        .into_iter()
                        .map(|(_, row)| row.decode(&self.metadata.id))
                        .collect::<Result<Vec<_>, _>>()
                })
                .map_err(SqliteError::into_session)?;
            Ok(items)
        })
    }

    fn get_name<'a>(&'a self) -> BoxFuture<'a, Result<Option<String>, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| {
                    let row = read_latest_fact(connection, &self.metadata.id, "name", None)?;
                    parse_json_string(
                        row.as_ref().and_then(|row| row.value.as_deref()),
                        &self.metadata.id,
                        "name",
                    )
                })
                .map_err(SqliteError::into_session)
        })
    }

    fn set_name<'a>(&'a self, name: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let value = name.map(|name| Value::String(name.to_owned()).to_string());
            self.enqueue_write(|connection| {
                let seq = get_next_sequence(connection, &self.metadata.id)?;
                append_fact(connection, &self.metadata.id, seq, "name", None, value.as_deref())?;
                advance_sequence(connection, &self.metadata.id, seq)
            })
            .map_err(SqliteError::into_session)
        })
    }

    fn get_label<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<String>, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| {
                    let row = read_latest_fact(connection, &self.metadata.id, "label", Some(id))?;
                    parse_json_string(
                        row.as_ref().and_then(|row| row.value.as_deref()),
                        &self.metadata.id,
                        "label",
                    )
                })
                .map_err(SqliteError::into_session)
        })
    }

    fn set_label<'a>(&'a self, id: &'a str, label: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(async move {
            let value = label.map(|label| Value::String(label.to_owned()).to_string());
            self.enqueue_write(|connection| {
                if read_entry_row(connection, &self.metadata.id, id)?.is_none() {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::NotFound,
                        format!("Entry not found: {id}"),
                    )));
                }
                let seq = get_next_sequence(connection, &self.metadata.id)?;
                append_fact(connection, &self.metadata.id, seq, "label", Some(id), value.as_deref())?;
                advance_sequence(connection, &self.metadata.id, seq)
            })
            .map_err(SqliteError::into_session)
        })
    }

    fn get_stats<'a>(&'a self) -> BoxFuture<'a, Result<SessionStats, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| read_stats(connection, &self.metadata.id))
                .map_err(SqliteError::into_session)
        })
    }
}

type StorageRegistry = Arc<Mutex<Vec<Arc<SqliteSessionStorage>>>>;

/// SQLite session repository.
pub struct SqliteSessionRepository {
    env: Arc<dyn FileSystem>,
    database_path_input: String,
    lease_options: ResolvedWriterLeaseOptions,
    database_path: Mutex<Option<String>>,
    database: Mutex<Option<Arc<Database>>>,
    active_storages: StorageRegistry,
    operations: tokio::sync::Mutex<()>,
    clock: Clock,
    id_generator: Arc<dyn IdGenerator>,
}

impl SqliteSessionRepository {
    pub fn new(options: SqliteSessionRepositoryOptions) -> Result<Self, SqliteWriterLeaseOptionsError> {
        Self::with_clock_and_id_generator(options, system_clock(), Arc::new(UuidV7IdGenerator))
    }

    pub fn with_clock(
        options: SqliteSessionRepositoryOptions,
        clock: Clock,
    ) -> Result<Self, SqliteWriterLeaseOptionsError> {
        Self::with_clock_and_id_generator(options, clock, Arc::new(UuidV7IdGenerator))
    }

    pub fn with_clock_and_id_generator(
        options: SqliteSessionRepositoryOptions,
        clock: Clock,
        id_generator: Arc<dyn IdGenerator>,
    ) -> Result<Self, SqliteWriterLeaseOptionsError> {
        let lease_options = resolve_writer_lease_options(options.writer_lease.as_ref())?;
        Ok(Self {
            env: options.env,
            database_path_input: options.database_path,
            lease_options,
            database_path: Mutex::new(None),
            database: Mutex::new(None),
            active_storages: Arc::new(Mutex::new(Vec::new())),
            operations: tokio::sync::Mutex::new(()),
            clock,
            id_generator,
        })
    }

    async fn release_storages_for_session(&self, session_id: &str) {
        let storages: Vec<Arc<SqliteSessionStorage>> = self
            .active_storages
            .lock()
            .expect("active storages mutex")
            .iter()
            .filter(|storage| storage.is_for_session(session_id))
            .cloned()
            .collect();
        for storage in storages {
            let _ = storage.release().await;
        }
    }

    fn build_storage(
        &self,
        db: Arc<Database>,
        metadata: SqliteSessionMetadata,
        lease: WriterLease,
    ) -> Arc<SqliteSessionStorage> {
        let registry = Arc::clone(&self.active_storages);
        let clock = Arc::clone(&self.clock);
        let lease_options = self.lease_options;
        let storage = Arc::new_cyclic(|weak| {
            let weak = weak.clone();
            SqliteSessionStorage::new(
                db,
                metadata,
                lease,
                lease_options,
                clock,
                Box::new(move || {
                    if let Some(storage) = weak.upgrade() {
                        registry
                            .lock()
                            .expect("active storages mutex")
                            .retain(|candidate| !Arc::ptr_eq(candidate, &storage));
                    }
                }),
            )
        });
        self.active_storages
            .lock()
            .expect("active storages mutex")
            .push(Arc::clone(&storage));
        storage.start_heartbeat();
        storage
    }

    fn claim_session(
        &self,
        db: Arc<Database>,
        metadata: &SqliteSessionMetadata,
    ) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        let active = self
            .active_storages
            .lock()
            .expect("active storages mutex")
            .iter()
            .find(|storage| storage.is_for_session(&metadata.id))
            .cloned();
        if let Some(active) = active {
            db.with_connection(|connection| read_lanes(connection, &metadata.id))
                .map_err(SqliteError::into_session)?;
            return Ok(Session::new(active));
        }
        let storage = self.claim_storage(db, metadata)?;
        Ok(Session::new(storage))
    }

    fn claim_storage(
        &self,
        db: Arc<Database>,
        metadata: &SqliteSessionMetadata,
    ) -> Result<Arc<SqliteSessionStorage>, SessionError> {
        let path = metadata.path.clone();
        let lease_options = self.lease_options;
        let now = (self.clock)();
        let (row, lease) = db
            .with_connection(|connection| {
                require_session_row(connection, &metadata.id)?;
                transaction(connection, |connection| {
                    let lease = claim_writer_lease(connection, &metadata.id, lease_options, now)?;
                    let row = require_session_row(connection, &metadata.id)?;
                    read_lanes(connection, &metadata.id)?;
                    Ok((row, lease))
                })
            })
            .map_err(SqliteError::into_session)?;
        let metadata = decode_session_metadata(&row, &path).map_err(SqliteError::into_session)?;
        Ok(self.build_storage(db, metadata, lease))
    }

    pub async fn create(
        &self,
        options: SqliteSessionCreateOptions,
    ) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        let _guard = self.operations.lock().await;
        let db = self.get_database().await?;
        let path = self.get_database_path().await?;
        let id = options.id.clone().unwrap_or_else(|| self.id_generator.next());
        db.with_connection(|connection| {
            if session_exists(connection, &id)? {
                return Err(SqliteError::Session(SessionError::new(
                    SessionErrorCode::AlreadyExists,
                    format!("Session already exists: {id}"),
                )));
            }
            Ok(())
        })
        .map_err(SqliteError::into_session)?;
        let created_at = (self.clock)();
        let lease_options = self.lease_options;
        let lease = db
            .with_connection(|connection| {
                transaction(connection, |connection| {
                    insert_session_row(
                        connection,
                        &NewSessionRow {
                            id: id.clone(),
                            created_at,
                            cwd: options.cwd.clone(),
                            parent_session_id: options.parent_session_id.clone(),
                            metadata: options.metadata.clone(),
                        },
                    )?;
                    create_sequence(connection, &id, 1)?;
                    create_stats(connection, &id, 0)?;
                    create_initial_lane(connection, &id, "main", None)?;
                    claim_writer_lease(connection, &id, lease_options, created_at)
                })
            })
            .map_err(SqliteError::into_session)?;
        let row = db
            .with_connection(|connection| require_session_row(connection, &id))
            .map_err(SqliteError::into_session)?;
        let metadata = decode_session_metadata(&row, &path).map_err(SqliteError::into_session)?;
        Ok(Session::new(self.build_storage(db, metadata, lease)))
    }

    pub async fn open(&self, metadata: &SqliteSessionMetadata) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        let _guard = self.operations.lock().await;
        let db = self.get_database().await?;
        self.claim_session(db, metadata)
    }

    /// Rebuilds the private branch cache from canonical entry parent links.
    pub async fn repair_branch_cache(&self, metadata: &SqliteSessionMetadata) -> Result<(), SessionError> {
        let _guard = self.operations.lock().await;
        self.release_storages_for_session(&metadata.id).await;
        let db = self.get_database().await?;
        let lease_options = self.lease_options;
        let now = (self.clock)();
        db.with_connection(|connection| {
            transaction(connection, |connection| {
                let lease = claim_writer_lease(connection, &metadata.id, lease_options, now)?;
                require_session_row(connection, &metadata.id)?;
                rebuild_branch_cache(connection, &metadata.id)?;
                release_writer_lease(connection, &metadata.id, &lease)
            })
        })
        .map_err(SqliteError::into_session)
    }

    /// Reads the session catalog without acquiring writer leases.
    pub async fn list(&self, options: SqliteSessionListOptions) -> Result<Vec<SqliteSessionMetadata>, SessionError> {
        let _guard = self.operations.lock().await;
        let path = self.get_database_path().await?;
        let exists = result_or_throw(
            self.env.exists(&path, None).await,
            &format!("Failed to check database {path}"),
        )?;
        if !exists {
            return Ok(Vec::new());
        }
        let db = self.get_database().await?;
        db.with_connection(|connection| {
            let rows = read_session_rows(connection, options.cwd.as_deref())?;
            rows.iter().map(|row| decode_session_metadata(row, &path)).collect()
        })
        .map_err(SqliteError::into_session)
    }

    pub async fn delete(&self, metadata: &SqliteSessionMetadata) -> Result<(), SessionError> {
        let _guard = self.operations.lock().await;
        self.release_storages_for_session(&metadata.id).await;
        let db = self.get_database().await?;
        let lease_options = self.lease_options;
        let now = (self.clock)();
        db.with_connection(|connection| {
            transaction(connection, |connection| {
                if !session_exists(connection, &metadata.id)? {
                    delete_writer_lease(connection, &metadata.id)?;
                    return Ok(());
                }
                claim_writer_lease(connection, &metadata.id, lease_options, now)?;
                delete_branch_cache(connection, &metadata.id)?;
                delete_fact_rows(connection, &metadata.id)?;
                delete_lane_rows(connection, &metadata.id)?;
                delete_record_rows(connection, &metadata.id)?;
                delete_entry_rows(connection, &metadata.id)?;
                delete_writer_lease(connection, &metadata.id)?;
                delete_stats(connection, &metadata.id)?;
                delete_sequence(connection, &metadata.id)?;
                delete_session_row(connection, &metadata.id)
            })
        })
        .map_err(SqliteError::into_session)
    }

    pub async fn fork(
        &self,
        source: &SqliteSessionMetadata,
        options: ForkOptions,
        create_options: SqliteSessionCreateOptions,
    ) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        let _guard = self.operations.lock().await;
        let db = self.get_database().await?;
        let path = self.get_database_path().await?;
        let source_metadata = db
            .with_connection(|connection| {
                let row = require_session_row(connection, &source.id)?;
                decode_session_metadata(&row, &path)
            })
            .map_err(SqliteError::into_session)?;
        let id = create_options.id.clone().unwrap_or_else(|| self.id_generator.next());
        db.with_connection(|connection| {
            if session_exists(connection, &id)? {
                return Err(SqliteError::Session(SessionError::new(
                    SessionErrorCode::AlreadyExists,
                    format!("Session already exists: {id}"),
                )));
            }
            Ok(())
        })
        .map_err(SqliteError::into_session)?;

        let is_tree = matches!(options, ForkOptions::Tree);
        let mut entries: Vec<EntryRow> = Vec::new();
        let mut lanes: Vec<LanePointer> = Vec::new();
        let mut branch_tips: Vec<String> = Vec::new();
        let mut branch_fork_target_id: Option<String> = None;

        db.with_connection(|connection| {
            if is_tree {
                entries = read_entry_rows(
                    connection,
                    &source.id,
                    &EntryRowsQuery {
                        order: Some(EntryOrder::OldestFirst),
                        ..EntryRowsQuery::default()
                    },
                )?;
                lanes = read_lanes(connection, &source.id)?
                    .into_iter()
                    .map(|row| LanePointer {
                        lane: row.lane,
                        leaf_id: row.leaf_id,
                    })
                    .collect();
                branch_tips = read_branch_tip_ids(connection, &source.id)?;
                return Ok(());
            }
            let Some(main) = read_lane(connection, &source.id, "main")? else {
                return Err(SqliteError::Session(SessionError::new(
                    SessionErrorCode::InvalidLane,
                    "Lane not found: main",
                )));
            };
            let (explicit_entry_id, explicit_position) = fork_selection(&options);
            let selected_entry_id = explicit_entry_id.clone().or(main.leaf_id.clone());
            if let Some(selected_entry_id) = selected_entry_id {
                let target = read_entry_row(connection, &source.id, &selected_entry_id)?;
                let Some(target) = target else {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::InvalidForkTarget,
                        format!("Fork target is not a message entry: {selected_entry_id}"),
                    )));
                };
                if target.entry_type != EntryType::Message.as_str() {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::InvalidForkTarget,
                        format!("Fork target is not a message entry: {selected_entry_id}"),
                    )));
                }
                let position = explicit_position.unwrap_or(if explicit_entry_id.is_some() {
                    ForkPosition::Before
                } else {
                    ForkPosition::At
                });
                branch_fork_target_id = match position {
                    ForkPosition::At => Some(target.id.clone()),
                    ForkPosition::Before => target.parent_id.clone(),
                };
            }
            lanes.push(LanePointer {
                lane: "main".to_owned(),
                leaf_id: branch_fork_target_id.clone(),
            });
            if let Some(target_id) = branch_fork_target_id.clone() {
                let Some(cached) = read_cached_branch(connection, &source.id, &target_id)? else {
                    return Err(SqliteError::Session(SessionError::new(
                        SessionErrorCode::InvalidForkTarget,
                        format!("Fork target is not on a cached branch: {target_id}"),
                    )));
                };
                let rows = query_cached_branch_rows(
                    connection,
                    &source.id,
                    cached,
                    &CachedBranchQuery {
                        order: Some(EntryOrder::OldestFirst),
                        ..CachedBranchQuery::default()
                    },
                )?;
                entries.extend(rows.iter().map(entry_row_from_cached));
                branch_tips.push(target_id);
            }
            Ok(())
        })
        .map_err(SqliteError::into_session)?;

        let copied_ids: Vec<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        let (latest_name, latest_labels) = db
            .with_connection(|connection| {
                Ok((
                    read_latest_fact(connection, &source.id, "name", None)?,
                    read_latest_label_facts(connection, &source.id)?,
                ))
            })
            .map_err(SqliteError::into_session)?;
        let labels_to_copy: Vec<(Option<String>, String)> = latest_labels
            .into_iter()
            .filter(|(key, _)| is_tree || key.as_ref().is_some_and(|key| copied_ids.iter().any(|id| id == key)))
            .collect();
        let created_at = (self.clock)();
        let metadata = create_options
            .metadata
            .clone()
            .or_else(|| source_metadata.metadata.clone());
        let parent_session_id = create_options
            .parent_session_id
            .clone()
            .or_else(|| Some(source.id.clone()));
        let cwd = create_options.cwd.clone();
        let lease_options = self.lease_options;
        let fork_id = id.clone();
        let name_value = latest_name.as_ref().and_then(|row| row.value.clone());
        let fork_error_id = id.clone();

        let lease = db
            .with_connection(|connection| {
                transaction(connection, |connection| {
                    insert_session_row(
                        connection,
                        &NewSessionRow {
                            id: fork_id.clone(),
                            created_at,
                            cwd: cwd.clone(),
                            parent_session_id: parent_session_id.clone(),
                            metadata: metadata.clone(),
                        },
                    )?;
                    create_sequence(connection, &fork_id, 1)?;
                    let message_count = entries
                        .iter()
                        .filter(|entry| entry.entry_type == EntryType::Message.as_str())
                        .count() as u64;
                    create_stats(connection, &fork_id, message_count)?;

                    let mut next_seq = 1_i64;
                    for entry in &entries {
                        insert_entry_row(
                            connection,
                            &fork_id,
                            &NewEntryRow {
                                seq: next_seq,
                                id: entry.id.clone(),
                                parent_id: entry.parent_id.clone(),
                                entry_type: entry.entry_type.clone(),
                                timestamp: entry.timestamp,
                                payload: entry.payload.clone(),
                            },
                        )?;
                        next_seq += 1;
                    }

                    if is_tree {
                        for lane in &lanes {
                            create_lane(connection, &fork_id, next_seq, &lane.lane, lane.leaf_id.as_deref())?;
                            next_seq += 1;
                        }
                    } else {
                        create_initial_lane(connection, &fork_id, "main", branch_fork_target_id.as_deref())?;
                    }

                    if let Some(name) = name_value.as_deref() {
                        append_fact(connection, &fork_id, next_seq, "name", None, Some(name))?;
                        next_seq += 1;
                    }
                    for (key, value) in &labels_to_copy {
                        append_fact(connection, &fork_id, next_seq, "label", key.as_deref(), Some(value))?;
                        next_seq += 1;
                    }

                    set_next_sequence(connection, &fork_id, next_seq)?;
                    for tip in &branch_tips {
                        build_cached_branch(connection, &fork_id, tip)?;
                    }
                    claim_writer_lease(connection, &fork_id, lease_options, created_at)
                })
                .map_err(|error| match error {
                    SqliteError::Session(error) => SqliteError::Session(error),
                    other => SqliteError::Session(SessionError::new(
                        SessionErrorCode::Storage,
                        format!("Failed to fork SQLite session {fork_error_id}: {other}"),
                    )),
                })
            })
            .map_err(SqliteError::into_session)?;

        let row = db
            .with_connection(|connection| require_session_row(connection, &id))
            .map_err(SqliteError::into_session)?;
        let metadata = decode_session_metadata(&row, &path).map_err(SqliteError::into_session)?;
        Ok(Session::new(self.build_storage(db, metadata, lease)))
    }

    pub async fn close(&self) -> Result<(), SessionError> {
        let _guard = self.operations.lock().await;
        let storages: Vec<Arc<SqliteSessionStorage>> =
            self.active_storages.lock().expect("active storages mutex").clone();
        let mut first_error = None;
        for storage in storages {
            if let Err(error) = storage.release().await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        let database = self.database.lock().expect("database mutex").take();
        if let Some(database) = database {
            database.close();
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn get_database_path(&self) -> Result<String, SessionError> {
        if let Some(path) = self.database_path.lock().expect("database path mutex").clone() {
            return Ok(path);
        }
        let path = result_or_throw(
            self.env.absolute_path(&self.database_path_input, None).await,
            &format!(
                "Failed to resolve SQLite sessions database {}",
                self.database_path_input
            ),
        )?;
        *self.database_path.lock().expect("database path mutex") = Some(path.clone());
        Ok(path)
    }

    async fn get_database(&self) -> Result<Arc<Database>, SessionError> {
        if let Some(database) = self.database.lock().expect("database mutex").clone() {
            return Ok(database);
        }
        let path = self.get_database_path().await?;
        let directory = get_parent_path(&path);
        result_or_throw(
            self.env
                .create_dir(&directory, CreateDirOptions { recursive: Some(true) })
                .await,
            &format!("Failed to create SQLite sessions directory {path}"),
        )?;
        let connection = open_database(&path).map_err(SqliteError::into_session)?;
        configure_sqlite_database(&connection)
            .and_then(|()| apply_migrations(&connection))
            .map_err(SqliteError::into_session)?;
        let database = Arc::new(Database::new(connection));
        *self.database.lock().expect("database mutex") = Some(Arc::clone(&database));
        Ok(database)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForkPosition {
    At,
    Before,
}

fn fork_selection(options: &ForkOptions) -> (Option<String>, Option<ForkPosition>) {
    match options {
        ForkOptions::Branch => (None, None),
        ForkOptions::BranchDefaultBefore => (None, Some(ForkPosition::Before)),
        ForkOptions::BranchAt { entry_id } => (Some(entry_id.clone()), Some(ForkPosition::At)),
        ForkOptions::BranchBefore { entry_id } => (Some(entry_id.clone()), Some(ForkPosition::Before)),
        ForkOptions::Tree => (None, None),
    }
}
