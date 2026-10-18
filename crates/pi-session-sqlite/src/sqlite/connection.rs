//! Connection and transaction boundary.
//!
//! Port of `packages/session-backends/sqlite-node/src/index.ts` (the `node:sqlite` adapter) plus
//! the PRAGMA configuration shared by `repo.ts` and `search-backend.ts`.
//!
//! TypeScript wraps `DatabaseSync` in `SqliteDatabase`/`SqliteStatement` capability interfaces so
//! callers can substitute another driver. The Rust port has exactly one driver (`rusqlite`), so
//! the capability interfaces collapse into [`Database`] and [`transaction`]. The observable
//! contract is unchanged: `BEGIN IMMEDIATE`, commit on success, rollback on failure, and
//! `journal_mode=WAL`, `synchronous=FULL`, `busy_timeout=5000` on every opened database.

use std::sync::{Mutex, MutexGuard};

use pi_agent_core::harness::session::{SessionError, SessionErrorCode};
use rusqlite::Connection;

use crate::error::SqliteError;
use crate::sqlite::sql::SqlQuery;

/// Opens a SQLite database at `path` (`:memory:` is accepted).
pub fn open_database(path: &str) -> Result<Connection, SqliteError> {
    Ok(Connection::open(path)?)
}

/// Applies the backend PRAGMAs.
pub fn configure_sqlite_database(db: &Connection) -> Result<(), SqliteError> {
    SqlQuery::new("PRAGMA journal_mode=WAL", Vec::new()).exec(db)?;
    SqlQuery::new("PRAGMA synchronous=FULL", Vec::new()).exec(db)?;
    SqlQuery::new("PRAGMA busy_timeout=5000", Vec::new()).exec(db)?;
    Ok(())
}

/// Runs a synchronous write transaction.
pub fn transaction<T>(
    db: &Connection,
    operation: impl FnOnce(&Connection) -> Result<T, SqliteError>,
) -> Result<T, SqliteError> {
    SqlQuery::new("BEGIN IMMEDIATE", Vec::new()).exec(db)?;
    match operation(db) {
        Ok(value) => {
            SqlQuery::new("COMMIT", Vec::new()).exec(db)?;
            Ok(value)
        }
        Err(error) => {
            let _ = SqlQuery::new("ROLLBACK", Vec::new()).exec(db);
            Err(error)
        }
    }
}

/// One shared connection that closes exactly once.
pub(crate) struct Database {
    connection: Mutex<Option<Connection>>,
}

impl Database {
    pub(crate) fn new(connection: Connection) -> Self {
        Self {
            connection: Mutex::new(Some(connection)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Connection>> {
        self.connection.lock().expect("sqlite connection mutex")
    }

    pub(crate) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, SqliteError>,
    ) -> Result<T, SqliteError> {
        let guard = self.lock();
        match guard.as_ref() {
            Some(connection) => operation(connection),
            None => Err(SqliteError::Session(SessionError::new(
                SessionErrorCode::Storage,
                "SQLite database is closed",
            ))),
        }
    }

    pub(crate) fn close(&self) {
        let mut guard = self.lock();
        *guard = None;
    }
}
