use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio_util::sync::CancellationToken;

use super::abort::AbortError;

pub const DEFAULT_MAX_RETRY_DELAY_MS: f64 = 60_000.0;
const RETRYABLE_CLIENT_STATUSES: [f64; 3] = [408.0, 409.0, 429.0];
const SERVER_ERROR_STATUS: f64 = 500.0;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderErrorHeaders {
    values: BTreeMap<String, String>,
}

impl ProviderErrorHeaders {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into().to_lowercase(), value.into());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(&name.to_lowercase()).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderError {
    pub message: String,
    pub status: Option<f64>,
    pub headers: ProviderErrorHeaders,
    /// SDK-parsed body object text (`normalizeProviderError`'s `error` field).
    pub body: Option<String>,
    /// SDK-parsed inner `error` object, retained for provider-specific metadata.
    pub inner_error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetryDelayLimitError {
    pub delay_ms: f64,
    pub max_ms: f64,
    pub provider_message: String,
}

impl fmt::Display for RetryDelayLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Server requested {}s retry delay (max: {}s). {}",
            (self.delay_ms / 1000.0).ceil(),
            (self.max_ms / 1000.0).ceil(),
            self.provider_message
        )
    }
}

impl std::error::Error for RetryDelayLimitError {}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderRequestError<E> {
    Provider(ProviderError),
    Other(E),
    Abort(AbortError),
    RetryDelayLimit(RetryDelayLimitError),
}

#[derive(Debug, Clone, Default)]
pub struct ProviderRetryOptions {
    pub max_retries: usize,
    pub max_retry_delay_ms: Option<f64>,
    pub signal: Option<CancellationToken>,
}

/// Reproduces the OpenAI/Anthropic SDK retry policy with an abortable sleep.
pub async fn retry_provider_request<T, E, F, Fut>(
    mut request: F,
    options: ProviderRetryOptions,
) -> Result<T, ProviderRequestError<E>>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, ProviderRequestError<E>>>,
{
    let max_retries = options.max_retries;
    let mut retries_remaining = max_retries;

    loop {
        match request().await {
            Ok(value) => return Ok(value),
            Err(ProviderRequestError::Provider(error)) => {
                if is_aborted(options.signal.as_ref()) {
                    return Err(ProviderRequestError::Abort(AbortError::new()));
                }
                if retries_remaining == 0 || !is_retryable_provider_error(&error) {
                    return Err(ProviderRequestError::Provider(error));
                }

                let retry_index = max_retries - retries_remaining;
                retries_remaining -= 1;
                let delay_ms = get_retry_delay_ms(&error, retry_index, options.max_retry_delay_ms)
                    .map_err(ProviderRequestError::RetryDelayLimit)?;
                abortable_sleep(delay_ms, options.signal.as_ref())
                    .await
                    .map_err(ProviderRequestError::Abort)?;
            }
            Err(other) => return Err(other),
        }
    }
}

fn is_aborted(signal: Option<&CancellationToken>) -> bool {
    signal.is_some_and(CancellationToken::is_cancelled)
}

fn is_retryable_provider_error(error: &ProviderError) -> bool {
    match error.headers.get("x-should-retry") {
        Some("true") => return true,
        Some("false") => return false,
        _ => {}
    }

    match error.status {
        None => true,
        Some(status) => RETRYABLE_CLIENT_STATUSES.contains(&status) || status >= SERVER_ERROR_STATUS,
    }
}

fn get_retry_delay_ms(
    error: &ProviderError,
    retry_index: usize,
    max_retry_delay_ms: Option<f64>,
) -> Result<f64, RetryDelayLimitError> {
    if let Some(retry_after_ms) = error.headers.get("retry-after-ms")
        && let Some(value) = parse_float(retry_after_ms)
    {
        return validate_server_retry_delay_ms(value, max_retry_delay_ms, &error.message);
    }

    if let Some(retry_after) = error.headers.get("retry-after") {
        let delay_ms = match parse_float(retry_after) {
            Some(seconds) => seconds * 1000.0,
            None => parse_http_date_delay_ms(retry_after),
        };
        return validate_server_retry_delay_ms(delay_ms, max_retry_delay_ms, &error.message);
    }

    let exponential_delay = (0.5 * 2_f64.powi(retry_index.min(64) as i32)).min(8.0) * 1000.0;
    Ok(exponential_delay * (1.0 - random_fraction() * 0.25))
}

fn validate_server_retry_delay_ms(
    delay_ms: f64,
    max_retry_delay_ms: Option<f64>,
    provider_message: &str,
) -> Result<f64, RetryDelayLimitError> {
    let max_ms = max_retry_delay_ms.unwrap_or(DEFAULT_MAX_RETRY_DELAY_MS);
    if max_ms > 0.0 && delay_ms > max_ms {
        return Err(RetryDelayLimitError {
            delay_ms,
            max_ms,
            provider_message: provider_message.to_owned(),
        });
    }
    Ok(delay_ms)
}

fn parse_http_date_delay_ms(retry_after: &str) -> f64 {
    let Ok(retry_at) = httpdate::parse_http_date(retry_after) else {
        return f64::NAN;
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return f64::NAN;
    };
    match retry_at.duration_since(UNIX_EPOCH) {
        Ok(retry_at) => retry_at.as_millis() as f64 - now.as_millis() as f64,
        Err(_) => f64::NAN,
    }
}

/// JavaScript `Number.parseFloat` for a leading numeric prefix.
fn parse_float(text: &str) -> Option<f64> {
    let trimmed = text.trim_start();
    let bytes = trimmed.as_bytes();
    let mut index = 0;

    if matches!(bytes.first(), Some(b'+' | b'-')) {
        index += 1;
    }
    let mut seen_digit = consume_digits(bytes, &mut index);
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        seen_digit |= consume_digits(bytes, &mut index);
    }
    if seen_digit && matches!(bytes.get(index), Some(b'e' | b'E')) {
        let exponent_start = index;
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        if !consume_digits(bytes, &mut index) {
            index = exponent_start;
        }
    }

    if !seen_digit {
        return None;
    }
    trimmed[..index].parse::<f64>().ok().filter(|value| value.is_finite())
}

fn consume_digits(bytes: &[u8], index: &mut usize) -> bool {
    let start = *index;
    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    *index > start
}

fn random_fraction() -> f64 {
    let mut bytes = [0_u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        return 0.0;
    }
    (u64::from_le_bytes(bytes) as f64) / (u64::MAX as f64)
}

async fn abortable_sleep(delay_ms: f64, signal: Option<&CancellationToken>) -> Result<(), AbortError> {
    if is_aborted(signal) {
        return Err(AbortError::new());
    }

    let delay = Duration::try_from_secs_f64((delay_ms.max(0.0)) / 1000.0).unwrap_or(Duration::ZERO);
    match signal {
        Some(signal) => {
            tokio::select! {
                biased;
                _ = signal.cancelled() => Err(AbortError::new()),
                _ = tokio::time::sleep(delay) => Ok(()),
            }
        }
        None => {
            tokio::time::sleep(delay).await;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_with_headers(headers: &[(&str, &str)]) -> ProviderError {
        let mut values = ProviderErrorHeaders::new();
        for (name, value) in headers {
            values.insert(*name, *value);
        }
        ProviderError {
            message: "boom".to_owned(),
            status: Some(429.0),
            headers: values,
            body: None,
            inner_error: None,
        }
    }

    #[test]
    fn parse_float_matches_javascript_prefix_parsing() {
        assert_eq!(parse_float("1000"), Some(1000.0));
        assert_eq!(parse_float(" 1.5e3ms"), Some(1500.0));
        assert_eq!(parse_float("277403"), Some(277_403.0));
        assert_eq!(parse_float("-12.5"), Some(-12.5));
        assert_eq!(parse_float("0x10"), Some(0.0));
        assert_eq!(parse_float("abc"), None);
        assert_eq!(parse_float("Infinity"), None);
        assert_eq!(parse_float(""), None);
    }

    #[test]
    fn exponential_delay_is_capped_at_eight_seconds() {
        let error = ProviderError {
            message: "boom".to_owned(),
            status: Some(500.0),
            headers: ProviderErrorHeaders::new(),
            body: None,
            inner_error: None,
        };
        for retry_index in 0..10 {
            let delay = get_retry_delay_ms(&error, retry_index, None).unwrap();
            let base = (0.5 * 2_f64.powi(retry_index as i32)).min(8.0) * 1000.0;
            assert!(
                delay <= base && delay >= base * 0.75,
                "index {retry_index}: {delay} vs {base}"
            );
        }
    }

    #[test]
    fn server_delays_are_validated_against_the_cap() {
        let error = error_with_headers(&[("retry-after-ms", "1500")]);
        assert_eq!(get_retry_delay_ms(&error, 0, Some(2000.0)).unwrap(), 1500.0);
        assert!(get_retry_delay_ms(&error, 0, Some(1000.0)).is_err());
        assert_eq!(get_retry_delay_ms(&error, 0, Some(0.0)).unwrap(), 1500.0);
    }
}
