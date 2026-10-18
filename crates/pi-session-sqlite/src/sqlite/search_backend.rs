//! Port of `packages/session-backends/sqlite-node/src/sqlite/search-backend.ts`.

use std::sync::{Arc, Mutex};

use pi_agent_core::harness::session::{SessionError, SessionErrorCode};
use pi_agent_core::harness::types::{CreateDirOptions, FileError, FileErrorCode, FileSystem};
use pi_agent_core::search::{SearchError, SessionSearch, SessionSearchHit, SessionSearchOptions};
use pi_ai::request::BoxFuture;
use rusqlite::types::Value;

use crate::error::SqliteError;
use crate::sqlite::connection::{configure_sqlite_database, open_database, transaction};
use crate::sqlite::migrations::apply_migrations;
use crate::sqlite::path::get_parent_path;
use crate::sqlite::sql::{SqlQuery, int_param, text_param};
use crate::sqlite::storage::sessions::{SessionRow, decode_session_metadata};
use crate::sqlite::types::SqliteSessionMetadata;

/// Search options.
#[derive(Clone)]
pub struct SqliteSessionSearchOptions {
    pub env: Arc<dyn FileSystem>,
    pub database_path: String,
}

/// One FTS hit.
#[derive(Debug, Clone, PartialEq)]
pub struct SqliteSessionSearchHit {
    pub session_id: String,
    pub metadata: SqliteSessionMetadata,
    pub entry_id: String,
    pub timestamp: i64,
    pub score: f64,
}

impl SessionSearchHit for SqliteSessionSearchHit {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn entry_id(&self) -> &str {
        &self.entry_id
    }
}

fn get_file_system_result_or_throw<T>(result: Result<T, FileError>, message: &str) -> Result<T, SessionError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let code = if error.code == FileErrorCode::NotFound {
                SessionErrorCode::NotFound
            } else {
                SessionErrorCode::Storage
            };
            Err(SessionError::new(code, format!("{message}: {}", error.message)))
        }
    }
}

fn throw_if_aborted(options: &SessionSearchOptions) -> Result<(), SearchError> {
    match &options.signal {
        Some(signal) if signal.is_cancelled() => Err(SearchError::Aborted),
        _ => Ok(()),
    }
}

fn table_exists(db: &rusqlite::Connection, name: &str) -> Result<bool, SqliteError> {
    let row = SqlQuery::new(
        "SELECT 1 AS found FROM sqlite_master WHERE type = 'table' AND name = ? LIMIT 1",
        vec![text_param(name)],
    )
    .get(db, |row| row.get::<_, i64>(0))?;
    Ok(row.is_some())
}

fn rebuild_search_index(db: &rusqlite::Connection) -> Result<(), SqliteError> {
    SqlQuery::new(
        "INSERT INTO session_search_fts(session_search_fts) VALUES('rebuild')",
        Vec::new(),
    )
    .run(db)?;
    Ok(())
}

fn ensure_search_schema(db: &rusqlite::Connection) -> Result<(), SqliteError> {
    let fts_exists = table_exists(db, "session_search_fts")?;
    let entries_exist = table_exists(db, "entries")?;
    transaction(db, |db| {
        SqlQuery::new(
            "CREATE VIRTUAL TABLE IF NOT EXISTS session_search_fts USING fts5(
  payload,
  content = 'entries',
  content_rowid = 'rowid',
  tokenize = 'trigram remove_diacritics 1'
);
CREATE TRIGGER IF NOT EXISTS session_search_fts_ai AFTER INSERT ON entries BEGIN
  INSERT INTO session_search_fts(rowid, payload) VALUES (new.rowid, new.payload);
END;
CREATE TRIGGER IF NOT EXISTS session_search_fts_ad AFTER DELETE ON entries BEGIN
  INSERT INTO session_search_fts(session_search_fts, rowid, payload) VALUES('delete', old.rowid, old.payload);
END;
CREATE TRIGGER IF NOT EXISTS session_search_fts_au AFTER UPDATE OF payload ON entries BEGIN
  INSERT INTO session_search_fts(session_search_fts, rowid, payload) VALUES('delete', old.rowid, old.payload);
  INSERT INTO session_search_fts(rowid, payload) VALUES (new.rowid, new.payload);
END;
",
            Vec::new(),
        )
        .exec(db)?;
        if !fts_exists && entries_exist {
            rebuild_search_index(db)?;
        }
        Ok(())
    })
}

/// SQLite FTS search over a co-located canonical session database.
pub struct SqliteSessionSearch {
    options: SqliteSessionSearchOptions,
    database_path: Mutex<Option<String>>,
}

impl SqliteSessionSearch {
    pub fn new(options: SqliteSessionSearchOptions) -> Self {
        Self {
            options,
            database_path: Mutex::new(None),
        }
    }

    async fn get_database_path(&self) -> Result<String, SessionError> {
        if let Some(path) = self.database_path.lock().expect("search database path mutex").clone() {
            return Ok(path);
        }
        let path = get_file_system_result_or_throw(
            self.options.env.absolute_path(&self.options.database_path, None).await,
            &format!(
                "Failed to resolve SQLite search database {}",
                self.options.database_path
            ),
        )?;
        *self.database_path.lock().expect("search database path mutex") = Some(path.clone());
        Ok(path)
    }

    async fn open_database(&self) -> Result<rusqlite::Connection, SessionError> {
        let path = self.get_database_path().await?;
        let directory = get_parent_path(&path);
        get_file_system_result_or_throw(
            self.options
                .env
                .create_dir(&directory, CreateDirOptions { recursive: Some(true) })
                .await,
            &format!("Failed to create SQLite search directory {directory}"),
        )?;
        let connection = open_database(&path).map_err(SqliteError::into_session)?;
        configure_sqlite_database(&connection)
            .and_then(|()| apply_migrations(&connection))
            .and_then(|()| ensure_search_schema(&connection))
            .map_err(SqliteError::into_session)?;
        Ok(connection)
    }
}

type SearchRow = (SessionRow, String, i64, f64);

fn is_empty_search(text: &str, options: &SessionSearchOptions) -> bool {
    text.trim().is_empty()
        || options.limit == Some(0)
        || options
            .entry_types
            .as_ref()
            .is_some_and(|entry_types| entry_types.is_empty())
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count).collect::<Vec<_>>().join(", ")
}

fn build_search_query(text: &str, options: &SessionSearchOptions) -> SqlQuery {
    let query = format!("\"{}\"", text.trim().replace('"', "\"\""));
    let mut predicates = vec!["session_search_fts MATCH ?".to_owned()];
    let mut params: Vec<Value> = vec![text_param(query)];
    if let Some(entry_types) = options.entry_types.as_ref() {
        predicates.push(format!("se.type IN ({})", placeholders(entry_types.len())));
        for entry_type in entry_types {
            params.push(text_param(entry_type.as_str()));
        }
    }
    params.push(int_param(options.limit.map_or(-1, |limit| limit as i64)));
    let query_text = format!(
        "SELECT s.id, s.created_at, s.metadata, s.cwd, s.parent_session_id,
						name_fact.seq IS NOT NULL AS has_session_name,
						name_fact.value AS session_name,
						se.id AS entry_id, se.timestamp, bm25(session_search_fts) AS score
					FROM session_search_fts
					JOIN entries AS se ON se.rowid = session_search_fts.rowid
					JOIN sessions AS s ON s.id = se.session_id
					LEFT JOIN facts AS name_fact
						ON name_fact.session_id = s.id
						AND name_fact.kind = 'name'
						AND name_fact.key IS NULL
						AND name_fact.seq = (
							SELECT MAX(f.seq)
							FROM facts AS f
							WHERE f.session_id = s.id AND f.kind = 'name' AND f.key IS NULL
						)
					WHERE {}
					ORDER BY score
					LIMIT ?",
        predicates.join(" AND ")
    );
    SqlQuery::new(query_text, params)
}

fn read_search_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchRow> {
    Ok((
        SessionRow {
            id: row.get(0)?,
            created_at: row.get(1)?,
            metadata: row.get(2)?,
            cwd: row.get(3)?,
            parent_session_id: row.get(4)?,
            has_session_name: row.get(5)?,
            session_name: row.get(6)?,
        },
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn build_hit(row: SearchRow, path: &str) -> Result<SqliteSessionSearchHit, SessionError> {
    let (session, entry_id, timestamp, score) = row;
    let metadata = decode_session_metadata(&session, path).map_err(SqliteError::into_session)?;
    Ok(SqliteSessionSearchHit {
        session_id: session.id,
        metadata,
        entry_id,
        timestamp,
        score,
    })
}

impl SessionSearch<SqliteSessionSearchHit> for SqliteSessionSearch {
    fn search<'a>(
        &'a self,
        text: &'a str,
        options: SessionSearchOptions,
    ) -> BoxFuture<'a, Result<Vec<SqliteSessionSearchHit>, SearchError>> {
        Box::pin(async move {
            if is_empty_search(text, &options) {
                return Ok(Vec::new());
            }
            throw_if_aborted(&options)?;
            let db = self.open_database().await?;
            let rows = build_search_query(text, &options)
                .all(&db, read_search_row)
                .map_err(|error| SearchError::Session(error.into_session()))?;
            let path = self.get_database_path().await?;
            let mut hits = Vec::with_capacity(rows.len());
            for row in rows {
                throw_if_aborted(&options)?;
                hits.push(build_hit(row, &path)?);
            }
            Ok(hits)
        })
    }
}

/// Creates the SQLite FTS search backend.
pub fn create_sqlite_session_search(options: SqliteSessionSearchOptions) -> SqliteSessionSearch {
    SqliteSessionSearch::new(options)
}
