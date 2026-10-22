//! Port of `packages/ai/src/auth/oauth/openrouter.ts`.

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::auth::oauth::oauth_page::{oauth_error_html, oauth_success_html};
use crate::auth::oauth::pkce::generate_pkce_with;
use crate::auth::oauth::support::{
    CallbackReply, CallbackRequest, CallbackServer, OAuthHttpResponse, OAuthRuntime, send, start_callback_server,
};
use crate::auth::{
    AuthError, AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, ModelAuth, OAuthAuth, OAuthCredential,
};
use crate::request::CallbackError;
use crate::utils::provider_env::get_provider_env_value;

const AUTHORIZE_URL: &str = "https://openrouter.ai/auth";
const TOKEN_URL: &str = "https://openrouter.ai/api/v1/auth/keys";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const TOKEN_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

pub struct OpenRouterOAuth {
    runtime: OAuthRuntime,
    callback_host: JsString,
}

impl OpenRouterOAuth {
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

impl Default for OpenRouterOAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuthAuth for OpenRouterOAuth {
    fn name(&self) -> &str {
        "OpenRouter OAuth"
    }

    fn login_label(&self) -> Option<&str> {
        Some("Sign in with OpenRouter")
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.runtime.clone();
        let callback_host = self.callback_host.clone();
        Box::pin(async move { login_openrouter(&runtime, &callback_host, interaction).await })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        _signal: CancellationToken,
    ) -> crate::request::BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async move { Ok(credential) })
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

fn parse_authorization_input(input: &JsString) -> Option<JsString> {
    let value = input.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(url) = url::Url::parse(&pi_js_compat::js_json::encode_node_utf8(&value)) {
        return url
            .query_pairs()
            .find(|(name, _)| name == "code")
            .map(|(_, value)| JsString::from(value.into_owned()));
    }
    if value.contains("code=") {
        return url::form_urlencoded::parse(pi_js_compat::js_json::encode_node_utf8(&value).as_bytes())
            .find(|(name, _)| name == "code")
            .map(|(_, value)| JsString::from(value.into_owned()));
    }
    Some(value.to_owned())
}

fn error_detail(body: &Value) -> Option<String> {
    if let Some(description) = body.get("error_description").and_then(Value::as_str) {
        return Some(description.to_owned());
    }
    if let Some(message) = body.get("message").and_then(Value::as_str) {
        return Some(message.to_owned());
    }
    if let Some(error) = body.get("error").and_then(Value::as_str) {
        return Some(error.to_owned());
    }
    body.get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

async fn exchange_authorization_code(
    runtime: &OAuthRuntime,
    code: &JsString,
    verifier: &str,
    signal: CancellationToken,
) -> Result<OAuthCredential, CallbackError> {
    if signal.is_cancelled() {
        return Err(CallbackError::new("Login cancelled"));
    }
    let body = pi_js_compat::js_json!({
        "code": code,
        "code_verifier": verifier,
        "code_challenge_method": "S256",
    });
    let request = crate::request::HttpRequest {
        method: "POST".to_owned(),
        url: TOKEN_URL.to_owned(),
        headers: vec![
            ("accept".to_owned(), "application/json".to_owned()),
            ("content-type".to_owned(), "application/json".to_owned()),
        ],
        body: Some(pi_js_compat::js_json::stringify_lossless(&body).into_bytes()),
    };
    let result = tokio::time::timeout(
        TOKEN_EXCHANGE_TIMEOUT,
        send(&runtime.fetch, request, Some(signal.clone())),
    )
    .await;
    let response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            if signal.is_cancelled() {
                return Err(CallbackError::new("Login cancelled"));
            }
            return Err(error);
        }
        Err(_) => {
            if signal.is_cancelled() {
                return Err(CallbackError::new("Login cancelled"));
            }
            return Err(CallbackError::new("OpenRouter OAuth token exchange timed out"));
        }
    };
    credential_from_response(response)
}

fn credential_from_response(response: OAuthHttpResponse) -> Result<OAuthCredential, CallbackError> {
    if response.ok() && response.json.is_none() {
        return Err(CallbackError::new("OpenRouter OAuth returned invalid JSON"));
    }
    let body = response.json.clone().unwrap_or(Value::Null);
    if !response.ok() {
        let detail = error_detail(&body);
        let suffix = detail.map(|detail| format!(": {detail}")).unwrap_or_default();
        return Err(CallbackError::new(format!(
            "OpenRouter OAuth key exchange failed (HTTP {}){suffix}",
            response.status
        )));
    }
    let Some(key) = body.get("key").and_then(Value::as_str).filter(|key| !key.is_empty()) else {
        return Err(CallbackError::new("OpenRouter OAuth response carries no \"key\""));
    };
    Ok(OAuthCredential::new("", key, MAX_SAFE_INTEGER))
}

#[derive(Default)]
struct CallbackState {
    claimed: bool,
    settled: bool,
}

type CallbackResult = Result<Option<OAuthCredential>, CallbackError>;
type CallbackSettle = Arc<Mutex<Option<oneshot::Sender<CallbackResult>>>>;

struct CallbackServerInfo {
    server: CallbackServer,
    callback_url: String,
    receiver: oneshot::Receiver<CallbackResult>,
    settle: CallbackSettle,
    state: Arc<Mutex<CallbackState>>,
}

fn finish(settle: &CallbackSettle, state: &Arc<Mutex<CallbackState>>, result: CallbackResult) {
    let mut state = state.lock().expect("callback state mutex");
    if state.settled {
        return;
    }
    state.settled = true;
    drop(state);
    if let Some(sender) = settle.lock().expect("callback settle mutex").take() {
        let _ = sender.send(result);
    }
}

fn cancel_wait(settle: &CallbackSettle, state: &Arc<Mutex<CallbackState>>) {
    let claimed = state.lock().expect("callback state mutex").claimed;
    if !claimed {
        finish(settle, state, Ok(None));
    }
}

async fn start_callback(
    runtime: &OAuthRuntime,
    host: &JsString,
    callback_path: String,
    verifier: String,
    signal: CancellationToken,
) -> Result<CallbackServerInfo, CallbackError> {
    let (sender, receiver) = oneshot::channel();
    let settle = Arc::new(Mutex::new(Some(sender)));
    let state = Arc::new(Mutex::new(CallbackState::default()));
    let handler_settle = Arc::clone(&settle);
    let handler_state = Arc::clone(&state);
    let runtime = runtime.clone();
    let handler_path = callback_path.clone();
    let handler = Arc::new(move |request: CallbackRequest| {
        let callback_path = handler_path.clone();
        let verifier = verifier.clone();
        let signal = signal.clone();
        let runtime = runtime.clone();
        let settle = Arc::clone(&handler_settle);
        let state = Arc::clone(&handler_state);
        Box::pin(async move {
            if request.method != "GET" || request.path != callback_path {
                return CallbackReply::no_store(404, oauth_error_html("OAuth callback route not found.", None));
            }
            {
                let state = state.lock().expect("callback state mutex");
                if state.claimed || state.settled {
                    return CallbackReply::no_store(
                        409,
                        oauth_error_html("This OAuth callback has already been used.", None),
                    );
                }
            }
            if let Some(error) = request.query_param("error") {
                let description = request
                    .query_param("error_description")
                    .unwrap_or_else(|| error.clone());
                finish(
                    &settle,
                    &state,
                    Err(CallbackError::new(format!(
                        "OpenRouter authorization failed: {description}"
                    ))),
                );
                return CallbackReply::no_store(
                    400,
                    oauth_error_html("OpenRouter authorization was denied.", Some(&description)),
                );
            }
            let Some(code) = request.query_param("code") else {
                return CallbackReply::no_store(
                    400,
                    oauth_error_html("OpenRouter returned no authorization code.", None),
                );
            };
            state.lock().expect("callback state mutex").claimed = true;
            match exchange_authorization_code(&runtime, &code.into(), &verifier, signal).await {
                Ok(credential) => {
                    finish(&settle, &state, Ok(Some(credential)));
                    CallbackReply::no_store(
                        200,
                        oauth_success_html("Signed in to OpenRouter. You may now close this page."),
                    )
                }
                Err(error) => {
                    let message = error.message.clone();
                    finish(&settle, &state, Err(error));
                    CallbackReply::no_store(
                        502,
                        oauth_error_html(
                            "OpenRouter key exchange failed.",
                            Some(&pi_js_compat::js_json::encode_node_utf8(&message)),
                        ),
                    )
                }
            }
        }) as crate::request::BoxFuture<'static, CallbackReply>
    });
    let server = start_callback_server(host, 0, handler)
        .await
        .map_err(|error| CallbackError::new(error.to_string()))?;
    let callback_url = format!("http://{host}:{}{callback_path}", server.port());
    Ok(CallbackServerInfo {
        server,
        callback_url,
        receiver,
        settle,
        state,
    })
}

async fn login_openrouter(
    runtime: &OAuthRuntime,
    callback_host: &JsString,
    interaction: Arc<dyn AuthInteraction>,
) -> Result<OAuthCredential, AuthError> {
    let pkce = generate_pkce_with(|bytes| runtime.random.fill(bytes));
    let callback_path = format!("/oauth/callback/{}", runtime.random.uuid());
    let callback = start_callback(
        runtime,
        callback_host,
        callback_path,
        pkce.verifier.clone(),
        interaction.signal(),
    )
    .await
    .map_err(AuthError::from)?;
    let manual_abort = CancellationToken::new();
    let authorize_url = crate::auth::oauth::support::url_with_query(
        AUTHORIZE_URL,
        &[
            ("callback_url", callback.callback_url.as_str()),
            ("code_challenge", pkce.challenge.as_str()),
            ("code_challenge_method", "S256"),
        ],
    );
    interaction.notify(AuthEvent::Progress {
        message: format!("Listening for OpenRouter OAuth callback on {}", callback.callback_url),
    });
    interaction.notify(AuthEvent::AuthUrl {
        url: (authorize_url).into(),
        instructions: Some(
            ("Complete sign-in in your browser. If the browser is on another machine, paste the final redirect URL here."
                .to_owned()).into(),
        ),
    });

    let mut prompt = Box::pin(
        interaction.prompt(AuthPrompt {
            signal: Some(manual_abort.clone()),
            kind: AuthPromptKind::ManualCode {
                message: ("Complete sign-in in your browser, or paste the authorization code / redirect URL here:"
                    .to_owned())
                .into(),
                placeholder: Some((callback.callback_url.clone()).into()),
            },
        }),
    );
    let CallbackServerInfo {
        server,
        receiver,
        settle,
        state,
        ..
    } = callback;
    let mut receiver = Box::pin(receiver);
    let interaction_signal = interaction.signal();
    let mut manual: Option<Result<JsString, CallbackError>> = None;
    let deadline = tokio::time::sleep(LOGIN_TIMEOUT);
    tokio::pin!(deadline);
    let result: Result<Option<OAuthCredential>, CallbackError> = loop {
        tokio::select! {
            biased;
            response = &mut receiver => break response.unwrap_or(Ok(None)),
            response = &mut prompt, if manual.is_none() => {
                manual = Some(response);
                cancel_wait(&settle, &state);
            }
            () = interaction_signal.cancelled() => cancel_wait(&settle, &state),
            () = &mut deadline => break Err(CallbackError::new("OpenRouter OAuth login timed out")),
        }
    };

    if let Some(Err(error)) = &manual {
        server.close();
        manual_abort.cancel();
        return Err(AuthError::failed(error.message.clone()));
    }
    if let Ok(Some(credential)) = &result {
        server.close();
        manual_abort.cancel();
        return Ok(credential.clone());
    }
    if let Err(error) = result {
        server.close();
        manual_abort.cancel();
        return Err(AuthError::failed(error.message));
    }

    if manual.is_none() {
        manual = Some(prompt.await);
    }
    if let Some(Err(error)) = &manual {
        server.close();
        manual_abort.cancel();
        return Err(AuthError::failed(error.message.clone()));
    }
    let code = manual
        .and_then(Result::ok)
        .and_then(|input| parse_authorization_input(&input));
    let Some(code) = code else {
        server.close();
        manual_abort.cancel();
        return Err(AuthError::failed("Missing authorization code"));
    };
    interaction.notify(AuthEvent::Progress {
        message: ("Exchanging authorization code for an API key...".to_owned()).into(),
    });
    let credential = exchange_authorization_code(runtime, &code, &pkce.verifier, interaction.signal()).await;
    server.close();
    manual_abort.cancel();
    credential.map_err(AuthError::from)
}
