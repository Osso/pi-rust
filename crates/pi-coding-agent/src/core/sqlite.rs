//! SQLite connection configuration for shared, multi-process control databases.
//!
//! Port of `core/sqlite.ts`. The TypeScript module wraps two drivers (`bun:sqlite` and
//! `node:sqlite`); the Rust port has one driver (`rusqlite`), so the `SqliteDatabase`
//! capability interface collapses into [`rusqlite::Connection`]. Observable behavior is
//! unchanged: WAL journal mode, `synchronous=NORMAL`, and a 5000 ms busy timeout.

use std::fmt;

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use rusqlite::{Connection, ErrorCode, OpenFlags};

/// Default multi-consumer open settings for shared process-local SQLite DBs.
pub const DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS: u64 = 5000;

/// Error raised by a control-database operation.
///
/// The message is the value the TypeScript runtime puts in `Error#message`. `contention`
/// records whether the underlying driver reported `SQLITE_BUSY`/`SQLITE_LOCKED`, which
/// `isSqliteContentionError` uses to retry.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlDbError {
    message: JsString,
    contention: bool,
    config: Option<Box<crate::config::ConfigError>>,
}

impl ControlDbError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            contention: false,
            config: None,
        }
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }

    pub fn is_contention(&self) -> bool {
        self.contention
    }

    fn from_sqlite(error: rusqlite::Error) -> Self {
        let contention = matches!(
            error.sqlite_error_code(),
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
        );
        Self {
            message: error.to_string().into(),
            contention,
            config: None,
        }
    }
}

impl fmt::Display for ControlDbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for ControlDbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.config.as_deref().map(|error| error as &dyn std::error::Error)
    }
}
impl From<crate::config::ConfigError> for ControlDbError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self {
            message: error.message().clone(),
            contention: false,
            config: Some(Box::new(error)),
        }
    }
}

impl From<rusqlite::Error> for ControlDbError {
    fn from(error: rusqlite::Error) -> Self {
        Self::from_sqlite(error)
    }
}

/// `createSqliteDatabase(path)`.
pub fn create_sqlite_database(path: &str) -> Result<Connection, ControlDbError> {
    Ok(Connection::open(path)?)
}

/// `createReadOnlySqliteDatabase(path)`.
pub fn create_read_only_sqlite_database(path: &str) -> Result<Connection, ControlDbError> {
    Ok(Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?)
}

/// `configureReadOnlySqliteDatabase`.
pub fn configure_read_only_sqlite_database(db: &Connection, busy_timeout_ms: u64) -> Result<(), ControlDbError> {
    db.execute_batch(&format!("PRAGMA busy_timeout = {busy_timeout_ms}"))?;
    Ok(())
}

/// `configureSharedSqliteDatabase`.
pub fn configure_shared_sqlite_database(db: &Connection, busy_timeout_ms: u64) -> Result<(), ControlDbError> {
    db.execute_batch(&format!("PRAGMA busy_timeout = {busy_timeout_ms}"))?;
    let journal_mode = read_pragma_value(db, "PRAGMA journal_mode")?;
    if journal_mode != "wal" {
        db.execute_batch("PRAGMA journal_mode = WAL")?;
    }
    let synchronous = read_pragma_value(db, "PRAGMA synchronous")?;
    // 1 == NORMAL. WAL + NORMAL is the standard multi-consumer throughput tradeoff.
    if synchronous != "1" && synchronous != "normal" {
        db.execute_batch("PRAGMA synchronous = NORMAL")?;
    }
    Ok(())
}

/// `isSqliteContentionError`.
pub fn is_sqlite_contention_error(error: &ControlDbError) -> bool {
    error.is_contention()
}

fn read_pragma_value(db: &Connection, sql: &str) -> Result<String, ControlDbError> {
    let value: rusqlite::types::Value = db.query_row(sql, [], |row| row.get(0))?;
    Ok(match value {
        rusqlite::types::Value::Text(value) => value.to_lowercase(),
        rusqlite::types::Value::Integer(value) => value.to_string(),
        rusqlite::types::Value::Real(value) => value.to_string(),
        _ => String::new(),
    })
}
