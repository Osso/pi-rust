//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/records.ts`.

use pi_agent_core::harness::session::{EntryOrder, SessionError, SessionErrorCode};
use rusqlite::Row;

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, optional_text_param, text_param};

/// One stored lane record row.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordRow {
    pub session_id: String,
    pub seq: i64,
    pub id: String,
    pub lane: String,
    pub run_id: Option<String>,
    pub record_type: String,
    pub op_kind: Option<String>,
    pub timestamp: i64,
    pub payload: String,
}

/// A row to insert.
#[derive(Debug, Clone)]
pub struct NewRecordRow {
    pub seq: i64,
    pub id: String,
    pub lane: String,
    pub run_id: Option<String>,
    pub record_type: String,
    pub op_kind: Option<String>,
    pub timestamp: i64,
    pub payload: String,
}

/// Query options for [`read_record_rows`].
#[derive(Debug, Clone, Default)]
pub struct RecordRowsQuery {
    pub lane: Option<String>,
    pub record_type: Option<String>,
    pub run_id: Option<String>,
    pub operation_kind: Option<String>,
    pub after_seq: Option<i64>,
    pub order: Option<EntryOrder>,
    pub limit: Option<usize>,
}

fn read_record_row(row: &Row<'_>) -> rusqlite::Result<RecordRow> {
    Ok(RecordRow {
        session_id: row.get(0)?,
        seq: row.get(1)?,
        id: row.get(2)?,
        lane: row.get(3)?,
        run_id: row.get(4)?,
        record_type: row.get(5)?,
        op_kind: row.get(6)?,
        timestamp: row.get(7)?,
        payload: row.get(8)?,
    })
}

pub fn append_record_row(
    db: &rusqlite::Connection,
    session_id: &str,
    record: &NewRecordRow,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO records
			(session_id, seq, id, lane, run_id, type, op_kind, timestamp, payload)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        vec![
            text_param(session_id),
            int_param(record.seq),
            text_param(record.id.clone()),
            text_param(record.lane.clone()),
            optional_text_param(record.run_id.as_deref()),
            text_param(record.record_type.clone()),
            optional_text_param(record.op_kind.as_deref()),
            int_param(record.timestamp),
            text_param(record.payload.clone()),
        ],
    )
    .run(db)?;
    Ok(())
}

pub fn id_exists_in_records(db: &rusqlite::Connection, session_id: &str, id: &str) -> Result<bool, SqliteError> {
    let row = SqlQuery::new(
        "SELECT 1 AS found FROM records WHERE session_id = ? AND id = ? LIMIT 1",
        vec![text_param(session_id), text_param(id)],
    )
    .get(db, |row| row.get::<_, i64>(0))?;
    Ok(row.is_some())
}

pub fn delete_record_rows(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new("DELETE FROM records WHERE session_id = ?", vec![text_param(session_id)]).run(db)?;
    Ok(())
}

pub fn read_record_rows(
    db: &rusqlite::Connection,
    session_id: &str,
    query: &RecordRowsQuery,
) -> Result<Vec<RecordRow>, SqliteError> {
    let mut predicates = vec!["session_id = ?".to_owned()];
    let mut params = vec![text_param(session_id)];
    if let Some(lane) = query.lane.as_deref() {
        predicates.push("lane = ?".to_owned());
        params.push(text_param(lane));
    }
    if let Some(record_type) = query.record_type.as_deref() {
        predicates.push("type = ?".to_owned());
        params.push(text_param(record_type));
    }
    if let Some(run_id) = query.run_id.as_deref() {
        predicates.push("run_id = ?".to_owned());
        params.push(text_param(run_id));
    }
    if let Some(operation_kind) = query.operation_kind.as_deref() {
        predicates.push("op_kind = ?".to_owned());
        params.push(text_param(operation_kind));
    }
    if let Some(after_seq) = query.after_seq {
        predicates.push("seq > ?".to_owned());
        params.push(int_param(after_seq));
    }
    let direction = if query.order == Some(EntryOrder::OldestFirst) {
        "ASC"
    } else {
        "DESC"
    };
    let mut query_text = format!(
        "SELECT session_id, seq, id, lane, run_id, type, op_kind, timestamp, payload
		FROM records
		WHERE {}
		ORDER BY seq {direction}",
        predicates.join(" AND ")
    );
    if let Some(limit) = query.limit {
        query_text.push_str(" LIMIT ?");
        params.push(int_param(limit as i64));
    }
    SqlQuery::new(query_text, params).all(db, read_record_row)
}

pub fn read_open_operation_rows(
    db: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
) -> Result<Vec<RecordRow>, SqliteError> {
    let open_operation_id = SqlQuery::new(
        "SELECT open_operation_id FROM lanes WHERE session_id = ? AND lane = ?",
        vec![text_param(session_id), text_param(lane)],
    )
    .get(db, |row| row.get::<_, Option<String>>(0))?;
    let Some(Some(open_operation_id)) = open_operation_id else {
        return Ok(Vec::new());
    };
    let record = SqlQuery::new(
        "SELECT session_id, seq, id, lane, run_id, type, op_kind, timestamp, payload
		FROM records
		WHERE session_id = ?
			AND id = ?",
        vec![text_param(session_id), text_param(open_operation_id.clone())],
    )
    .get(db, read_record_row)?;
    let Some(record) = record else {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Lane {lane} points at missing open operation {open_operation_id}"),
        )));
    };
    if record.lane != lane || record.record_type != "operation_started" {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Lane {lane} points at invalid open operation {open_operation_id}"),
        )));
    }
    Ok(vec![record])
}
