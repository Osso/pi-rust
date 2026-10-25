//! Provider-owned auth behavior: Anthropic, Amazon Bedrock, Cloudflare, Vertex,
//! and GitHub Copilot model filtering.

use pi_js_compat::js_json::JsString;
use std::collections::BTreeMap;
use std::sync::Arc;

use pi_ai::auth::{
    ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthPromptKind, AuthResult, Credential, CredentialStore,
    InMemoryCredentialStore, OAuthCredential,
};
use pi_ai::models::{CreateModelsOptions, Models, MutableModels, Provider, create_models};
use pi_ai::providers::amazon_bedrock::amazon_bedrock_provider;
use pi_ai::providers::anthropic::anthropic_provider;
use pi_ai::providers::cloudflare_ai_gateway::cloudflare_ai_gateway_provider;
use pi_ai::providers::cloudflare_workers_ai::cloudflare_workers_ai_provider;
use pi_ai::providers::github_copilot::github_copilot_provider;
use pi_ai::providers::google_vertex::google_vertex_provider;
use pi_ai::request::BoxFuture;
use pi_ai::types::ProviderEnv;
use serde_json::{Value, json};

use crate::shared_oauth as oauth_support;
use crate::shared_oracle as oracle;

#[test]
fn anthropic_provider_matches_frozen_module() {
    let provider = anthropic_provider();
    let mut oracle = oracle::TsOracle::spawn("provider-module-oracle.ts", "packages/ai/src/providers/anthropic.ts");
    assert_eq!(
        provider_descriptor(provider.as_ref()),
        oracle.request(&json!({
            "factory": "anthropicProvider", "operation": "describe"
        }))
    );
    for env in [
        vec![],
        vec![("ANTHROPIC_AUTH_TOKEN", "ignored")],
        vec![("ANTHROPIC_API_KEY", "key"), ("ANTHROPIC_OAUTH_TOKEN", "oauth")],
    ] {
        let expected = oracle.request(&json!({
            "factory": "anthropicProvider", "operation": "resolve",
            "env": env.iter().cloned().collect::<BTreeMap<_, _>>()
        }));
        assert_eq!(
            serde_json::to_value(resolve_optional(provider.as_ref(), auth_context(&env, &[]), None)).unwrap(),
            expected
        );
    }
}

#[test]
fn kimi_provider_matches_frozen_without_subscription_oauth() {
    let provider = pi_ai::providers::kimi_coding::kimi_coding_provider();
    let mut oracle = oracle::TsOracle::spawn("provider-module-oracle.ts", "packages/ai/src/providers/kimi-coding.ts");
    assert_eq!(
        provider_descriptor(provider.as_ref()),
        oracle.request(&json!({
            "factory": "kimiCodingProvider", "operation": "describe"
        }))
    );
}

#[test]
fn xai_provider_matches_frozen_without_subscription_oauth() {
    let provider = pi_ai::providers::xai::xai_provider();
    let mut oracle = oracle::TsOracle::spawn("provider-module-oracle.ts", "packages/ai/src/providers/xai.ts");
    assert_eq!(
        provider_descriptor(provider.as_ref()),
        oracle.request(&json!({
            "factory": "xaiProvider", "operation": "describe"
        }))
    );
}

fn provider_descriptor(provider: &dyn Provider) -> Value {
    let auth = provider.auth();
    let api_key = auth.api_key.as_ref().unwrap();
    json!({
        "id": provider.id(), "name": provider.name(), "baseUrl": provider.base_url(),
        "apiKeyName": api_key.name(),
        "canLogin": api_key.login().is_some(),
        "oauthName": auth.oauth.as_ref().map(|auth| auth.name()),
        "models": provider.get_models().unwrap().iter().map(|model| &model.id).collect::<Vec<_>>(),
    })
}

use oauth_support::{PromptAnswer, TestInteraction};

struct TestAuthContext {
    env: BTreeMap<String, String>,
    files: Vec<String>,
}

impl AuthContext for TestAuthContext {
    fn env(
        &self,
        name: &str,
    ) -> BoxFuture<'_, Result<Option<pi_js_compat::js_json::JsString>, pi_ai::auth::AuthError>> {
        let value = self.env.get(name).cloned().map(JsString::from);
        Box::pin(async move { Ok(value) })
    }

    fn file_exists(
        &self,
        path: &pi_js_compat::js_json::JsString,
    ) -> BoxFuture<'_, Result<bool, pi_ai::auth::AuthError>> {
        let exists = self.files.iter().any(|file| *path == *file.as_str());
        Box::pin(async move { Ok(exists) })
    }
}

fn auth_context(env: &[(&str, &str)], files: &[&str]) -> Arc<dyn AuthContext> {
    Arc::new(TestAuthContext {
        env: env
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        files: files.iter().map(|file| (*file).to_owned()).collect(),
    })
}

fn resolve_input(
    provider: &dyn Provider,
    ctx: Arc<dyn AuthContext>,
    credential: Option<ApiKeyCredential>,
) -> ApiKeyResolveInput {
    ApiKeyResolveInput {
        model: pi_ai::auth::AuthModel::Chat(
            provider
                .get_models()
                .unwrap()
                .into_iter()
                .next()
                .expect("request model"),
        ),
        ctx,
        credential,
    }
}

fn api_key_credential(key: &str) -> ApiKeyCredential {
    ApiKeyCredential {
        key: Some((key.to_owned()).into()),
        env: None,
    }
}

fn resolve(provider: &dyn Provider, ctx: Arc<dyn AuthContext>, credential: Option<ApiKeyCredential>) -> AuthResult {
    let auth = provider.auth().api_key.as_ref().expect("api key auth");
    let result = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.resolve(resolve_input(provider, ctx, credential)))
        .expect("resolve");
    result.expect("resolved auth")
}

fn resolve_optional(
    provider: &dyn Provider,
    ctx: Arc<dyn AuthContext>,
    credential: Option<ApiKeyCredential>,
) -> Option<AuthResult> {
    let auth = provider.auth().api_key.as_ref().expect("api key auth");
    tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.resolve(resolve_input(provider, ctx, credential)))
        .expect("resolve")
}

#[test]
fn anthropic_ignores_auth_token_and_resolves_oauth_key() {
    let provider = anthropic_provider();
    let result = resolve(
        provider.as_ref(),
        auth_context(
            &[
                ("ANTHROPIC_AUTH_TOKEN", "auth-token"),
                ("ANTHROPIC_OAUTH_TOKEN", "oauth-token"),
                ("ANTHROPIC_API_KEY", "api-key"),
            ],
            &[],
        ),
        None,
    );
    assert_eq!(
        result,
        AuthResult {
            auth: pi_ai::auth::ModelAuth {
                api_key: Some("oauth-token".into()),
                headers: None,
                base_url: None,
            },
            env: None,
            source: Some("ANTHROPIC_OAUTH_TOKEN".into()),
        }
    );
    assert!(
        resolve_optional(
            provider.as_ref(),
            auth_context(&[("ANTHROPIC_AUTH_TOKEN", "ignored")], &[]),
            None
        )
        .is_none()
    );
}

#[test]
fn anthropic_prefers_oauth_token_over_api_key() {
    let provider = anthropic_provider();
    let result = resolve(
        provider.as_ref(),
        auth_context(
            &[("ANTHROPIC_API_KEY", "key"), ("ANTHROPIC_OAUTH_TOKEN", "oauth-token")],
            &[],
        ),
        None,
    );
    assert_eq!(result.auth.api_key.as_ref(), Some(&JsString::from("oauth-token")));
    assert_eq!(result.source.as_ref(), Some(&JsString::from("ANTHROPIC_OAUTH_TOKEN")));
}

#[test]
fn anthropic_api_key_is_last_and_stored_credential_wins() {
    let provider = anthropic_provider();
    let keyed = resolve(
        provider.as_ref(),
        auth_context(&[("ANTHROPIC_API_KEY", "api-key")], &[]),
        None,
    );
    assert_eq!(keyed.auth.api_key.as_ref(), Some(&JsString::from("api-key")));
    assert_eq!(keyed.source.as_ref(), Some(&JsString::from("ANTHROPIC_API_KEY")));

    let stored = resolve(
        provider.as_ref(),
        auth_context(&[("ANTHROPIC_API_KEY", "api-key")], &[]),
        Some(ApiKeyCredential {
            key: Some(("stored".to_owned()).into()),
            env: Some(ProviderEnv::from([("SCOPED".into(), "value".into())])),
        }),
    );
    assert_eq!(stored.auth.api_key.as_ref(), Some(&JsString::from("stored")));
    assert_eq!(stored.source.as_ref(), Some(&JsString::from("stored credential")));
    assert_eq!(stored.env, None);
}

#[test]
fn anthropic_login_prompts_for_a_secret() {
    let provider = anthropic_provider();
    let auth = provider.auth().api_key.clone().expect("api key auth");
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("entered-key".to_owned())]);
    let credential = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login().expect("login")(interaction.clone()))
        .expect("credential");
    assert_eq!(credential, api_key_credential("entered-key"));
    assert!(matches!(
        interaction.prompts()[0].kind,
        AuthPromptKind::Secret { ref message, .. } if message == "Enter Anthropic API key"
    ));
}

#[test]
fn bedrock_provider_matches_frozen_module_without_login() {
    let provider = amazon_bedrock_provider();
    let mut oracle = oracle::TsOracle::spawn(
        "provider-module-oracle.ts",
        "packages/ai/src/providers/amazon-bedrock.ts",
    );
    assert_eq!(
        provider_descriptor(provider.as_ref()),
        oracle.request(&json!({
            "factory": "amazonBedrockProvider", "operation": "describe"
        }))
    );
    for env in [
        vec![],
        vec![("AWS_PROFILE", "dev")],
        vec![("AWS_BEARER_TOKEN_BEDROCK", "token")],
        vec![("AWS_ACCESS_KEY_ID", "id"), ("AWS_SECRET_ACCESS_KEY", "secret")],
        vec![("AWS_CONTAINER_CREDENTIALS_FULL_URI", "http://unused")],
    ] {
        let expected = oracle.request(&json!({
            "factory": "amazonBedrockProvider", "operation": "resolve",
            "env": env.iter().cloned().collect::<BTreeMap<_, _>>()
        }));
        assert_eq!(
            serde_json::to_value(resolve_optional(provider.as_ref(), auth_context(&env, &[]), None)).unwrap(),
            expected
        );
    }
}

#[test]
fn bedrock_resolve_detects_stored_and_ambient_credentials() {
    let provider = amazon_bedrock_provider();

    let stored = resolve(
        provider.as_ref(),
        auth_context(&[], &[]),
        Some(api_key_credential("token")),
    );
    assert_eq!(stored.auth.api_key, Some("token".into()));
    assert_eq!(stored.env, None);
    assert_eq!(stored.source.as_ref(), Some(&JsString::from("stored credential")));

    let bearer = resolve(
        provider.as_ref(),
        auth_context(&[("AWS_BEARER_TOKEN_BEDROCK", "token")], &[]),
        None,
    );
    assert_eq!(
        bearer.source.as_ref(),
        Some(&JsString::from("AWS_BEARER_TOKEN_BEDROCK"))
    );

    let profile = resolve(provider.as_ref(), auth_context(&[("AWS_PROFILE", "dev")], &[]), None);
    assert_eq!(profile.auth, pi_ai::auth::ModelAuth::default());
    assert_eq!(profile.source.as_ref(), Some(&JsString::from("AWS_PROFILE")));

    let stored_profile = resolve_optional(
        provider.as_ref(),
        auth_context(&[], &[]),
        Some(ApiKeyCredential {
            key: None,
            env: Some(ProviderEnv::from([("AWS_PROFILE".into(), "work".into())])),
        }),
    );
    assert_eq!(stored_profile, None);

    let keys = resolve(
        provider.as_ref(),
        auth_context(&[("AWS_ACCESS_KEY_ID", "id"), ("AWS_SECRET_ACCESS_KEY", "secret")], &[]),
        None,
    );
    assert_eq!(keys.source.as_ref(), Some(&JsString::from("AWS access keys")));

    let ecs = resolve(
        provider.as_ref(),
        auth_context(&[("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/v2/creds")], &[]),
        None,
    );
    assert_eq!(ecs.source.as_ref(), Some(&JsString::from("ECS task role")));

    let web_identity = resolve(
        provider.as_ref(),
        auth_context(&[("AWS_WEB_IDENTITY_TOKEN_FILE", "/tmp/token")], &[]),
        None,
    );
    assert_eq!(
        web_identity.source.as_ref(),
        Some(&JsString::from("web identity token"))
    );

    assert!(resolve_optional(provider.as_ref(), auth_context(&[], &[]), None).is_none());
}

#[test]
fn cloudflare_workers_ai_requires_account_and_returns_scoped_env() {
    let provider = cloudflare_workers_ai_provider();
    assert!(
        resolve_optional(
            provider.as_ref(),
            auth_context(&[("CLOUDFLARE_API_KEY", "cf-key")], &[]),
            None
        )
        .is_none()
    );

    let result = resolve(
        provider.as_ref(),
        auth_context(
            &[
                ("CLOUDFLARE_API_KEY", "cf-key"),
                ("CLOUDFLARE_ACCOUNT_ID", "account-id"),
            ],
            &[],
        ),
        None,
    );
    assert_eq!(result.auth.api_key.as_ref(), Some(&JsString::from("cf-key")));
    assert_eq!(
        result.env,
        Some(ProviderEnv::from([(
            "CLOUDFLARE_ACCOUNT_ID".into(),
            "account-id".into()
        )]))
    );
    assert_eq!(result.source.as_ref(), Some(&JsString::from("CLOUDFLARE_API_KEY")));
}

#[test]
fn cloudflare_workers_ai_stored_credential_never_merges_ambient_env() {
    let provider = cloudflare_workers_ai_provider();
    let result = resolve_optional(
        provider.as_ref(),
        auth_context(&[("CLOUDFLARE_ACCOUNT_ID", "env-account")], &[]),
        Some(ApiKeyCredential {
            key: Some(("stored-key".to_owned()).into()),
            env: None,
        }),
    );
    assert!(result.is_none());
}

#[test]
fn cloudflare_ai_gateway_requires_gateway_and_suppresses_default_auth_headers() {
    let provider = cloudflare_ai_gateway_provider();
    assert!(
        resolve_optional(
            provider.as_ref(),
            auth_context(
                &[
                    ("CLOUDFLARE_API_KEY", "cf-key"),
                    ("CLOUDFLARE_ACCOUNT_ID", "account-id"),
                ],
                &[],
            ),
            None,
        )
        .is_none()
    );

    let result = resolve(
        provider.as_ref(),
        auth_context(
            &[
                ("CLOUDFLARE_API_KEY", "cf-key"),
                ("CLOUDFLARE_ACCOUNT_ID", "account-id"),
                ("CLOUDFLARE_GATEWAY_ID", "gateway-id"),
            ],
            &[],
        ),
        None,
    );
    let headers = result.auth.headers.expect("headers");
    assert_eq!(headers.get("cf-aig-authorization"), Some(&Some("Bearer cf-key".into())));
    assert_eq!(headers.get("Authorization"), Some(&None));
    assert_eq!(headers.get("x-api-key"), Some(&None));
    assert_eq!(result.auth.api_key, None);
    assert_eq!(
        result.env,
        Some(ProviderEnv::from([
            ("CLOUDFLARE_ACCOUNT_ID".into(), "account-id".into()),
            ("CLOUDFLARE_GATEWAY_ID".into(), "gateway-id".into()),
        ]))
    );
}

#[test]
fn cloudflare_login_prompts_for_key_account_and_gateway() {
    let provider = cloudflare_ai_gateway_provider();
    let auth = provider.auth().api_key.clone().expect("api key auth");
    let interaction = TestInteraction::new(vec![
        PromptAnswer::Text("cf-key".to_owned()),
        PromptAnswer::Text("account-id".to_owned()),
        PromptAnswer::Text("gateway-id".to_owned()),
    ]);
    let credential = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login().expect("login")(interaction.clone()))
        .expect("credential");
    assert_eq!(
        credential,
        ApiKeyCredential {
            key: Some(("cf-key".to_owned()).into()),
            env: Some(ProviderEnv::from([
                ("CLOUDFLARE_ACCOUNT_ID".into(), "account-id".into()),
                ("CLOUDFLARE_GATEWAY_ID".into(), "gateway-id".into()),
            ])),
        }
    );
    let prompts: Vec<JsString> = interaction
        .prompts()
        .into_iter()
        .map(|prompt| match prompt.kind {
            AuthPromptKind::Secret { message, .. } | AuthPromptKind::Text { message, .. } => message,
            other => panic!("unexpected prompt {other:?}"),
        })
        .collect();
    assert_eq!(
        prompts,
        vec![
            "Enter Cloudflare API key",
            "Enter Cloudflare account ID",
            "Enter Cloudflare AI Gateway ID"
        ]
    );
}

const ADC_PATH: &str = "~/.config/gcloud/application_default_credentials.json";

#[test]
fn vertex_factory_is_ambient_only_at_common_pin() {
    let provider = google_vertex_provider();
    let mut oracle = oracle::TsOracle::spawn(
        "provider-module-oracle.ts",
        "packages/ai/src/providers/google-vertex.ts",
    );
    assert_eq!(
        provider_descriptor(provider.as_ref()),
        oracle.request(&json!({
            "factory":"googleVertexProvider", "operation":"describe"
        }))
    );
}

#[test]
fn vertex_resolve_prefers_api_key_then_adc_with_project_and_location() {
    let provider = google_vertex_provider();

    let keyed = resolve(
        provider.as_ref(),
        auth_context(&[("GOOGLE_CLOUD_API_KEY", "vertex-key")], &[]),
        None,
    );
    assert_eq!(keyed.auth.api_key.as_ref(), Some(&JsString::from("vertex-key")));
    assert_eq!(keyed.source.as_ref(), Some(&JsString::from("GOOGLE_CLOUD_API_KEY")));

    let adc = resolve(
        provider.as_ref(),
        auth_context(
            &[
                ("GOOGLE_CLOUD_PROJECT", "proj"),
                ("GOOGLE_CLOUD_LOCATION", "us-central1"),
            ],
            &[ADC_PATH],
        ),
        None,
    );
    assert_eq!(adc.auth, pi_ai::auth::ModelAuth::default());
    assert_eq!(
        adc.source.as_ref(),
        Some(&JsString::from("gcloud application default credentials"))
    );

    assert!(
        resolve_optional(
            provider.as_ref(),
            auth_context(&[("GOOGLE_CLOUD_PROJECT", "proj")], &[ADC_PATH]),
            None,
        )
        .is_none()
    );

    // An explicit key wins over ADC.
    let keyed_over_adc = resolve(
        provider.as_ref(),
        auth_context(
            &[
                ("GOOGLE_CLOUD_API_KEY", "vertex-key"),
                ("GOOGLE_CLOUD_PROJECT", "proj"),
                ("GOOGLE_CLOUD_LOCATION", "us-central1"),
            ],
            &[ADC_PATH],
        ),
        None,
    );
    assert_eq!(
        keyed_over_adc.auth.api_key.as_ref(),
        Some(&JsString::from("vertex-key"))
    );

    // GCLOUD_PROJECT is the second project fallback.
    let gcloud_project = resolve(
        provider.as_ref(),
        auth_context(
            &[("GCLOUD_PROJECT", "proj"), ("GOOGLE_CLOUD_LOCATION", "us-central1")],
            &[ADC_PATH],
        ),
        None,
    );
    assert_eq!(
        gcloud_project.source.as_ref(),
        Some(&JsString::from("gcloud application default credentials"))
    );
}

#[test]
fn vertex_stored_credential_owns_resolution_without_ambient_fallback() {
    let provider = google_vertex_provider();
    let mut oracle = oracle::TsOracle::spawn(
        "provider-module-oracle.ts",
        "packages/ai/src/providers/google-vertex.ts",
    );
    let env = [
        ("GOOGLE_CLOUD_API_KEY", "ambient"),
        ("GOOGLE_CLOUD_PROJECT", "proj"),
        ("GOOGLE_CLOUD_LOCATION", "us-central1"),
    ];
    let stored_env = ProviderEnv::from([("GOOGLE_APPLICATION_CREDENTIALS".into(), "/tmp/sa.json".into())]);
    for credential in [
        None,
        Some(api_key_credential("stored")),
        Some(api_key_credential("")),
        Some(ApiKeyCredential { key: None, env: None }),
        Some(ApiKeyCredential {
            key: None,
            env: Some(stored_env),
        }),
    ] {
        let expected = oracle.request(&json!({
            "factory":"googleVertexProvider", "operation":"resolve", "credential":credential,
            "env":env.into_iter().collect::<BTreeMap<_,_>>(), "files":[ADC_PATH, "/tmp/sa.json"]
        }));
        let actual = resolve_optional(
            provider.as_ref(),
            auth_context(&env, &[ADC_PATH, "/tmp/sa.json"]),
            credential.clone(),
        );
        assert_eq!(json!(actual), expected, "credential {credential:?}");
    }
}

fn oauth_credential(extra: Value) -> Credential {
    let mut credential = OAuthCredential::new("refresh", "access", 0.0);
    let Value::Object(extra) = extra else { panic!("object") };
    credential.extra = extra
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
    Credential::OAuth(credential)
}

fn copilot_available_ids(_credential: Option<&Credential>) -> Vec<String> {
    let provider = github_copilot_provider();
    provider
        .get_models()
        .unwrap()
        .into_iter()
        .map(|model| model.id)
        .collect()
}

#[test]
fn github_copilot_preserves_models_independent_of_available_model_ids() {
    let provider = github_copilot_provider();
    let all: Vec<String> = provider
        .get_models()
        .unwrap()
        .into_iter()
        .map(|model| model.id)
        .collect();
    assert!(all.contains(&"gpt-5.4".to_owned()));

    let filtered = copilot_available_ids(Some(&oauth_credential(json!({
        "availableModelIds": ["gpt-5.4", "claude-opus-4.7"]
    }))));
    // Common-pin github-copilot.ts does not install filterModels.
    assert_eq!(filtered, all);

    // Non-array / non-string payloads fall back to the full catalog.
    assert_eq!(
        copilot_available_ids(Some(&oauth_credential(json!({"availableModelIds": "gpt-5.4"})))),
        all
    );
    assert_eq!(
        copilot_available_ids(Some(&oauth_credential(json!({"availableModelIds": ["gpt-5.4", 7]})))),
        all
    );
    assert_eq!(copilot_available_ids(Some(&oauth_credential(json!({})))), all);
    assert_eq!(copilot_available_ids(None), all);
    assert_eq!(
        copilot_available_ids(Some(&Credential::ApiKey(api_key_credential("token")))),
        all
    );
}

#[test]
fn github_copilot_available_models_ignore_picker_ids_in_stored_credential() {
    let store = Arc::new(InMemoryCredentialStore::new());
    let credential = oauth_credential(json!({"availableModelIds": ["gpt-5.4"]}));
    tokio::runtime::Runtime::new().expect("runtime").block_on(async {
        store
            .modify(
                "github-copilot",
                Arc::new(move |_| {
                    let credential = credential.clone();
                    Box::pin(async move { Ok(Some(credential)) })
                }),
            )
            .await
            .expect("store credential");
    });

    let models = create_models(CreateModelsOptions {
        credentials: Some(store as Arc<dyn CredentialStore>),
        ..Default::default()
    });
    models.set_provider(github_copilot_provider());
    let ids: Vec<String> = models
        .get_models(Some("github-copilot"))
        .into_iter()
        .map(|model| model.id)
        .collect();
    let expected: Vec<String> = github_copilot_provider()
        .get_models()
        .unwrap()
        .into_iter()
        .map(|model| model.id)
        .collect();
    assert_eq!(ids, expected);
}

#[test]
fn builtin_models_exposes_provider_owned_auth() {
    let models = create_models(CreateModelsOptions {
        auth_context: Some(auth_context(&[("ANTHROPIC_OAUTH_TOKEN", "oauth-token")], &[])),
        ..Default::default()
    });
    models.set_provider(anthropic_provider());
    let auth = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(models.get_auth(&pi_ai::compat::get_model("anthropic", "claude-sonnet-4-5").expect("catalog model")))
        .expect("auth")
        .expect("resolved");
    assert_eq!(auth.auth.api_key, Some("oauth-token".into()));
    assert_eq!(auth.auth.headers, None);
    assert_eq!(auth.source.as_ref(), Some(&JsString::from("ANTHROPIC_OAUTH_TOKEN")));
}
