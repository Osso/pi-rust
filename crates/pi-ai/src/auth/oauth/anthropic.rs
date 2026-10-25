//! Port of `packages/ai/src/auth/oauth/anthropic.ts` (Claude Pro/Max).

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_js_compat::abort::{AbortController, AbortSignal};
use tokio::sync::oneshot;

use crate::auth::oauth::oauth_page::{oauth_error_html, oauth_success_html};
use crate::auth::oauth::pkce::generate_pkce_with;
use crate::auth::oauth::support::{
    AuthorizationInput, CallbackReply, CallbackRequest, CallbackServer, OAuthRuntime, json_request,
    parse_authorization_input, send, start_callback_server,
};
use crate::auth::{
    AuthError, AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, ModelAuth, OAuthAuth, OAuthCredential,
};
use crate::request::CallbackError;
use crate::utils::provider_env::get_provider_env_value;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CALLBACK_PORT: u16 = 53692;
const CALLBACK_PATH: &str = "/callback";
const REDIRECT_URI: &str = "http://localhost:53692/callback";
const SCOPES: &str =
    "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
const REQUEST_TIMEOUT_MS: f64 = 30_000.0;
const EXPIRY_SKEW_MS: f64 = 5.0 * 60.0 * 1000.0;

pub struct AnthropicOAuth {
    runtime: OAuthRuntime,
    callback_host: JsString,
}

impl AnthropicOAuth {
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

    pub fn runtime(&self) -> &OAuthRuntime {
        &self.runtime
    }
}

impl Default for AnthropicOAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuthAuth for AnthropicOAuth {
    fn name(&self) -> JsString {
        "Anthropic (Claude Pro/Max)".into()
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        let callback_host = self.callback_host.clone();
        Box::pin(async move { login_anthropic(&runtime, &callback_host, interaction).await })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        Box::pin(async move {
            refresh_anthropic(&runtime, &credential.refresh)
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct CallbackCode {
    code: String,
    state: String,
}

struct CallbackWait {
    receiver: oneshot::Receiver<Option<CallbackCode>>,
    settle: Arc<Mutex<Option<oneshot::Sender<Option<CallbackCode>>>>>,
}

fn settle_wait(sender: &Arc<Mutex<Option<oneshot::Sender<Option<CallbackCode>>>>>, value: Option<CallbackCode>) {
    if let Some(sender) = sender.lock().expect("callback sender mutex").take() {
        let _ = sender.send(value);
    }
}

async fn start_anthropic_callback_server(
    host: &JsString,
    expected_state: String,
) -> Result<(CallbackServer, CallbackWait), CallbackError> {
    let (sender, receiver) = oneshot::channel();
    let settle = Arc::new(Mutex::new(Some(sender)));
    let handler_settle = Arc::clone(&settle);
    let handler = Arc::new(move |request: CallbackRequest| {
        let expected_state = expected_state.clone();
        let settle_sender = Arc::clone(&handler_settle);
        Box::pin(async move {
            if request.path != CALLBACK_PATH {
                return CallbackReply::html(404, oauth_error_html("Callback route not found.", None));
            }
            if let Some(error) = request.query_param("error").filter(|error| !error.is_empty()) {
                return CallbackReply::html(
                    400,
                    oauth_error_html(
                        "Anthropic authentication did not complete.",
                        Some(&format!("Error: {error}")),
                    ),
                );
            }
            let Some(code) = request.query_param("code").filter(|code| !code.is_empty()) else {
                return CallbackReply::html(400, oauth_error_html("Missing code or state parameter.", None));
            };
            let Some(state) = request.query_param("state").filter(|state| !state.is_empty()) else {
                return CallbackReply::html(400, oauth_error_html("Missing code or state parameter.", None));
            };
            if state != expected_state {
                return CallbackReply::html(400, oauth_error_html("State mismatch.", None));
            }
            settle_wait(&settle_sender, Some(CallbackCode { code, state }));
            CallbackReply::html(
                200,
                oauth_success_html("Anthropic authentication completed. You can close this window."),
            )
        }) as crate::request::BoxFuture<'static, CallbackReply>
    });
    let server = start_callback_server(host, CALLBACK_PORT, handler)
        .await
        .map_err(|error| crate::auth::oauth::support::callback_listen_error(&error, host, CALLBACK_PORT))?;
    Ok((server, CallbackWait { receiver, settle }))
}

async fn post_json(
    runtime: &OAuthRuntime,
    url: &str,
    body: impl Into<pi_js_compat::js_json::JsValue>,
) -> Result<String, CallbackError> {
    let request = json_request(url, body, true);
    let signal = AbortSignal::timeout(REQUEST_TIMEOUT_MS)
        .map_err(|error| CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Error(error)))?;
    let response = send(&runtime.fetch, request, Some(signal)).await?;
    if !response.ok() {
        return Err(CallbackError::new(format!(
            "HTTP request failed. status={}; url={url}; body={}",
            response.status, response.body
        )));
    }
    Ok(response.body)
}

fn stringify_error_value(value: &pi_js_compat::js_json::JsValue) -> Result<JsString, CallbackError> {
    pi_js_compat::js_json::to_js_string(value).map_err(|message| {
        CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Error(
            pi_js_compat::node_error::NativeNodeError::new("TypeError", message),
        ))
    })
}

fn format_error_details(error: &CallbackError) -> Result<JsString, CallbackError> {
    use pi_js_compat::abort::AbortReason;
    use pi_js_compat::js_json::is_truthy;
    let error = match error.thrown_value() {
        AbortReason::Value(value) => return stringify_error_value(&value),
        AbortReason::Error(error) | AbortReason::DomException(error) => error,
    };
    let mut details = vec![JsString::join(
        [error.name.clone(), ": ".into(), error.message.clone()],
        "",
    )];
    let value = error.value();
    for key in ["code", "errno"] {
        if let Some(property) = value.get(key)
            && (key != "code" || is_truthy(property))
        {
            details.push(JsString::join(
                [format!("{key}=").into(), stringify_error_value(property)?],
                "",
            ));
        }
    }
    if let Some(cause) = error.cause() {
        let cause = format_error_details(&CallbackError::from_rejection(cause.clone()))?;
        details.push(pi_js_compat::js_format!("cause={}", cause));
    }
    Ok(JsString::join(details, "; "))
}

fn wrap_request_error(prefix: impl Into<JsString>, error: CallbackError) -> CallbackError {
    match format_error_details(&error) {
        Ok(details) => CallbackError::new(JsString::join([prefix.into(), details], "")),
        Err(error) => error,
    }
}

async fn exchange_authorization_code(
    runtime: &OAuthRuntime,
    code: &JsString,
    state: &JsString,
    verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthCredential, CallbackError> {
    let body = pi_js_compat::js_json!({
        "grant_type": "authorization_code",
        "client_id": CLIENT_ID,
        "code": code,
        "state": state,
        "redirect_uri": redirect_uri,
        "code_verifier": verifier,
    });
    let response_body = post_json(runtime, TOKEN_URL, body)
        .await
        .map_err(|error| {
            let prefix = format!("Token exchange request failed. url={TOKEN_URL}; redirect_uri={redirect_uri}; response_type=authorization_code; details=");
            wrap_request_error(prefix, error)
        })?;

    let token_data: pi_js_compat::js_json::JsValue = crate::auth::oauth::support::parse_response_json(&response_body)
        .map_err(|error| {
        wrap_request_error(
            format!("Token exchange returned invalid JSON. url={TOKEN_URL}; body={response_body}; details="),
            error,
        )
    })?;
    credential_from_token(&token_data, runtime.now())
}

fn credential_from_token(
    token_data: &pi_js_compat::js_json::JsValue,
    now: f64,
) -> Result<OAuthCredential, CallbackError> {
    let access = token_data
        .get("access_token")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .ok_or_else(|| CallbackError::new("Token exchange response missing access_token"))?;
    let refresh = token_data
        .get("refresh_token")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .ok_or_else(|| CallbackError::new("Token exchange response missing refresh_token"))?;
    let expires_in = token_data
        .get("expires_in")
        .and_then(pi_js_compat::js_json::JsValue::as_f64)
        .ok_or_else(|| CallbackError::new("Token exchange response missing expires_in"))?;
    Ok(OAuthCredential::new(
        refresh,
        access,
        now + expires_in * 1000.0 - EXPIRY_SKEW_MS,
    ))
}

async fn login_anthropic(
    runtime: &OAuthRuntime,
    callback_host: &JsString,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let pkce = generate_pkce_with(|bytes| runtime.random.fill(bytes))?;
    let (server, wait) = start_anthropic_callback_server(callback_host, pkce.verifier.clone())
        .await
        .map_err(AuthError::from)?;

    let manual_abort = AbortController::new();
    let authorize_url = crate::auth::oauth::support::url_with_query(
        AUTHORIZE_URL,
        &[
            ("code", "true"),
            ("client_id", CLIENT_ID),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT_URI),
            ("scope", SCOPES),
            ("code_challenge", &pkce.challenge),
            ("code_challenge_method", "S256"),
            ("state", &pkce.verifier),
        ],
    );
    if let Err(error) = interaction.notify(AuthEvent::AuthUrl {
        url: (authorize_url).into(),
        instructions: Some(
            ("Complete login in your browser. If the browser is on another machine, paste the final redirect URL here."
                .to_owned()).into(),
        ),
    }) {
        server.close();
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
    let mut manual: Option<Result<JsString, CallbackError>> = None;
    let CallbackWait {
        receiver,
        settle: wait_settle,
    } = wait;
    let mut receiver = Box::pin(receiver);
    let result = loop {
        tokio::select! {
            biased;
            response = &mut receiver => {
                break response.ok().flatten();
            }
            response = &mut prompt, if manual.is_none() => {
                manual = Some(response.map_err(crate::auth::oauth::support::normalize_manual_rejection));
                settle_wait(&wait_settle, None);
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
        server.close();
        manual_abort.abort(None);
        return Err(AuthError::from(error.clone()));
    }

    let (mut code, mut state) = match result {
        Some(CallbackCode { code, state }) => (Some(code.into()), Some(state.into())),
        None => match manual.and_then(Result::ok).filter(|input| !input.is_empty()) {
            Some(input) => match resolve_manual_input(&input, &pkce.verifier) {
                Ok(resolved) => resolved,
                Err(error) => {
                    server.close();
                    manual_abort.abort(None);
                    return Err(error);
                }
            },
            None => (None, None),
        },
    };

    if code.as_ref().is_none_or(JsString::is_empty) {
        let input = interaction
            .prompt(AuthPrompt {
                signal: None,
                kind: AuthPromptKind::Text {
                    message: "Paste the authorization code or full redirect URL:".into(),
                    placeholder: Some(REDIRECT_URI.into()),
                },
            })
            .await;
        let resolved = match input {
            Ok(input) => resolve_manual_input(&input, &pkce.verifier),
            Err(error) => Err(error.into()),
        };
        match resolved {
            Ok(resolved) => (code, state) = resolved,
            Err(error) => {
                server.close();
                manual_abort.abort(None);
                return Err(error);
            }
        }
    }
    let Some(code) = code.filter(|code| !code.is_empty()) else {
        server.close();
        manual_abort.abort(None);
        return Err(AuthError::failed("Missing authorization code"));
    };
    let Some(state) = state.filter(|state| !state.is_empty()) else {
        server.close();
        manual_abort.abort(None);
        return Err(AuthError::failed("Missing OAuth state"));
    };

    if let Err(error) = interaction.notify(AuthEvent::Progress {
        message: ("Exchanging authorization code for tokens...".to_owned()).into(),
    }) {
        server.close();
        manual_abort.abort(None);
        return Err(AuthError::from(error));
    }
    // Frozen returns the exchange promise without awaiting it inside the server-close finally.
    server.close();
    let credential = exchange_authorization_code(runtime, &code, &state, &pkce.verifier, REDIRECT_URI).await;
    manual_abort.abort(None);
    credential.map_err(AuthError::from)
}

fn resolve_manual_input(input: &JsString, verifier: &str) -> Result<(Option<JsString>, Option<JsString>), AuthError> {
    let AuthorizationInput { code, state } = parse_authorization_input(input);
    if state
        .as_ref()
        .is_some_and(|state| !state.is_empty() && state != verifier)
    {
        return Err(AuthError::failed("OAuth state mismatch"));
    }
    Ok((code, Some(state.unwrap_or_else(|| verifier.into()))))
}

async fn refresh_anthropic(
    runtime: &OAuthRuntime,
    refresh_token: &pi_js_compat::js_json::JsString,
) -> Result<OAuthCredential, CallbackError> {
    use pi_js_compat::js_json::{JsObject, JsValue};
    let body = JsValue::Object(JsObject::from_iter([
        ("grant_type".into(), JsValue::String("refresh_token".into())),
        ("client_id".into(), JsValue::String(CLIENT_ID.into())),
        ("refresh_token".into(), JsValue::String(refresh_token.clone())),
    ]));
    let response_body = post_json(runtime, TOKEN_URL, body).await.map_err(|error| {
        wrap_request_error(
            format!("Anthropic token refresh request failed. url={TOKEN_URL}; details="),
            error,
        )
    })?;
    let data = crate::auth::oauth::support::parse_response_json(&response_body).map_err(|error| {
        wrap_request_error(
            format!("Anthropic token refresh returned invalid JSON. url={TOKEN_URL}; body={response_body}; details="),
            error,
        )
    })?;
    let access = data
        .get("access_token")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .ok_or_else(|| CallbackError::new("Anthropic token refresh response missing access_token"))?;
    let refresh = data
        .get("refresh_token")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .ok_or_else(|| CallbackError::new("Anthropic token refresh response missing refresh_token"))?;
    let expires_in = data
        .get("expires_in")
        .and_then(pi_js_compat::js_json::JsValue::as_f64)
        .ok_or_else(|| CallbackError::new("Anthropic token refresh response missing expires_in"))?;
    Ok(OAuthCredential::new(
        refresh,
        access,
        runtime.now() + expires_in * 1000.0 - EXPIRY_SKEW_MS,
    ))
}
