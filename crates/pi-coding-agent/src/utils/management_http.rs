use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use ureq::Agent;

const RETRYABLE_STATUS_CODES: [u16; 7] = [408, 425, 429, 500, 502, 503, 504];

/// A management HTTP request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
}

impl HttpRequest {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }
}

/// A completed management HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    Transport(String),
    Cancelled,
    Timeout,
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(message) => formatter.write_str(message),
            Self::Cancelled => formatter.write_str("The operation was aborted"),
            Self::Timeout => formatter.write_str("The operation timed out"),
        }
    }
}

impl std::error::Error for FetchError {}

/// Transport for management HTTP resources.
pub trait HttpFetcher {
    fn fetch(&self, request: &HttpRequest, timeout: Option<Duration>) -> Result<HttpResponse, FetchError>;
}

/// Bounded immediate retry for idempotent management HTTP requests.
#[derive(Debug, Clone)]
pub struct FetchRetryOptions {
    /// Additional attempts after the initial request. Defaults to two.
    pub max_retries: Option<usize>,
    /// Retry transient HTTP responses as well as transport failures. Defaults to true.
    pub retry_on_status: bool,
    /// Overall time budget shared by all attempts.
    pub timeout_ms: Option<u64>,
    /// Caller cancellation. Checked before every attempt.
    pub cancellation: Option<CancellationToken>,
}

impl Default for FetchRetryOptions {
    fn default() -> Self {
        Self {
            max_retries: None,
            retry_on_status: true,
            timeout_ms: None,
            cancellation: None,
        }
    }
}

/// Fetch a management HTTP resource with bounded immediate retry.
pub fn fetch_with_retry(
    fetcher: &dyn HttpFetcher,
    request: &HttpRequest,
    options: &FetchRetryOptions,
) -> Result<HttpResponse, FetchError> {
    let max_retries = options.max_retries.unwrap_or(2);
    let deadline = options
        .timeout_ms
        .filter(|timeout| *timeout > 0)
        .map(|timeout| Instant::now() + Duration::from_millis(timeout));

    for attempt in 0..=max_retries {
        if options
            .cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err(FetchError::Cancelled);
        }

        let remaining = remaining_budget(deadline)?;
        match fetcher.fetch(request, remaining) {
            Ok(response) => {
                let should_retry = options.retry_on_status
                    && RETRYABLE_STATUS_CODES.contains(&response.status)
                    && attempt < max_retries;
                if !should_retry {
                    return Ok(response);
                }
            }
            Err(error) => {
                if options
                    .cancellation
                    .as_ref()
                    .is_some_and(CancellationToken::is_cancelled)
                {
                    return Err(FetchError::Cancelled);
                }
                if attempt >= max_retries {
                    return Err(error);
                }
            }
        }
    }

    Err(FetchError::Timeout)
}

fn remaining_budget(deadline: Option<Instant>) -> Result<Option<Duration>, FetchError> {
    let Some(deadline) = deadline else {
        return Ok(None);
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(FetchError::Timeout);
    }
    Ok(Some(remaining))
}

/// A `ureq`-backed transport.
#[derive(Debug, Clone, Copy, Default)]
pub struct UreqFetcher;

impl UreqFetcher {
    pub fn new() -> Self {
        Self
    }
}

impl HttpFetcher for UreqFetcher {
    fn fetch(&self, request: &HttpRequest, timeout: Option<Duration>) -> Result<HttpResponse, FetchError> {
        let agent: Agent = Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(timeout)
            .build()
            .into();

        let mut response = agent
            .get(&request.url)
            .call()
            .map_err(|error| FetchError::Transport(error.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(u64::MAX)
            .read_to_vec()
            .map_err(|error| FetchError::Transport(error.to_string()))?;

        Ok(HttpResponse { status, body })
    }
}
