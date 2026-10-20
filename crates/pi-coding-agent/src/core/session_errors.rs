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
    /// Display prefix; callers that only receive the message use it to detect
    /// this error type.
    pub const MESSAGE_PREFIX: &'static str = "File not found: ";

    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
        }
    }
}

impl fmt::Display for SessionImportFileNotFoundError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", Self::MESSAGE_PREFIX, self.file_path)
    }
}

impl std::error::Error for SessionImportFileNotFoundError {}
