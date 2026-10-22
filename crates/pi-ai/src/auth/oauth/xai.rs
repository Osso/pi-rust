//! Port of `packages/ai/src/auth/oauth/xai.ts`.

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::auth::oauth::device_code::{
    OAuthDeviceCodeError, OAuthDeviceCodePollOptions, OAuthDeviceCodePollResult, poll_oauth_device_code_flow,
};
use crate::auth::oauth::support::{OAuthHttpResponse, OAuthRuntime, form_encode, form_request, send};
use crate::auth::{AuthError, AuthEvent, AuthInteraction, ModelAuth, OAuthAuth, OAuthCredential};
use crate::request::CallbackError;

const XAI_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const XAI_DEVICE_CODE_URL: &str = "https://auth.x.ai/oauth2/device/code";
const XAI_TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const REFRESH_SKEW_MS: f64 = 5.0 * 60.0 * 1000.0;
const DEFAULT_TOKEN_LIFETIME_SECONDS: f64 = 3600.0;

pub struct XaiOAuth {
    runtime: OAuthRuntime,
}

impl XaiOAuth {
    pub fn new() -> Self {
        Self::with_runtime(OAuthRuntime::system())
    }

    pub fn with_runtime(runtime: OAuthRuntime) -> Self {
        Self { runtime }
    }
}

impl Default for XaiOAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuthAuth for XaiOAuth {
    fn name(&self) -> &str {
        "xAI (Grok/X subscription)"
    }

    fn is_subscription(&self) -> bool {
        true
    }

    fn login_label(&self) -> Option<&str> {
        Some("Sign in with SuperGrok or X Premium")
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        Box::pin(async move { login_xai(&runtime, interaction).await })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        Box::pin(async move {
            refresh_xai_token(&runtime, &credential.refresh, signal)
                .await
                .map_err(AuthError::from)
        })
    }

    fn to_auth(&self, credential: OAuthCredential) -> crate::request::BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async move {
            Ok(ModelAuth {
                api_key: Some(credential.access.clone()),
                ..ModelAuth::default()
            })
        })
    }
}

use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
struct XaiDeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    interval_seconds: Option<f64>,
    expires_in_seconds: f64,
}

fn required_string(body: &Value, field: &str) -> Result<String, CallbackError> {
    body.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| CallbackError::new(format!("Invalid xAI OAuth response field: {field}")))
}

fn positive_number(body: &Value, field: &str) -> Result<f64, CallbackError> {
    body.get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| CallbackError::new(format!("Invalid xAI OAuth response field: {field}")))
}

fn validate_verification_uri(raw: &str) -> Result<String, CallbackError> {
    let url =
        url::Url::parse(raw).map_err(|_| CallbackError::new("Untrusted verification URI in xAI OAuth response"))?;
    if url.scheme() != "https" {
        return Err(CallbackError::new("Untrusted verification URI in xAI OAuth response"));
    }
    Ok(url.to_string())
}

async fn post_form(
    runtime: &OAuthRuntime,
    url: &str,
    fields: &[(&str, &str)],
    signal: CancellationToken,
) -> Result<OAuthHttpResponse, CallbackError> {
    let request = form_request(url, form_encode(fields), true);
    let response = match send(&runtime.fetch, request, Some(signal.clone())).await {
        Ok(response) => response,
        Err(_error) if signal.is_cancelled() => return Err(CallbackError::new("Login cancelled")),
        Err(error) => return Err(error),
    };
    if response.json.is_none() {
        if signal.is_cancelled() {
            return Err(CallbackError::new("Login cancelled"));
        }
        return Err(CallbackError::new(format!(
            "xAI OAuth returned invalid JSON (HTTP {})",
            response.status
        )));
    }
    Ok(response)
}

fn request_failure(action: &str, response: &OAuthHttpResponse) -> CallbackError {
    let body = response.json.as_ref().cloned().unwrap_or(Value::Null);
    let error = body.get("error").and_then(Value::as_str);
    let description = body.get("error_description").and_then(Value::as_str);
    let detail = [error, description]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(": ");
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    };
    CallbackError::new(format!("xAI OAuth {action} failed (HTTP {}){suffix}", response.status))
}

fn parse_device_code(body: &Value) -> Result<XaiDeviceCode, CallbackError> {
    let interval_seconds = body
        .get("interval")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0);
    let verification_uri_complete = body
        .get("verification_uri_complete")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(validate_verification_uri)
        .transpose()?;
    Ok(XaiDeviceCode {
        device_code: required_string(body, "device_code")?,
        user_code: required_string(body, "user_code")?,
        verification_uri: validate_verification_uri(&required_string(body, "verification_uri")?)?,
        verification_uri_complete,
        interval_seconds,
        expires_in_seconds: positive_number(body, "expires_in")?,
    })
}

fn credentials_from_token_response(
    body: &Value,
    previous_refresh_token: Option<&pi_js_compat::js_json::JsString>,
    now: f64,
) -> Result<OAuthCredential, CallbackError> {
    let access = required_string(body, "access_token")?;
    let refresh = match body.get("refresh_token") {
        None => previous_refresh_token
            .map(str::to_owned)
            .ok_or_else(|| CallbackError::new("Invalid xAI OAuth response field: refresh_token"))?,
        Some(_) => required_string(body, "refresh_token")?,
    };
    let expires_in_seconds = match body.get("expires_in") {
        None => DEFAULT_TOKEN_LIFETIME_SECONDS,
        Some(_) => positive_number(body, "expires_in")?,
    };
    Ok(OAuthCredential::new(
        refresh,
        access,
        now + expires_in_seconds * 1000.0 - REFRESH_SKEW_MS,
    ))
}

async fn request_device_code(
    runtime: &OAuthRuntime,
    signal: CancellationToken,
) -> Result<XaiDeviceCode, CallbackError> {
    let response = post_form(
        runtime,
        XAI_DEVICE_CODE_URL,
        &[("client_id", XAI_CLIENT_ID), ("scope", XAI_SCOPE), ("referrer", "pi")],
        signal,
    )
    .await?;
    if !response.ok() {
        return Err(request_failure("device authorization", &response));
    }
    let body = response.json.unwrap_or(Value::Null);
    parse_device_code(&body)
}

async fn poll_for_tokens(
    runtime: &OAuthRuntime,
    device: &XaiDeviceCode,
    signal: CancellationToken,
) -> Result<OAuthCredential, CallbackError> {
    let options = OAuthDeviceCodePollOptions {
        interval_seconds: device.interval_seconds,
        expires_in_seconds: Some(device.expires_in_seconds),
        wait_before_first_poll: true,
        signal: signal.clone(),
    };
    let device = device.clone();
    let runtime = runtime.clone();
    poll_oauth_device_code_flow(options, move || {
        let device = device.clone();
        let runtime = runtime.clone();
        let signal = signal.clone();
        async move {
            let response = match post_form(
                &runtime,
                XAI_TOKEN_URL,
                &[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", XAI_CLIENT_ID),
                    ("device_code", device.device_code.as_str()),
                ],
                signal.clone(),
            )
            .await
            {
                Ok(response) => response,
                Err(error) => {
                    return OAuthDeviceCodePollResult::Failed { message: error.message };
                }
            };
            if response.ok() {
                let body = response.json.unwrap_or(Value::Null);
                return match credentials_from_token_response(&body, None, runtime.now()) {
                    Ok(credential) => OAuthDeviceCodePollResult::Complete(credential),
                    Err(error) => OAuthDeviceCodePollResult::Failed { message: error.message },
                };
            }
            let body = response.json.clone().unwrap_or(Value::Null);
            match body.get("error").and_then(Value::as_str) {
                Some("authorization_pending") => OAuthDeviceCodePollResult::Pending,
                Some("slow_down") => OAuthDeviceCodePollResult::SlowDown {
                    interval_seconds: body.get("interval").and_then(Value::as_f64),
                },
                Some("access_denied" | "authorization_denied") => OAuthDeviceCodePollResult::Failed {
                    message: "xAI device authorization was denied".to_owned().into(),
                },
                Some("expired_token") => OAuthDeviceCodePollResult::Failed {
                    message: "xAI device code expired".to_owned().into(),
                },
                _ => OAuthDeviceCodePollResult::Failed {
                    message: request_failure("device token polling", &response).message,
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

async fn login_xai(
    runtime: &OAuthRuntime,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let device = request_device_code(runtime, interaction.signal())
        .await
        .map_err(AuthError::from)?;
    interaction.notify(AuthEvent::DeviceCode {
        user_code: (device.user_code.clone()).into(),
        verification_uri: (device
            .verification_uri_complete
            .clone()
            .unwrap_or_else(|| device.verification_uri.clone()))
        .into(),
        interval_seconds: device.interval_seconds,
        expires_in_seconds: Some(device.expires_in_seconds),
    });
    poll_for_tokens(runtime, &device, interaction.signal())
        .await
        .map_err(AuthError::from)
}

async fn refresh_xai_token(
    runtime: &OAuthRuntime,
    refresh_token: &pi_js_compat::js_json::JsString,
    signal: CancellationToken,
) -> Result<OAuthCredential, CallbackError> {
    let response = post_form(
        runtime,
        XAI_TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", XAI_CLIENT_ID),
            (
                "refresh_token",
                &crate::auth::oauth::support::to_form_usv_string(refresh_token),
            ),
        ],
        signal,
    )
    .await?;
    if !response.ok() {
        return Err(request_failure("token refresh", &response));
    }
    let body = response.json.unwrap_or(Value::Null);
    credentials_from_token_response(&body, Some(refresh_token), runtime.now())
}
