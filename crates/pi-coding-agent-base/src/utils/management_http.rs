//! Port of `packages/coding-agent/src/utils/management-http.ts`.

use std::time::Duration;

use pi_ai::http::ABORT_ERROR_MESSAGE;
use pi_ai::request::{CallbackError, Fetch, HttpRequest, HttpResponse};
use pi_js_compat::abort::AbortSignal;

const RETRYABLE_STATUS_CODES: [u16; 7] = [408, 425, 429, 500, 502, 503, 504];
const DEFAULT_MAX_RETRIES: usize = 2;

/// Message of the `TimeoutError` raised by `AbortSignal.timeout()`.
pub const TIMEOUT_ERROR_MESSAGE: &str = "The operation was aborted due to timeout";

/// `FetchRetryOptions`.
#[derive(Debug, Clone)]
pub struct FetchRetryOptions {
    /// Additional attempts after the initial request. Defaults to two.
    pub max_retries: Option<usize>,
    /// Retry transient HTTP responses as well as transport failures. Defaults to true.
    pub retry_on_status: bool,
    /// Overall time budget shared by all attempts; absent or zero disables it.
    pub timeout_ms: Option<u64>,
}

impl Default for FetchRetryOptions {
    fn default() -> Self {
        Self {
            max_retries: None,
            retry_on_status: true,
            timeout_ms: None,
        }
    }
}

/// `fetchWithRetry`: bounded immediate retry for idempotent management requests.
///
/// `signal` is the caller's `init.signal`; caller cancellation and the timeout are terminal.
pub async fn fetch_with_retry(
    fetch: &dyn Fetch,
    request: HttpRequest,
    signal: Option<AbortSignal>,
    options: &FetchRetryOptions,
) -> Result<HttpResponse, CallbackError> {
    let max_retries = options.max_retries.unwrap_or(DEFAULT_MAX_RETRIES);
    let deadline = RequestDeadline::start(signal.as_ref(), options.timeout_ms);
    let attempt_signal = deadline
        .as_ref()
        .map(|deadline| deadline.signal.clone())
        .or(signal.clone());

    for attempt in 0.. {
        throw_if_aborted(signal.as_ref(), deadline.as_ref())?;
        match fetch.fetch(request.clone(), attempt_signal.clone()).await {
            Ok(response) => {
                let retryable = RETRYABLE_STATUS_CODES.contains(&response.status);
                if !(options.retry_on_status && retryable && attempt < max_retries) {
                    return Ok(response);
                }
                // The discarded response body is dropped (cancelled) before retrying.
            }
            Err(error) => {
                throw_if_aborted(signal.as_ref(), deadline.as_ref())?;
                if attempt >= max_retries {
                    return Err(error);
                }
            }
        }
    }
    unreachable!("the retry loop returns on every terminal attempt")
}

/// `AbortSignal.any([init.signal, AbortSignal.timeout(timeoutMs)])`.
pub(crate) struct RequestDeadline {
    /// Signal handed to `fetch`: cancelled by either the caller or the timeout.
    signal: AbortSignal,
    timed_out: AbortSignal,
    timer: tokio::task::JoinHandle<()>,
}

impl RequestDeadline {
    pub(crate) fn start(parent: Option<&AbortSignal>, timeout_ms: Option<u64>) -> Option<Self> {
        let timeout = Duration::from_millis(timeout_ms.filter(|ms| *ms > 0)?);
        let timer_signal = pi_js_compat::abort::AbortController::new();
        let signal = AbortSignal::any(&[parent.cloned().unwrap_or_default(), timer_signal.signal()]);
        let timer_flag = pi_js_compat::abort::AbortController::new();
        let timed_out = timer_flag.signal();
        let timer = tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            timer_flag.abort(None);
            timer_signal.abort(None);
        });
        Some(Self {
            signal,
            timed_out,
            timer,
        })
    }
}

impl Drop for RequestDeadline {
    fn drop(&mut self) {
        self.timer.abort();
    }
}

/// `signal?.throwIfAborted()` with the reason of whichever source fired.
pub(crate) fn throw_if_aborted(
    parent: Option<&AbortSignal>,
    deadline: Option<&RequestDeadline>,
) -> Result<(), CallbackError> {
    if parent.is_some_and(AbortSignal::aborted) {
        return Err(CallbackError::new(ABORT_ERROR_MESSAGE));
    }
    if deadline.is_some_and(|deadline| deadline.timed_out.aborted()) {
        return Err(CallbackError::new(TIMEOUT_ERROR_MESSAGE));
    }
    Ok(())
}
