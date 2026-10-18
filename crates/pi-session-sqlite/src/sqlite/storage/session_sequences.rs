//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/session-sequences.ts`.

use pi_agent_core::harness::session::{SessionError, SessionErrorCode};

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, text_param};

pub fn create_sequence(db: &rusqlite::Connection, session_id: &str, next_seq: i64) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO session_sequences (session_id, next_seq) VALUES (?, ?)",
        vec![text_param(session_id), int_param(next_seq)],
    )
    .run(db)?;
    Ok(())
}

pub fn get_next_sequence(db: &rusqlite::Connection, session_id: &str) -> Result<i64, SqliteError> {
    let row = SqlQuery::new(
        "SELECT next_seq FROM session_sequences WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .get(db, |row| row.get::<_, i64>(0))?;
    match row {
        Some(next_seq) => Ok(next_seq),
        None => Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Missing sequence row for session {session_id}"),
        ))),
    }
}

pub fn set_next_sequence(db: &rusqlite::Connection, session_id: &str, next_seq: i64) -> Result<(), SqliteError> {
    SqlQuery::new(
        "UPDATE session_sequences SET next_seq = ? WHERE session_id = ?",
        vec![int_param(next_seq), text_param(session_id)],
    )
    .run(db)?;
    Ok(())
}

pub fn advance_sequence(db: &rusqlite::Connection, session_id: &str, seq: i64) -> Result<(), SqliteError> {
    set_next_sequence(db, session_id, seq + 1)
}

pub fn delete_sequence(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new(
        "DELETE FROM session_sequences WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .run(db)?;
    Ok(())
}
