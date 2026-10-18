use std::future::Future;

use tokio_util::sync::CancellationToken;

/// An operation abandoned because its abort signal fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbortError {
    message: String,
}

impl AbortError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for AbortError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AbortError {}

/// Normalize an optional public cancellation token.
pub fn operation_signal(signal: Option<CancellationToken>) -> CancellationToken {
    signal.unwrap_or_default()
}

/// Stop waiting on abort while observing the abandoned operation through
/// settlement.
///
/// Rust futures are lazy: when the signal is already cancelled the operation is
/// dropped without being polled.
pub async fn race_with_abort_signal<T, F>(operation: F, signal: Option<&CancellationToken>) -> Result<T, AbortError>
where
    F: Future<Output = T>,
{
    let Some(signal) = signal else {
        return Ok(operation.await);
    };

    if signal.is_cancelled() {
        return Err(abort_error());
    }

    tokio::select! {
        value = operation => Ok(value),
        () = signal.cancelled() => Err(abort_error()),
    }
}

fn abort_error() -> AbortError {
    AbortError::new("The operation was aborted")
}
