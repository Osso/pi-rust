//! SQLite connection configuration for shared, multi-process control databases.
//!
//! Port of `core/sqlite.ts`. The TypeScript module wraps two drivers (`bun:sqlite` and
//! `node:sqlite`); the Rust port has one driver (`rusqlite`), so the `SqliteDatabase`
//! capability interface collapses into [`rusqlite::Connection`]. Observable behavior is
//! unchanged: WAL journal mode, `synchronous=NORMAL`, and a 5000 ms busy timeout.

use std::fmt;

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
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
    original: Option<NativeNodeError>,
    callback: Option<Box<pi_ai::request::CallbackError>>,
}

impl ControlDbError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            contention: false,
            config: None,
            original: None,
            callback: None,
        }
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }

    pub fn is_contention(&self) -> bool {
        self.contention
    }

    pub(crate) fn native_error(&self) -> Option<&NativeNodeError> {
        self.original.as_ref().or_else(|| {
            self.config
                .as_deref()
                .and_then(crate::config::ConfigError::native_error)
        })
    }

    fn from_sqlite_open(error: rusqlite::Error) -> Self {
        let rusqlite::Error::SqliteFailure(failure, Some(message)) = error else {
            return Self::from_sqlite(error);
        };
        // node:sqlite exposes SQLite's message, not rusqlite's added open-path suffix.
        let message = if failure.code == ErrorCode::CannotOpen {
            JsString::from("unable to open database file")
        } else {
            JsString::from(message)
        };
        let original = NativeNodeError::new("Error", message.clone())
            .with_property(
                "code",
                pi_js_compat::js_json::JsValue::String("ERR_SQLITE_ERROR".into()),
            )
            .with_property(
                "errcode",
                pi_js_compat::js_json::JsValue::Number(f64::from(failure.extended_code)),
            )
            .with_property("errstr", pi_js_compat::js_json::JsValue::String(message.clone()));
        Self {
            message,
            contention: matches!(failure.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked),
            config: None,
            original: Some(original),
            callback: None,
        }
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
            original: None,
            callback: None,
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
        self.original
            .as_ref()
            .map(|error| error as &dyn std::error::Error)
            .or_else(|| self.config.as_deref().map(|error| error as &dyn std::error::Error))
            .or_else(|| self.callback.as_deref().map(|error| error as &dyn std::error::Error))
    }
}
impl From<pi_ai::request::CallbackError> for ControlDbError {
    fn from(error: pi_ai::request::CallbackError) -> Self {
        Self {
            message: error.message.clone(),
            contention: false,
            config: None,
            original: None,
            callback: Some(Box::new(error)),
        }
    }
}

impl From<ControlDbError> for pi_ai::request::CallbackError {
    fn from(error: ControlDbError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}
impl From<crate::config::ConfigError> for ControlDbError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self {
            message: error.message().clone(),
            contention: false,
            config: Some(Box::new(error)),
            original: None,
            callback: None,
        }
    }
}

impl From<NativeNodeError> for ControlDbError {
    fn from(error: NativeNodeError) -> Self {
        Self {
            message: error.message.clone(),
            contention: false,
            config: None,
            original: Some(error),
            callback: None,
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
    Connection::open(path).map_err(ControlDbError::from_sqlite_open)
}

/// `createReadOnlySqliteDatabase(path)`.
pub fn create_read_only_sqlite_database(path: &str) -> Result<Connection, ControlDbError> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(ControlDbError::from_sqlite_open)
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
