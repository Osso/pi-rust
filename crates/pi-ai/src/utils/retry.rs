use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use regress::Regex;
use tokio_util::sync::CancellationToken;

use crate::types::{AssistantMessage, StopReason};

const NON_RETRYABLE_PROVIDER_ERROR_PATTERNS: [&str; 10] = [
    "GoUsageLimitError",
    "FreeUsageLimitError",
    "Monthly usage limit reached",
    "available balance",
    "insufficient_quota",
    "out of budget",
    "quota exceeded",
    "billing",
    "Incomplete response returned, reason: max_output_tokens",
    "Incomplete response returned, reason: content_filter",
];

const RETRYABLE_PROVIDER_ERROR_PATTERNS: [&str; 41] = [
    "overloaded",
    "rate.?limit",
    "too many requests",
    "429",
    "500",
    "502",
    "503",
    "504",
    "524",
    "service.?unavailable",
    "server.?error",
    "server had an error",
    "internal.?error",
    "provider.?returned.?error",
    "network.?error",
    "connection.?error",
    "connection.?refused",
    "connection.?lost",
    "other side closed",
    "fetch failed",
    "upstream.?connect",
    "reset before headers",
    "socket hang up",
    "socket connection was closed",
    "closed unexpectedly",
    "timed? out",
    "timeout",
    "terminated",
    "websocket.?closed",
    "websocket.?error",
    r"^(?:Error:\s*)?WebSocket connection to 'wss://[^'\s]+' failed: Expected 101 status\s+code$",
    "Incomplete response returned",
    "ended without",
    "stream ended before message_stop",
    "stream ended before a terminal response event",
    "http2 request did not get a response",
    "retry delay",
    r"^Codex error: Unable to verify Daybreak Blue access\. Please try again\.(?:\nOpenAI request ID: [^\r\n]+)?$",
    "you can retry your request",
    "try your request again",
    "please retry your request",
];

/// Retry policy: bounded attempts with exponential backoff (`baseDelayMs * 2^(attempt-1)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryPolicy {
    pub enabled: bool,
    /// Max retry attempts (0 = no retries). The initial call never counts as a retry.
    pub max_retries: u32,
    /// Base delay in ms.
    pub base_delay_ms: f64,
}

/// Optional callbacks emitted by [`retry_assistant_call`] around each retry.
pub trait RetryCallbacks {
    /// Emitted before the backoff sleep of each retry attempt (1-indexed).
    fn on_retry_scheduled<'a>(
        &'a mut self,
        _attempt: u32,
        _max_attempts: u32,
        _delay_ms: f64,
        _error_message: &'a pi_js_compat::js_json::JsString,
    ) -> impl Future<Output = ()> + Send + 'a {
        async {}
    }

    /// Emitted after the backoff sleep, immediately before the retried call starts.
    fn on_retry_attempt_start(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Emitted once when the loop ends after at least one retry was scheduled.
    fn on_retry_finished(
        &mut self,
        _success: bool,
        _attempt: u32,
        _final_error: Option<&pi_js_compat::js_json::JsString>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
}

#[derive(Debug, Clone)]
struct LastRetry {
    attempt: u32,
    error_message: pi_js_compat::js_json::JsString,
}

/// Run a single assistant-producing call with bounded retry on transient errors.
pub async fn retry_assistant_call<P, F, C>(
    mut produce: P,
    policy: Option<&RetryPolicy>,
    signal: Option<&CancellationToken>,
    mut callbacks: Option<&mut C>,
) -> AssistantMessage
where
    P: FnMut() -> F,
    F: Future<Output = AssistantMessage>,
    C: RetryCallbacks + ?Sized,
{
    let retry_policy = policy.filter(|policy| policy.enabled);
    let mut attempt = 0;
    let mut last_retry: Option<LastRetry> = None;

    loop {
        let response = produce().await;
        if !should_retry(&response, retry_policy, attempt) {
            return finish_assistant_call(response, last_retry.as_ref(), &mut callbacks).await;
        }

        attempt += 1;
        let last = LastRetry {
            attempt,
            error_message: response
                .error_message
                .clone()
                .filter(|message| !message.is_empty())
                .unwrap_or_else(|| "Unknown error".to_owned().into()),
        };
        let policy = retry_policy.expect("retries are only scheduled with an enabled policy");
        let delay_ms = retry_delay_ms(policy, attempt);
        if let Some(callbacks) = callbacks.as_deref_mut() {
            callbacks
                .on_retry_scheduled(attempt, policy.max_retries, delay_ms, &last.error_message)
                .await;
        }

        if let Some(aborted) = wait_for_retry(response, &last, delay_ms, signal, &mut callbacks).await {
            return aborted;
        }
        if let Some(callbacks) = callbacks.as_deref_mut() {
            callbacks.on_retry_attempt_start().await;
        }
        last_retry = Some(last);
    }
}

fn should_retry(response: &AssistantMessage, retry_policy: Option<&RetryPolicy>, attempt: u32) -> bool {
    if response.stop_reason != StopReason::Error {
        return false;
    }
    let retries_exhausted = retry_policy.is_none_or(|policy| attempt >= policy.max_retries);
    !retries_exhausted && is_retryable_assistant_error(response)
}

fn retry_delay_ms(policy: &RetryPolicy, attempt: u32) -> f64 {
    policy.base_delay_ms * 2f64.powi((attempt - 1) as i32)
}

async fn finish_assistant_call<C>(
    response: AssistantMessage,
    last_retry: Option<&LastRetry>,
    callbacks: &mut Option<&mut C>,
) -> AssistantMessage
where
    C: RetryCallbacks + ?Sized,
{
    let Some(last) = last_retry else {
        return response;
    };

    let succeeded = !matches!(response.stop_reason, StopReason::Error | StopReason::Aborted);
    let final_error = if response.stop_reason == StopReason::Error {
        response.error_message.as_ref()
    } else {
        None
    };
    if let Some(callbacks) = callbacks.as_deref_mut() {
        callbacks.on_retry_finished(succeeded, last.attempt, final_error).await;
    }
    response
}

async fn wait_for_retry<C>(
    mut response: AssistantMessage,
    last_retry: &LastRetry,
    delay_ms: f64,
    signal: Option<&CancellationToken>,
    callbacks: &mut Option<&mut C>,
) -> Option<AssistantMessage>
where
    C: RetryCallbacks + ?Sized,
{
    let delay = Duration::from_secs_f64(delay_ms.max(0.0) / 1000.0);
    let aborted = match signal {
        Some(token) => tokio::select! {
            () = tokio::time::sleep(delay) => false,
            () = token.cancelled() => true,
        },
        None => {
            tokio::time::sleep(delay).await;
            false
        }
    };
    if !aborted {
        return None;
    }

    if let Some(callbacks) = callbacks.as_deref_mut() {
        callbacks
            .on_retry_finished(false, last_retry.attempt, Some(&last_retry.error_message))
            .await;
    }
    response.stop_reason = StopReason::Aborted;
    response.error_message = None;
    Some(response)
}

fn non_retryable_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| build_provider_error_pattern(&NON_RETRYABLE_PROVIDER_ERROR_PATTERNS))
}

fn retryable_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| build_provider_error_pattern(&RETRYABLE_PROVIDER_ERROR_PATTERNS))
}

fn build_provider_error_pattern(patterns: &[&str]) -> Regex {
    Regex::with_flags(&patterns.join("|").to_ascii_lowercase(), "").expect("provider error patterns are valid regexes")
}

/// Identifies the canonical Codex cybersecurity restriction, including suffixes.
pub fn is_codex_cybersecurity_restriction(message: &AssistantMessage) -> bool {
    const RESTRICTION: &str = "This content was flagged for possible cybersecurity risk.";
    if message.stop_reason != StopReason::Error || message.api != "openai-codex-responses" {
        return false;
    }
    let Some(error_message) = &message.error_message else {
        return false;
    };
    error_message.starts_with(RESTRICTION) || error_message.starts_with(&format!("Codex error: {RESTRICTION}"))
}

/// Classifies whether a failed assistant message looks like a transient provider
/// or transport error.
pub fn is_retryable_assistant_error(message: &AssistantMessage) -> bool {
    if message.stop_reason != StopReason::Error {
        return false;
    }
    let Some(error_message) = &message.error_message else {
        return false;
    };
    if error_message.is_empty() || is_codex_cybersecurity_restriction(message) {
        return false;
    }
    let error_message = super::error_body::fold_ascii_error_text(error_message);
    if non_retryable_pattern()
        .find_from_ucs2(error_message.units(), 0)
        .next()
        .is_some()
    {
        return false;
    }
    retryable_pattern()
        .find_from_ucs2(error_message.units(), 0)
        .next()
        .is_some()
}
