//! Session import errors (`core/session-errors.ts`).

use std::fmt;

/// Raised when `/import` references a JSONL file path that does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionImportFileNotFoundError {
    pub file_path: String,
}

impl SessionImportFileNotFoundError {
    /// The source error's `name`.
    pub const NAME: &'static str = "SessionImportFileNotFoundError";

    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }
}

impl fmt::Display for SessionImportFileNotFoundError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "File not found: {}", self.file_path)
    }
}

impl std::error::Error for SessionImportFileNotFoundError {}
