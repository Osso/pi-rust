//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/session-stats.ts`.

use pi_agent_core::harness::session::{SessionError, SessionErrorCode, SessionStats};
use pi_ai::types::Usage;

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, real_param, text_param};

pub fn create_stats(db: &rusqlite::Connection, session_id: &str, message_count: u64) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO session_stats
			(session_id, message_count, cached_tokens, uncached_tokens, total_tokens, cost_total)
			VALUES (?, ?, 0, 0, 0, 0)",
        vec![text_param(session_id), int_param(message_count as i64)],
    )
    .run(db)?;
    Ok(())
}

pub fn read_stats(db: &rusqlite::Connection, session_id: &str) -> Result<SessionStats, SqliteError> {
    let row = SqlQuery::new(
        "SELECT session_id, message_count, cached_tokens, uncached_tokens, total_tokens, cost_total
		FROM session_stats
		WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .get(db, |row| {
        Ok(SessionStats {
            message_count: row.get::<_, i64>(1)? as u64,
            cached_tokens: row.get(2)?,
            uncached_tokens: row.get(3)?,
            total_tokens: row.get(4)?,
            cost_total: row.get(5)?,
        })
    })?;
    match row {
        Some(stats) => Ok(stats),
        None => Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Missing stats row for session {session_id}"),
        ))),
    }
}

pub fn increment_message_count(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    let result = SqlQuery::new(
        "UPDATE session_stats SET message_count = message_count + 1 WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .run(db)?;
    if result.changes != 1 {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Missing stats row for session {session_id}"),
        )));
    }
    Ok(())
}

pub fn add_usage_to_stats(db: &rusqlite::Connection, session_id: &str, usage: &Usage) -> Result<(), SqliteError> {
    let result = SqlQuery::new(
        "UPDATE session_stats
		SET cached_tokens = cached_tokens + ?,
			uncached_tokens = uncached_tokens + ?,
			total_tokens = total_tokens + ?,
			cost_total = cost_total + ?
		WHERE session_id = ?",
        vec![
            real_param(usage.cache_read),
            real_param(usage.input + usage.cache_write),
            real_param(usage.total_tokens),
            real_param(usage.cost.total),
            text_param(session_id),
        ],
    )
    .run(db)?;
    if result.changes != 1 {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Missing stats row for session {session_id}"),
        )));
    }
    Ok(())
}

pub fn delete_stats(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new(
        "DELETE FROM session_stats WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .run(db)?;
    Ok(())
}
