//! Port of `packages/ai/src/auth/oauth/radius.ts`.

use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::auth::oauth::device_code::{
    OAuthDeviceCodeError, OAuthDeviceCodePollOptions, OAuthDeviceCodePollResult, poll_oauth_device_code_flow,
};
use crate::auth::oauth::oauth_page::{oauth_error_html, oauth_success_html};
use crate::auth::oauth::pkce::generate_pkce_with;
use crate::auth::oauth::support::{
    CallbackReply, CallbackRequest, CallbackServer, OAuthHttpResponse, OAuthRuntime, form_encode, form_request, send,
    start_callback_server,
};
use crate::auth::{
    AuthError, AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, AuthSelectOption, ModelAuth, OAuthAuth,
    OAuthCredential,
};
use crate::providers::radius_config::normalize_radius_gateway_url;
use crate::request::CallbackError;

const CALLBACK_HOST: &str = "127.0.0.1";
const CALLBACK_PORT: u16 = 1456;
const CALLBACK_PATH: &str = "/oauth/callback";
const REDIRECT_URI: &str = "http://127.0.0.1:1456/oauth/callback";
const TOKEN_EXPIRY_SKEW_MS: f64 = 60_000.0;
const LOGIN_METHOD_BROWSER: &str = "browser";
const LOGIN_METHOD_DEVICE_CODE: &str = "device-code";
const OAUTH_CLIENT_ID: &str = "pi-gateway";
const OAUTH_SCOPE: &str = "gateway offline_access";
const OAUTH_DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

pub struct RadiusOAuthOptions {
    pub name: String,
    pub gateway: String,
}

pub struct RadiusOAuth {
    name: String,
    gateway: String,
    runtime: OAuthRuntime,
}

impl RadiusOAuth {
    pub fn with_runtime(mut self, runtime: OAuthRuntime) -> Self {
        self.runtime = runtime;
        self
    }
}

pub fn create_radius_oauth(options: RadiusOAuthOptions) -> RadiusOAuth {
    RadiusOAuth {
        name: options.name,
        gateway: normalize_radius_gateway_url(&options.gateway),
        runtime: OAuthRuntime::system(),
    }
}

impl OAuthAuth for RadiusOAuth {
    fn name(&self) -> &str {
        &self.name
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        let name = self.name.clone();
        let gateway = self.gateway.clone();
        Box::pin(async move { login_radius(&runtime, &name, &gateway, interaction).await })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        let gateway = self.gateway.clone();
        Box::pin(async move {
            request_oauth_token(
                &runtime,
                &gateway,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", OAUTH_CLIENT_ID),
                    (
                        "refresh_token",
                        &crate::auth::oauth::support::to_form_usv_string(&credential.refresh),
                    ),
                ],
                signal,
            )
            .await
            .map_err(|error| AuthError::failed(error.into_callback_error().message))
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

struct OAuthResponseError {
    oauth_error: Option<String>,
    message: pi_js_compat::js_json::JsString,
}

impl OAuthResponseError {
    fn into_callback_error(self) -> CallbackError {
        CallbackError::new(self.message)
    }
}

enum RadiusRequestError {
    OAuth(OAuthResponseError),
    Other(CallbackError),
}

impl RadiusRequestError {
    fn message(&self) -> &pi_js_compat::js_json::JsString {
        match self {
            Self::OAuth(error) => &error.message,
            Self::Other(error) => &error.message,
        }
    }

    fn into_callback_error(self) -> CallbackError {
        match self {
            Self::OAuth(error) => error.into_callback_error(),
            Self::Other(error) => error,
        }
    }
}

fn read_oauth_response_error(response: &OAuthHttpResponse, message: &str) -> OAuthResponseError {
    let text = response.body.as_str();
    let (oauth_error, description) = match serde_json::from_str::<Value>(text) {
        Ok(data) => (
            data.get("error").and_then(Value::as_str).map(str::to_owned),
            data.get("error_description").and_then(Value::as_str).map(str::to_owned),
        ),
        Err(_) => (None, (!text.is_empty()).then(|| text.to_owned())),
    };
    let detail = match &oauth_error {
        Some(oauth_error) => match &description {
            Some(description) => format!("{oauth_error}: {description}"),
            None => oauth_error.clone(),
        },
        None => description.unwrap_or_else(|| response.status.to_string()),
    };
    OAuthResponseError {
        oauth_error,
        message: format!("{message}: {detail}").into(),
    }
}

async fn request_oauth_token(
    runtime: &OAuthRuntime,
    gateway: &str,
    pairs: &[(&str, &str)],
    signal: CancellationToken,
) -> Result<OAuthCredential, RadiusRequestError> {
    let request = form_request(&format!("{gateway}/v1/oauth/token"), form_encode(pairs), true);
    let response = send(&runtime.fetch, request, Some(signal.clone()))
        .await
        .map_err(|error| {
            if signal.is_cancelled() {
                RadiusRequestError::Other(CallbackError::new("Login cancelled"))
            } else {
                RadiusRequestError::Other(error)
            }
        })?;
    if !response.ok() {
        return Err(RadiusRequestError::OAuth(read_oauth_response_error(
            &response,
            "Radius OAuth token request failed",
        )));
    }
    let data = response.json.unwrap_or(Value::Null);
    let access = data.get("access_token").and_then(Value::as_str);
    let refresh = data.get("refresh_token").and_then(Value::as_str);
    let expires_in = data.get("expires_in").and_then(Value::as_f64);
    let (Some(access), Some(refresh), Some(expires_in)) = (access, refresh, expires_in) else {
        return Err(RadiusRequestError::Other(CallbackError::new(
            "Radius OAuth token response missing fields",
        )));
    };
    let mut credential = OAuthCredential::new(
        refresh,
        access,
        runtime.now() + expires_in * 1000.0 - TOKEN_EXPIRY_SKEW_MS,
    );
    if let Some(scope) = data.get("scope").and_then(Value::as_str) {
        credential
            .extra
            .insert("scope".into(), pi_js_compat::js_json::JsValue::String(scope.into()));
    }
    Ok(credential)
}

async fn load_discovery(
    runtime: &OAuthRuntime,
    gateway: &str,
    signal: CancellationToken,
) -> Result<String, CallbackError> {
    let request = crate::request::HttpRequest {
        method: "GET".to_owned(),
        url: format!("{gateway}/v1/oauth"),
        headers: vec![("accept".to_owned(), "application/json".to_owned())],
        body: None,
    };
    let response = send(&runtime.fetch, request, Some(signal)).await?;
    if !response.ok() {
        return Err(CallbackError::new(format!(
            "Could not load Radius OAuth config from {gateway}: {} {}",
            response.status, response.body
        )));
    }
    let discovery = response.json.unwrap_or(Value::Null);
    discovery
        .get("authorizationEndpoint")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CallbackError::new(format!("Invalid Radius OAuth config from {gateway}")))
}

struct CallbackCodeServer {
    server: Option<CallbackServer>,
    receiver: oneshot::Receiver<Option<String>>,
    settle: Arc<Mutex<Option<oneshot::Sender<Option<String>>>>>,
}

fn settle_code(sender: &Arc<Mutex<Option<oneshot::Sender<Option<String>>>>>, value: Option<String>) {
    if let Some(sender) = sender.lock().expect("code sender mutex").take() {
        let _ = sender.send(value);
    }
}

async fn start_callback(expected_state: String, _signal: CancellationToken) -> CallbackCodeServer {
    let (sender, receiver) = oneshot::channel();
    let settle = Arc::new(Mutex::new(Some(sender)));
    let handler_settle = Arc::clone(&settle);
    let handler = Arc::new(move |request: CallbackRequest| {
        let expected_state = expected_state.clone();
        let settle = Arc::clone(&handler_settle);
        Box::pin(async move {
            if request.path != CALLBACK_PATH {
                return CallbackReply::html(404, oauth_error_html("Callback route not found.", None));
            }
            if request.query_param("state").as_deref() != Some(expected_state.as_str()) {
                return CallbackReply::html(400, oauth_error_html("OAuth state mismatch.", None));
            }
            if let Some(error) = request.query_param("error") {
                let message = request.query_param("error_description").unwrap_or(error);
                settle_code(&settle, None);
                return CallbackReply::html(400, oauth_error_html(&message, None));
            }
            let Some(code) = request.query_param("code") else {
                return CallbackReply::html(400, oauth_error_html("Missing authorization code.", None));
            };
            settle_code(&settle, Some(code));
            CallbackReply::html(
                200,
                oauth_success_html("Signed in to Radius. You may now close this page."),
            )
        }) as crate::request::BoxFuture<'static, CallbackReply>
    });
    match start_callback_server(CALLBACK_HOST, CALLBACK_PORT, handler).await {
        Ok(server) => CallbackCodeServer {
            server: Some(server),
            receiver,
            settle,
        },
        Err(_) => {
            settle_code(&settle, None);
            CallbackCodeServer {
                server: None,
                receiver,
                settle,
            }
        }
    }
}

async fn login_with_browser(
    runtime: &OAuthRuntime,
    gateway: &str,
    authorization_endpoint: &str,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let pkce = generate_pkce_with(|bytes| runtime.random.fill(bytes));
    let state = runtime.random.uuid();
    let authorize_url = crate::auth::oauth::support::url_with_query(
        authorization_endpoint,
        &[
            ("response_type", "code"),
            ("client_id", OAUTH_CLIENT_ID),
            ("redirect_uri", REDIRECT_URI),
            ("scope", OAUTH_SCOPE),
            ("code_challenge", pkce.challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("handoff", "url"),
            ("state", state.as_str()),
        ],
    );
    let callback = start_callback(state, interaction.signal()).await;
    interaction.notify(AuthEvent::Progress {
        message: format!("Listening for OAuth callback on {REDIRECT_URI}"),
    });
    interaction.notify(AuthEvent::AuthUrl {
        url: authorize_url,
        instructions: Some("Continue in your browser.".to_owned()),
    });

    let CallbackCodeServer {
        server,
        receiver,
        settle,
    } = callback;
    let mut receiver = Box::pin(receiver);
    let signal = interaction.signal();
    let code = loop {
        tokio::select! {
            biased;
            response = &mut receiver => break response.ok().flatten(),
            () = signal.cancelled() => {
                settle_code(&settle, None);
            }
        }
    };
    let Some(code) = code else {
        if let Some(server) = server {
            server.close();
        }
        if signal.is_cancelled() {
            return Err(AuthError::failed("Login cancelled"));
        }
        return Err(AuthError::failed("OAuth callback did not complete."));
    };

    let credential = request_oauth_token(
        runtime,
        gateway,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", OAUTH_CLIENT_ID),
            ("redirect_uri", REDIRECT_URI),
            ("code", code.as_str()),
            ("code_verifier", pkce.verifier.as_str()),
        ],
        interaction.signal(),
    )
    .await;
    if let Some(server) = server {
        server.close();
    }
    credential.map_err(|error| AuthError::failed(error.message().to_owned()))
}

#[derive(Debug, Clone, PartialEq)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: f64,
    interval: Option<f64>,
}

async fn request_device_authorization(
    runtime: &OAuthRuntime,
    gateway: &str,
    signal: CancellationToken,
) -> Result<DeviceAuthorization, CallbackError> {
    let request = form_request(
        &format!("{gateway}/v1/oauth/device"),
        form_encode(&[("client_id", OAUTH_CLIENT_ID), ("scope", OAUTH_SCOPE)]),
        true,
    );
    let response = send(&runtime.fetch, request, Some(signal.clone()))
        .await
        .map_err(|error| {
            if signal.is_cancelled() {
                CallbackError::new("Login cancelled")
            } else {
                error
            }
        })?;
    if !response.ok() {
        return Err(
            read_oauth_response_error(&response, "Radius OAuth device authorization failed").into_callback_error(),
        );
    }
    let data = response.json.unwrap_or(Value::Null);
    let device_code = data.get("device_code").and_then(Value::as_str);
    let user_code = data.get("user_code").and_then(Value::as_str);
    let verification_uri = data.get("verification_uri").and_then(Value::as_str);
    let expires_in = data
        .get("expires_in")
        .and_then(Value::as_f64)
        .filter(|value| *value != 0.0);
    let (Some(device_code), Some(user_code), Some(verification_uri), Some(expires_in)) =
        (device_code, user_code, verification_uri, expires_in)
    else {
        return Err(CallbackError::new(
            "Radius OAuth device authorization response is missing required fields",
        ));
    };
    Ok(DeviceAuthorization {
        device_code: device_code.to_owned(),
        user_code: user_code.to_owned(),
        verification_uri: verification_uri.to_owned(),
        expires_in,
        interval: data.get("interval").and_then(Value::as_f64),
    })
}

async fn login_with_device_code(
    runtime: &OAuthRuntime,
    gateway: &str,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let device = request_device_authorization(runtime, gateway, interaction.signal())
        .await
        .map_err(|error| AuthError::failed(error.message))?;
    interaction.notify(AuthEvent::DeviceCode {
        user_code: device.user_code.clone(),
        verification_uri: device.verification_uri.clone(),
        interval_seconds: device.interval,
        expires_in_seconds: Some(device.expires_in),
    });

    let options = OAuthDeviceCodePollOptions {
        interval_seconds: device.interval,
        expires_in_seconds: Some(device.expires_in),
        wait_before_first_poll: false,
        signal: interaction.signal(),
    };
    let device_code = device.device_code.clone();
    let runtime = runtime.clone();
    let gateway = gateway.to_owned();
    let signal = interaction.signal();
    poll_oauth_device_code_flow(options, move || {
        let device_code = device_code.clone();
        let runtime = runtime.clone();
        let gateway = gateway.clone();
        let signal = signal.clone();
        async move {
            match request_oauth_token(
                &runtime,
                &gateway,
                &[
                    ("grant_type", OAUTH_DEVICE_CODE_GRANT_TYPE),
                    ("client_id", OAUTH_CLIENT_ID),
                    ("device_code", device_code.as_str()),
                ],
                signal,
            )
            .await
            {
                Ok(credentials) => OAuthDeviceCodePollResult::Complete(credentials),
                Err(RadiusRequestError::Other(error)) => OAuthDeviceCodePollResult::Failed { message: error.message },
                Err(RadiusRequestError::OAuth(error)) => match error.oauth_error.as_deref() {
                    Some("authorization_pending") => OAuthDeviceCodePollResult::Pending,
                    Some("slow_down") => OAuthDeviceCodePollResult::SlowDown { interval_seconds: None },
                    Some("expired_token") => OAuthDeviceCodePollResult::Failed {
                        message: "Device authorization expired.".to_owned().into(),
                    },
                    Some("access_denied") => OAuthDeviceCodePollResult::Failed {
                        message: "Device authorization was denied.".to_owned().into(),
                    },
                    _ => OAuthDeviceCodePollResult::Failed { message: error.message },
                },
            }
        }
    })
    .await
    .map_err(|error| match error {
        OAuthDeviceCodeError::Cancelled => AuthError::failed("Login cancelled"),
        other => AuthError::failed(other.to_string()),
    })
}

async fn login_radius(
    runtime: &OAuthRuntime,
    name: &str,
    gateway: &str,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let method = interaction
        .prompt(AuthPrompt {
            signal: None,
            kind: AuthPromptKind::Select {
                message: format!("Sign in to {name}:"),
                options: vec![
                    AuthSelectOption {
                        id: LOGIN_METHOD_BROWSER.to_owned(),
                        label: "Sign in with browser (recommended)".to_owned(),
                        description: None,
                    },
                    AuthSelectOption {
                        id: LOGIN_METHOD_DEVICE_CODE.to_owned(),
                        label: "Sign in with device code (when signing in from another device)".to_owned(),
                        description: None,
                    },
                ],
            },
        })
        .await
        .map_err(|error| AuthError::failed(error.message))?;

    match method.as_str() {
        LOGIN_METHOD_DEVICE_CODE => login_with_device_code(runtime, gateway, interaction).await,
        LOGIN_METHOD_BROWSER => {
            let authorization_endpoint = load_discovery(runtime, gateway, interaction.signal())
                .await
                .map_err(|error| AuthError::failed(error.message))?;
            login_with_browser(runtime, gateway, &authorization_endpoint, interaction).await
        }
        other => Err(AuthError::failed(format!("Unknown {name} sign-in method: {other}"))),
    }
}
