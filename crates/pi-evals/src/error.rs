//! Error values thrown by the eval harness and vitest-evals adapters.
//!
//! The TypeScript source throws `Error`, `TypeError`, and `AggregateError`
//! values. Rust keeps the JavaScript class in the variant so callers and tests
//! can distinguish them, while `Display` always yields the source message.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PiEvalError {
    /// `new Error(message)`.
    Error(String),
    /// `new TypeError(message)`.
    TypeError(String),
    /// `new DOMException(message, "AbortError")` from `AbortSignal.throwIfAborted()`.
    Abort(String),
    /// `new AggregateError(errors, message)`.
    Aggregate { message: String, errors: Vec<PiEvalError> },
}

impl PiEvalError {
    pub fn error(message: impl Into<String>) -> Self {
        Self::Error(message.into())
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        Self::TypeError(message.into())
    }

    pub fn aggregate(message: impl Into<String>, errors: Vec<PiEvalError>) -> Self {
        Self::Aggregate {
            message: message.into(),
            errors,
        }
    }

    /// JavaScript error class name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Error(_) => "Error",
            Self::TypeError(_) => "TypeError",
            Self::Abort(_) => "AbortError",
            Self::Aggregate { .. } => "AggregateError",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Error(message)
            | Self::TypeError(message)
            | Self::Abort(message)
            | Self::Aggregate { message, .. } => message,
        }
    }

    /// Nested errors of an `AggregateError`; empty for other classes.
    pub fn aggregate_errors(&self) -> &[PiEvalError] {
        match self {
            Self::Aggregate { errors, .. } => errors,
            _ => &[],
        }
    }
}

impl fmt::Display for PiEvalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for PiEvalError {}

impl From<String> for PiEvalError {
    fn from(message: String) -> Self {
        Self::Error(message)
    }
}

impl From<&str> for PiEvalError {
    fn from(message: &str) -> Self {
        Self::Error(message.to_owned())
    }
}
