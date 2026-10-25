//! Port of `packages/ai/src/auth/oauth/github-copilot.ts`.

use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use std::sync::Arc;

use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};

use crate::auth::oauth::device_code::{
    OAuthDeviceCodePollOptions, OAuthDeviceCodePollResult, poll_oauth_device_code_flow,
};
use crate::auth::oauth::support::{OAuthRuntime, form_encode, send};
use crate::auth::types::{
    AuthError, AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, ModelAuth, OAuthAuth, OAuthCredential,
};
use crate::providers::models_data;
use crate::request::{BoxFuture, CallbackError, HttpRequest};

/// `atob("SXYxLmI1MDdhMDhjODdlY2ZlOTg=")`.
const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const COPILOT_API_VERSION: &str = "2026-06-01";
const DEFAULT_DOMAIN: &str = "github.com";
const DEFAULT_BASE_URL: &str = "https://api.individual.githubcopilot.com";
const USER_AGENT: &str = "GitHubCopilotChat/0.35.0";
const MODELS_TIMEOUT_MS: f64 = 5000.0;

/// `COPILOT_HEADERS`.
const COPILOT_HEADERS: [(&str, &str); 4] = [
    ("User-Agent", USER_AGENT),
    ("Editor-Version", "vscode/1.107.0"),
    ("Editor-Plugin-Version", "copilot-chat/0.35.0"),
    ("Copilot-Integration-Id", "vscode-chat"),
];

/// `githubCopilotOAuth`.
pub struct GitHubCopilotOAuth {
    runtime: OAuthRuntime,
}

impl GitHubCopilotOAuth {
    pub fn new() -> Self {
        Self::with_runtime(OAuthRuntime::system())
    }

    pub fn with_runtime(runtime: OAuthRuntime) -> Self {
        Self { runtime }
    }
}

impl Default for GitHubCopilotOAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuthAuth for GitHubCopilotOAuth {
    fn name(&self) -> pi_js_compat::js_json::JsString {
        "GitHub Copilot".into()
    }

    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        Box::pin(async move { login(&runtime, &interaction).await })
    }

    fn refresh(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let signal = AbortSignal::default();
        let runtime = self.runtime.clone();
        Box::pin(async move {
            let enterprise_domain = copilot_enterprise_domain(&credential);
            refresh_token(&runtime, &credential.refresh, enterprise_domain.as_deref(), signal).await
        })
    }

    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async move {
            let enterprise_domain = copilot_enterprise_domain(&credential);
            Ok(ModelAuth {
                api_key: Some(credential.access.clone()),
                headers: None,
                base_url: Some(get_github_copilot_base_url(
                    Some(&credential.access),
                    enterprise_domain.as_deref(),
                )),
            })
        })
    }
}

/// `normalizeDomain`: accepts a bare domain or a URL and returns its hostname.
pub fn normalize_domain(input: &JsString) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        js_format!("https://{}", trimmed)
    };
    let url = url::Url::parse(&encode_node_utf8(&candidate)).ok()?;
    Some(url.host_str()?.to_owned())
}

struct Urls {
    device_code: String,
    access_token: String,
    copilot_token: String,
}

fn get_urls(domain: &str) -> Urls {
    Urls {
        device_code: format!("https://{domain}/login/device/code"),
        access_token: format!("https://{domain}/login/oauth/access_token"),
        copilot_token: format!("https://api.{domain}/copilot_internal/v2/token"),
    }
}

/// `getBaseUrlFromToken`: derives the API host from the token's `proxy-ep`.
pub fn get_base_url_from_token(token: &JsString) -> Option<JsString> {
    let mut rest = token.clone();
    let proxy_host = loop {
        let start = rest.find("proxy-ep=")? + "proxy-ep=".len();
        rest = rest.slice(start, rest.len());
        let end = rest.find(";").unwrap_or(rest.len());
        if end > 0 {
            break rest.slice(0, end);
        }
    };
    let api_host = if proxy_host.starts_with("proxy.") {
        js_format!("api.{}", proxy_host.slice(6, proxy_host.len()))
    } else {
        proxy_host
    };
    Some(js_format!("https://{}", api_host))
}

fn get_github_copilot_base_url(token: Option<&JsString>, enterprise_domain: Option<&str>) -> JsString {
    if let Some(token) = token
        && let Some(url) = get_base_url_from_token(token)
    {
        return url;
    }
    match enterprise_domain {
        Some(domain) => js_format!("https://copilot-api.{}", domain),
        None => DEFAULT_BASE_URL.into(),
    }
}

/// `parseAvailableCopilotModelIds`.
pub fn parse_available_copilot_model_ids(raw: &Value) -> Result<Vec<String>, String> {
    let data = raw
        .as_object()
        .and_then(|raw| raw.get("data"))
        .and_then(Value::as_array)
        .ok_or_else(|| "Invalid Copilot models response".to_owned())?;

    let mut picker_ids = Vec::new();
    for raw_item in data {
        let Some(item) = raw_item.as_object() else { continue };
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        let supports = item
            .get("capabilities")
            .and_then(Value::as_object)
            .and_then(|capabilities| capabilities.get("supports"))
            .and_then(Value::as_object);
        if supports.and_then(|supports| supports.get("tool_calls")) == Some(&Value::Bool(false)) {
            continue;
        }
        let policy_state = item
            .get("policy")
            .and_then(Value::as_object)
            .and_then(|policy| policy.get("state"))
            .and_then(Value::as_str);
        if item.get("model_picker_enabled") == Some(&Value::Bool(true)) && policy_state != Some("disabled") {
            picker_ids.push(id.to_owned());
        }
    }
    Ok(picker_ids)
}

struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: Option<f64>,
    expires_in: f64,
}

fn json_request(url: &str, method: &str, headers: Vec<(String, String)>, body: Option<String>) -> HttpRequest {
    HttpRequest {
        method: method.to_owned(),
        url: url.to_owned(),
        headers,
        body: body.map(String::into_bytes),
    }
}

fn copilot_header_pairs() -> Vec<(String, String)> {
    COPILOT_HEADERS
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn form_headers() -> Vec<(String, String)> {
    vec![
        ("Accept".to_owned(), "application/json".to_owned()),
        (
            "Content-Type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        ),
        ("User-Agent".to_owned(), USER_AGENT.to_owned()),
    ]
}

fn to_auth_error(error: CallbackError) -> AuthError {
    AuthError::from(error)
}

async fn fetch_json(
    runtime: &OAuthRuntime,
    request: HttpRequest,
    signal: &AbortSignal,
) -> Result<Value, CallbackError> {
    let response = send(&runtime.fetch, request, Some(signal.clone())).await?;
    if !response.ok() {
        return Err(CallbackError::new(format!(
            "{} {}: {}",
            response.status, response.status_text, response.body
        )));
    }
    response
        .json
        .ok_or_else(|| CallbackError::new("Unexpected end of JSON input"))
}

async fn fetch_json_with_timeout(runtime: &OAuthRuntime, request: HttpRequest) -> Result<Value, CallbackError> {
    let signal = AbortSignal::timeout(MODELS_TIMEOUT_MS)
        .map_err(|error| CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Error(error)))?;
    fetch_json(runtime, request, &signal).await
}

async fn start_device_flow(
    runtime: &OAuthRuntime,
    domain: &str,
    signal: &AbortSignal,
) -> Result<DeviceCodeResponse, CallbackError> {
    let urls = get_urls(domain);
    let body = form_encode(&[("client_id", CLIENT_ID), ("scope", "read:user")]);
    let request = json_request(&urls.device_code, "POST", form_headers(), Some(body));
    let data = fetch_json(runtime, request, signal).await?;

    let device_code = data.get("device_code").and_then(Value::as_str);
    let user_code = data.get("user_code").and_then(Value::as_str);
    let verification_uri = data.get("verification_uri").and_then(Value::as_str);
    let interval = data.get("interval");
    let expires_in = data.get("expires_in");

    if interval.is_some_and(|interval| !interval.is_number()) {
        return Err(CallbackError::new("Invalid device code response fields"));
    }
    let (Some(device_code), Some(user_code), Some(verification_uri), Some(expires_in)) = (
        device_code,
        user_code,
        verification_uri,
        expires_in.and_then(Value::as_f64),
    ) else {
        return Err(CallbackError::new("Invalid device code response fields"));
    };

    let parsed_uri = url::Url::parse(verification_uri)
        .map_err(|_| CallbackError::new("Untrusted verification_uri in device code response"))?;
    if parsed_uri.scheme() != "https" && parsed_uri.scheme() != "http" {
        return Err(CallbackError::new("Untrusted verification_uri in device code response"));
    }

    Ok(DeviceCodeResponse {
        device_code: device_code.to_owned(),
        user_code: user_code.to_owned(),
        verification_uri: parsed_uri.to_string(),
        interval: interval.and_then(Value::as_f64),
        expires_in,
    })
}

async fn poll_for_github_access_token(
    runtime: &OAuthRuntime,
    domain: &str,
    device: &DeviceCodeResponse,
    signal: &AbortSignal,
) -> Result<String, CallbackError> {
    let urls = get_urls(domain);
    let options = OAuthDeviceCodePollOptions {
        interval_seconds: device.interval,
        expires_in_seconds: Some(device.expires_in),
        wait_before_first_poll: true,
        signal: signal.clone(),
    };
    let poll_runtime = runtime.clone();
    let device_code = device.device_code.clone();
    poll_oauth_device_code_flow(options, move || {
        let runtime = poll_runtime.clone();
        let url = urls.access_token.clone();
        let device_code = device_code.clone();
        let signal = signal.clone();
        async move {
            let body = form_encode(&[
                ("client_id", CLIENT_ID),
                ("device_code", device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ]);
            let request = json_request(&url, "POST", form_headers(), Some(body));
            let raw = fetch_json(&runtime, request, &signal).await?;
            let object = match raw.as_object() {
                Some(object) => object,
                None => {
                    return Ok(OAuthDeviceCodePollResult::Failed {
                        message: "Invalid device token response".to_owned().into(),
                    });
                }
            };
            if let Some(access_token) = object.get("access_token").and_then(Value::as_str) {
                return Ok(OAuthDeviceCodePollResult::Complete(access_token.to_owned()));
            }
            let Some(error) = object.get("error").and_then(Value::as_str) else {
                return Ok(OAuthDeviceCodePollResult::Failed {
                    message: "Invalid device token response".to_owned().into(),
                });
            };
            if error == "authorization_pending" {
                return Ok(OAuthDeviceCodePollResult::Pending);
            }
            if error == "slow_down" {
                return Ok(OAuthDeviceCodePollResult::SlowDown {
                    interval_seconds: object.get("interval").and_then(Value::as_f64),
                });
            }
            let description = object
                .get("error_description")
                .and_then(Value::as_str)
                .map(|description| format!(": {description}"))
                .unwrap_or_default();
            Ok(OAuthDeviceCodePollResult::Failed {
                message: format!("Device flow failed: {error}{description}").into(),
            })
        }
    })
    .await
}

async fn refresh_access_token(
    runtime: &OAuthRuntime,
    refresh_token: &JsString,
    enterprise_domain: Option<&str>,
    signal: &AbortSignal,
) -> Result<OAuthCredential, CallbackError> {
    let domain = enterprise_domain.unwrap_or(DEFAULT_DOMAIN);
    let urls = get_urls(domain);
    let mut headers = vec![
        ("Accept".to_owned(), "application/json".to_owned()),
        (
            "Authorization".to_owned(),
            crate::request::header_value(&js_format!("Bearer {}", refresh_token))?,
        ),
    ];
    headers.extend(copilot_header_pairs());
    let request = json_request(&urls.copilot_token, "GET", headers, None);
    let response = send(&runtime.fetch, request, Some(signal.clone())).await?;
    if !response.ok() {
        return Err(CallbackError::new(format!(
            "{} {}: {}",
            response.status, response.status_text, response.body
        )));
    }
    let raw =
        pi_js_compat::js_json::parse_lossless(&response.body).map_err(|error| CallbackError::new(error.to_string()))?;
    let token = raw.get("token").and_then(pi_js_compat::js_json::JsValue::as_string);
    let expires_at = raw.get("expires_at").and_then(pi_js_compat::js_json::JsValue::as_f64);
    let (Some(token), Some(expires_at)) = (token, expires_at) else {
        return Err(CallbackError::new("Invalid Copilot token response fields"));
    };

    let mut credential = OAuthCredential::new(
        refresh_token.to_owned(),
        token.to_owned(),
        expires_at * 1000.0 - 300_000.0,
    );
    if let Some(domain) = enterprise_domain {
        credential.extra.insert(
            "enterpriseUrl".into(),
            pi_js_compat::js_json::JsValue::String(domain.into()),
        );
    }
    Ok(credential)
}

async fn refresh_token(
    runtime: &OAuthRuntime,
    refresh_token: &JsString,
    enterprise_domain: Option<&str>,
    signal: AbortSignal,
) -> Result<OAuthCredential, AuthError> {
    let mut credentials = refresh_access_token(runtime, refresh_token, enterprise_domain, &signal)
        .await
        .map_err(to_auth_error)?;
    let available = fetch_available_model_ids(runtime, &credentials.access, enterprise_domain)
        .await
        .map_err(to_auth_error)?;
    credentials.extra.insert(
        "availableModelIds".into(),
        pi_js_compat::js_json::JsValue::Array(
            available
                .into_iter()
                .map(|s| pi_js_compat::js_json::JsValue::String(s.into()))
                .collect(),
        ),
    );
    Ok(credentials)
}

async fn fetch_available_model_ids(
    runtime: &OAuthRuntime,
    copilot_token: &JsString,
    enterprise_domain: Option<&str>,
) -> Result<Vec<String>, CallbackError> {
    let base_url = get_github_copilot_base_url(Some(copilot_token), enterprise_domain);
    let mut headers = vec![
        ("Accept".to_owned(), "application/json".to_owned()),
        (
            "Authorization".to_owned(),
            crate::request::header_value(&js_format!("Bearer {}", copilot_token))?,
        ),
    ];
    headers.extend(copilot_header_pairs());
    headers.push(("X-GitHub-Api-Version".to_owned(), COPILOT_API_VERSION.to_owned()));
    let request = json_request(
        &crate::request::request_url(&js_format!("{}/models", base_url))?,
        "GET",
        headers,
        None,
    );
    let raw = fetch_json_with_timeout(runtime, request).await?;
    parse_available_copilot_model_ids(&raw).map_err(CallbackError::new)
}

async fn enable_github_copilot_model(
    runtime: &OAuthRuntime,
    token: &JsString,
    model_id: &str,
    enterprise_domain: Option<&str>,
) {
    let base_url = get_github_copilot_base_url(Some(token), enterprise_domain);
    // Frozen github-copilot.ts:307-321 catches request construction/fetch failures.
    let Ok(header) = crate::request::header_value(&js_format!("Bearer {}", token)) else {
        return;
    };
    let Ok(url) = crate::request::request_url(&js_format!("{}/models/{}/policy", base_url, model_id)) else {
        return;
    };
    let mut headers = vec![
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("Authorization".to_owned(), header),
    ];
    headers.extend(copilot_header_pairs());
    headers.push(("openai-intent".to_owned(), "chat-policy".to_owned()));
    headers.push(("x-interaction-type".to_owned(), "chat-policy".to_owned()));
    let request = json_request(&url, "POST", headers, Some(json!({"state": "enabled"}).to_string()));
    // Frozen policy enablement catches every fetch failure and does not pass the login signal.
    let _ = send(&runtime.fetch, request, None).await;
}

async fn enable_all_models(runtime: &OAuthRuntime, token: &JsString, enterprise_domain: Option<&str>) {
    let models = models_data::provider_models("github-copilot").unwrap_or_default();
    let policies = models
        .iter()
        .map(|model| enable_github_copilot_model(runtime, token, &model.id, enterprise_domain));
    futures_util::future::join_all(policies).await;
}

fn copilot_enterprise_domain(credential: &OAuthCredential) -> Option<String> {
    let enterprise_url = credential
        .extra
        .get(&pi_js_compat::js_json::JsString::from("enterpriseUrl"))?
        .as_string()?;
    if enterprise_url.is_empty() {
        return None;
    }
    normalize_domain(enterprise_url)
}

async fn login(runtime: &OAuthRuntime, interaction: &Arc<dyn AuthInteraction>) -> Result<OAuthCredential, AuthError> {
    let signal = interaction.signal();
    let input = interaction
        .prompt(AuthPrompt {
            signal: None,
            kind: AuthPromptKind::Text {
                message: ("GitHub Enterprise URL/domain (blank for github.com)".to_owned()).into(),
                placeholder: Some(("company.ghe.com".to_owned()).into()),
            },
        })
        .await
        .map_err(AuthError::from)?;
    if signal.aborted() {
        return Err(AuthError::failed("Login cancelled"));
    }

    let trimmed = input.trim();
    let enterprise_domain = normalize_domain(&input);
    if !trimmed.is_empty() && enterprise_domain.is_none() {
        return Err(AuthError::failed("Invalid GitHub Enterprise URL/domain"));
    }
    let domain = enterprise_domain.clone().unwrap_or_else(|| DEFAULT_DOMAIN.to_owned());

    let device = start_device_flow(runtime, &domain, &signal)
        .await
        .map_err(to_auth_error)?;
    interaction.notify(AuthEvent::DeviceCode {
        user_code: (device.user_code.clone()).into(),
        verification_uri: (device.verification_uri.clone()).into(),
        interval_seconds: device.interval,
        expires_in_seconds: Some(device.expires_in),
    })?;

    let github_access_token = poll_for_github_access_token(runtime, &domain, &device, &signal)
        .await
        .map_err(to_auth_error)?;
    let mut credentials = refresh_access_token(
        runtime,
        &github_access_token.into(),
        enterprise_domain.as_deref(),
        &signal,
    )
    .await
    .map_err(to_auth_error)?;

    interaction.notify(AuthEvent::Progress {
        message: ("Enabling models...".to_owned()).into(),
    })?;
    enable_all_models(runtime, &credentials.access, enterprise_domain.as_deref()).await;

    let available = fetch_available_model_ids(runtime, &credentials.access, enterprise_domain.as_deref())
        .await
        .map_err(to_auth_error)?;
    credentials.extra.insert(
        "availableModelIds".into(),
        pi_js_compat::js_json::JsValue::Array(
            available
                .into_iter()
                .map(|s| pi_js_compat::js_json::JsValue::String(s.into()))
                .collect(),
        ),
    );
    Ok(credentials)
}
