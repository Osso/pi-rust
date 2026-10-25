//! Opt-in native port of `custom-provider-anthropic/index.ts`.
//! Embed and register explicitly; no provider is installed on import.
use pi_ai::auth::oauth::pkce::generate_pkce_with;
use pi_ai::auth::oauth::support::{OAuthRuntime, url_with_query};
use pi_ai::auth::types::*;
use pi_ai::oauth::{OAuthAuthInfo, OAuthLoginCallbacks, OAuthPrompt};
use pi_ai::request::{BoxFuture, CallbackError, HttpRequest, read_body_text};
use pi_coding_agent::core::extensions::ExtensionApi;
use pi_coding_agent::core::model_registry::{ExtensionOAuthConfig, ProviderConfigInput, ProviderModelInput};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::JsString;
use serde_json::json;
use std::sync::Arc;

#[path = "custom_provider_anthropic/request.rs"]
mod request;
#[path = "custom_provider_anthropic/stream.rs"]
mod stream;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";
const PROVIDER: &str = "custom-anthropic";
const API: &str = "custom-anthropic-api";

pub fn custom_anthropic_extension(api: &ExtensionApi) -> Result<(), CallbackError> {
    custom_anthropic_extension_with_runtime(api, OAuthRuntime::system())
}

/// Existing OAuth runtime supplies transport, PKCE randomness and clock to embedders.
pub fn custom_anthropic_extension_with_runtime(api: &ExtensionApi, runtime: OAuthRuntime) -> Result<(), CallbackError> {
    let login = runtime.clone();
    let refresh = runtime.clone();
    api.register_provider(
        PROVIDER,
        ProviderConfigInput {
            base_url: Some("https://api.anthropic.com".into()),
            api_key: Some("$CUSTOM_ANTHROPIC_API_KEY".into()),
            api: Some(API.into()),
            models: Some(catalog()),
            stream_simple: Some(Arc::new(move |model, context, options| {
                Ok(stream::start(
                    runtime.clone(),
                    model,
                    context,
                    options.unwrap_or_default(),
                ))
            })),
            oauth: Some(ExtensionOAuthConfig {
                name: "Custom Anthropic (Claude Pro/Max)".into(),
                uses_callback_server: None,
                login: Arc::new(move |callbacks| CustomOAuth(login.clone()).login(callbacks)),
                refresh_token: Arc::new(move |credentials, _| CustomOAuth(refresh.clone()).refresh(credentials)),
                get_api_key: Arc::new(|credentials| Ok(credentials.access.clone())),
                modify_models: None,
            }),
            ..Default::default()
        },
    )
}

struct CustomOAuth(OAuthRuntime);
impl CustomOAuth {
    fn login(
        &self,
        interaction: Arc<dyn OAuthLoginCallbacks>,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.0.clone();
        Box::pin(async move {
            let pkce = generate_pkce_with(|bytes| runtime.random.fill(bytes))?;
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
            interaction.on_auth(OAuthAuthInfo {
                url: JsString::from(url),
                instructions: None,
            })?;
            let answer = interaction
                .on_prompt(OAuthPrompt {
                    message: "Paste the authorization code:".into(),
                    placeholder: None,
                    allow_empty: None,
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
    fn refresh(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let runtime = self.0.clone();
        Box::pin(async move {
            use pi_js_compat::js_json::{JsObject, JsValue};
            let body = JsValue::Object(JsObject::from_iter([
                ("grant_type".into(), JsValue::String("refresh_token".into())),
                ("client_id".into(), JsValue::String(CLIENT_ID.into())),
                ("refresh_token".into(), JsValue::String(credential.refresh)),
            ]));
            exchange_token(&runtime, body, None, "Token refresh failed")
                .await
                .map_err(Into::into)
        })
    }
}

async fn exchange_token(
    runtime: &OAuthRuntime,
    body: impl Into<pi_js_compat::js_json::JsValue>,
    signal: Option<AbortSignal>,
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
    let data = pi_js_compat::js_json::parse_lossless(&text).map_err(|e| CallbackError::new(e.into_message()))?;
    Ok(OAuthCredential::new(
        data["refresh_token"]
            .as_string()
            .cloned()
            .unwrap_or_else(JsString::default),
        data["access_token"]
            .as_string()
            .cloned()
            .unwrap_or_else(JsString::default),
        runtime.now() + data["expires_in"].as_f64().unwrap_or(f64::NAN) * 1000.0 - 300000.0,
    ))
}

fn catalog() -> Vec<ProviderModelInput> {
    [
        ("opus", "Opus", 5.0, 25.0, 0.5, 6.25),
        ("sonnet", "Sonnet", 3.0, 15.0, 0.3, 3.75),
    ]
    .into_iter()
    .map(|(id, name, input, output, read, write)| {
        serde_json::from_value(json!({
            "id":format!("claude-{id}-4-5"),
            "name":format!("Claude {name} 4.5 (Custom)"),
            "reasoning":true,"input":["text","image"],
            "cost":{"input":input,"output":output,"cacheRead":read,"cacheWrite":write},
            "contextWindow":200000,"maxTokens":64000
        }))
        .expect("static registration model")
    })
    .collect()
}
