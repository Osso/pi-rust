//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/sessions.ts`.

use pi_agent_core::harness::session::{SessionError, SessionErrorCode};
use pi_ai::types::JsonObject;
use rusqlite::Row;
use serde_json::Value;

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, optional_text_param, text_param};
use crate::sqlite::types::SqliteSessionMetadata;

const SELECT_SESSION_ROW: &str = "SELECT s.id, s.created_at, s.metadata, s.cwd, s.parent_session_id,
		name_fact.seq IS NOT NULL AS has_session_name,
		name_fact.value AS session_name
	FROM sessions AS s
	LEFT JOIN facts AS name_fact
		ON name_fact.session_id = s.id
		AND name_fact.kind = 'name'
		AND name_fact.key IS NULL
		AND name_fact.seq = (
			SELECT MAX(f.seq)
			FROM facts AS f
			WHERE f.session_id = s.id AND f.kind = 'name' AND f.key IS NULL
		)";

/// One projected `sessions` row.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub id: String,
    pub created_at: i64,
    pub metadata: Option<String>,
    pub cwd: String,
    pub parent_session_id: Option<String>,
    pub has_session_name: i64,
    pub session_name: Option<String>,
}

/// A row to insert.
#[derive(Debug, Clone)]
pub struct NewSessionRow {
    pub id: String,
    pub created_at: i64,
    pub cwd: String,
    pub parent_session_id: Option<String>,
    pub metadata: Option<JsonObject>,
}

fn read_session_row_from_row(row: &Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        created_at: row.get(1)?,
        metadata: row.get(2)?,
        cwd: row.get(3)?,
        parent_session_id: row.get(4)?,
        has_session_name: row.get(5)?,
        session_name: row.get(6)?,
    })
}

fn parse_metadata(metadata: Option<&str>, session_id: &str) -> Result<Option<JsonObject>, SqliteError> {
    let Some(metadata) = metadata else {
        return Ok(None);
    };
    let parsed: Value = serde_json::from_str(metadata).map_err(|_| {
        SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Invalid SQLite session {session_id}: metadata is not valid JSON"),
        ))
    })?;
    match parsed {
        Value::Object(object) => Ok(Some(object)),
        _ => Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Invalid SQLite session {session_id}: metadata must be an object"),
        ))),
    }
}

fn serialize_metadata(metadata: Option<&JsonObject>) -> Value {
    match metadata {
        Some(metadata) => Value::Object(metadata.clone()),
        None => Value::Null,
    }
}

pub fn session_exists(db: &rusqlite::Connection, session_id: &str) -> Result<bool, SqliteError> {
    let row = SqlQuery::new(
        "SELECT 1 AS found FROM sessions WHERE id = ?",
        vec![text_param(session_id)],
    )
    .get(db, |row| row.get::<_, i64>(0))?;
    Ok(row.is_some())
}

pub fn insert_session_row(db: &rusqlite::Connection, session: &NewSessionRow) -> Result<(), SqliteError> {
    let metadata = match serialize_metadata(session.metadata.as_ref()) {
        Value::Null => rusqlite::types::Value::Null,
        value => text_param(value.to_string()),
    };
    SqlQuery::new(
        "INSERT INTO sessions (id, created_at, metadata, cwd, parent_session_id)
		VALUES (?, ?, ?, ?, ?)",
        vec![
            text_param(session.id.clone()),
            int_param(session.created_at),
            metadata,
            text_param(session.cwd.clone()),
            optional_text_param(session.parent_session_id.as_deref()),
        ],
    )
    .run(db)?;
    Ok(())
}

pub fn read_session_row(db: &rusqlite::Connection, session_id: &str) -> Result<Option<SessionRow>, SqliteError> {
    let query = format!("{SELECT_SESSION_ROW}\n\t\tWHERE s.id = ?");
    SqlQuery::new(query, vec![text_param(session_id)]).get(db, read_session_row_from_row)
}

pub fn read_session_rows(db: &rusqlite::Connection, cwd: Option<&str>) -> Result<Vec<SessionRow>, SqliteError> {
    let (where_clause, params) = match cwd {
        Some(cwd) => ("WHERE s.cwd = ?".to_owned(), vec![text_param(cwd)]),
        None => (String::new(), Vec::new()),
    };
    let query = format!("{SELECT_SESSION_ROW}\n\t\t{where_clause}\n\t\tORDER BY s.created_at DESC");
    SqlQuery::new(query, params).all(db, read_session_row_from_row)
}

pub fn delete_session_row(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new("DELETE FROM sessions WHERE id = ?", vec![text_param(session_id)]).run(db)?;
    Ok(())
}

fn parse_session_name(value: Option<&str>, session_id: &str) -> Result<Option<String>, SqliteError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let parsed: Value = serde_json::from_str(value).map_err(|_| {
        SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Invalid SQLite session {session_id}: name is not valid JSON"),
        ))
    })?;
    match parsed {
        Value::String(name) => Ok(Some(name)),
        _ => Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Invalid SQLite session {session_id}: name must be a string"),
        ))),
    }
}

pub fn decode_session_metadata(row: &SessionRow, path: &str) -> Result<SqliteSessionMetadata, SqliteError> {
    let metadata = parse_metadata(row.metadata.as_deref(), &row.id)?;
    let name = if row.has_session_name == 0 {
        None
    } else {
        parse_session_name(row.session_name.as_deref(), &row.id)?
    };
    Ok(SqliteSessionMetadata {
        id: row.id.clone(),
        created_at: row.created_at,
        cwd: row.cwd.clone(),
        path: path.to_owned(),
        parent_session_id: row.parent_session_id.clone(),
        name,
        metadata,
    })
}
