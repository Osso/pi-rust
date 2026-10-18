//! `SessionStorage` implementation backed by one SQLite session.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_agent_core::harness::session::{
    Clock, Entry, EntryBody, EntryOrder, EntryQuery, EntryType, LanePointer, LaneRecord, LogItem, LogOptions,
    RecordBody, RecordQuery, RecordType, SessionError, SessionErrorCode, SessionStats, SessionStorage,
    StorageBranchQuery,
};
use pi_ai::request::BoxFuture;
use serde_json::Value;

use crate::error::SqliteError;
use crate::sqlite::branch_cache::append_entry_to_branch_cache;
use crate::sqlite::connection::{Database, transaction};
use crate::sqlite::repo::ResolvedWriterLeaseOptions;
use crate::sqlite::repo::decode::{
    PendingLogRow, assert_unused_id, decode_entry, decode_record, entry_row_from_cached, matches_entry_query,
    parse_json_string, record_op_kind, record_run_id, require_session_row, validate_cached_branch_rows,
};
use crate::sqlite::storage::branch_entries::{CachedBranchQuery, query_cached_branch_rows, read_cached_branch};
use crate::sqlite::storage::entries::{
    EntryRow, EntryRowsQuery, NewEntryRow, entry_payload, insert_entry_row, read_entry_row, read_entry_rows,
};
use crate::sqlite::storage::facts::{append_fact, read_fact_rows, read_latest_fact};
use crate::sqlite::storage::lanes::{
    create_lane, finish_lane_operation, move_lane, read_lane, read_lane_head, read_lane_move_rows, read_lanes,
    set_lane_leaf, start_lane_operation,
};
use crate::sqlite::storage::records::{
    NewRecordRow, RecordRowsQuery, append_record_row, read_open_operation_rows, read_record_rows,
};
use crate::sqlite::storage::session_sequences::{advance_sequence, get_next_sequence};
use crate::sqlite::storage::session_stats::{add_usage_to_stats, increment_message_count, read_stats};
use crate::sqlite::storage::sessions::decode_session_metadata;
use crate::sqlite::storage::writer_leases::{WriterLease, release_writer_lease, renew_writer_lease};
use crate::sqlite::types::SqliteSessionMetadata;

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

fn lost_writer_error(session_id: &str) -> SessionError {
    SessionError::new(
        SessionErrorCode::Storage,
        format!("SQLite session {session_id} writer lease was lost"),
    )
}

fn invalid_lane_error(lane: &str) -> SqliteError {
    SqliteError::Session(SessionError::new(
        SessionErrorCode::InvalidLane,
        format!("Lane not found: {lane}"),
    ))
}

fn not_found_entry_error(id: &str) -> SqliteError {
    SqliteError::Session(SessionError::new(
        SessionErrorCode::NotFound,
        format!("Entry not found: {id}"),
    ))
}

impl SqliteSessionStorage {
    pub(crate) fn new(
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
    pub(crate) fn start_heartbeat(self: &Arc<Self>) {
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

fn commit_entry(
    connection: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
    entry: EntryBody,
    timestamp: i64,
) -> Result<Entry, SqliteError> {
    let parent_id = read_lane_head(connection, session_id, lane)?;
    assert_unused_id(connection, session_id, entry.id())?;
    let seq = get_next_sequence(connection, session_id)?;
    let committed = Entry {
        body: entry,
        parent_id,
        seq,
        timestamp,
    };
    insert_entry_row(connection, session_id, &new_entry_row(&committed))?;
    apply_entry_side_effects(connection, session_id, lane, &committed)?;
    Ok(committed)
}

fn new_entry_row(entry: &Entry) -> NewEntryRow {
    NewEntryRow {
        seq: entry.seq,
        id: entry.id().to_owned(),
        parent_id: entry.parent_id.clone(),
        entry_type: entry.entry_type().as_str().to_owned(),
        timestamp: entry.timestamp,
        payload: Value::Object(entry_payload(&entry.body)).to_string(),
    }
}

fn apply_entry_side_effects(
    connection: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
    entry: &Entry,
) -> Result<(), SqliteError> {
    set_lane_leaf(connection, session_id, lane, Some(entry.id()))?;
    append_entry_to_branch_cache(
        connection,
        session_id,
        entry.id(),
        entry.seq,
        entry.entry_type().as_str(),
        entry.body.custom_type(),
        entry.parent_id.as_deref(),
    )?;
    if entry.entry_type() == EntryType::Message {
        increment_message_count(connection, session_id)?;
    }
    advance_sequence(connection, session_id, entry.seq)
}

fn commit_record(
    connection: &rusqlite::Connection,
    session_id: &str,
    record: RecordBody,
    timestamp: i64,
) -> Result<LaneRecord, SqliteError> {
    let lane = record.lane().to_owned();
    if read_lane(connection, session_id, &lane)?.is_none() {
        return Err(invalid_lane_error(&lane));
    }
    assert_unused_id(connection, session_id, record.id())?;
    let seq = get_next_sequence(connection, session_id)?;
    let committed = LaneRecord {
        body: record,
        seq,
        timestamp,
    };
    if committed.record_type() == RecordType::OperationStarted {
        start_lane_operation(connection, session_id, &lane, committed.id())?;
    }
    append_record_row(connection, session_id, &new_record_row(&committed, &lane))?;
    apply_record_side_effects(connection, session_id, &lane, &committed)?;
    Ok(committed)
}

fn new_record_row(record: &LaneRecord, lane: &str) -> NewRecordRow {
    let payload = serde_json::to_value(&record.body).expect("records serialize");
    NewRecordRow {
        seq: record.seq,
        id: record.id().to_owned(),
        lane: lane.to_owned(),
        run_id: record_run_id(&record.body),
        record_type: record.record_type().as_str().to_owned(),
        op_kind: record_op_kind(&record.body),
        timestamp: record.timestamp,
        payload: payload.to_string(),
    }
}

fn apply_record_side_effects(
    connection: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
    record: &LaneRecord,
) -> Result<(), SqliteError> {
    if record.record_type() == RecordType::OperationFinished {
        let run_id = record.body.run_id().unwrap_or_default();
        finish_lane_operation(connection, session_id, lane, run_id)?;
    }
    if let Some(usage) = record.body.usage() {
        add_usage_to_stats(connection, session_id, usage)?;
    }
    advance_sequence(connection, session_id, record.seq)
}

fn decode_matching_entries(rows: &[EntryRow], query: &EntryQuery) -> Result<Vec<Entry>, SqliteError> {
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let entry = decode_entry(row)?;
        if matches_entry_query(&entry, query) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn read_matching_entries(
    connection: &rusqlite::Connection,
    session_id: &str,
    query: &EntryQuery,
) -> Result<Vec<Entry>, SqliteError> {
    let sql_type = match (&query.entry_type, &query.custom_type) {
        (Some(entry_type), _) => Some(entry_type.as_str().to_owned()),
        (None, Some(_)) => Some(EntryType::Custom.as_str().to_owned()),
        (None, None) => None,
    };
    let sql_limit = if query.custom_type.is_none() { query.limit } else { None };
    let rows = read_entry_rows(
        connection,
        session_id,
        &EntryRowsQuery {
            cursor_after_seq: query.cursor.map(|cursor| cursor.after_seq),
            limit: sql_limit,
            order: query.order,
            entry_type: sql_type,
            after_seq: None,
        },
    )?;
    decode_matching_entries(&rows, query)
}

fn read_branch_entries(
    connection: &rusqlite::Connection,
    session_id: &str,
    query: &StorageBranchQuery,
) -> Result<Vec<Entry>, SqliteError> {
    let Some(cached) = read_cached_branch(connection, session_id, &query.start)? else {
        if read_entry_row(connection, session_id, &query.start)?.is_none() {
            return Err(not_found_entry_error(&query.start));
        }
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidEntry,
            format!("Branch cache missing entry {}", query.start),
        )));
    };
    let rows = query_cached_branch_rows(
        connection,
        session_id,
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
    validate_cached_branch_rows(&rows, query)?;
    let rows: Vec<EntryRow> = rows.iter().map(entry_row_from_cached).collect();
    decode_matching_entries(&rows, &query.entry)
}

fn read_pending_log_rows(
    connection: &rusqlite::Connection,
    session_id: &str,
    options: &LogOptions,
) -> Result<Vec<(i64, PendingLogRow)>, SqliteError> {
    let after_seq = options.after_seq.unwrap_or(0);
    let limit = options.limit;
    let entry_rows = read_entry_rows(
        connection,
        session_id,
        &EntryRowsQuery {
            after_seq: Some(after_seq),
            order: Some(EntryOrder::OldestFirst),
            limit,
            ..EntryRowsQuery::default()
        },
    )?;
    let record_rows = read_record_rows(
        connection,
        session_id,
        &RecordRowsQuery {
            after_seq: Some(after_seq),
            order: Some(EntryOrder::OldestFirst),
            limit,
            ..RecordRowsQuery::default()
        },
    )?;
    let lane_rows = read_lane_move_rows(connection, session_id, Some(after_seq), limit)?;
    let fact_rows = read_fact_rows(connection, session_id, Some(after_seq), limit)?;

    // Rows are decoded only after the combined log is truncated, like the source's lazy
    // `decode` closures.
    let mut items: Vec<(i64, PendingLogRow)> = Vec::new();
    items.extend(entry_rows.into_iter().map(|row| (row.seq, PendingLogRow::Entry(row))));
    items.extend(record_rows.into_iter().map(|row| (row.seq, PendingLogRow::Record(row))));
    items.extend(lane_rows.into_iter().map(|row| (row.seq, PendingLogRow::Lane(row))));
    items.extend(fact_rows.into_iter().map(|row| (row.seq, PendingLogRow::Fact(row))));
    sort_and_truncate_log_rows(&mut items, limit);
    Ok(items)
}

fn sort_and_truncate_log_rows(items: &mut Vec<(i64, PendingLogRow)>, limit: Option<usize>) {
    items.sort_by_key(|(seq, _)| *seq);
    if let Some(limit) = limit {
        items.truncate(limit);
    }
}

fn read_log_items(
    connection: &rusqlite::Connection,
    session_id: &str,
    options: &LogOptions,
) -> Result<Vec<LogItem>, SqliteError> {
    read_pending_log_rows(connection, session_id, options)?
        .into_iter()
        .map(|(_, row)| row.decode(session_id))
        .collect()
}

fn read_open_operations(
    connection: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
) -> Result<Vec<LaneRecord>, SqliteError> {
    let rows = read_open_operation_rows(connection, session_id, lane)?;
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
}

fn truncate_to_limit(entries: Vec<Entry>, limit: Option<usize>) -> Vec<Entry> {
    match limit {
        Some(limit) => entries.into_iter().take(limit).collect(),
        None => entries,
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
                    return Err(not_found_entry_error(at));
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
                    return Err(invalid_lane_error(lane));
                }
                if let Some(to) = to
                    && read_entry_row(connection, &self.metadata.id, to)?.is_none()
                {
                    return Err(not_found_entry_error(to));
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
            let timestamp = (self.clock)();
            self.enqueue_write(|connection| commit_entry(connection, &self.metadata.id, lane, entry, timestamp))
                .map_err(SqliteError::into_session)
        })
    }

    fn append_record<'a>(&'a self, record: RecordBody) -> BoxFuture<'a, Result<LaneRecord, SessionError>> {
        Box::pin(async move {
            let timestamp = (self.clock)();
            self.enqueue_write(|connection| commit_record(connection, &self.metadata.id, record, timestamp))
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
            let entries = self
                .db
                .with_connection(|connection| read_matching_entries(connection, &self.metadata.id, &query))
                .map_err(SqliteError::into_session)?;
            Ok(truncate_to_limit(entries, query.limit))
        })
    }

    fn find_entries_on_branch<'a>(
        &'a self,
        query: StorageBranchQuery,
    ) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>> {
        Box::pin(async move {
            let entries = self
                .db
                .with_connection(|connection| read_branch_entries(connection, &self.metadata.id, &query))
                .map_err(SqliteError::into_session)?;
            Ok(truncate_to_limit(entries, query.entry.limit))
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
                .with_connection(|connection| read_open_operations(connection, &self.metadata.id, lane))
                .map_err(SqliteError::into_session)
        })
    }

    fn get_log<'a>(&'a self, options: LogOptions) -> BoxFuture<'a, Result<Vec<LogItem>, SessionError>> {
        Box::pin(async move {
            self.db
                .with_connection(|connection| read_log_items(connection, &self.metadata.id, &options))
                .map_err(SqliteError::into_session)
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
                    return Err(not_found_entry_error(id));
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
