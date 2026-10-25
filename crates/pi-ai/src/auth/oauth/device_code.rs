use std::future::Future;
use std::time::Duration;

use pi_js_compat::abort::AbortSignal;
use tokio::time::Instant;

use crate::request::CallbackError;

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
    Failed { message: pi_js_compat::js_json::JsString },
    Complete(T),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OAuthDeviceCodePollOptions {
    pub interval_seconds: Option<f64>,
    pub expires_in_seconds: Option<f64>,
    pub wait_before_first_poll: bool,
    pub signal: AbortSignal,
}

pub async fn poll_oauth_device_code_flow<T, F, Fut>(
    options: OAuthDeviceCodePollOptions,
    mut poll: F,
) -> Result<T, CallbackError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<OAuthDeviceCodePollResult<T>, CallbackError>>,
{
    let deadline = deadline_from(options.expires_in_seconds);
    let mut state = PollState::new(options.interval_seconds);

    if options.wait_before_first_poll {
        sleep_until_deadline(state.interval_ms, deadline, &options.signal).await?;
    }

    while !is_expired(deadline) {
        if options.signal.aborted() {
            return Err(CallbackError::new(CANCEL_MESSAGE));
        }
        match poll().await? {
            OAuthDeviceCodePollResult::Complete(value) => return Ok(value),
            OAuthDeviceCodePollResult::Failed { message } => return Err(CallbackError::new(message)),
            OAuthDeviceCodePollResult::SlowDown { interval_seconds } => state.record_slow_down(interval_seconds),
            OAuthDeviceCodePollResult::Pending => {}
        }
        if is_expired(deadline) {
            break;
        }
        sleep_until_deadline(state.interval_ms, deadline, &options.signal).await?;
    }

    Err(state.timeout_error())
}

struct PollState {
    interval_ms: f64,
    slow_down_responses: usize,
}

impl PollState {
    fn new(interval_seconds: Option<f64>) -> Self {
        let seconds = interval_seconds.unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS);
        let interval_ms = (seconds * 1000.0).floor().max(MINIMUM_INTERVAL_MS);
        Self {
            interval_ms,
            slow_down_responses: 0,
        }
    }

    fn record_slow_down(&mut self, interval_seconds: Option<f64>) {
        self.slow_down_responses += 1;
        self.interval_ms = slow_down_interval(self.interval_ms, interval_seconds);
    }

    fn timeout_error(&self) -> CallbackError {
        if self.slow_down_responses > 0 {
            CallbackError::new(SLOW_DOWN_TIMEOUT_MESSAGE)
        } else {
            CallbackError::new(TIMEOUT_MESSAGE)
        }
    }
}

fn deadline_from(expires_in_seconds: Option<f64>) -> Option<Instant> {
    expires_in_seconds.map(|seconds| Instant::now() + duration_from_ms(seconds * 1000.0))
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
    signal: &AbortSignal,
) -> Result<(), CallbackError> {
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

async fn abortable_sleep(delay_ms: f64, signal: &AbortSignal) -> Result<(), CallbackError> {
    if signal.aborted() {
        return Err(CallbackError::new(CANCEL_MESSAGE));
    }

    tokio::select! {
        biased;
        _ = signal.wait() => Err(CallbackError::new(CANCEL_MESSAGE)),
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
