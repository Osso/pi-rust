//! Port of `packages/ai/src/auth/oauth/openai-codex.ts`.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::auth::oauth::device_code::{
    OAuthDeviceCodeError, OAuthDeviceCodePollOptions, OAuthDeviceCodePollResult, poll_oauth_device_code_flow,
};
use crate::auth::oauth::oauth_page::{oauth_error_html, oauth_success_html};
use crate::auth::oauth::pkce::generate_pkce_with;
use crate::auth::oauth::support::{
    AuthorizationInput, CallbackReply, CallbackRequest, CallbackServer, OAuthHttpResponse, OAuthRuntime, form_request,
    json_request, parse_authorization_input, random_hex, send, start_callback_server,
};
use crate::auth::{
    AuthError, AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, AuthSelectOption, ModelAuth, OAuthAuth,
    OAuthCredential,
};
use crate::request::CallbackError;
use crate::utils::provider_env::get_provider_env_value;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const DEVICE_USER_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const DEVICE_VERIFICATION_URI: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
const DEVICE_CODE_TIMEOUT_SECONDS: f64 = 900.0;
const BROWSER_LOGIN_METHOD: &str = "browser";
const DEVICE_CODE_LOGIN_METHOD: &str = "device_code";
const SCOPE: &str = "openid profile email offline_access";
const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";

pub struct OpenAICodexOAuth {
    runtime: OAuthRuntime,
    callback_host: String,
}

impl OpenAICodexOAuth {
    pub fn new() -> Self {
        Self::with_runtime(OAuthRuntime::system())
    }

    pub fn with_runtime(runtime: OAuthRuntime) -> Self {
        let callback_host =
            get_provider_env_value("PI_OAUTH_CALLBACK_HOST", None).unwrap_or_else(|| "127.0.0.1".to_owned());
        Self { runtime, callback_host }
    }

    pub fn with_callback_host(mut self, host: impl Into<String>) -> Self {
        self.callback_host = host.into();
        self
    }
}

impl Default for OpenAICodexOAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuthAuth for OpenAICodexOAuth {
    fn name(&self) -> &str {
        "OpenAI (ChatGPT Plus/Pro)"
    }

    fn is_subscription(&self) -> bool {
        true
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        let callback_host = self.callback_host.clone();
        Box::pin(async move { login_openai_codex(&runtime, &callback_host, interaction).await })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        Box::pin(async move {
            refresh_openai_codex(
                &runtime,
                &crate::auth::oauth::support::to_form_usv_string(&credential.refresh),
                signal,
            )
            .await
            .map_err(|error| AuthError::failed(error.message))
        })
    }

    fn to_auth(&self, credential: OAuthCredential) -> crate::request::BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async move {
            Ok(ModelAuth {
                api_key: Some(credential.access_utf8()?),
                ..ModelAuth::default()
            })
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct OAuthToken {
    access: pi_js_compat::js_json::JsString,
    refresh: pi_js_compat::js_json::JsString,
    expires: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct DeviceAuthInfo {
    device_auth_id: String,
    user_code: String,
    interval_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceTokenSuccess {
    authorization_code: String,
    code_verifier: String,
}

fn create_state(runtime: &OAuthRuntime) -> String {
    random_hex(runtime.random.as_ref(), 16)
}

fn fetch_with_login_cancellation(
    runtime: &OAuthRuntime,
    request: crate::request::HttpRequest,
    signal: CancellationToken,
) -> crate::request::BoxFuture<'_, Result<OAuthHttpResponse, CallbackError>> {
    let fetch = Arc::clone(&runtime.fetch);
    Box::pin(async move {
        match send(&fetch, request, Some(signal.clone())).await {
            Ok(response) => Ok(response),
            Err(_error) if signal.is_cancelled() => Err(CallbackError::new("Login cancelled")),
            Err(error) => Err(error),
        }
    })
}

fn read_token_response(response: OAuthHttpResponse, operation: &str) -> Result<OAuthToken, CallbackError> {
    if !response.ok() {
        let text = if response.body.is_empty() {
            response.status_text.as_str()
        } else {
            response.body.as_str()
        };
        return Err(CallbackError::new(format!(
            "OpenAI Codex token {operation} failed ({}): {text}",
            response.status
        )));
    }
    use pi_js_compat::js_json::{self, JsValue};
    let json = js_json::parse_lossless(&response.body).map_err(|error| CallbackError::new(error.to_string()))?;
    let access = json.get("access_token").and_then(JsValue::as_string);
    let refresh = json.get("refresh_token").and_then(JsValue::as_string);
    let expires_in = json.get("expires_in").and_then(JsValue::as_f64);
    let (Some(access), Some(refresh), Some(expires_in)) = (access, refresh, expires_in) else {
        return Err(CallbackError::new(format!(
            "OpenAI Codex token {operation} response missing fields: {}",
            js_json::stringify_lossless(&json)
        )));
    };
    Ok(OAuthToken {
        access: access.to_owned(),
        refresh: refresh.to_owned(),
        expires: expires_in * 1000.0,
    })
}

async fn exchange_authorization_code(
    runtime: &OAuthRuntime,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    signal: CancellationToken,
    now: f64,
) -> Result<OAuthCredential, CallbackError> {
    let body = crate::auth::oauth::support::form_encode(&[
        ("grant_type", "authorization_code"),
        ("client_id", CLIENT_ID),
        ("code", code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect_uri),
    ]);
    let request = form_request(TOKEN_URL, body, false);
    let response = fetch_with_login_cancellation(runtime, request, signal).await?;
    let token = read_token_response(response, "exchange")?;
    credentials_from_token(token, now)
}

async fn refresh_access_token(
    runtime: &OAuthRuntime,
    refresh_token: &str,
    signal: CancellationToken,
    now: f64,
) -> Result<OAuthToken, CallbackError> {
    let body = crate::auth::oauth::support::form_encode(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ]);
    let request = form_request(TOKEN_URL, body, false);
    let response = send(&runtime.fetch, request, Some(signal))
        .await
        .map_err(|error| CallbackError::new(format!("OpenAI Codex token refresh error: {}", error.message)))?;
    let mut token = read_token_response(response, "refresh")?;
    token.expires += now;
    Ok(token)
}

fn credentials_from_token(token: OAuthToken, now: f64) -> Result<OAuthCredential, CallbackError> {
    let account_id = token
        .access
        .to_utf8()
        .ok()
        .as_deref()
        .and_then(get_account_id)
        .ok_or_else(|| CallbackError::new("Failed to extract accountId from token"))?;
    let mut credential = OAuthCredential::new(token.refresh, token.access, now + token.expires);
    credential
        .extra
        .insert("accountId".into(), pi_js_compat::js_json::JsValue::String(account_id));
    Ok(credential)
}

fn get_account_id(access_token: &str) -> Option<pi_js_compat::js_json::JsString> {
    let mut parts = access_token.split('.');
    parts.next()?;
    let encoded = parts.next()?;
    parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let bytes = pi_js_compat::base64::atob(encoded).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let payload = pi_js_compat::js_json::parse_lossless(&text).ok()?;
    let account_id = payload.get(JWT_CLAIM_PATH)?.get("chatgpt_account_id")?.as_string()?;
    (!account_id.units().is_empty()).then(|| account_id.clone())
}

async fn start_device_auth(runtime: &OAuthRuntime, signal: CancellationToken) -> Result<DeviceAuthInfo, CallbackError> {
    let request = json_request(DEVICE_USER_CODE_URL, json!({"client_id": CLIENT_ID}), false);
    let response = fetch_with_login_cancellation(runtime, request, signal).await?;
    if !response.ok() {
        if response.status == 404 {
            return Err(CallbackError::new(
                "OpenAI Codex device code login is not enabled for this server. Use browser login or verify the server URL.",
            ));
        }
        let suffix = if response.body.is_empty() {
            String::new()
        } else {
            format!(": {}", response.body)
        };
        return Err(CallbackError::new(format!(
            "OpenAI Codex device code request failed with status {}{suffix}",
            response.status
        )));
    }
    let json = response.json.unwrap_or(Value::Null);
    let interval_seconds = match json.get("interval") {
        Some(Value::String(text)) => Some(pi_js_compat::number::parse_number(text)),
        Some(Value::Number(number)) => number.as_f64(),
        _ => None,
    };
    let device_auth_id = json.get("device_auth_id").and_then(Value::as_str);
    let user_code = json.get("user_code").and_then(Value::as_str);
    let Some((device_auth_id, user_code, interval_seconds)) = device_auth_id
        .zip(user_code)
        .zip(interval_seconds.filter(|seconds| seconds.is_finite() && *seconds >= 0.0))
        .map(|((id, code), interval)| (id, code, interval))
    else {
        return Err(CallbackError::new(format!(
            "Invalid OpenAI Codex device code response: {}",
            crate::utils::error_body::safe_json_stringify(&json)
        )));
    };
    Ok(DeviceAuthInfo {
        device_auth_id: device_auth_id.to_owned(),
        user_code: user_code.to_owned(),
        interval_seconds,
    })
}

async fn poll_device_auth(
    runtime: &OAuthRuntime,
    device: &DeviceAuthInfo,
    signal: CancellationToken,
) -> Result<DeviceTokenSuccess, CallbackError> {
    let options = OAuthDeviceCodePollOptions {
        interval_seconds: Some(device.interval_seconds),
        expires_in_seconds: Some(DEVICE_CODE_TIMEOUT_SECONDS),
        wait_before_first_poll: false,
        signal: signal.clone(),
    };
    let device = device.clone();
    let runtime = runtime.clone();
    poll_oauth_device_code_flow(options, move || {
        let device = device.clone();
        let runtime = runtime.clone();
        let signal = signal.clone();
        async move {
            let body = json!({
                "device_auth_id": device.device_auth_id,
                "user_code": device.user_code,
            });
            let request = json_request(DEVICE_TOKEN_URL, &body, false);
            let signal_for_error = signal.clone();
            let response = match fetch_with_login_cancellation(&runtime, request, signal).await {
                Ok(response) => response,
                Err(_error) if signal_for_error.is_cancelled() => {
                    return OAuthDeviceCodePollResult::Failed {
                        message: "Login cancelled".to_owned(),
                    };
                }
                Err(error) => {
                    return OAuthDeviceCodePollResult::Failed { message: error.message };
                }
            };
            if response.ok() {
                let json = response.json.unwrap_or(Value::Null);
                let authorization_code = json.get("authorization_code").and_then(Value::as_str);
                let code_verifier = json.get("code_verifier").and_then(Value::as_str);
                let (Some(authorization_code), Some(code_verifier)) = (authorization_code, code_verifier) else {
                    return OAuthDeviceCodePollResult::Failed {
                        message: format!(
                            "Invalid OpenAI Codex device auth token response: {}",
                            crate::utils::error_body::safe_json_stringify(&json)
                        ),
                    };
                };
                return OAuthDeviceCodePollResult::Complete(DeviceTokenSuccess {
                    authorization_code: authorization_code.to_owned(),
                    code_verifier: code_verifier.to_owned(),
                });
            }
            if response.status == 403 || response.status == 404 {
                return OAuthDeviceCodePollResult::Pending;
            }
            let error_code = response
                .json
                .as_ref()
                .and_then(|json| json.get("error"))
                .and_then(|error| match error {
                    Value::Object(object) => object.get("code").and_then(Value::as_str),
                    Value::String(code) => Some(code.as_str()),
                    _ => None,
                });
            match error_code {
                Some("deviceauth_authorization_pending") => OAuthDeviceCodePollResult::Pending,
                Some("slow_down") => OAuthDeviceCodePollResult::SlowDown { interval_seconds: None },
                _ => {
                    let suffix = if response.body.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", response.body)
                    };
                    OAuthDeviceCodePollResult::Failed {
                        message: format!(
                            "OpenAI Codex device auth failed with status {}{suffix}",
                            response.status
                        ),
                    }
                }
            }
        }
    })
    .await
    .map_err(|error| match error {
        OAuthDeviceCodeError::Cancelled => CallbackError::new("Login cancelled"),
        other => CallbackError::new(other.to_string()),
    })
}

async fn login_device_code(
    runtime: &OAuthRuntime,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let device = start_device_auth(runtime, interaction.signal())
        .await
        .map_err(|error| AuthError::failed(error.message))?;
    interaction.notify(AuthEvent::DeviceCode {
        user_code: device.user_code.clone(),
        verification_uri: DEVICE_VERIFICATION_URI.to_owned(),
        interval_seconds: Some(device.interval_seconds),
        expires_in_seconds: Some(DEVICE_CODE_TIMEOUT_SECONDS),
    });
    let code = poll_device_auth(runtime, &device, interaction.signal())
        .await
        .map_err(|error| AuthError::failed(error.message))?;
    exchange_authorization_code(
        runtime,
        &code.authorization_code,
        &code.code_verifier,
        DEVICE_REDIRECT_URI,
        interaction.signal(),
        runtime.now(),
    )
    .await
    .map_err(|error| AuthError::failed(error.message))
}

struct LocalServer {
    server: Option<CallbackServer>,
    receiver: oneshot::Receiver<Option<String>>,
    settle: Arc<Mutex<Option<oneshot::Sender<Option<String>>>>>,
}

fn settle_local(sender: &Arc<Mutex<Option<oneshot::Sender<Option<String>>>>>, value: Option<String>) {
    if let Some(sender) = sender.lock().expect("local sender mutex").take() {
        let _ = sender.send(value);
    }
}

async fn start_local_server(host: &str, state: String) -> LocalServer {
    let (sender, receiver) = oneshot::channel();
    let settle = Arc::new(Mutex::new(Some(sender)));
    let handler_settle = Arc::clone(&settle);
    let handler = Arc::new(move |request: CallbackRequest| {
        let state = state.clone();
        let settle = Arc::clone(&handler_settle);
        Box::pin(async move {
            if request.path != "/auth/callback" {
                return CallbackReply::html(404, oauth_error_html("Callback route not found.", None));
            }
            if request.query_param("state").as_deref() != Some(state.as_str()) {
                return CallbackReply::html(400, oauth_error_html("State mismatch.", None));
            }
            let Some(code) = request.query_param("code") else {
                return CallbackReply::html(400, oauth_error_html("Missing authorization code.", None));
            };
            settle_local(&settle, Some(code));
            CallbackReply::html(
                200,
                oauth_success_html("OpenAI authentication completed. You can close this window."),
            )
        }) as crate::request::BoxFuture<'static, CallbackReply>
    });
    match start_callback_server(host, 1455, handler).await {
        Ok(server) => LocalServer {
            server: Some(server),
            receiver,
            settle,
        },
        Err(_) => {
            settle_local(&settle, None);
            LocalServer {
                server: None,
                receiver,
                settle,
            }
        }
    }
}

async fn login_browser(
    runtime: &OAuthRuntime,
    callback_host: &str,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let pkce = generate_pkce_with(|bytes| runtime.random.fill(bytes));
    let state = create_state(runtime);
    let url = crate::auth::oauth::support::url_append_query(
        AUTHORIZE_URL,
        &[
            ("response_type", "code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", REDIRECT_URI),
            ("scope", SCOPE),
            ("code_challenge", &pkce.challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("id_token_add_organizations", "true"),
            ("codex_cli_simplified_flow", "true"),
            ("originator", "pi"),
        ],
    );
    let server = start_local_server(callback_host, state.clone()).await;
    let manual_abort = CancellationToken::new();
    interaction.notify(AuthEvent::AuthUrl {
        url,
        instructions: Some("A browser window should open. Complete login to finish.".to_owned()),
    });

    let mut prompt = Box::pin(interaction.prompt(AuthPrompt {
        signal: Some(manual_abort.clone()),
        kind: AuthPromptKind::ManualCode {
            message: "Complete login in your browser, or paste the authorization code / redirect URL here:".to_owned(),
            placeholder: Some(REDIRECT_URI.to_owned()),
        },
    }));
    let LocalServer {
        server: callback_server,
        receiver,
        settle,
    } = server;
    let mut receiver = Box::pin(receiver);
    let interaction_signal = interaction.signal();
    let mut manual: Option<Result<String, CallbackError>> = None;
    let result = loop {
        tokio::select! {
            biased;
            response = &mut receiver => break response.ok().flatten(),
            response = &mut prompt, if manual.is_none() => {
                manual = Some(response);
                settle_local(&settle, None);
            }
            () = interaction_signal.cancelled() => settle_local(&settle, None),
        }
    };

    if result.is_none() && manual.is_none() {
        manual = Some(prompt.await);
    }
    if let Some(Err(error)) = &manual {
        close_server(callback_server);
        manual_abort.cancel();
        return Err(AuthError::failed(error.message.clone()));
    }

    let code = match result {
        Some(code) => Some(code),
        None => match manual.and_then(Result::ok).filter(|input| !input.is_empty()) {
            Some(input) => match resolve_manual_code(&input, &state) {
                Ok(code) => code,
                Err(message) => {
                    close_server(callback_server);
                    manual_abort.cancel();
                    return Err(AuthError::failed(message));
                }
            },
            None => None,
        },
    };
    let Some(code) = code else {
        close_server(callback_server);
        manual_abort.cancel();
        return Err(AuthError::failed("Missing authorization code"));
    };

    let credential = exchange_authorization_code(
        runtime,
        &code,
        &pkce.verifier,
        REDIRECT_URI,
        interaction.signal(),
        runtime.now(),
    )
    .await;
    close_server(callback_server);
    manual_abort.cancel();
    credential.map_err(|error| AuthError::failed(error.message))
}

fn close_server(server: Option<CallbackServer>) {
    if let Some(server) = server {
        server.close();
    }
}

fn resolve_manual_code(input: &str, expected_state: &str) -> Result<Option<String>, &'static str> {
    let AuthorizationInput { code, state } = parse_authorization_input(input);
    if state.as_deref().is_some_and(|state| state != expected_state) {
        return Err("State mismatch");
    }
    Ok(code)
}

async fn login_openai_codex(
    runtime: &OAuthRuntime,
    callback_host: &str,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let method = interaction
        .prompt(AuthPrompt {
            signal: None,
            kind: AuthPromptKind::Select {
                message: "Select OpenAI Codex login method:".to_owned(),
                options: vec![
                    AuthSelectOption {
                        id: BROWSER_LOGIN_METHOD.to_owned(),
                        label: "Browser login (default)".to_owned(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: DEVICE_CODE_LOGIN_METHOD.to_owned(),
                        label: "Device code login (headless)".to_owned(),
                        description: None,
                    },
                ],
            },
        })
        .await
        .map_err(|error| AuthError::failed(error.message))?;

    match method.as_str() {
        DEVICE_CODE_LOGIN_METHOD => login_device_code(runtime, interaction).await,
        BROWSER_LOGIN_METHOD => login_browser(runtime, callback_host, interaction).await,
        other => Err(AuthError::failed(format!("Unknown OpenAI Codex login method: {other}"))),
    }
}

async fn refresh_openai_codex(
    runtime: &OAuthRuntime,
    refresh_token: &str,
    signal: CancellationToken,
) -> Result<OAuthCredential, CallbackError> {
    let token = refresh_access_token(runtime, refresh_token, signal, runtime.now()).await?;
    credentials_from_token(token, 0.0)
}
