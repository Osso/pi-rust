use std::fmt;
use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const CANCEL_MESSAGE: &str = "Login cancelled";
const TIMEOUT_MESSAGE: &str = "Device flow timed out";
const SLOW_DOWN_TIMEOUT_MESSAGE: &str = "Device flow timed out after one or more slow_down responses. This is often caused by clock drift in WSL or VM environments. Please sync or restart the VM clock and try again.";
const MINIMUM_INTERVAL_MS: f64 = 1000.0;
const DEFAULT_POLL_INTERVAL_SECONDS: f64 = 5.0;
const SLOW_DOWN_INTERVAL_INCREMENT_MS: f64 = 5000.0;

#[derive(Debug, Clone, PartialEq)]
pub enum OAuthDeviceCodePollResult<T> {
    Pending,
    SlowDown { interval_seconds: Option<f64> },
    Failed { message: String },
    Complete(T),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OAuthDeviceCodePollOptions {
    pub interval_seconds: Option<f64>,
    pub expires_in_seconds: Option<f64>,
    pub wait_before_first_poll: bool,
    pub signal: CancellationToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuthDeviceCodeError {
    Cancelled,
    TimedOut,
    SlowDownTimedOut,
    Failed(String),
}

impl fmt::Display for OAuthDeviceCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str(CANCEL_MESSAGE),
            Self::TimedOut => formatter.write_str(TIMEOUT_MESSAGE),
            Self::SlowDownTimedOut => formatter.write_str(SLOW_DOWN_TIMEOUT_MESSAGE),
            Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for OAuthDeviceCodeError {}

pub async fn poll_oauth_device_code_flow<T, F, Fut>(
    options: OAuthDeviceCodePollOptions,
    mut poll: F,
) -> Result<T, OAuthDeviceCodeError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = OAuthDeviceCodePollResult<T>>,
{
    let deadline = options
        .expires_in_seconds
        .map(|seconds| Instant::now() + duration_from_ms(seconds * 1000.0));
    let mut interval_ms = options
        .interval_seconds
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS)
        .mul_add(1000.0, 0.0)
        .floor()
        .max(MINIMUM_INTERVAL_MS);

    let mut slow_down_responses = 0;
    if options.wait_before_first_poll {
        sleep_until_deadline(interval_ms, deadline, &options.signal).await?;
    }

    while !is_expired(deadline) {
        if options.signal.is_cancelled() {
            return Err(OAuthDeviceCodeError::Cancelled);
        }

        match poll().await {
            OAuthDeviceCodePollResult::Complete(value) => return Ok(value),
            OAuthDeviceCodePollResult::Failed { message } => return Err(OAuthDeviceCodeError::Failed(message)),
            OAuthDeviceCodePollResult::SlowDown { interval_seconds } => {
                slow_down_responses += 1;
                interval_ms = slow_down_interval(interval_ms, interval_seconds);
            }
            OAuthDeviceCodePollResult::Pending => {}
        }

        if is_expired(deadline) {
            break;
        }
        sleep_until_deadline(interval_ms, deadline, &options.signal).await?;
    }

    Err(if slow_down_responses > 0 {
        OAuthDeviceCodeError::SlowDownTimedOut
    } else {
        OAuthDeviceCodeError::TimedOut
    })
}

fn slow_down_interval(current_interval_ms: f64, interval_seconds: Option<f64>) -> f64 {
    match interval_seconds {
        Some(seconds) if seconds.is_finite() && seconds > 0.0 => (seconds * 1000.0).floor().max(MINIMUM_INTERVAL_MS),
        _ => (current_interval_ms + SLOW_DOWN_INTERVAL_INCREMENT_MS).max(MINIMUM_INTERVAL_MS),
    }
}

async fn sleep_until_deadline(
    interval_ms: f64,
    deadline: Option<Instant>,
    signal: &CancellationToken,
) -> Result<(), OAuthDeviceCodeError> {
    let delay_ms = match deadline {
        Some(deadline) => {
            let remaining_ms = deadline.saturating_duration_since(Instant::now()).as_secs_f64() * 1000.0;
            if remaining_ms <= 0.0 {
                return Ok(());
            }
            interval_ms.min(remaining_ms)
        }
        None => interval_ms,
    };
    abortable_sleep(delay_ms, signal).await
}

async fn abortable_sleep(delay_ms: f64, signal: &CancellationToken) -> Result<(), OAuthDeviceCodeError> {
    if signal.is_cancelled() {
        return Err(OAuthDeviceCodeError::Cancelled);
    }

    tokio::select! {
        biased;
        _ = signal.cancelled() => Err(OAuthDeviceCodeError::Cancelled),
        _ = tokio::time::sleep(duration_from_ms(delay_ms)) => Ok(()),
    }
}

fn is_expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

fn duration_from_ms(milliseconds: f64) -> Duration {
    if !milliseconds.is_finite() || milliseconds <= 0.0 {
        return Duration::ZERO;
    }
    Duration::from_millis(milliseconds.floor() as u64)
}
