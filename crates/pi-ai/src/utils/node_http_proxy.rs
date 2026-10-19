//! Port of `packages/ai/src/utils/node-http-proxy.ts`.
//!
//! The selection rules mirror the TypeScript implementation. [`proxy_fetch`]
//! applies them to the crate's default reqwest client as a [`Fetch`] wrapper.

use std::sync::Arc;

use url::Url;

use crate::http::send_reqwest;
use crate::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse};
use crate::types::ProviderEnv;

pub const UNSUPPORTED_PROXY_PROTOCOL_MESSAGE: &str =
    "Unsupported proxy protocol. SOCKS and PAC proxy URLs are not supported; use an HTTP or HTTPS proxy URL.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyError {
    InvalidProxyUrl { proxy: String, message: String },
    UnsupportedProxyProtocol { protocol: String },
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidProxyUrl { proxy, message } => {
                let quoted = serde_json::to_string(proxy).unwrap_or_else(|_| format!("\"{proxy}\""));
                write!(formatter, "Invalid proxy URL {quoted}: {message}")
            }
            Self::UnsupportedProxyProtocol { protocol } => {
                write!(formatter, "{UNSUPPORTED_PROXY_PROTOCOL_MESSAGE} Got {protocol}")
            }
        }
    }
}

impl std::error::Error for ProxyError {}

/// `resolveHttpProxyUrlForTarget()` against the process environment.
pub fn resolve_http_proxy_url_for_target(
    target_url: &str,
    env: Option<&ProviderEnv>,
) -> Result<Option<Url>, ProxyError> {
    resolve_http_proxy_url_for_target_with(target_url, env, pi_js_compat::process_env::get)
}

/// Testable [`resolve_http_proxy_url_for_target`] with an injected process
/// environment.
pub fn resolve_http_proxy_url_for_target_with(
    target_url: &str,
    env: Option<&ProviderEnv>,
    process_env: impl Fn(&str) -> Option<String>,
) -> Result<Option<Url>, ProxyError> {
    let Some(proxy) = get_proxy_for_url(target_url, env, &process_env) else {
        return Ok(None);
    };
    let proxy_url = Url::parse(&proxy).map_err(|error| ProxyError::InvalidProxyUrl {
        proxy: proxy.clone(),
        message: error.to_string(),
    })?;
    if !matches!(proxy_url.scheme(), "http" | "https") {
        return Err(ProxyError::UnsupportedProxyProtocol {
            protocol: format!("{}:", proxy_url.scheme()),
        });
    }
    Ok(Some(proxy_url))
}

/// A [`Fetch`] implementation that selects an HTTP(S) proxy per target URL.
pub struct ProxyFetch {
    env: Option<ProviderEnv>,
}

/// `Fetch` wrapper applying `HTTP(S)_PROXY`/`NO_PROXY` selection.
pub fn proxy_fetch(env: Option<ProviderEnv>) -> Arc<dyn Fetch> {
    Arc::new(ProxyFetch { env })
}

impl Fetch for ProxyFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            let proxy = resolve_http_proxy_url_for_target(&request.url, self.env.as_ref())
                .map_err(|error| CallbackError::new(error.to_string()))?;
            send_reqwest(request, signal, proxy.as_ref()).await
        })
    }
}

fn get_proxy_for_url(
    target_url: &str,
    env: Option<&ProviderEnv>,
    process_env: &impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let parsed = Url::parse(target_url).ok()?;
    let protocol = parsed.scheme().to_owned();
    let (hostname, port) = target_host_and_port(&parsed)?;
    if !should_proxy_hostname(&hostname, port, env, process_env) {
        return None;
    }

    let mut proxy = get_proxy_env(&format!("{protocol}_proxy"), env, process_env);
    if proxy.is_empty() {
        proxy = get_proxy_env("all_proxy", env, process_env);
    }
    if proxy.is_empty() {
        return None;
    }
    if !proxy.contains("://") {
        proxy = format!("{protocol}://{proxy}");
    }
    Some(proxy)
}

fn target_host_and_port(parsed: &Url) -> Option<(String, f64)> {
    let host = parsed.host()?;
    let host_with_port = match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    let hostname = strip_port_suffix(&host_with_port);
    let port = parsed
        .port()
        .map(f64::from)
        .or_else(|| default_proxy_port(parsed.scheme()))
        .unwrap_or(0.0);
    Some((hostname, port))
}

fn strip_port_suffix(host: &str) -> String {
    let Some((candidate, suffix)) = host.rsplit_once(':') else {
        return host.to_owned();
    };
    if suffix.chars().all(|character| character.is_ascii_digit()) {
        candidate.to_owned()
    } else {
        host.to_owned()
    }
}

fn default_proxy_port(protocol: &str) -> Option<f64> {
    match protocol {
        "ftp" => Some(21.0),
        "gopher" => Some(70.0),
        "http" | "ws" => Some(80.0),
        "https" | "wss" => Some(443.0),
        _ => None,
    }
}

fn should_proxy_hostname(
    hostname: &str,
    port: f64,
    env: Option<&ProviderEnv>,
    process_env: &impl Fn(&str) -> Option<String>,
) -> bool {
    let no_proxy = get_proxy_env("no_proxy", env, process_env).to_lowercase();
    if no_proxy.is_empty() {
        return true;
    }
    if no_proxy == "*" {
        return false;
    }

    no_proxy
        .split(|character: char| character == ',' || character.is_whitespace())
        .all(|proxy| !proxy_matches(proxy, hostname, port))
}

fn proxy_matches(proxy: &str, hostname: &str, port: f64) -> bool {
    if proxy.is_empty() {
        return false;
    }

    let (proxy_hostname, proxy_port) = match proxy.rsplit_once(':') {
        Some((host, suffix))
            if !host.is_empty() && !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) =>
        {
            (host, suffix.parse::<f64>().ok())
        }
        _ => (proxy, None),
    };
    if let Some(proxy_port) = proxy_port
        && proxy_port != port
    {
        return false;
    }

    if !proxy_hostname.starts_with(['.', '*']) {
        return hostname == proxy_hostname;
    }

    let suffix = proxy_hostname.strip_prefix('*').unwrap_or(proxy_hostname);
    hostname.ends_with(suffix)
}

fn get_proxy_env(key: &str, env: Option<&ProviderEnv>, process_env: &impl Fn(&str) -> Option<String>) -> String {
    let lowercase = key.to_ascii_lowercase();
    let uppercase = key.to_ascii_uppercase();
    let scoped = |name: &str| {
        env.and_then(|env| env.get(name))
            .filter(|value| !value.is_empty())
            .cloned()
    };
    let process = |name: &str| process_env(name).filter(|value| !value.is_empty());

    scoped(&lowercase)
        .or_else(|| scoped(&uppercase))
        .or_else(|| process(&lowercase))
        .or_else(|| process(&uppercase))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_port_suffix_matches_the_typescript_regex() {
        assert_eq!(strip_port_suffix("example.com:8080"), "example.com");
        assert_eq!(strip_port_suffix("example.com:"), "example.com");
        assert_eq!(strip_port_suffix("example.com"), "example.com");
        assert_eq!(strip_port_suffix("[::1]:8080"), "[::1]");
        assert_eq!(strip_port_suffix("[::1]"), "[::1]");
        assert_eq!(strip_port_suffix("example.com:notaport"), "example.com:notaport");
    }

    #[test]
    fn ipv6_hosts_keep_brackets_for_no_proxy_matching() {
        let parsed = Url::parse("https://[::1]:8443/path").expect("valid URL");
        assert_eq!(parsed.host(), Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST)));
        assert_eq!(parsed.port(), Some(8443));
    }
}
