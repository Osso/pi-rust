//! EnvHttpProxyAgent selection: proxy URLs are captured at configuration;
//! NO_PROXY is read again for each dispatch.

use pi_js_compat::process_env;
use url::Url;

fn read_proxy_url(lowercase: &str, uppercase: &str) -> Result<Option<Url>, String> {
    process_env::get(lowercase)
        .or_else(|| process_env::get(uppercase))
        .filter(|value| !value.is_empty())
        .map(|value| Url::parse(&value).map_err(|error| error.to_string()))
        .transpose()
}

pub(super) fn capture_environment_proxy() -> Result<reqwest::Proxy, String> {
    let http = read_proxy_url("http_proxy", "HTTP_PROXY")?;
    let https = read_proxy_url("https_proxy", "HTTPS_PROXY")?.or_else(|| http.clone());
    Ok(reqwest::Proxy::custom(move |target| {
        if bypass_environment_proxy(target) {
            return None;
        }
        if target.scheme() == "https" {
            https.clone()
        } else {
            http.clone()
        }
    }))
}

fn bypass_environment_proxy(target: &Url) -> bool {
    let no_proxy = process_env::get("no_proxy")
        .or_else(|| process_env::get("NO_PROXY"))
        .unwrap_or_default();
    if no_proxy == "*" {
        return true;
    }
    let hostname = target
        .host_str()
        .unwrap_or_default()
        .trim_matches(['[', ']'])
        .to_lowercase();
    let port = target.port_or_known_default().unwrap_or(0);
    no_proxy
        .split(|unit: char| unit == ',' || unit.is_whitespace() || unit == '\u{feff}')
        .filter(|entry| !entry.is_empty())
        .any(|entry| matches_no_proxy_entry(entry, &hostname, port))
}

fn split_host_port(entry: &str) -> (&str, Option<u16>) {
    if let Some(bracketed) = entry.strip_prefix('[')
        && let Some((host, suffix)) = bracketed.split_once(']')
    {
        return (host, suffix.strip_prefix(':').and_then(|port| port.parse().ok()));
    }
    if entry.matches(':').count() == 1
        && let Some((host, port)) = entry.rsplit_once(':')
        && !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
    {
        return (host, port.parse().ok());
    }
    (entry, None)
}

fn matches_no_proxy_entry(entry: &str, hostname: &str, port: u16) -> bool {
    let (host, required_port) = split_host_port(entry);
    if required_port.is_some_and(|required| required != 0 && required != port) {
        return false;
    }
    let host = host
        .strip_prefix("*.")
        .or_else(|| host.strip_prefix('.'))
        .unwrap_or(host)
        .to_lowercase();
    hostname == host || hostname.ends_with(&format!(".{host}"))
}
