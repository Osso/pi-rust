//! Error boundary shared by the SQLite session backend.
//!
//! The TypeScript backend throws either structured `SessionError`s or raw SQLite/`TypeError`
//! values. The Rust `SessionStorage`/`SessionSearch` contracts only accept `SessionError`, so raw
//! database failures keep their message and surface with the `storage` code.

use pi_agent_core::harness::session::{SessionError, SessionErrorCode};

/// Failure raised by one SQLite backend operation.
#[derive(Debug)]
pub enum SqliteError {
    /// Structured session failure.
    Session(SessionError),
    /// Raw SQLite failure; `message` is the SQLite error text.
    Database(String),
    /// Query-builder misuse, e.g. parameters on `exec`.
    TypeError(String),
}

impl SqliteError {
    pub fn database(message: impl Into<String>) -> Self {
        Self::Database(message.into())
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        Self::TypeError(message.into())
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Session(error) => error.message(),
            Self::Database(message) | Self::TypeError(message) => message,
        }
    }

    pub const fn code(&self) -> Option<SessionErrorCode> {
        match self {
            Self::Session(error) => Some(error.code()),
            Self::Database(_) | Self::TypeError(_) => None,
        }
    }

    /// Collapses the raw failure into the structured error surface of the storage contracts.
    pub fn into_session(self) -> SessionError {
        match self {
            Self::Session(error) => error,
            Self::Database(message) | Self::TypeError(message) => SessionError::new(SessionErrorCode::Storage, message),
        }
    }
}

impl std::fmt::Display for SqliteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for SqliteError {}

impl From<SessionError> for SqliteError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}

impl From<rusqlite::Error> for SqliteError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.to_string())
    }
}
