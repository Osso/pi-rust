//! Opt-in native port of `custom-provider-anthropic/index.ts`.
//! Embed and register explicitly; no provider is installed on import.
use pi_ai::auth::oauth::pkce::generate_pkce_with;
use pi_ai::auth::oauth::support::{OAuthRuntime, url_with_query};
use pi_ai::auth::types::*;
use pi_ai::models::Provider;
use pi_ai::request::{BoxFuture, CallbackError, HttpRequest, SimpleStreamOptions, StreamOptions, read_body_text};
use pi_ai::types::{Context, Model};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_coding_agent::core::extensions::ExtensionApi;
use pi_coding_agent::core::model_config::ModelConfig;
use pi_coding_agent::core::provider_composer::{ProviderConfigInput, compose_model_provider};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[path = "custom_provider_anthropic/request.rs"]
mod request;
#[path = "custom_provider_anthropic/stream.rs"]
mod stream;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";
const PROVIDER: &str = "custom-anthropic";
const API: &str = "custom-anthropic-api";

pub fn custom_anthropic_extension(api: &ExtensionApi) -> Result<(), String> {
    custom_anthropic_extension_with_runtime(api, OAuthRuntime::system())
}

/// Existing OAuth runtime supplies transport, PKCE randomness and clock to embedders.
pub fn custom_anthropic_extension_with_runtime(api: &ExtensionApi, runtime: OAuthRuntime) -> Result<(), String> {
    let auth = ProviderAuth::oauth(Arc::new(CustomOAuth(runtime.clone())));
    let provider = compose_model_provider(
        PROVIDER,
        Some(Arc::new(CustomAnthropic { auth, runtime })),
        &ModelConfig::default(),
        Some(&ProviderConfigInput {
            api_key: Some("$CUSTOM_ANTHROPIC_API_KEY".into()),
            ..Default::default()
        }),
    )
    .map_err(|error| error.to_string())?;
    api.register_native_provider(provider)
}

struct CustomOAuth(OAuthRuntime);
impl OAuthAuth for CustomOAuth {
    fn name(&self) -> &str {
        "Custom Anthropic (Claude Pro/Max)"
    }
    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.0.clone();
        Box::pin(async move {
            let pkce = generate_pkce_with(|bytes| runtime.random.fill(bytes));
            let url = url_with_query(
                "https://claude.ai/oauth/authorize",
                &[
                    ("code", "true"),
                    ("client_id", CLIENT_ID),
                    ("response_type", "code"),
                    ("redirect_uri", REDIRECT_URI),
                    ("scope", "org:create_api_key user:profile user:inference"),
                    ("code_challenge", &pkce.challenge),
                    ("code_challenge_method", "S256"),
                    ("state", &pkce.verifier),
                ],
            );
            interaction.notify(AuthEvent::AuthUrl {
                url,
                instructions: None,
            });
            let answer = interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::Text {
                        message: "Paste the authorization code:".into(),
                        placeholder: None,
                    },
                })
                .await?;
            let mut parts = answer.split('#');
            let mut body = json!({"grant_type":"authorization_code", "client_id":CLIENT_ID,
                "code":parts.next().unwrap_or_default(), "redirect_uri":REDIRECT_URI, "code_verifier":pkce.verifier});
            if let Some(state) = parts.next() {
                body["state"] = json!(state);
            }
            exchange_token(&runtime, body, None, "Token exchange failed")
                .await
                .map_err(Into::into)
        })
    }
    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.0.clone();
        Box::pin(async move {
            use pi_js_compat::js_json::{JsObject, JsValue};
            let body = JsValue::Object(JsObject::from_iter([
                ("grant_type".into(), JsValue::String("refresh_token".into())),
                ("client_id".into(), JsValue::String(CLIENT_ID.into())),
                ("refresh_token".into(), JsValue::String(credential.refresh)),
            ]));
            exchange_token(&runtime, body, Some(signal), "Token refresh failed")
                .await
                .map_err(Into::into)
        })
    }
    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async move {
            Ok(ModelAuth {
                api_key: Some(credential.access_utf8()?),
                ..Default::default()
            })
        })
    }
}

async fn exchange_token(
    runtime: &OAuthRuntime,
    body: impl Into<pi_js_compat::js_json::JsValue>,
    signal: Option<CancellationToken>,
    failure: &str,
) -> Result<OAuthCredential, CallbackError> {
    let response = runtime
        .fetch
        .fetch(
            HttpRequest {
                method: "POST".into(),
                url: TOKEN_URL.into(),
                headers: vec![("Content-Type".into(), "application/json".into())],
                body: Some(pi_js_compat::js_json::stringify_lossless(&body.into()).into_bytes()),
            },
            signal,
        )
        .await?;
    let ok = (200..300).contains(&response.status);
    let text = read_body_text(response.body).await?;
    if !ok {
        return Err(CallbackError::new(format!("{failure}: {text}")));
    }
    let data: Value = pi_js_compat::js_json::parse(&text).map_err(|e| CallbackError::new(e.to_string()))?;
    Ok(OAuthCredential::new(
        data["refresh_token"].as_str().unwrap_or_default(),
        data["access_token"].as_str().unwrap_or_default(),
        runtime.now() + data["expires_in"].as_f64().unwrap_or(f64::NAN) * 1000.0 - 300000.0,
    ))
}

struct CustomAnthropic {
    auth: ProviderAuth,
    runtime: OAuthRuntime,
}
impl Provider for CustomAnthropic {
    fn id(&self) -> &str {
        PROVIDER
    }
    fn name(&self) -> &str {
        "Custom Anthropic (Claude Pro/Max)"
    }
    fn base_url(&self) -> Option<&str> {
        Some("https://api.anthropic.com")
    }
    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }
    fn get_models(&self) -> Vec<Model> {
        [("opus", "Opus", 5.0, 25.0, 0.5, 6.25), ("sonnet", "Sonnet", 3.0, 15.0, 0.3, 3.75)].into_iter().map(|(id,name,input,output,read,write)| {
            serde_json::from_value(json!({"id":format!("claude-{id}-4-5"), "name":format!("Claude {name} 4.5 (Custom)"),
                "api":API,"provider":PROVIDER,"baseUrl":"https://api.anthropic.com", "reasoning":true,"input":["text","image"],
                "cost":{"input":input,"output":output,"cacheRead":read,"cacheWrite":write},"contextWindow":200000,"maxTokens":64000})).expect("static model")
        }).collect()
    }
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        self.stream_simple(
            model,
            context,
            SimpleStreamOptions {
                stream: options,
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
        stream::start(self.runtime.clone(), model, context, options)
    }
}
