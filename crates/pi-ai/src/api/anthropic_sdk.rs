//! Shared port of the `@anthropic-ai/sdk` 0.91.1 request path used by
//! `anthropic-messages.ts`: URL, headers (including SDK-added ones), JSON body,
//! status-error formatting, and the SDK's `maxRetries: 0` fetch call.
//!
//! The SDK identity headers report the Rust runtime instead of Node
//! (`x-stainless-runtime` / `x-stainless-runtime-version`); every other
//! SDK-added header matches the pinned TypeScript SDK. Tests pin the
//! host-derived values through `model.headers` for exact comparisons.

use pi_js_compat::{js_format, js_json::JsString};
use std::sync::Arc;

use pi_js_compat::abort::AbortSignal;

use crate::request::{CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use crate::types::{Model, ProviderHeaders};
use crate::utils::abort::AbortError;
use crate::utils::headers::headers_to_record;
use crate::utils::provider_retry::{ProviderError, ProviderErrorHeaders, ProviderRequestError};

pub(crate) const SDK_VERSION: &str = "0.91.1";
pub(crate) const SDK_USER_AGENT: &str = "Anthropic/JS 0.91.1";
pub(crate) const ANTHROPIC_VERSION: &str = "2023-06-01";
pub(crate) const DEFAULT_TIMEOUT_MS: f64 = 600_000.0;
const MESSAGES_PATH: &str = "/v1/messages";

/// Authentication shape selected by `createClient`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    None,
    ApiKey(JsString),
    Bearer(JsString),
}

#[derive(Debug, Clone)]
pub struct SdkRequest {
    pub url: JsString,
    pub headers: Vec<(String, JsString)>,
    pub body: String,
    pub timeout_ms: f64,
}

pub fn build_request(
    model: &Model,
    params: &impl serde::Serialize,
    auth: Auth,
    default_headers: &ProviderHeaders,
    timeout_ms: Option<f64>,
) -> Result<SdkRequest, CallbackError> {
    let mut headers = HeaderSet::default();
    headers.set("Accept", "application/json");
    headers.set("User-Agent", SDK_USER_AGENT);
    headers.set("X-Stainless-Retry-Count", "0");
    let timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    headers.set("X-Stainless-Timeout", (timeout / 1000.0).trunc().to_string());
    headers.set("X-Stainless-Lang", "js");
    headers.set("X-Stainless-Package-Version", SDK_VERSION);
    headers.set_owned("X-Stainless-OS", platform_os());
    headers.set_owned("X-Stainless-Arch", platform_arch());
    headers.set("X-Stainless-Runtime", "rust");
    headers.set("X-Stainless-Runtime-Version", "unknown");
    headers.set("anthropic-dangerous-direct-browser-access", "true");
    headers.set("anthropic-version", ANTHROPIC_VERSION);
    match auth {
        Auth::None => {}
        Auth::ApiKey(api_key) => headers.set("X-Api-Key", &api_key),
        Auth::Bearer(token) => headers.set("Authorization", js_format!("Bearer {}", token)),
    }
    for (name, value) in default_headers {
        match value {
            Some(value) => headers.set(name, value),
            None => headers.remove(name),
        }
    }
    headers.set("content-type", "application/json");

    let body = serde_json::to_string(params).map_err(|error| CallbackError::new(error.to_string()))?;
    Ok(SdkRequest {
        url: build_url(&model.base_url),
        headers: headers.entries,
        body,
        timeout_ms: timeout,
    })
}

/// Matches `new URL(base + path).href`: percent-encodes characters such as the
/// `{PLACEHOLDER}` segments Cloudflare gateway base URLs carry.
fn build_url(base_url: &JsString) -> JsString {
    if base_url.ends_with('/') && MESSAGES_PATH.starts_with('/') {
        js_format!("{}{}", base_url, &MESSAGES_PATH[1..])
    } else {
        js_format!("{}{}", base_url, MESSAGES_PATH)
    }
}

fn platform_os() -> String {
    match std::env::consts::OS {
        "linux" => "Linux".to_owned(),
        "macos" => "MacOS".to_owned(),
        "windows" => "Windows".to_owned(),
        "freebsd" => "FreeBSD".to_owned(),
        "openbsd" => "OpenBSD".to_owned(),
        "android" => "Android".to_owned(),
        other => format!("Other:{other}"),
    }
}

fn platform_arch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "x64".to_owned(),
        "x86" => "x32".to_owned(),
        "aarch64" => "arm64".to_owned(),
        "arm" => "arm".to_owned(),
        other => format!("other:{other}"),
    }
}

#[derive(Default)]
struct HeaderSet {
    entries: Vec<(String, JsString)>,
}

impl HeaderSet {
    fn set(&mut self, name: &str, value: impl Into<JsString>) {
        self.remove(name);
        self.entries.push((name.to_owned(), value.into()));
    }

    fn set_owned(&mut self, name: &str, value: String) {
        self.remove(name);
        self.entries.push((name.to_owned(), value.into()));
    }

    fn remove(&mut self, name: &str) {
        self.entries
            .retain(|(existing, _)| !existing.eq_ignore_ascii_case(name));
    }
}

/// Executes one SDK request. Non-2xx responses become [`ProviderError`]s with
/// the same message shape as `APIError.makeMessage`.
pub async fn execute(
    request: &SdkRequest,
    fetch: Arc<dyn Fetch>,
    signal: Option<AbortSignal>,
) -> Result<HttpResponse, ProviderRequestError<CallbackError>> {
    if signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Err(ProviderRequestError::Abort(AbortError::new()));
    }
    let http_request = HttpRequest {
        method: "POST".to_owned(),
        url: crate::request::request_url(&request.url).map_err(ProviderRequestError::Other)?,
        headers: request
            .headers
            .iter()
            .map(|(name, value)| crate::request::header_value(value).map(|value| (name.clone(), value)))
            .collect::<Result<_, _>>()
            .map_err(ProviderRequestError::Other)?,
        body: Some(request.body.clone().into_bytes()),
    };
    let response = match super::stainless_fetch::fetch_with_timeout(
        fetch,
        http_request,
        signal.as_ref(),
        request.timeout_ms,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => {
            if signal.as_ref().is_some_and(AbortSignal::aborted) {
                return Err(ProviderRequestError::Abort(AbortError::new()));
            }
            let timeout = super::stainless_fetch::is_connection_timeout(&error).map_err(ProviderRequestError::Other)?;
            return Err(ProviderRequestError::Provider(ProviderError {
                message: if timeout {
                    "Request timed out."
                } else {
                    "Connection error."
                }
                .into(),
                status: None,
                headers: ProviderErrorHeaders::new(),
                body: None,
                inner_error: None,
            }));
        }
    };
    if (200..300).contains(&response.status) {
        return Ok(response);
    }

    let HttpResponse {
        status,
        status_text: _,
        headers,
        mut body,
    } = response;
    let text = read_text(&mut *body).await;
    Err(ProviderRequestError::Provider(status_error(status, &text, &headers)))
}

async fn read_text(body: &mut dyn HttpBody) -> pi_js_compat::js_json::JsString {
    let mut bytes = Vec::new();
    loop {
        match body.next_chunk().await {
            None => break,
            Some(Ok(chunk)) => bytes.extend_from_slice(&chunk),
            Some(Err(error)) => return error.message,
        }
    }
    String::from_utf8_lossy(&bytes).into_owned().into()
}

fn status_error(status: u16, text: &pi_js_compat::js_json::JsString, headers: &[(String, String)]) -> ProviderError {
    let parsed = pi_js_compat::js_json::parse_lossless(text);
    let inner_error = match &parsed {
        Ok(value) => value.get("error").cloned(),
        Err(_) => None,
    };
    let message = match &parsed {
        Ok(value) => error_message(value).unwrap_or_else(|| text.clone()),
        Err(_) => text.clone(),
    };
    let message = if message.is_empty() {
        format!("{status} status code (no body)").into()
    } else {
        pi_js_compat::js_json::JsString::join([format!("{status} ").into(), message], "")
    };
    let body = inner_error
        .as_ref()
        .filter(|error| matches!(error, pi_js_compat::js_json::JsValue::Object(object) if !object.is_empty()))
        .map(|error| pi_js_compat::js_json::stringify_lossless(error).into());
    let mut error_headers = ProviderErrorHeaders::new();
    for (name, value) in headers_to_record(headers.iter().cloned()) {
        error_headers.insert(name, value);
    }
    ProviderError {
        message,
        status: Some(f64::from(status)),
        headers: error_headers,
        body,
        inner_error: inner_error.map(Box::new),
    }
}

fn error_message(value: &pi_js_compat::js_json::JsValue) -> Option<pi_js_compat::js_json::JsString> {
    use pi_js_compat::js_json::{JsValue, is_truthy, stringify_lossless};
    let JsValue::Object(object) = value else {
        return is_truthy(value).then(|| stringify_lossless(value).into());
    };
    if let Some(message) = object.get("message").filter(|message| is_truthy(message)) {
        return Some(match message {
            JsValue::String(message) => message.clone(),
            other => stringify_lossless(other).into(),
        });
    }
    is_truthy(value).then(|| stringify_lossless(value).into())
}
