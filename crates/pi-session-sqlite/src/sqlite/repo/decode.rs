//! Row decoding and query matching for the SQLite session repository.

use pi_agent_core::harness::session::{
    Entry, EntryBody, EntryCursor, EntryOrder, EntryQuery, EntryType, FactItem, LaneRecord, LogItem, OptionalValue,
    RecordBody, SessionError, SessionErrorCode, StorageBranchQuery,
};
use pi_ai::types::Usage;
use serde_json::Value;

use crate::error::SqliteError;
use crate::sqlite::storage::branch_entries::CachedBranchEntryRow;
use crate::sqlite::storage::entries::{EntryRow, id_exists_in_entries};
use crate::sqlite::storage::facts::FactRow;
use crate::sqlite::storage::lanes::LaneMoveRow;
use crate::sqlite::storage::records::{RecordRow, id_exists_in_records};
use crate::sqlite::storage::sessions::{SessionRow, read_session_row};

pub(crate) fn entry_row_from_cached(row: &CachedBranchEntryRow) -> EntryRow {
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

pub(crate) fn decode_entry(row: &EntryRow) -> Result<Entry, SqliteError> {
    let payload: Value = serde_json::from_str(&row.payload).map_err(|_| invalid_entry_error(row))?;
    let Value::Object(object) = payload else {
        return Err(invalid_entry_error(row));
    };
    Ok(Entry {
        body: decode_entry_body(row, &object)?,
        parent_id: row.parent_id.clone(),
        seq: row.seq,
        timestamp: row.timestamp,
    })
}

fn decode_entry_body(row: &EntryRow, payload: &serde_json::Map<String, Value>) -> Result<EntryBody, SqliteError> {
    match row.entry_type.as_str() {
        "message" => decode_message_body(row, payload),
        "model_change" => decode_model_change_body(row, payload),
        "thinking_level_change" => decode_thinking_level_body(row, payload),
        "active_tools_change" => decode_active_tools_body(row, payload),
        "compaction" => decode_compaction_body(row, payload),
        "branch_summary" => decode_branch_summary_body(row, payload),
        "custom" => decode_custom_body(row, payload),
        _ => Err(invalid_entry_error(row)),
    }
}

fn decode_message_body(row: &EntryRow, payload: &serde_json::Map<String, Value>) -> Result<EntryBody, SqliteError> {
    let message = payload
        .get("message")
        .filter(|value| value.is_object())
        .ok_or_else(|| invalid_entry_error(row))?;
    let message = serde_json::from_value(message.clone()).map_err(|_| invalid_entry_error(row))?;
    let terminate = match payload.get("terminate") {
        Some(Value::Bool(true)) => Some(true),
        _ => None,
    };
    Ok(EntryBody::Message {
        id: row.id.clone(),
        message,
        terminate,
    })
}

fn decode_model_change_body(
    row: &EntryRow,
    payload: &serde_json::Map<String, Value>,
) -> Result<EntryBody, SqliteError> {
    Ok(EntryBody::ModelChange {
        id: row.id.clone(),
        provider: string_field(payload, "provider", row)?,
        model_id: string_field(payload, "modelId", row)?,
    })
}

fn decode_thinking_level_body(
    row: &EntryRow,
    payload: &serde_json::Map<String, Value>,
) -> Result<EntryBody, SqliteError> {
    Ok(EntryBody::ThinkingLevelChange {
        id: row.id.clone(),
        thinking_level: string_field(payload, "thinkingLevel", row)?,
    })
}

fn decode_active_tools_body(
    row: &EntryRow,
    payload: &serde_json::Map<String, Value>,
) -> Result<EntryBody, SqliteError> {
    let names = payload
        .get("activeToolNames")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_entry_error(row))?;
    let mut active_tool_names = Vec::with_capacity(names.len());
    for name in names {
        active_tool_names.push(name.as_str().ok_or_else(|| invalid_entry_error(row))?.to_owned());
    }
    Ok(EntryBody::ActiveToolsChange {
        id: row.id.clone(),
        active_tool_names,
    })
}

fn decode_compaction_body(row: &EntryRow, payload: &serde_json::Map<String, Value>) -> Result<EntryBody, SqliteError> {
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
        id: row.id.clone(),
        summary,
        tokens_before,
        retained_tail,
        duration_ms: payload.get("durationMs").and_then(Value::as_f64),
        details: optional_value(payload, "details"),
        usage: optional_usage(payload, row)?,
    })
}

fn decode_branch_summary_body(
    row: &EntryRow,
    payload: &serde_json::Map<String, Value>,
) -> Result<EntryBody, SqliteError> {
    Ok(EntryBody::BranchSummary {
        id: row.id.clone(),
        from_id: string_field(payload, "fromId", row)?,
        summary: string_field(payload, "summary", row)?,
        details: optional_value(payload, "details"),
        usage: optional_usage(payload, row)?,
    })
}

fn decode_custom_body(row: &EntryRow, payload: &serde_json::Map<String, Value>) -> Result<EntryBody, SqliteError> {
    Ok(EntryBody::Custom {
        id: row.id.clone(),
        custom_type: string_field(payload, "customType", row)?,
        data: optional_value(payload, "data"),
    })
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

pub(crate) fn record_run_id(record: &RecordBody) -> Option<String> {
    record.run_id().map(str::to_owned)
}

pub(crate) fn record_op_kind(record: &RecordBody) -> Option<String> {
    match record {
        RecordBody::OperationStarted { intent, .. } => Some(intent.kind().as_str().to_owned()),
        _ => None,
    }
}

pub(crate) fn decode_record(row: &RecordRow) -> Result<LaneRecord, SqliteError> {
    let payload: Value = serde_json::from_str(&row.payload).map_err(|_| invalid_record_error(row.seq))?;
    let Value::Object(mut object) = payload else {
        return Err(invalid_record_error(row.seq));
    };
    object.insert("seq".to_owned(), Value::from(row.seq));
    object.insert("timestamp".to_owned(), Value::from(row.timestamp));
    serde_json::from_value(Value::Object(object)).map_err(|_| invalid_record_error(row.seq))
}

pub(crate) fn validate_cached_branch_rows(
    rows: &[CachedBranchEntryRow],
    query: &StorageBranchQuery,
) -> Result<(), SqliteError> {
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

pub(crate) fn matches_entry_query(entry: &Entry, query: &EntryQuery) -> bool {
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

pub(crate) fn assert_unused_id(db: &rusqlite::Connection, session_id: &str, id: &str) -> Result<(), SqliteError> {
    if id_exists_in_entries(db, session_id, id)? || id_exists_in_records(db, session_id, id)? {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::AlreadyExists,
            format!("ID already exists: {id}"),
        )));
    }
    Ok(())
}

pub(crate) fn require_session_row(db: &rusqlite::Connection, session_id: &str) -> Result<SessionRow, SqliteError> {
    match read_session_row(db, session_id)? {
        Some(row) => Ok(row),
        None => Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::NotFound,
            format!("Session not found: {session_id}"),
        ))),
    }
}

pub(crate) fn parse_json_string(
    value: Option<&str>,
    session_id: &str,
    field: &str,
) -> Result<Option<String>, SqliteError> {
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

/// One decoded log row; decoding is deferred until after the combined limit is applied.
pub(crate) enum PendingLogRow {
    Entry(EntryRow),
    Record(RecordRow),
    Lane(LaneMoveRow),
    Fact(FactRow),
}

impl PendingLogRow {
    pub(crate) fn decode(self, session_id: &str) -> Result<LogItem, SqliteError> {
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
