use std::fmt;
use std::future::Future;

use pi_js_compat::abort::AbortSignal;

/// Cancellation error raised when a signal aborts an operation.
/// Reason-aware consumer semantics are tracked separately from this boundary migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbortError {
    message: String,
}

impl AbortError {
    pub fn new() -> Self {
        Self {
            message: "The operation was aborted".to_owned(),
        }
    }
}

impl Default for AbortError {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AbortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AbortError {}

/// Creates an operation-local signal for APIs whose signal is optional.
pub fn operation_signal(signal: Option<&AbortSignal>) -> AbortSignal {
    signal.cloned().unwrap_or_default()
}

/// Stops waiting for an operation when its signal aborts.
///
/// Dropping the abandoned future replaces the TypeScript code that attaches a
/// rejection handler to the abandoned promise.
pub async fn race_with_abort_signal<T>(
    operation: impl Future<Output = T>,
    signal: &AbortSignal,
) -> Result<T, AbortError> {
    if signal.aborted() {
        return Err(AbortError::new());
    }

    tokio::select! {
        biased;
        _ = signal.wait() => Err(AbortError::new()),
        value = operation => Ok(value),
    }
}
