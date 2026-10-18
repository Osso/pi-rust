//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/branch-tips.ts`.

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, text_param};

pub fn read_branch_tip_ids(db: &rusqlite::Connection, session_id: &str) -> Result<Vec<String>, SqliteError> {
    SqlQuery::new(
        "SELECT tip_id FROM branch_tips WHERE session_id = ? ORDER BY tip_id",
        vec![text_param(session_id)],
    )
    .all(db, |row| row.get::<_, String>(0))
}

pub fn read_branch_tip_branch_id(
    db: &rusqlite::Connection,
    session_id: &str,
    tip_id: &str,
) -> Result<Option<String>, SqliteError> {
    SqlQuery::new(
        "SELECT branch_id FROM branch_tips WHERE session_id = ? AND tip_id = ?",
        vec![text_param(session_id), text_param(tip_id)],
    )
    .get(db, |row| row.get::<_, String>(0))
}

pub fn insert_branch_tip(
    db: &rusqlite::Connection,
    session_id: &str,
    tip_id: &str,
    branch_id: &str,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO branch_tips (session_id, tip_id, branch_id) VALUES (?, ?, ?)",
        vec![text_param(session_id), text_param(tip_id), text_param(branch_id)],
    )
    .run(db)?;
    Ok(())
}

pub fn update_branch_tip(
    db: &rusqlite::Connection,
    session_id: &str,
    branch_id: &str,
    old_tip_id: &str,
    new_tip_id: &str,
) -> Result<bool, SqliteError> {
    let result = SqlQuery::new(
        "UPDATE branch_tips SET tip_id = ?
		WHERE session_id = ? AND branch_id = ? AND tip_id = ?",
        vec![
            text_param(new_tip_id),
            text_param(session_id),
            text_param(branch_id),
            text_param(old_tip_id),
        ],
    )
    .run(db)?;
    Ok(result.changes == 1)
}

pub fn delete_branch_tips(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new(
        "DELETE FROM branch_tips WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .run(db)?;
    Ok(())
}
