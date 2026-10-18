//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/lanes.ts`.

use pi_agent_core::harness::session::{SessionError, SessionErrorCode};
use rusqlite::Row;

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, optional_text_param, text_param};

/// One lane row.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneRow {
    pub session_id: String,
    pub lane: String,
    pub leaf_id: Option<String>,
    pub open_operation_id: Option<String>,
}

/// One lane move row.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneMoveRow {
    pub session_id: String,
    pub seq: i64,
    pub lane: String,
    pub leaf_id: Option<String>,
}

fn read_lane_row(row: &Row<'_>) -> rusqlite::Result<LaneRow> {
    Ok(LaneRow {
        session_id: row.get(0)?,
        lane: row.get(1)?,
        leaf_id: row.get(2)?,
        open_operation_id: row.get(3)?,
    })
}

fn read_lane_move_row(row: &Row<'_>) -> rusqlite::Result<LaneMoveRow> {
    Ok(LaneMoveRow {
        session_id: row.get(0)?,
        seq: row.get(1)?,
        lane: row.get(2)?,
        leaf_id: row.get(3)?,
    })
}

pub fn create_initial_lane(
    db: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
    leaf_id: Option<&str>,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO lanes (session_id, lane, leaf_id, open_operation_id)
		VALUES (?, ?, ?, NULL)",
        vec![text_param(session_id), text_param(lane), optional_text_param(leaf_id)],
    )
    .run(db)?;
    Ok(())
}

pub fn read_lanes(db: &rusqlite::Connection, session_id: &str) -> Result<Vec<LaneRow>, SqliteError> {
    let rows = SqlQuery::new(
        "SELECT
			l.session_id,
			l.lane,
			l.leaf_id,
			l.open_operation_id,
			(l.leaf_id IS NULL OR EXISTS (
				SELECT 1 FROM entries AS e WHERE e.session_id = l.session_id AND e.id = l.leaf_id
			)) AS leaf_exists
		FROM lanes AS l
		WHERE l.session_id = ?
		ORDER BY l.lane",
        vec![text_param(session_id)],
    )
    .all(db, |row| Ok((read_lane_row(row)?, row.get::<_, i64>(4)?)))?;
    let mut lanes = Vec::with_capacity(rows.len());
    for (row, leaf_exists) in rows {
        if leaf_exists == 0 {
            return Err(SqliteError::Session(SessionError::new(
                SessionErrorCode::Storage,
                format!(
                    "Lane {} points at missing entry {}",
                    row.lane,
                    row.leaf_id.clone().unwrap_or_default()
                ),
            )));
        }
        lanes.push(row);
    }
    Ok(lanes)
}

pub fn read_lane(db: &rusqlite::Connection, session_id: &str, lane: &str) -> Result<Option<LaneRow>, SqliteError> {
    SqlQuery::new(
        "SELECT session_id, lane, leaf_id, open_operation_id
		FROM lanes
		WHERE session_id = ? AND lane = ?",
        vec![text_param(session_id), text_param(lane)],
    )
    .get(db, read_lane_row)
}

pub fn read_lane_head(db: &rusqlite::Connection, session_id: &str, lane: &str) -> Result<Option<String>, SqliteError> {
    let row = SqlQuery::new(
        "SELECT
			l.leaf_id,
			(l.leaf_id IS NULL OR EXISTS (
				SELECT 1 FROM entries AS e WHERE e.session_id = l.session_id AND e.id = l.leaf_id
			)) AS leaf_exists
		FROM lanes AS l
		WHERE l.session_id = ? AND l.lane = ?",
        vec![text_param(session_id), text_param(lane)],
    )
    .get(db, |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?)))?;
    let Some((leaf_id, leaf_exists)) = row else {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidLane,
            format!("Lane not found: {lane}"),
        )));
    };
    if leaf_exists == 0 {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::Storage,
            format!("Entry {} not found", leaf_id.unwrap_or_default()),
        )));
    }
    Ok(leaf_id)
}

pub fn create_lane(
    db: &rusqlite::Connection,
    session_id: &str,
    seq: i64,
    lane: &str,
    leaf_id: Option<&str>,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO lanes (session_id, lane, leaf_id, open_operation_id)
		VALUES (?, ?, ?, NULL)",
        vec![text_param(session_id), text_param(lane), optional_text_param(leaf_id)],
    )
    .run(db)?;
    append_lane_move(db, session_id, seq, lane, leaf_id)
}

pub fn move_lane(
    db: &rusqlite::Connection,
    session_id: &str,
    seq: i64,
    lane: &str,
    leaf_id: Option<&str>,
) -> Result<(), SqliteError> {
    let result = SqlQuery::new(
        "UPDATE lanes SET leaf_id = ? WHERE session_id = ? AND lane = ?",
        vec![optional_text_param(leaf_id), text_param(session_id), text_param(lane)],
    )
    .run(db)?;
    if result.changes != 1 {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidLane,
            format!("Lane not found: {lane}"),
        )));
    }
    append_lane_move(db, session_id, seq, lane, leaf_id)
}

pub fn set_lane_leaf(
    db: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
    leaf_id: Option<&str>,
) -> Result<(), SqliteError> {
    let result = SqlQuery::new(
        "UPDATE lanes SET leaf_id = ? WHERE session_id = ? AND lane = ?",
        vec![optional_text_param(leaf_id), text_param(session_id), text_param(lane)],
    )
    .run(db)?;
    if result.changes != 1 {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidLane,
            format!("Lane not found: {lane}"),
        )));
    }
    Ok(())
}

pub fn start_lane_operation(
    db: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
    run_id: &str,
) -> Result<(), SqliteError> {
    let result = SqlQuery::new(
        "UPDATE lanes SET open_operation_id = ?
		WHERE session_id = ? AND lane = ? AND open_operation_id IS NULL",
        vec![text_param(run_id), text_param(session_id), text_param(lane)],
    )
    .run(db)?;
    if result.changes == 1 {
        return Ok(());
    }
    let Some(current) = read_lane(db, session_id, lane)? else {
        return Err(SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidLane,
            format!("Lane not found: {lane}"),
        )));
    };
    Err(SqliteError::Session(SessionError::new(
        SessionErrorCode::Storage,
        format!(
            "Lane {lane} already has an open operation {}",
            current.open_operation_id.unwrap_or_default()
        ),
    )))
}

pub fn finish_lane_operation(
    db: &rusqlite::Connection,
    session_id: &str,
    lane: &str,
    run_id: &str,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "UPDATE lanes SET open_operation_id = NULL
		WHERE session_id = ? AND lane = ? AND open_operation_id = ?",
        vec![text_param(session_id), text_param(lane), text_param(run_id)],
    )
    .run(db)?;
    Ok(())
}

pub fn read_lane_move_rows(
    db: &rusqlite::Connection,
    session_id: &str,
    after_seq: Option<i64>,
    limit: Option<usize>,
) -> Result<Vec<LaneMoveRow>, SqliteError> {
    let mut query_text = String::from(
        "SELECT session_id, seq, lane, leaf_id
		FROM lane_moves
		WHERE session_id = ?",
    );
    let mut params = vec![text_param(session_id)];
    if let Some(after_seq) = after_seq {
        query_text.push_str(" AND seq > ?");
        params.push(int_param(after_seq));
    }
    query_text.push_str(" ORDER BY seq");
    if let Some(limit) = limit {
        query_text.push_str(" LIMIT ?");
        params.push(int_param(limit as i64));
    }
    SqlQuery::new(query_text, params).all(db, read_lane_move_row)
}

pub fn delete_lane_rows(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new(
        "DELETE FROM lane_moves WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .run(db)?;
    SqlQuery::new("DELETE FROM lanes WHERE session_id = ?", vec![text_param(session_id)]).run(db)?;
    Ok(())
}

fn append_lane_move(
    db: &rusqlite::Connection,
    session_id: &str,
    seq: i64,
    lane: &str,
    leaf_id: Option<&str>,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO lane_moves (session_id, seq, lane, leaf_id) VALUES (?, ?, ?, ?)",
        vec![
            text_param(session_id),
            int_param(seq),
            text_param(lane),
            optional_text_param(leaf_id),
        ],
    )
    .run(db)?;
    Ok(())
}
