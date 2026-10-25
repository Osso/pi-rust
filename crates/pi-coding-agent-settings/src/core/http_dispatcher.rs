//! HTTP idle timeout, proxy settings, and configured `Fetch` construction
//! (`core/http-dispatcher.ts`).

use std::sync::Arc;
use std::time::Duration;

use pi_ai::http::{FetchClientOptions, fetch_with_options};
use pi_ai::request::Fetch;
use pi_js_compat::number::{js_number_to_string, parse_number};
use pi_js_compat::string::js_trim;

pub const DEFAULT_HTTP_IDLE_TIMEOUT_MS: u64 = 300_000;

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
            let trimmed = js_trim(value);
            if trimmed.eq_ignore_ascii_case("disabled") {
                return Some(0);
            }
            if trimmed.is_empty() {
                return None;
            }
            parse_http_idle_timeout_ms(HttpIdleTimeout::Number(parse_number(trimmed)))
        }
        HttpIdleTimeout::Number(value) => {
            if !value.is_finite() || value < 0.0 {
                return None;
            }
            Some(value.floor() as u64)
        }
    }
}

pub fn format_http_idle_timeout_ms(timeout_ms: f64) -> String {
    if let Some((label, _)) = HTTP_IDLE_TIMEOUT_CHOICES
        .iter()
        .find(|(_, choice_ms)| *choice_ms as f64 == timeout_ms)
    {
        return (*label).to_owned();
    }
    format!("{} sec", pi_js_compat::number::js_number_to_string(timeout_ms / 1000.0))
}

/// `applyHttpProxySettings` as a pure decision: the proxy is used only when the
/// corresponding variable is unset (`??=`), and blank input changes nothing.
pub fn resolve_http_proxy_settings(
    http_proxy: Option<&str>,
    existing_http_proxy: Option<&str>,
    existing_https_proxy: Option<&str>,
) -> HttpProxySettings {
    let proxy = http_proxy.map(js_trim).filter(|proxy| !proxy.is_empty());
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
            js_number_to_string(timeout_ms)
        ));
    };
    let options = FetchClientOptions {
        http1_only: true,
        idle_timeout: (normalized_timeout_ms > 0).then(|| Duration::from_millis(normalized_timeout_ms)),
    };
    let fetch = fetch_with_options(options)?;
    pi_ai::http::install_default_fetch(Arc::clone(&fetch));
    Ok(fetch)
}
