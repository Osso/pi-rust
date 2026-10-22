//! Port of `packages/ai/src/auth/oauth/kimi-coding.ts`.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::auth::oauth::device_code::{
    OAuthDeviceCodeError, OAuthDeviceCodePollOptions, OAuthDeviceCodePollResult, poll_oauth_device_code_flow,
};
use crate::auth::oauth::support::{OAuthHttpResponse, OAuthRuntime, form_encode, form_request, send};
use crate::auth::{AuthError, AuthEvent, AuthInteraction, ModelAuth, OAuthAuth, OAuthCredential};
use crate::http::ABORT_ERROR_MESSAGE;
use crate::request::CallbackError;

use crate::utils::provider_env::get_provider_env_value;

const CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
const DEVICE_CODE_TIMEOUT_SECONDS: f64 = 900.0;
const DEFAULT_POLL_INTERVAL_SECONDS: f64 = 5.0;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const REFRESH_MAX_RETRIES: usize = 3;

pub struct KimiCodingOAuth {
    runtime: OAuthRuntime,
    oauth_host: String,
}

impl KimiCodingOAuth {
    pub fn new() -> Self {
        Self::with_runtime(OAuthRuntime::system())
    }

    pub fn with_runtime(runtime: OAuthRuntime) -> Self {
        let override_host = get_provider_env_value("KIMI_CODE_OAUTH_HOST", None)
            .or_else(|| get_provider_env_value("KIMI_OAUTH_HOST", None));
        let oauth_host = trim_trailing_slashes(override_host.as_deref().unwrap_or(DEFAULT_OAUTH_HOST));
        Self { runtime, oauth_host }
    }

    pub fn with_oauth_host(mut self, host: impl Into<String>) -> Self {
        let host = host.into();
        self.oauth_host = trim_trailing_slashes(&host);
        self
    }
}

impl Default for KimiCodingOAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuthAuth for KimiCodingOAuth {
    fn name(&self) -> &str {
        "Kimi Code (subscription)"
    }

    fn is_subscription(&self) -> bool {
        true
    }

    fn login_label(&self) -> Option<&str> {
        Some("Sign in with Kimi Code")
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        let oauth_host = self.oauth_host.clone();
        Box::pin(async move { login_kimi_coding(&runtime, &oauth_host, interaction).await })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        let oauth_host = self.oauth_host.clone();
        Box::pin(async move {
            refresh_token(
                &runtime,
                &oauth_host,
                &crate::auth::oauth::support::to_form_usv_string(&credential.refresh),
                signal,
            )
            .await
            .map_err(|error| AuthError::failed(error.message))
        })
    }

    fn to_auth(&self, credential: OAuthCredential) -> crate::request::BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async move {
            let mut headers = crate::types::ProviderHeaders::new();
            headers.insert(
                "Authorization".to_owned(),
                Some(format!("Bearer {}", credential.access_utf8()?)),
            );
            Ok(ModelAuth {
                api_key: None,
                headers: Some(headers),
                base_url: None,
            })
        })
    }
}

fn trim_trailing_slashes(value: &str) -> String {
    value.trim_end_matches('/').to_owned()
}

#[derive(Debug, Clone, PartialEq)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri_complete: String,
    interval_seconds: f64,
    expires_in_seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct TokenResponse {
    access: String,
    refresh: String,
    expires: f64,
}

async fn send_with_timeout(
    runtime: &OAuthRuntime,
    request: crate::request::HttpRequest,
    signal: CancellationToken,
) -> Result<OAuthHttpResponse, CallbackError> {
    match tokio::time::timeout(REQUEST_TIMEOUT, send(&runtime.fetch, request, Some(signal))).await {
        Ok(result) => result,
        Err(_) => Err(CallbackError::new(ABORT_ERROR_MESSAGE)),
    }
}

fn trusted_http_url(value: &Value) -> Option<String> {
    let value = value.as_str().filter(|value| !value.is_empty())?;
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return None;
    }
    Some(url.to_string())
}

async fn start_device_authorization(
    runtime: &OAuthRuntime,
    oauth_host: &str,
    signal: CancellationToken,
) -> Result<DeviceAuthorization, CallbackError> {
    let url = format!("{oauth_host}/api/oauth/device_authorization");
    let request = form_request(&url, form_encode(&[("client_id", CLIENT_ID)]), true);
    let response = send_with_timeout(runtime, request, signal).await?;
    if !response.ok() {
        let suffix = if response.body.is_empty() {
            String::new()
        } else {
            format!(": {}", response.body)
        };
        return Err(CallbackError::new(format!(
            "Kimi Code device authorization failed with status {}{suffix}",
            response.status
        )));
    }
    let json = response.json.unwrap_or(Value::Null);
    let device_code = json.get("device_code").and_then(Value::as_str);
    let user_code = json.get("user_code").and_then(Value::as_str);
    let verification_uri = trusted_http_url(json.get("verification_uri").unwrap_or(&Value::Null));
    let verification_uri_complete = trusted_http_url(json.get("verification_uri_complete").unwrap_or(&Value::Null));
    let (Some(device_code), Some(user_code), Some(_verification_uri), Some(verification_uri_complete)) =
        (device_code, user_code, verification_uri, verification_uri_complete)
    else {
        return Err(CallbackError::new(format!(
            "Invalid Kimi Code device authorization response: {}",
            pi_js_compat::js_json::stringify(&json)
        )));
    };
    let interval = json.get("interval").and_then(Value::as_f64);
    let expires_in = json.get("expires_in").and_then(Value::as_f64);
    Ok(DeviceAuthorization {
        device_code: device_code.to_owned(),
        user_code: user_code.to_owned(),
        verification_uri_complete,
        interval_seconds: interval
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECONDS),
        expires_in_seconds: expires_in
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(DEVICE_CODE_TIMEOUT_SECONDS),
    })
}

fn parse_token_response(json: &Value, operation: &str, now: f64) -> Result<TokenResponse, CallbackError> {
    let access = json
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let refresh = json
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    let expires_in = json
        .get("expires_in")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0);
    let (Some(access), Some(refresh), Some(expires_in)) = (access, refresh, expires_in) else {
        return Err(CallbackError::new(format!(
            "Kimi Code token {operation} response missing fields: {}",
            pi_js_compat::js_json::stringify(json)
        )));
    };
    Ok(TokenResponse {
        access: access.to_owned(),
        refresh: refresh.to_owned(),
        expires: now + expires_in * 1000.0,
    })
}

async fn poll_for_token(
    runtime: &OAuthRuntime,
    oauth_host: &str,
    device: &DeviceAuthorization,
    signal: CancellationToken,
) -> Result<TokenResponse, CallbackError> {
    let options = OAuthDeviceCodePollOptions {
        interval_seconds: Some(device.interval_seconds),
        expires_in_seconds: Some(device.expires_in_seconds),
        wait_before_first_poll: true,
        signal: signal.clone(),
    };
    let device = device.clone();
    let runtime = runtime.clone();
    let oauth_host = oauth_host.to_owned();
    poll_oauth_device_code_flow(options, move || {
        let device = device.clone();
        let runtime = runtime.clone();
        let oauth_host = oauth_host.clone();
        let signal = signal.clone();
        async move {
            let url = format!("{oauth_host}/api/oauth/token");
            let body = form_encode(&[
                ("client_id", CLIENT_ID),
                ("device_code", device.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ]);
            let request = form_request(&url, body, true);
            let response = match send_with_timeout(&runtime, request, signal).await {
                Ok(response) => response,
                Err(error) => {
                    return OAuthDeviceCodePollResult::Failed { message: error.message };
                }
            };
            if response.status >= 500 {
                let suffix = if response.body.is_empty() {
                    String::new()
                } else {
                    format!(": {}", response.body)
                };
                return OAuthDeviceCodePollResult::Failed {
                    message: format!(
                        "Kimi Code device token request failed with status {}{suffix}",
                        response.status
                    )
                    .into(),
                };
            }
            let json = response.json.clone().unwrap_or(Value::Null);
            if response.ok() && json.get("access_token").is_some_and(Value::is_string) {
                return match parse_token_response(&json, "poll", runtime.now()) {
                    Ok(token) => OAuthDeviceCodePollResult::Complete(token),
                    Err(error) => OAuthDeviceCodePollResult::Failed { message: error.message },
                };
            }
            let error = json.get("error").and_then(Value::as_str);
            let description = json
                .get("error_description")
                .and_then(Value::as_str)
                .map(|description| format!(": {description}"))
                .unwrap_or_default();
            match error {
                Some("authorization_pending") => OAuthDeviceCodePollResult::Pending,
                Some("slow_down") => OAuthDeviceCodePollResult::SlowDown {
                    interval_seconds: json
                        .get("interval")
                        .and_then(Value::as_f64)
                        .filter(|value| *value > 0.0),
                },
                Some("expired_token") => OAuthDeviceCodePollResult::Failed {
                    message: "Kimi Code device authorization expired. Please restart login."
                        .to_owned()
                        .into(),
                },
                Some("access_denied") => OAuthDeviceCodePollResult::Failed {
                    message: "Kimi Code login was denied.".to_owned().into(),
                },
                _ => OAuthDeviceCodePollResult::Failed {
                    message: format!(
                        "Kimi Code device token request failed (status {}){}{}",
                        response.status,
                        error.map(|error| format!(": {error}")).unwrap_or_default(),
                        description
                    )
                    .into(),
                },
            }
        }
    })
    .await
    .map_err(|error| match error {
        OAuthDeviceCodeError::Cancelled => CallbackError::new("Login cancelled"),
        other => CallbackError::new(other.to_string()),
    })
}

async fn login_kimi_coding(
    runtime: &OAuthRuntime,
    oauth_host: &str,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let device = start_device_authorization(runtime, oauth_host, interaction.signal())
        .await
        .map_err(|error| AuthError::failed(error.message))?;
    interaction.notify(AuthEvent::DeviceCode {
        user_code: (device.user_code.clone()).into(),
        verification_uri: (device.verification_uri_complete.clone()).into(),
        interval_seconds: Some(device.interval_seconds),
        expires_in_seconds: Some(device.expires_in_seconds),
    });
    let token = poll_for_token(runtime, oauth_host, &device, interaction.signal())
        .await
        .map_err(|error| AuthError::failed(error.message))?;
    Ok(OAuthCredential::new(token.refresh, token.access, token.expires))
}

fn is_retryable_refresh_failure(status: u16) -> bool {
    status == 429 || status >= 500
}

async fn abortable_sleep(delay: Duration, signal: &CancellationToken) -> Result<(), CallbackError> {
    tokio::select! {
        biased;
        () = signal.cancelled() => Err(CallbackError::new(ABORT_ERROR_MESSAGE)),
        () = tokio::time::sleep(delay) => Ok(()),
    }
}

async fn refresh_token(
    runtime: &OAuthRuntime,
    oauth_host: &str,
    refresh_token_value: &str,
    signal: CancellationToken,
) -> Result<OAuthCredential, CallbackError> {
    let mut last_error: Option<CallbackError> = None;
    for attempt in 0..=REFRESH_MAX_RETRIES {
        if attempt > 0 {
            let delay = Duration::from_millis(1000 * 2_u64.pow(attempt as u32 - 1));
            abortable_sleep(delay, &signal).await?;
        }
        if signal.is_cancelled() {
            return Err(CallbackError::new("Kimi Code token refresh aborted"));
        }

        let url = format!("{oauth_host}/api/oauth/token");
        let body = form_encode(&[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token_value),
        ]);
        let request = form_request(&url, body, true);
        let response = match send_with_timeout(runtime, request, signal.clone()).await {
            Ok(response) => response,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };

        let json = response.json.clone().unwrap_or(Value::Null);
        if response.ok() {
            return parse_token_response(&json, "refresh", runtime.now())
                .map(|token| OAuthCredential::new(token.refresh, token.access, token.expires));
        }

        let error = json.get("error").and_then(Value::as_str);
        if response.status == 401 || response.status == 403 || error == Some("invalid_grant") {
            let description = json
                .get("error_description")
                .and_then(Value::as_str)
                .map(|description| format!(": {description}"))
                .unwrap_or_default();
            return Err(CallbackError::new(format!(
                "Kimi Code token refresh unauthorized (status {}){description}",
                response.status
            )));
        }

        if is_retryable_refresh_failure(response.status) && attempt < REFRESH_MAX_RETRIES {
            last_error = Some(CallbackError::new(format!(
                "Kimi Code token refresh failed with status {}",
                response.status
            )));
            continue;
        }

        return Err(CallbackError::new(format!(
            "Kimi Code token refresh failed with status {}: {}",
            response.status,
            pi_js_compat::js_json::stringify(&json)
        )));
    }

    Err(last_error.unwrap_or_else(|| CallbackError::new("Kimi Code token refresh failed")))
}
