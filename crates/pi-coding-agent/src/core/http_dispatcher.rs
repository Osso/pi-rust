//! HTTP idle timeout, proxy settings, and configured `Fetch` construction
//! (`core/http-dispatcher.ts`).

use std::sync::Arc;
use std::time::Duration;

use pi_ai::http::{FetchClientOptions, fetch_with_options};
use pi_ai::request::Fetch;

pub const DEFAULT_HTTP_IDLE_TIMEOUT_MS: u64 = 300_000;

/// Node's 250 ms default can terminate valid connection attempts on high-latency
/// routes; the source raises it to 2 s.
const DEFAULT_AUTO_SELECT_FAMILY_ATTEMPT_TIMEOUT_MS: u64 = 2_000;

pub const HTTP_IDLE_TIMEOUT_CHOICES: [(&str, u64); 5] = [
    ("30 sec", 30_000),
    ("1 min", 60_000),
    ("2 min", 120_000),
    ("5 min", 300_000),
    ("disabled", 0),
];

/// `unknown` input of the source, split into its two runtime shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HttpIdleTimeout<'a> {
    String(&'a str),
    Number(f64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpProxySettings {
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
}

/// `parseHttpIdleTimeoutMs`: strings are trimmed, `disabled` → 0, empty → none,
/// numeric strings re-enter as numbers; numbers must be finite and non-negative.
pub fn parse_http_idle_timeout_ms(value: HttpIdleTimeout<'_>) -> Option<u64> {
    match value {
        HttpIdleTimeout::String(value) => {
            let trimmed = value.trim();
            if trimmed.eq_ignore_ascii_case("disabled") {
                return Some(0);
            }
            if trimmed.is_empty() {
                return None;
            }
            parse_http_idle_timeout_ms(HttpIdleTimeout::Number(parse_javascript_number(trimmed)))
        }
        HttpIdleTimeout::Number(value) => {
            if !value.is_finite() || value < 0.0 {
                return None;
            }
            Some(value.floor() as u64)
        }
    }
}

pub fn format_http_idle_timeout_ms(timeout_ms: u64) -> String {
    if let Some((label, _)) = HTTP_IDLE_TIMEOUT_CHOICES
        .iter()
        .find(|(_, choice_ms)| *choice_ms == timeout_ms)
    {
        return (*label).to_owned();
    }
    format!("{} sec", timeout_ms as f64 / 1000.0)
}

/// `applyHttpProxySettings` as a pure decision: the proxy is used only when the
/// corresponding variable is unset (`??=`), and blank input changes nothing.
pub fn resolve_http_proxy_settings(
    http_proxy: Option<&str>,
    existing_http_proxy: Option<&str>,
    existing_https_proxy: Option<&str>,
) -> HttpProxySettings {
    let proxy = http_proxy.map(str::trim).filter(|proxy| !proxy.is_empty());
    HttpProxySettings {
        http_proxy: existing_http_proxy
            .map(str::to_owned)
            .or_else(|| proxy.map(str::to_owned)),
        https_proxy: existing_https_proxy
            .map(str::to_owned)
            .or_else(|| proxy.map(str::to_owned)),
    }
}

/// Build the configured `Fetch`; invalid timeouts keep the source's error text.
pub fn configure_http_dispatcher(timeout_ms: f64) -> Result<Arc<dyn Fetch>, String> {
    let Some(normalized_timeout_ms) = parse_http_idle_timeout_ms(HttpIdleTimeout::Number(timeout_ms)) else {
        return Err(format!(
            "Invalid HTTP idle timeout: {}",
            format_javascript_number(timeout_ms)
        ));
    };
    let options = FetchClientOptions {
        http1_only: true,
        idle_timeout: (normalized_timeout_ms > 0).then(|| Duration::from_millis(normalized_timeout_ms)),
    };
    fetch_with_options(options)
}

/// Milliseconds the source allows per connection attempt before trying the next address.
pub const fn auto_select_family_attempt_timeout_ms() -> u64 {
    DEFAULT_AUTO_SELECT_FAMILY_ATTEMPT_TIMEOUT_MS
}

/// `Number(value)` for the shapes a settings string can take.
fn parse_javascript_number(value: &str) -> f64 {
    let trimmed = value.trim_matches(|character: char| character.is_whitespace() || character == '\u{a0}');
    if trimmed.is_empty() {
        return 0.0;
    }
    match trimmed {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if let Some((radix, digits)) = radix_prefix(trimmed) {
        return u64::from_str_radix(digits, radix).map_or(f64::NAN, |value| value as f64);
    }
    if is_decimal_number(trimmed) {
        return trimmed.parse().unwrap_or(f64::NAN);
    }
    f64::NAN
}

fn radix_prefix(value: &str) -> Option<(u32, &str)> {
    let (radix, prefix_length) = if value.len() >= 2 && value[..2].eq_ignore_ascii_case("0x") {
        (16, 2)
    } else if value.len() >= 2 && value[..2].eq_ignore_ascii_case("0o") {
        (8, 2)
    } else if value.len() >= 2 && value[..2].eq_ignore_ascii_case("0b") {
        (2, 2)
    } else {
        return None;
    };
    let digits = &value[prefix_length..];
    (!digits.is_empty() && digits.chars().all(|digit| digit.is_digit(radix))).then_some((radix, digits))
}

fn is_decimal_number(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer_digits = count_digits(bytes, index);
    index += integer_digits;
    let mut fraction_digits = 0;
    if bytes.get(index) == Some(&b'.') {
        fraction_digits = count_digits(bytes, index + 1);
        index += 1 + fraction_digits;
    }
    if integer_digits == 0 && fraction_digits == 0 {
        return false;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_digits = count_digits(bytes, index);
        if exponent_digits == 0 {
            return false;
        }
        index += exponent_digits;
    }
    index == bytes.len()
}

fn count_digits(bytes: &[u8], start: usize) -> usize {
    bytes[start.min(bytes.len())..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

/// `String(number)` for error messages.
fn format_javascript_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "Infinity"
        } else {
            "-Infinity"
        }
        .to_owned();
    }
    format!("{value}")
}
