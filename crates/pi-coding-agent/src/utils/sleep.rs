use std::time::Duration;

use pi_js_compat::abort::AbortSignal;

/// A sleep interrupted by its abort signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepError {
    Aborted,
}

impl std::fmt::Display for SleepError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Aborted")
    }
}

impl std::error::Error for SleepError {}

/// Sleep for `ms` milliseconds unless the cancellation token fires.
pub async fn sleep(ms: u64, signal: Option<&AbortSignal>) -> Result<(), SleepError> {
    let duration = Duration::from_millis(ms);
    let Some(signal) = signal else {
        tokio::time::sleep(duration).await;
        return Ok(());
    };

    if signal.aborted() {
        return Err(SleepError::Aborted);
    }

    tokio::select! {
        () = tokio::time::sleep(duration) => Ok(()),
        _ = signal.wait() => Err(SleepError::Aborted),
    }
}
