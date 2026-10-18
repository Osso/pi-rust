//! Port of `packages/session-backends/sqlite-node/src/sqlite/storage/facts.ts`.

use rusqlite::Row;

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, int_param, optional_text_param, text_param};

/// One global fact row.
#[derive(Debug, Clone, PartialEq)]
pub struct FactRow {
    pub session_id: String,
    pub seq: i64,
    pub kind: String,
    pub key: Option<String>,
    pub value: Option<String>,
}

fn read_fact_row(row: &Row<'_>) -> rusqlite::Result<FactRow> {
    Ok(FactRow {
        session_id: row.get(0)?,
        seq: row.get(1)?,
        kind: row.get(2)?,
        key: row.get(3)?,
        value: row.get(4)?,
    })
}

pub fn append_fact(
    db: &rusqlite::Connection,
    session_id: &str,
    seq: i64,
    kind: &str,
    key: Option<&str>,
    value: Option<&str>,
) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO facts (session_id, seq, kind, key, value) VALUES (?, ?, ?, ?, ?)",
        vec![
            text_param(session_id),
            int_param(seq),
            text_param(kind),
            optional_text_param(key),
            optional_text_param(value),
        ],
    )
    .run(db)?;
    Ok(())
}

pub fn read_latest_fact(
    db: &rusqlite::Connection,
    session_id: &str,
    kind: &str,
    key: Option<&str>,
) -> Result<Option<FactRow>, SqliteError> {
    let key_predicate = if key.is_some() { "key IS ?" } else { "key IS NULL" };
    let query_text = format!(
        "SELECT session_id, seq, kind, key, value
		FROM facts INDEXED BY idx_facts_session_kind_key_seq
		WHERE session_id = ? AND kind = ? AND {key_predicate}
		ORDER BY seq DESC
		LIMIT 1"
    );
    let mut params = vec![text_param(session_id), text_param(kind)];
    if let Some(key) = key {
        params.push(text_param(key));
    }
    SqlQuery::new(query_text, params).get(db, read_fact_row)
}

/// Returns the latest non-null label value per key, ordered by key.
pub fn read_latest_label_facts(
    db: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<(Option<String>, String)>, SqliteError> {
    SqlQuery::new(
        "SELECT f.key, f.value
		FROM facts AS f INDEXED BY idx_facts_session_kind_key_seq
		WHERE f.session_id = ?
			AND f.kind = 'label'
			AND f.value IS NOT NULL
			AND f.seq = (
				SELECT MAX(candidate.seq)
				FROM facts AS candidate INDEXED BY idx_facts_session_kind_key_seq
				WHERE candidate.session_id = f.session_id
					AND candidate.kind = f.kind
					AND candidate.key IS f.key
			)
		ORDER BY f.key",
        vec![text_param(session_id)],
    )
    .all(db, |row| Ok((row.get(0)?, row.get(1)?)))
}

pub fn read_fact_rows(
    db: &rusqlite::Connection,
    session_id: &str,
    after_seq: Option<i64>,
    limit: Option<usize>,
) -> Result<Vec<FactRow>, SqliteError> {
    let mut query_text = String::from(
        "SELECT session_id, seq, kind, key, value
		FROM facts
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
    SqlQuery::new(query_text, params).all(db, read_fact_row)
}

pub fn delete_fact_rows(db: &rusqlite::Connection, session_id: &str) -> Result<(), SqliteError> {
    SqlQuery::new("DELETE FROM facts WHERE session_id = ?", vec![text_param(session_id)]).run(db)?;
    Ok(())
}
