//! Error values thrown by the eval harness and vitest-evals adapters.
//!
//! The TypeScript source throws `Error`, `TypeError`, and `AggregateError`
//! values. Rust keeps the JavaScript class in the variant so callers and tests
//! can distinguish them, while `Display` always yields the source message.

use std::fmt;

use crate::harness::HarnessRun;

/// JavaScript error class and message.
#[derive(Debug, Clone, PartialEq)]
pub enum PiEvalErrorKind {
    /// `new Error(message)`.
    Error(String),
    /// `new TypeError(message)`.
    TypeError(String),
    /// `new DOMException(message, "AbortError")` from `AbortSignal.throwIfAborted()`.
    Abort(String),
    /// `new AggregateError(errors, message)`.
    Aggregate { message: String, errors: Vec<PiEvalError> },
}

/// A thrown eval error. `harness_run` is the `vitestEvalsRun` property that
/// `attachHarnessRunToError` adds to a thrown error so reporters can still read
/// the partial run of a failed harness.
#[derive(Debug, Clone, PartialEq)]
pub struct PiEvalError {
    kind: PiEvalErrorKind,
    harness_run: Option<Box<HarnessRun>>,
}

impl PiEvalError {
    fn from_kind(kind: PiEvalErrorKind) -> Self {
        Self {
            kind,
            harness_run: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::from_kind(PiEvalErrorKind::Error(message.into()))
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        Self::from_kind(PiEvalErrorKind::TypeError(message.into()))
    }

    pub fn abort(message: impl Into<String>) -> Self {
        Self::from_kind(PiEvalErrorKind::Abort(message.into()))
    }

    pub fn aggregate(message: impl Into<String>, errors: Vec<PiEvalError>) -> Self {
        Self::from_kind(PiEvalErrorKind::Aggregate {
            message: message.into(),
            errors,
        })
    }

    pub fn kind(&self) -> &PiEvalErrorKind {
        &self.kind
    }

    /// JavaScript error class name.
    pub fn name(&self) -> &'static str {
        match &self.kind {
            PiEvalErrorKind::Error(_) => "Error",
            PiEvalErrorKind::TypeError(_) => "TypeError",
            PiEvalErrorKind::Abort(_) => "AbortError",
            PiEvalErrorKind::Aggregate { .. } => "AggregateError",
        }
    }

    pub fn message(&self) -> &str {
        match &self.kind {
            PiEvalErrorKind::Error(message)
            | PiEvalErrorKind::TypeError(message)
            | PiEvalErrorKind::Abort(message)
            | PiEvalErrorKind::Aggregate { message, .. } => message,
        }
    }

    /// Nested errors of an `AggregateError`; empty for other classes.
    pub fn aggregate_errors(&self) -> &[PiEvalError] {
        match &self.kind {
            PiEvalErrorKind::Aggregate { errors, .. } => errors,
            _ => &[],
        }
    }

    /// `attachHarnessRunToError(error, run)`: same error, now carrying `run`.
    pub fn with_harness_run(mut self, run: HarnessRun) -> Self {
        self.harness_run = Some(Box::new(run));
        self
    }

    /// `getHarnessRunFromError(error)`.
    pub fn harness_run(&self) -> Option<&HarnessRun> {
        self.harness_run.as_deref()
    }

    /// Removes and returns the attached run.
    pub fn take_harness_run(&mut self) -> Option<HarnessRun> {
        self.harness_run.take().map(|run| *run)
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
        Self::error(message)
    }
}

impl From<&str> for PiEvalError {
    fn from(message: &str) -> Self {
        Self::error(message)
    }
}
