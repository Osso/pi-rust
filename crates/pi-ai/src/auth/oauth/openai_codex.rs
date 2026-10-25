//! Port of `packages/ai/src/auth/oauth/openai-codex.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::sync::{Arc, Mutex};

use pi_js_compat::abort::{AbortController, AbortSignal};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::auth::oauth::device_code::{
    OAuthDeviceCodePollOptions, OAuthDeviceCodePollResult, poll_oauth_device_code_flow,
};
use crate::auth::oauth::oauth_page::{oauth_error_html, oauth_success_html};
use crate::auth::oauth::pkce::generate_pkce_with;
use crate::auth::oauth::support::{
    AuthorizationInput, CallbackReply, CallbackRequest, CallbackServer, OAuthRuntime, form_request, json_request,
    parse_authorization_input, random_hex, start_callback_server,
};
use crate::auth::{
    AuthError, AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, AuthSelectOption, ModelAuth, OAuthAuth,
    OAuthCredential,
};
use crate::request::{CallbackError, HttpResponse, read_body_text};
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
pub const OPENAI_CODEX_BROWSER_LOGIN_METHOD: &str = "browser";
pub const OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD: &str = "device_code";
use self::{
    OPENAI_CODEX_BROWSER_LOGIN_METHOD as BROWSER_LOGIN_METHOD,
    OPENAI_CODEX_DEVICE_CODE_LOGIN_METHOD as DEVICE_CODE_LOGIN_METHOD,
};
const SCOPE: &str = "openid profile email offline_access";
const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";

pub struct OpenAICodexOAuth {
    runtime: OAuthRuntime,
    callback_host: JsString,
}

impl OpenAICodexOAuth {
    pub fn new() -> Self {
        Self::with_runtime(OAuthRuntime::system())
    }

    pub fn with_runtime(runtime: OAuthRuntime) -> Self {
        let callback_host =
            get_provider_env_value("PI_OAUTH_CALLBACK_HOST", None).unwrap_or_else(|| "127.0.0.1".into());
        Self { runtime, callback_host }
    }

    pub fn with_callback_host(mut self, host: impl Into<JsString>) -> Self {
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
    fn name(&self) -> pi_js_compat::js_json::JsString {
        "OpenAI (ChatGPT Plus/Pro)".into()
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
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        Box::pin(async move {
            refresh_openai_codex(&runtime, &credential.refresh)
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

#[derive(Debug, Clone, PartialEq)]
struct OAuthToken {
    access: pi_js_compat::js_json::JsString,
    refresh: pi_js_compat::js_json::JsString,
    expires: f64,
    id_token: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq)]
struct DeviceAuthInfo {
    device_auth_id: JsString,
    user_code: JsString,
    interval_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceTokenSuccess {
    authorization_code: JsString,
    code_verifier: JsString,
}

fn create_state(runtime: &OAuthRuntime) -> Result<String, CallbackError> {
    random_hex(runtime.random.as_ref(), 16)
}

fn fetch_with_login_cancellation(
    runtime: &OAuthRuntime,
    request: crate::request::HttpRequest,
    signal: Option<AbortSignal>,
) -> crate::request::BoxFuture<'_, Result<HttpResponse, CallbackError>> {
    let fetch = Arc::clone(&runtime.fetch);
    Box::pin(async move {
        match fetch.fetch(request, signal.clone()).await {
            Ok(response) => Ok(response),
            Err(_error) if signal.is_some_and(|signal| signal.aborted()) => Err(CallbackError::new("Login cancelled")),
            Err(error) => Err(error),
        }
    })
}

async fn read_token_response(response: HttpResponse, operation: &str) -> Result<OAuthToken, CallbackError> {
    let body = read_body_text(response.body).await;
    if !(200..300).contains(&response.status) {
        let body = body.unwrap_or_default();
        let text = if body.is_empty() { &response.status_text } else { &body };
        return Err(CallbackError::new(format!(
            "OpenAI Codex token {operation} failed ({}): {text}",
            response.status
        )));
    }
    use pi_js_compat::js_json::{self, JsValue};
    let json = crate::auth::oauth::support::parse_response_json(&body?)?;
    let access = json
        .get("access_token")
        .and_then(JsValue::as_string)
        .filter(|token| !token.is_empty());
    let refresh = json
        .get("refresh_token")
        .and_then(JsValue::as_string)
        .filter(|token| !token.is_empty());
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
        id_token: json.get("id_token").and_then(JsValue::as_string).cloned(),
    })
}

async fn exchange_authorization_code(
    runtime: &OAuthRuntime,
    code: &JsString,
    verifier: &JsString,
    redirect_uri: &str,
    signal: Option<AbortSignal>,
) -> Result<OAuthCredential, CallbackError> {
    let body = crate::auth::oauth::support::form_encode(&[
        ("grant_type", "authorization_code"),
        ("client_id", CLIENT_ID),
        ("code", &crate::auth::oauth::support::to_form_usv_string(code)),
        (
            "code_verifier",
            &crate::auth::oauth::support::to_form_usv_string(verifier),
        ),
        ("redirect_uri", redirect_uri),
    ]);
    let request = form_request(TOKEN_URL, body, false);
    let response = fetch_with_login_cancellation(runtime, request, signal).await?;
    let token = read_token_response(response, "exchange").await?;
    credentials_from_token(token, runtime.now())
}

async fn refresh_access_token(
    runtime: &OAuthRuntime,
    refresh_token: &pi_js_compat::js_json::JsString,
) -> Result<OAuthToken, CallbackError> {
    let body = crate::auth::oauth::support::form_encode(&[
        ("grant_type", "refresh_token"),
        (
            "refresh_token",
            &crate::auth::oauth::support::to_form_usv_string(refresh_token),
        ),
        ("client_id", CLIENT_ID),
    ]);
    let request = form_request(TOKEN_URL, body, false);
    let response = match runtime.fetch.fetch(request, None).await {
        Ok(response) => response,
        Err(error) => {
            return Err(CallbackError::new(js_format!(
                "OpenAI Codex token refresh error: {}",
                error.error_or_string_message()?
            )));
        }
    };
    let mut token = read_token_response(response, "refresh").await?;
    token.expires += runtime.now();
    Ok(token)
}

fn credentials_from_token(token: OAuthToken, now: f64) -> Result<OAuthCredential, CallbackError> {
    let account_id =
        get_account_id(&token.access).ok_or_else(|| CallbackError::new("Failed to extract accountId from token"))?;
    let email = get_email(token.id_token.as_ref());
    let mut credential = OAuthCredential::new(token.refresh, token.access, now + token.expires);
    credential
        .extra
        .insert("accountId".into(), pi_js_compat::js_json::JsValue::String(account_id));
    if let Some(email) = email {
        credential
            .extra
            .insert("email".into(), pi_js_compat::js_json::JsValue::String(email));
    }
    credential.key_order = ["access", "refresh", "expires"]
        .map(JsString::from)
        .into_iter()
        .chain(credential.extra.keys().cloned())
        .chain(std::iter::once("type".into()))
        .collect();
    Ok(credential)
}

fn decode_jwt(token: &JsString) -> Option<pi_js_compat::js_json::JsValue> {
    let mut parts = token.split('.');
    parts.next()?;
    let encoded = parts.next()?;
    parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    // Frozen openai-codex.ts:112-121 catches atob/JSON.parse errors and returns null.
    let bytes = match pi_js_compat::base64::atob(&encoded) {
        Ok(bytes) => bytes,
        Err(_) => return None,
    };
    let text = JsString::from_utf16(bytes.into_iter().map(u16::from).collect());
    pi_js_compat::js_json::parse_lossless(text).ok()
}

fn get_account_id(access_token: &JsString) -> Option<JsString> {
    let payload = decode_jwt(access_token)?;
    let account_id = payload.get(JWT_CLAIM_PATH)?.get("chatgpt_account_id")?.as_string()?;
    (!account_id.is_empty()).then(|| account_id.clone())
}

fn get_email(id_token: Option<&JsString>) -> Option<JsString> {
    let payload = decode_jwt(id_token.filter(|token| !token.is_empty())?)?;
    if let Some(email) = payload.get("email").and_then(pi_js_compat::js_json::JsValue::as_string)
        && !email.is_empty()
    {
        return Some(email.clone());
    }
    let email = payload.get("profile")?.get("email")?.as_string()?;
    (!email.is_empty()).then(|| email.clone())
}

async fn start_device_auth(runtime: &OAuthRuntime, signal: AbortSignal) -> Result<DeviceAuthInfo, CallbackError> {
    let request = json_request(DEVICE_USER_CODE_URL, json!({"client_id": CLIENT_ID}), false);
    let response = fetch_with_login_cancellation(runtime, request, Some(signal)).await?;
    if !(200..300).contains(&response.status) {
        if response.status == 404 {
            return Err(CallbackError::new(
                "OpenAI Codex device code login is not enabled for this server. Use browser login or verify the server URL.",
            ));
        }
        let body = read_body_text(response.body).await.unwrap_or_default();
        let suffix = if body.is_empty() {
            String::new()
        } else {
            format!(": {body}")
        };
        return Err(CallbackError::new(format!(
            "OpenAI Codex device code request failed with status {}{suffix}",
            response.status
        )));
    }
    use pi_js_compat::js_json::{self, JsValue};
    let json = crate::auth::oauth::support::parse_response_json(&read_body_text(response.body).await?)?;
    let interval_seconds = match json.get("interval") {
        Some(value @ (JsValue::String(_) | JsValue::Number(_))) => js_json::to_js_number(value).ok(),
        _ => None,
    };
    let device_auth_id = json
        .get("device_auth_id")
        .and_then(JsValue::as_string)
        .filter(|id| !id.is_empty());
    let user_code = json
        .get("user_code")
        .and_then(JsValue::as_string)
        .filter(|code| !code.is_empty());
    let Some((device_auth_id, user_code, interval_seconds)) = device_auth_id
        .zip(user_code)
        .zip(interval_seconds.filter(|seconds| seconds.is_finite() && *seconds >= 0.0))
        .map(|((id, code), interval)| (id, code, interval))
    else {
        return Err(CallbackError::new(format!(
            "Invalid OpenAI Codex device code response: {}",
            pi_js_compat::js_json::stringify_lossless(&json)
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
    signal: AbortSignal,
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
            let body = pi_js_compat::js_json!({
                "device_auth_id": device.device_auth_id,
                "user_code": device.user_code,
            });
            let request = json_request(DEVICE_TOKEN_URL, &body, false);
            let response = fetch_with_login_cancellation(&runtime, request, Some(signal)).await?;
            let status = response.status;
            if (200..300).contains(&status) {
                let json = crate::auth::oauth::support::parse_response_json(&read_body_text(response.body).await?)?;
                let authorization_code = json
                    .get("authorization_code")
                    .and_then(pi_js_compat::js_json::JsValue::as_string)
                    .filter(|code| !code.is_empty());
                let code_verifier = json
                    .get("code_verifier")
                    .and_then(pi_js_compat::js_json::JsValue::as_string)
                    .filter(|verifier| !verifier.is_empty());
                let (Some(authorization_code), Some(code_verifier)) = (authorization_code, code_verifier) else {
                    return Ok(OAuthDeviceCodePollResult::Failed {
                        message: format!(
                            "Invalid OpenAI Codex device auth token response: {}",
                            pi_js_compat::js_json::stringify_lossless(&json)
                        )
                        .into(),
                    });
                };
                return Ok(OAuthDeviceCodePollResult::Complete(DeviceTokenSuccess {
                    authorization_code: authorization_code.to_owned(),
                    code_verifier: code_verifier.to_owned(),
                }));
            }
            if status == 403 || status == 404 {
                return Ok(OAuthDeviceCodePollResult::Pending);
            }
            let body = read_body_text(response.body).await.unwrap_or_default();
            let json = serde_json::from_str::<Value>(&body).ok();
            let error_code = json
                .as_ref()
                .and_then(|json| json.get("error"))
                .and_then(|error| match error {
                    Value::Object(object) => object.get("code").and_then(Value::as_str),
                    Value::String(code) => Some(code.as_str()),
                    _ => None,
                });
            Ok(match error_code {
                Some("deviceauth_authorization_pending") => OAuthDeviceCodePollResult::Pending,
                Some("slow_down") => OAuthDeviceCodePollResult::SlowDown { interval_seconds: None },
                _ => {
                    let suffix = if body.is_empty() {
                        String::new()
                    } else {
                        format!(": {body}")
                    };
                    OAuthDeviceCodePollResult::Failed {
                        message: format!("OpenAI Codex device auth failed with status {}{suffix}", status).into(),
                    }
                }
            })
        }
    })
    .await
}

pub(crate) async fn login_device_code(
    runtime: &OAuthRuntime,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let device = start_device_auth(runtime, interaction.signal())
        .await
        .map_err(AuthError::from)?;
    interaction.notify(AuthEvent::DeviceCode {
        user_code: device.user_code.clone(),
        verification_uri: (DEVICE_VERIFICATION_URI.to_owned()).into(),
        interval_seconds: Some(device.interval_seconds),
        expires_in_seconds: Some(DEVICE_CODE_TIMEOUT_SECONDS),
    })?;
    let code = poll_device_auth(runtime, &device, interaction.signal())
        .await
        .map_err(AuthError::from)?;
    exchange_authorization_code(
        runtime,
        &code.authorization_code,
        &code.code_verifier,
        DEVICE_REDIRECT_URI,
        Some(interaction.signal()),
    )
    .await
    .map_err(AuthError::from)
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

async fn start_local_server(host: &JsString, state: String) -> LocalServer {
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
            let Some(code) = request.query_param("code").filter(|code| !code.is_empty()) else {
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

pub(crate) async fn login_browser(
    runtime: &OAuthRuntime,
    callback_host: &JsString,
    interaction: Arc<dyn AuthInteraction>,
    originator: &JsString,
) -> Result<OAuthCredential, AuthError> {
    let pkce = generate_pkce_with(|bytes| runtime.random.fill(bytes))?;
    let state = create_state(runtime)?;
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
            (
                "originator",
                &crate::auth::oauth::support::to_form_usv_string(originator),
            ),
        ],
    );
    let server = start_local_server(callback_host, state.clone()).await;
    let manual_abort = AbortController::new();
    if let Err(error) = interaction.notify(AuthEvent::AuthUrl {
        url: url.into(),
        instructions: Some(("A browser window should open. Complete login to finish.".to_owned()).into()),
    }) {
        // DISCREPANCY: frozen onAuth runs before the utility's server-close finally.
        // Its listener leaks on this throw; the outer auth wrapper still aborts input.
        manual_abort.abort(None);
        return Err(AuthError::from(error));
    }

    let manual_prompt = interaction.manual_code_input(AuthPrompt {
        signal: Some(manual_abort.signal()),
        kind: AuthPromptKind::ManualCode {
            message: ("Complete login in your browser, or paste the authorization code / redirect URL here:"
                .to_owned())
            .into(),
            placeholder: Some((REDIRECT_URI.to_owned()).into()),
        },
    });
    let has_manual_input = manual_prompt.is_some();
    let mut prompt = Box::pin(async move {
        match manual_prompt {
            Some(prompt) => prompt.await,
            None => std::future::pending().await,
        }
    });
    let LocalServer {
        server: callback_server,
        receiver,
        settle,
    } = server;
    let mut receiver = Box::pin(receiver);
    let mut manual: Option<Result<JsString, CallbackError>> = None;
    let result = loop {
        tokio::select! {
            biased;
            response = &mut receiver => break response.ok().flatten(),
            response = &mut prompt, if manual.is_none() => {
                manual = Some(response.map_err(crate::auth::oauth::support::normalize_manual_rejection));
                settle_local(&settle, None);
            }

        }
    };

    if result.is_none() && manual.is_none() && has_manual_input {
        manual = Some(
            prompt
                .await
                .map_err(crate::auth::oauth::support::normalize_manual_rejection),
        );
    }
    if let Some(Err(error)) = &manual {
        close_server(callback_server);
        manual_abort.abort(None);
        return Err(AuthError::from(error.clone()));
    }

    let mut code = match result {
        Some(code) => Some(JsString::from(code)),
        None => match manual.and_then(Result::ok).filter(|input| !input.is_empty()) {
            Some(input) => match resolve_manual_code(&input, &state) {
                Ok(code) => code,
                Err(message) => {
                    close_server(callback_server);
                    manual_abort.abort(None);
                    return Err(AuthError::failed(message));
                }
            },
            None => None,
        },
    };
    if code.as_ref().is_none_or(JsString::is_empty) {
        let input = interaction
            .prompt(AuthPrompt {
                signal: None,
                kind: AuthPromptKind::Text {
                    message: "Paste the authorization code (or full redirect URL):".into(),
                    placeholder: None,
                },
            })
            .await;
        code = match input {
            Ok(input) => match resolve_manual_code(&input, &state) {
                Ok(code) => code,
                Err(message) => {
                    close_server(callback_server);
                    manual_abort.abort(None);
                    return Err(AuthError::failed(message));
                }
            },
            Err(error) => {
                close_server(callback_server);
                manual_abort.abort(None);
                return Err(error.into());
            }
        };
    }
    let Some(code) = code.filter(|code| !code.is_empty()) else {
        close_server(callback_server);
        manual_abort.abort(None);
        return Err(AuthError::failed("Missing authorization code"));
    };

    // Frozen returns the exchange promise without awaiting it inside the server-close finally.
    close_server(callback_server);
    let credential =
        exchange_authorization_code(runtime, &code, &pkce.verifier.clone().into(), REDIRECT_URI, None).await;
    manual_abort.abort(None);
    credential.map_err(AuthError::from)
}

fn close_server(server: Option<CallbackServer>) {
    if let Some(server) = server {
        server.close();
    }
}

fn resolve_manual_code(input: &JsString, expected_state: &str) -> Result<Option<JsString>, &'static str> {
    let AuthorizationInput { code, state } = parse_authorization_input(input);
    if state
        .as_ref()
        .is_some_and(|state| !state.is_empty() && state != expected_state)
    {
        return Err("State mismatch");
    }
    Ok(code)
}

async fn login_openai_codex(
    runtime: &OAuthRuntime,
    callback_host: &JsString,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let method = interaction
        .prompt(AuthPrompt {
            signal: None,
            kind: AuthPromptKind::Select {
                message: ("Select OpenAI Codex login method:".to_owned()).into(),
                options: vec![
                    AuthSelectOption {
                        id: (BROWSER_LOGIN_METHOD.to_owned()).into(),
                        label: ("Browser login (default)".to_owned()).into(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: (DEVICE_CODE_LOGIN_METHOD.to_owned()).into(),
                        label: ("Device code login (headless)".to_owned()).into(),
                        description: None,
                    },
                ],
            },
        })
        .await
        .map_err(AuthError::from)?;

    if method == DEVICE_CODE_LOGIN_METHOD {
        login_device_code(runtime, interaction).await
    } else if method == BROWSER_LOGIN_METHOD {
        login_browser(runtime, callback_host, interaction, &"pi".into()).await
    } else {
        Err(AuthError::failed(js_format!(
            "Unknown OpenAI Codex login method: {}",
            method
        )))
    }
}

async fn refresh_openai_codex(
    runtime: &OAuthRuntime,
    refresh_token: &pi_js_compat::js_json::JsString,
) -> Result<OAuthCredential, CallbackError> {
    let token = refresh_access_token(runtime, refresh_token).await?;
    credentials_from_token(token, 0.0)
}
