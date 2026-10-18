//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/branch-entries.ts`.

use pi_agent_core::harness::session::{EntryOrder, SessionError, SessionErrorCode};
use rusqlite::Row;
use serde_json::Value;

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, optional_text_param, text_param};

/// Derived root-to-tip branch cache membership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedBranch {
    pub branch_id: String,
    pub leaf_seq: i64,
}

/// One cached branch entry joined with its canonical entry row.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedBranchEntryRow {
    pub session_id: String,
    pub id: String,
    pub entry_seq: i64,
    pub parent_id: Option<String>,
    pub entry_type: String,
    pub timestamp: i64,
    pub payload: String,
}

/// Branch cache query filters.
#[derive(Debug, Clone, Default)]
pub struct CachedBranchQuery {
    pub entry_type: Option<String>,
    pub custom_type: Option<String>,
    pub stop_at_type: Option<String>,
    pub stop_at_id: Option<String>,
    pub cursor_after_seq: Option<i64>,
    pub order: Option<EntryOrder>,
    pub limit: Option<usize>,
}

fn read_cached_branch_entry_row(row: &Row<'_>) -> rusqlite::Result<CachedBranchEntryRow> {
    Ok(CachedBranchEntryRow {
        session_id: row.get(0)?,
        id: row.get(1)?,
        entry_seq: row.get(2)?,
        parent_id: row.get(3)?,
        entry_type: row.get(4)?,
        timestamp: row.get(5)?,
        payload: row.get(6)?,
    })
}

pub fn read_cached_branch(
    db: &rusqlite::Connection,
    session_id: &str,
    leaf_id: &str,
) -> Result<Option<CachedBranch>, SqliteError> {
    let membership = SqlQuery::new(
        "SELECT branch_id, entry_seq
		FROM branch_entries
		WHERE session_id = ? AND entry_id = ?
		ORDER BY branch_id
		LIMIT 1",
        vec![text_param(session_id), text_param(leaf_id)],
    )
    .get(db, |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    Ok(membership.map(|(branch_id, leaf_seq)| CachedBranch { branch_id, leaf_seq }))
}

pub fn query_cached_branch_rows(
    db: &rusqlite::Connection,
    session_id: &str,
    branch: CachedBranch,
    query: &CachedBranchQuery,
) -> Result<Vec<CachedBranchEntryRow>, SqliteError> {
    let oldest_first = query.order == Some(EntryOrder::OldestFirst);
    let mut stop_predicates = Vec::new();
    let mut stop_params = Vec::new();
    if let Some(stop_at_type) = query.stop_at_type.as_deref() {
        stop_predicates.push("stop.entry_type = ?");
        stop_params.push(text_param(stop_at_type));
    }
    if let Some(stop_at_id) = query.stop_at_id.as_deref() {
        stop_predicates.push("stop.entry_id = ?");
        stop_params.push(text_param(stop_at_id));
    }

    let aggregate = if oldest_first { "MIN" } else { "MAX" };
    let boundary_comparison = if oldest_first { "<=" } else { ">=" };
    let cursor_comparison = if oldest_first { ">" } else { "<" };
    let direction = if oldest_first { "ASC" } else { "DESC" };

    let mut predicates = vec![
        "b.session_id = ?".to_owned(),
        "b.branch_id = ?".to_owned(),
        "b.entry_seq <= ?".to_owned(),
    ];
    let mut params = vec![
        text_param(session_id),
        text_param(branch.branch_id.clone()),
        int_param(branch.leaf_seq),
    ];
    if !stop_predicates.is_empty() {
        let boundary = format!(
            "SELECT {aggregate}(stop.entry_seq)
				FROM branch_entries AS stop
				WHERE stop.session_id = ?
					AND stop.branch_id = ?
					AND stop.entry_seq <= ?
					AND ({})",
            stop_predicates.join(" OR ")
        );
        predicates.push(format!(
            "b.entry_seq {boundary_comparison} COALESCE(({boundary}), {})",
            if oldest_first { branch.leaf_seq } else { 0 }
        ));
        params.push(text_param(session_id));
        params.push(text_param(branch.branch_id.clone()));
        params.push(int_param(branch.leaf_seq));
        params.extend(stop_params);
    }
    if let Some(cursor) = query.cursor_after_seq {
        predicates.push(format!("b.entry_seq {cursor_comparison} ?"));
        params.push(int_param(cursor));
    }
    if let Some(entry_type) = query.entry_type.as_deref() {
        predicates.push("b.entry_type = ?".to_owned());
        params.push(text_param(entry_type));
    }
    if let Some(custom_type) = query.custom_type.as_deref() {
        predicates.push("b.custom_type = ?".to_owned());
        params.push(text_param(custom_type));
    }

    let mut query_text = format!(
        "SELECT e.session_id, e.id, e.seq AS entry_seq, e.parent_id, e.type, e.timestamp, e.payload
		FROM branch_entries AS b
		JOIN entries AS e ON e.session_id = b.session_id AND e.id = b.entry_id
		WHERE {}
		ORDER BY b.entry_seq {direction}",
        predicates.join(" AND ")
    );
    if let Some(limit) = query.limit {
        query_text.push_str(" LIMIT ?");
        params.push(int_param(limit as i64));
    }
    SqlQuery::new(query_text, params).all(db, read_cached_branch_entry_row)
}

pub fn delete_branch_entries(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new(
        "DELETE FROM branch_entries WHERE session_id = ?",
        vec![text_param(session_id)],
    )
    .run(db)?;
    Ok(())
}

pub fn insert_branch_entry(
    db: &rusqlite::Connection,
    session_id: &str,
    branch_id: &str,
    entry_id: &str,
    entry_seq: i64,
    entry_type: &str,
    custom_type: Option<&str>,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO branch_entries
			(session_id, branch_id, entry_id, entry_seq, entry_type, custom_type)
			VALUES (?, ?, ?, ?, ?, ?)",
        vec![
            text_param(session_id),
            text_param(branch_id),
            text_param(entry_id),
            int_param(entry_seq),
            text_param(entry_type),
            optional_text_param(custom_type),
        ],
    )
    .run(db)?;
    Ok(())
}

struct BranchPathEntryRow {
    id: String,
    seq: i64,
    parent_id: Option<String>,
    entry_type: String,
    payload: String,
}

fn custom_type_from_payload(row: &BranchPathEntryRow) -> Result<Option<String>, SqliteError> {
    if row.entry_type != "custom" {
        return Ok(None);
    }
    let decoded = (|| -> Result<Option<String>, SqliteError> {
        let payload: Value =
            serde_json::from_str(&row.payload).map_err(|error| SqliteError::database(error.to_string()))?;
        let Value::Object(object) = payload else {
            return Err(SqliteError::database("Payload is not an object"));
        };
        match object.get("customType") {
            Some(Value::String(custom_type)) => Ok(Some(custom_type.clone())),
            _ => Err(SqliteError::database("Invalid custom payload")),
        }
    })();
    decoded.map_err(|_| {
        SqliteError::Session(SessionError::new(
            SessionErrorCode::InvalidEntry,
            format!(
                "Invalid SQLite session entry {}: failed to decode entry {}",
                row.id, row.id
            ),
        ))
    })
}

pub fn insert_branch_entries_for_path(
    db: &rusqlite::Connection,
    session_id: &str,
    branch_id: &str,
    leaf_id: &str,
) -> Result<(), SqliteError> {
    let mut path: Vec<BranchPathEntryRow> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut entry_id: Option<String> = Some(leaf_id.to_owned());

    while let Some(current) = entry_id {
        if seen.contains(&current) {
            return Err(SqliteError::Session(SessionError::new(
                SessionErrorCode::InvalidEntry,
                format!("Entry parent cycle at {current}"),
            )));
        }
        seen.push(current.clone());
        let row = SqlQuery::new(
            "SELECT id, seq, parent_id, type, payload
			FROM entries
			WHERE session_id = ? AND id = ?",
            vec![text_param(session_id), text_param(current.clone())],
        )
        .get(db, |row| {
            Ok(BranchPathEntryRow {
                id: row.get(0)?,
                seq: row.get(1)?,
                parent_id: row.get(2)?,
                entry_type: row.get(3)?,
                payload: row.get(4)?,
            })
        })?;
        let Some(row) = row else {
            return Err(SqliteError::Session(SessionError::new(
                SessionErrorCode::InvalidEntry,
                format!("Entry {current} not found"),
            )));
        };
        entry_id = row.parent_id.clone();
        path.push(row);
    }

    for row in path.iter().rev() {
        let custom_type = custom_type_from_payload(row)?;
        insert_branch_entry(
            db,
            session_id,
            branch_id,
            &row.id,
            row.seq,
            &row.entry_type,
            custom_type.as_deref(),
        )?;
    }
    Ok(())
}

pub fn read_branch_containing_entry(
    db: &rusqlite::Connection,
    session_id: &str,
    entry_id: &str,
) -> Result<Option<(String, i64)>, SqliteError> {
    let row = SqlQuery::new(
        "SELECT b.branch_id, b.entry_seq
		FROM branch_entries AS b
		WHERE b.session_id = ? AND b.entry_id = ?
		ORDER BY b.branch_id
		LIMIT 1",
        vec![text_param(session_id), text_param(entry_id)],
    )
    .get(db, |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    Ok(row)
}

pub fn copy_branch_entries_through_seq(
    db: &rusqlite::Connection,
    session_id: &str,
    target_branch_id: &str,
    source_branch_id: &str,
    through_seq: i64,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO branch_entries (session_id, branch_id, entry_id, entry_seq, entry_type, custom_type)
		SELECT session_id, ?, entry_id, entry_seq, entry_type, custom_type
		FROM branch_entries
		WHERE session_id = ? AND branch_id = ? AND entry_seq <= ?",
        vec![
            text_param(target_branch_id),
            text_param(session_id),
            text_param(source_branch_id),
            int_param(through_seq),
        ],
    )
    .run(db)?;
    Ok(())
}
