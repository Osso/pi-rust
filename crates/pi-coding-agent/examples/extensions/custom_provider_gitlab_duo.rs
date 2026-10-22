//! Opt-in GitLab Duo provider. Embed and explicitly register its native factory.
use pi_ai::{
    auth::{
        oauth::{
            pkce::generate_pkce_with,
            support::{OAuthRuntime, form_encode, url_with_query},
        },
        types::*,
    },
    models::Provider,
    request::{
        BoxFuture, CallbackError, HttpRequest, ProviderStreams, SimpleStreamOptions, StreamOptions, read_body_text,
    },
    types::{AnthropicMessagesCompat, AssistantMessageEvent, Context, Model, ModelCompat},
    utils::event_stream::{AssistantMessageEventStream, create_assistant_message_event_stream},
};
use pi_coding_agent::core::{
    extensions::ExtensionApi,
    model_config::ModelConfig,
    provider_composer::{ProviderConfigInput, compose_model_provider},
};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;
const CLIENT_ID: &str = "da4edff2e6ebd2bc3208611e2768bc1c1dd7be791dc5ff26ca34ca9ee44f7d4b";
const REDIRECT: &str = "http://127.0.0.1:8080/callback";
const GATEWAY: &str = "https://cloud.gitlab.com";
const TOKEN_URL: &str = "https://gitlab.com/oauth/token";
#[derive(Clone)]
pub struct Backends {
    pub anthropic: Arc<dyn ProviderStreams>,
    pub openai: Arc<dyn ProviderStreams>,
}
#[derive(Clone)]
struct DirectToken {
    token: String,
    headers: pi_ai::types::ProviderHeaders,
    expires: f64,
}
struct State {
    runtime: OAuthRuntime,
    cache: Mutex<Option<DirectToken>>,
}
impl State {
    async fn direct_token(&self, access: &JsString) -> Result<DirectToken, CallbackError> {
        let now = self.runtime.now();
        if let Some(token) = self.cache.lock().unwrap().as_ref().filter(|t| t.expires > now) {
            return Ok(token.clone());
        }
        let response = self
            .runtime
            .fetch
            .fetch(
                HttpRequest {
                    method: "POST".into(),
                    url: "https://gitlab.com/api/v4/ai/third_party_agents/direct_access".into(),
                    headers: vec![
                        (
                            "Authorization".into(),
                            pi_ai::request::header_value(&js_format!("Bearer {}", access))?,
                        ),
                        ("Content-Type".into(), "application/json".into()),
                    ],
                    body: Some(br#"{"feature_flags":{"DuoAgentPlatformNext":true}}"#.to_vec()),
                },
                None,
            )
            .await?;
        let status = response.status;
        let body = read_body_text(response.body).await?;
        if !(200..300).contains(&status) {
            return Err(CallbackError::new(if status == 403 {
                format!("GitLab Duo access denied. Ensure GitLab Duo is enabled for your account. Error: {body}")
            } else {
                format!("Failed to get direct access token: {status} {body}")
            }));
        }
        let data: Value = pi_js_compat::js_json::parse(&body).map_err(|e| CallbackError::new(e.to_string()))?;
        let headers = data["headers"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(key, v)| (key.clone(), v.as_str().map(JsString::from)))
            .collect();
        let token = DirectToken {
            token: data["token"].as_str().unwrap_or_default().into(),
            headers,
            expires: now + 25.0 * 60.0 * 1000.0,
        };
        *self.cache.lock().unwrap() = Some(token.clone());
        Ok(token)
    }
    async fn exchange(
        &self,
        pairs: &[(&str, &str)],
        signal: Option<CancellationToken>,
        failure: &str,
    ) -> Result<OAuthCredential, CallbackError> {
        let response = self
            .runtime
            .fetch
            .fetch(
                HttpRequest {
                    method: "POST".into(),
                    url: TOKEN_URL.into(),
                    headers: vec![("Content-Type".into(), "application/x-www-form-urlencoded".into())],
                    body: Some(form_encode(pairs).into_bytes()),
                },
                signal,
            )
            .await?;
        let ok = (200..300).contains(&response.status);
        let body = read_body_text(response.body).await?;
        if !ok {
            return Err(CallbackError::new(format!("{failure}: {body}")));
        }
        let data: Value = pi_js_compat::js_json::parse(&body).map_err(|e| CallbackError::new(e.to_string()))?;
        *self.cache.lock().unwrap() = None;
        Ok(OAuthCredential::new(
            data["refresh_token"].as_str().unwrap_or_default(),
            data["access_token"].as_str().unwrap_or_default(),
            (data["created_at"].as_f64().unwrap_or(f64::NAN) + data["expires_in"].as_f64().unwrap_or(f64::NAN))
                * 1000.0
                - 300000.0,
        ))
    }
}
struct GitlabOAuth(Arc<State>);
impl OAuthAuth for GitlabOAuth {
    fn name(&self) -> &str {
        "GitLab Duo"
    }
    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let state = self.0.clone();
        Box::pin(async move {
            let pkce = generate_pkce_with(|b| state.runtime.random.fill(b));
            let uuid = state.runtime.random.uuid();
            let url = url_with_query(
                "https://gitlab.com/oauth/authorize",
                &[
                    ("client_id", CLIENT_ID),
                    ("redirect_uri", REDIRECT),
                    ("response_type", "code"),
                    ("scope", "api"),
                    ("code_challenge", &pkce.challenge),
                    ("code_challenge_method", "S256"),
                    ("state", &uuid),
                ],
            );
            interaction.notify(AuthEvent::AuthUrl {
                url: JsString::from(url),
                instructions: None,
            });
            let callback = interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::Text {
                        message: "Paste the callback URL:".into(),
                        placeholder: None,
                    },
                })
                .await?;
            let callback = pi_ai::request::request_url(&callback).map_err(|_| CallbackError::new("Invalid URL"))?;
            let parsed = url::Url::parse(&callback).expect("callback URL already validated");
            let code = parsed
                .query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.into_owned())
                .filter(|c| !c.is_empty())
                .ok_or_else(|| CallbackError::new("No authorization code found in callback URL"))?;
            state
                .exchange(
                    &[
                        ("client_id", CLIENT_ID),
                        ("grant_type", "authorization_code"),
                        ("code", &code),
                        ("code_verifier", &pkce.verifier),
                        ("redirect_uri", REDIRECT),
                    ],
                    None,
                    "Token exchange failed",
                )
                .await
                .map_err(Into::into)
        })
    }
    fn refresh(
        &self,
        c: OAuthCredential,
        signal: CancellationToken,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let state = self.0.clone();
        Box::pin(async move {
            state
                .exchange(
                    &[
                        ("client_id", CLIENT_ID),
                        ("grant_type", "refresh_token"),
                        (
                            "refresh_token",
                            &pi_ai::auth::oauth::support::to_form_usv_string(&c.refresh),
                        ),
                    ],
                    Some(signal),
                    "Token refresh failed",
                )
                .await
                .map_err(Into::into)
        })
    }
    fn to_auth(&self, c: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async move {
            Ok(ModelAuth {
                api_key: Some(c.access.clone()),
                ..Default::default()
            })
        })
    }
}
pub fn gitlab_duo_extension(api: &ExtensionApi) -> Result<(), JsString> {
    gitlab_duo_extension_with_backends(
        api,
        OAuthRuntime::system(),
        Backends {
            anthropic: Arc::new(pi_ai::api::anthropic_messages_lazy::anthropic_messages_api()),
            openai: Arc::new(pi_ai::api::openai_responses::open_ai_responses_api()),
        },
    )
}
pub fn gitlab_duo_extension_with_backends(
    api: &ExtensionApi,
    runtime: OAuthRuntime,
    backends: Backends,
) -> Result<(), JsString> {
    let state = Arc::new(State {
        runtime,
        cache: Mutex::new(None),
    });
    let auth = ProviderAuth::oauth(Arc::new(GitlabOAuth(state.clone())));
    let native = Arc::new(GitlabDuo { state, auth, backends });
    let provider = compose_model_provider(
        "gitlab-duo",
        Some(native),
        &ModelConfig::default(),
        Some(&ProviderConfigInput {
            api_key: Some("$GITLAB_TOKEN".into()),
            ..Default::default()
        }),
    )
    .map_err(|e| e.to_string())?;
    api.register_native_provider(provider)
}
struct GitlabDuo {
    state: Arc<State>,
    auth: ProviderAuth,
    backends: Backends,
}
impl Provider for GitlabDuo {
    fn id(&self) -> &str {
        "gitlab-duo"
    }
    fn name(&self) -> &str {
        "GitLab Duo"
    }
    fn base_url(&self) -> Option<&JsString> {
        {
            static URL: std::sync::LazyLock<JsString> = std::sync::LazyLock::new(|| JsString::from(GATEWAY));
            Some(&URL)
        }
    }
    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }
    fn get_models(&self) -> Vec<Model> {
        let models: Vec<Value> =
            serde_json::from_str(include_str!("custom_provider_gitlab_duo/models.json")).expect("static catalog");
        models
            .into_iter()
            .map(|mut m| {
                m["api"] = json!("gitlab-duo-api");
                m["provider"] = json!("gitlab-duo");
                m["baseUrl"] = json!(GATEWAY);
                serde_json::from_value(m).expect("static model")
            })
            .collect()
    }
    fn stream(&self, m: Model, c: Context, o: StreamOptions) -> AssistantMessageEventStream {
        self.stream_simple(
            m,
            c,
            SimpleStreamOptions {
                stream: o,
                ..Default::default()
            },
        )
    }
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        let (tx, rx) = create_assistant_message_event_stream();
        let state = self.state.clone();
        let backends = self.backends.clone();
        let models = self.get_models();
        tokio::spawn(async move {
            let run = async {
                let key = options
                    .stream
                    .request
                    .api_key
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        CallbackError::new("No GitLab access token. Run /login gitlab-duo or set GITLAB_TOKEN")
                    })?;
                let index = models
                    .iter()
                    .position(|m| m.id == model.id)
                    .ok_or_else(|| CallbackError::new(format!("Unknown model: {}", model.id)))?;
                let direct = state.direct_token(key).await?;
                let mut headers = direct.headers;
                headers.insert(
                    "Authorization".into(),
                    Some(JsString::from(format!("Bearer {}", direct.token))),
                );
                let mut inner_model = model.clone();
                let anthropic = index < 5;
                inner_model.base_url = if anthropic {
                    "https://cloud.gitlab.com/ai/v1/proxy/anthropic/"
                } else {
                    "https://cloud.gitlab.com/ai/v1/proxy/openai/v1"
                }
                .into();
                if anthropic {
                    let mut compat = if let Some(ModelCompat::AnthropicMessages(c)) = inner_model.compat.take() {
                        c
                    } else {
                        AnthropicMessagesCompat::default()
                    };
                    compat.force_adaptive_thinking = Some(true);
                    inner_model.compat = Some(ModelCompat::AnthropicMessages(compat));
                }
                let mut options = options;
                options.stream.request.api_key = Some("gitlab-duo".into());
                options.stream.request.headers = Some(headers);
                let mut stream = if anthropic {
                    backends.anthropic.stream_simple(inner_model, context, options)
                } else {
                    backends.openai.stream_simple(inner_model, context, options)
                };
                while let Some(event) = stream.next().await {
                    tx.push(event.map_err(|e| CallbackError::new(e.to_string()))?);
                }
                Ok::<_, CallbackError>(())
            }
            .await;
            if let Err(error) = run {
                let event:AssistantMessageEvent=serde_json::from_value(json!({"type":"error","reason":"error","error":{"role":"assistant","content":[],"api":model.api,"provider":model.provider,"model":model.id,"usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"error","errorMessage":error.to_string(),"timestamp":state.runtime.now() as i64}})).expect("error event");
                tx.push(event);
            }
            tx.end(None);
        });
        rx
    }
}

#[cfg(test)]
mod native_direct_token_tests {
    use super::*;
    use pi_ai::request::{BoxFuture, Fetch, HttpBody, HttpRequest, HttpResponse};

    struct Body(Option<Vec<u8>>);
    impl HttpBody for Body {
        fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
            Box::pin(async { self.0.take().map(Ok) })
        }
    }
    struct Reply(&'static str);
    impl Fetch for Reply {
        fn fetch(
            &self,
            _: HttpRequest,
            _: Option<CancellationToken>,
        ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
            Box::pin(async move {
                Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: Box::new(Body(Some(self.0.as_bytes().to_vec()))),
                })
            })
        }
    }
    fn state(body: &'static str) -> State {
        State {
            runtime: OAuthRuntime::new(
                Arc::new(Reply(body)),
                Arc::new(|| 0.0),
                pi_ai::auth::oauth::support::os_random(),
            ),
            cache: Mutex::new(None),
        }
    }

    #[tokio::test]
    async fn direct_token_and_header_values_preserve_unpaired_units() {
        let state = state(r#"{"token":"\ud800","headers":{"X-Raw":"\udc00"}}"#);
        let token = state
            .direct_token(&JsString::from("access"))
            .await
            .expect("direct token");
        assert_eq!(JsString::from(token.token).units(), &[0xd800]);
        assert_eq!(
            token.headers["X-Raw"].as_ref().expect("header value").units(),
            &[0xdc00]
        );
    }

    #[tokio::test]
    async fn unpaired_header_name_is_rejected_before_caching() {
        let state = state(r#"{"token":"ok","headers":{"\ud800":"value"}}"#);
        let result = state.direct_token(&JsString::from("access")).await;
        match result {
            Ok(_) => panic!("unpaired HTTP header name must be rejected"),
            Err(error) => assert_eq!(error.message, "invalid utf-16: lone surrogate found"),
        }
        assert!(state.cache.lock().unwrap().is_none());
    }
}
