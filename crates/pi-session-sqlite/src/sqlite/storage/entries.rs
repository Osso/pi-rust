//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/entries.ts`.

use pi_agent_core::harness::session::{EntryBody, EntryOrder};
use rusqlite::Row;
use serde_json::Value;

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, optional_text_param, text_param};

/// One stored entry row.
#[derive(Debug, Clone, PartialEq)]
pub struct EntryRow {
    pub session_id: String,
    pub seq: i64,
    pub id: String,
    pub parent_id: Option<String>,
    pub entry_type: String,
    pub timestamp: i64,
    pub payload: String,
}

/// A row to insert.
#[derive(Debug, Clone)]
pub struct NewEntryRow {
    pub seq: i64,
    pub id: String,
    pub parent_id: Option<String>,
    pub entry_type: String,
    pub timestamp: i64,
    pub payload: String,
}

/// Query options for [`read_entry_rows`].
#[derive(Debug, Clone, Default)]
pub struct EntryRowsQuery {
    pub after_seq: Option<i64>,
    pub cursor_after_seq: Option<i64>,
    pub entry_type: Option<String>,
    pub order: Option<EntryOrder>,
    pub limit: Option<usize>,
}

fn read_entry_row_from_row(row: &Row<'_>) -> rusqlite::Result<EntryRow> {
    Ok(EntryRow {
        session_id: row.get(0)?,
        seq: row.get(1)?,
        id: row.get(2)?,
        parent_id: row.get(3)?,
        entry_type: row.get(4)?,
        timestamp: row.get(5)?,
        payload: row.get(6)?,
    })
}

/// Builds the stored payload: every type-specific field, without storage-assigned fields.
pub fn entry_payload(entry: &EntryBody) -> serde_json::Map<String, Value> {
    let value = serde_json::to_value(entry).expect("entry bodies serialize");
    let Value::Object(mut object) = value else {
        unreachable!("entry bodies serialize to objects");
    };
    object.remove("type");
    object.remove("id");
    object
}

pub fn insert_entry_row(db: &rusqlite::Connection, session_id: &str, entry: &NewEntryRow) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO entries (session_id, id, seq, parent_id, type, timestamp, payload)
		VALUES (?, ?, ?, ?, ?, ?, ?)",
        vec![
            text_param(session_id),
            text_param(entry.id.clone()),
            int_param(entry.seq),
            optional_text_param(entry.parent_id.as_deref()),
            text_param(entry.entry_type.clone()),
            int_param(entry.timestamp),
            text_param(entry.payload.clone()),
        ],
    )
    .run(db)?;
    Ok(())
}

pub fn read_entry_row(
    db: &rusqlite::Connection,
    session_id: &str,
    entry_id: &str,
) -> Result<Option<EntryRow>, SqliteError> {
    SqlQuery::new(
        "SELECT session_id, seq, id, parent_id, type, timestamp, payload
		FROM entries
		WHERE session_id = ? AND id = ?",
        vec![text_param(session_id), text_param(entry_id)],
    )
    .get(db, read_entry_row_from_row)
}

pub fn read_entry_rows(
    db: &rusqlite::Connection,
    session_id: &str,
    options: &EntryRowsQuery,
) -> Result<Vec<EntryRow>, SqliteError> {
    let oldest_first = options.order == Some(EntryOrder::OldestFirst);
    let mut query_text = String::from(
        "SELECT session_id, seq, id, parent_id, type, timestamp, payload
		FROM entries
		WHERE session_id = ?",
    );
    let mut params = vec![text_param(session_id)];
    if let Some(after_seq) = options.after_seq {
        query_text.push_str(" AND seq > ?");
        params.push(int_param(after_seq));
    }
    if let Some(cursor) = options.cursor_after_seq {
        query_text.push_str(if oldest_first { " AND seq > ?" } else { " AND seq < ?" });
        params.push(int_param(cursor));
    }
    if let Some(entry_type) = options.entry_type.as_deref() {
        query_text.push_str(" AND type = ?");
        params.push(text_param(entry_type));
    }
    query_text.push_str(if oldest_first {
        " ORDER BY seq ASC"
    } else {
        " ORDER BY seq DESC"
    });
    if let Some(limit) = options.limit {
        query_text.push_str(" LIMIT ?");
        params.push(int_param(limit as i64));
    }
    SqlQuery::new(query_text, params).all(db, read_entry_row_from_row)
}

pub fn id_exists_in_entries(db: &rusqlite::Connection, session_id: &str, id: &str) -> Result<bool, SqliteError> {
    let row = SqlQuery::new(
        "SELECT 1 AS found FROM entries WHERE session_id = ? AND id = ? LIMIT 1",
        vec![text_param(session_id), text_param(id)],
    )
    .get(db, |row| row.get::<_, i64>(0))?;
    Ok(row.is_some())
}

pub fn delete_entry_rows(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new("DELETE FROM entries WHERE session_id = ?", vec![text_param(session_id)]).run(db)?;
    Ok(())
}
