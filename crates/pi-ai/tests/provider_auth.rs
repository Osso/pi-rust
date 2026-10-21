//! Provider-owned auth behavior: Anthropic, Amazon Bedrock, Cloudflare, Vertex,
//! and GitHub Copilot model filtering.

use std::collections::BTreeMap;
use std::sync::Arc;

use pi_ai::auth::{
    ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthEvent, AuthInfoLink, AuthOperationOptions, AuthPromptKind,
    AuthResult, Credential, CredentialStore, InMemoryCredentialStore, OAuthCredential,
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
use tokio_util::sync::CancellationToken;

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::{PromptAnswer, TestInteraction};

struct TestAuthContext {
    env: BTreeMap<String, String>,
    files: Vec<String>,
}

impl AuthContext for TestAuthContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<String>> {
        let value = self.env.get(name).cloned();
        Box::pin(async move { value })
    }

    fn file_exists(&self, path: &str) -> BoxFuture<'_, bool> {
        let exists = self.files.iter().any(|file| file == path);
        Box::pin(async move { exists })
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

fn resolve_input(ctx: Arc<dyn AuthContext>, credential: Option<ApiKeyCredential>) -> ApiKeyResolveInput {
    ApiKeyResolveInput {
        ctx,
        credential,
        signal: CancellationToken::new(),
    }
}

fn api_key_credential(key: &str) -> ApiKeyCredential {
    ApiKeyCredential {
        key: Some(key.to_owned()),
        env: None,
    }
}

fn resolve(provider: &dyn Provider, ctx: Arc<dyn AuthContext>, credential: Option<ApiKeyCredential>) -> AuthResult {
    let auth = provider.auth().api_key.as_ref().expect("api key auth");
    let result = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.resolve(resolve_input(ctx, credential)))
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
        .block_on(auth.resolve(resolve_input(ctx, credential)))
        .expect("resolve")
}

#[test]
fn anthropic_resolves_auth_token_as_bearer_header() {
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
    let mut headers = pi_ai::types::ProviderHeaders::new();
    headers.insert("Authorization".to_owned(), Some("Bearer auth-token".to_owned()));
    assert_eq!(
        result,
        AuthResult {
            auth: pi_ai::auth::ModelAuth {
                api_key: None,
                headers: Some(headers),
                base_url: None,
            },
            env: None,
            source: Some("ANTHROPIC_AUTH_TOKEN".to_owned()),
        }
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
    assert_eq!(result.auth.api_key.as_deref(), Some("oauth-token"));
    assert_eq!(result.source.as_deref(), Some("ANTHROPIC_OAUTH_TOKEN"));
}

#[test]
fn anthropic_api_key_is_last_and_stored_credential_wins() {
    let provider = anthropic_provider();
    let keyed = resolve(
        provider.as_ref(),
        auth_context(&[("ANTHROPIC_API_KEY", "api-key")], &[]),
        None,
    );
    assert_eq!(keyed.auth.api_key.as_deref(), Some("api-key"));
    assert_eq!(keyed.source.as_deref(), Some("ANTHROPIC_API_KEY"));

    let stored = resolve(
        provider.as_ref(),
        auth_context(&[("ANTHROPIC_API_KEY", "api-key")], &[]),
        Some(ApiKeyCredential {
            key: Some("stored".to_owned()),
            env: Some(ProviderEnv::from([("SCOPED".to_owned(), "value".to_owned())])),
        }),
    );
    assert_eq!(stored.auth.api_key.as_deref(), Some("stored"));
    assert_eq!(stored.source.as_deref(), Some("stored credential"));
    assert_eq!(
        stored.env,
        Some(ProviderEnv::from([("SCOPED".to_owned(), "value".to_owned())]))
    );
}

#[test]
fn anthropic_login_prompts_for_a_secret() {
    let provider = anthropic_provider();
    let auth = provider.auth().api_key.clone().expect("api key auth");
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("entered-key".to_owned())]);
    let credential = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login(interaction.clone()).expect("login"))
        .expect("credential");
    assert_eq!(credential, api_key_credential("entered-key"));
    assert!(matches!(
        interaction.prompts()[0].kind,
        AuthPromptKind::Secret { ref message, .. } if message == "Enter Anthropic API key"
    ));
}

#[test]
fn anthropic_resolve_honors_abort_before_env_reads() {
    let provider = anthropic_provider();
    let auth = provider.auth().api_key.clone().expect("api key auth");
    let signal = CancellationToken::new();
    signal.cancel();
    let result = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.resolve(ApiKeyResolveInput {
            ctx: auth_context(&[("ANTHROPIC_API_KEY", "api-key")], &[]),
            credential: None,
            signal,
        }));
    assert!(matches!(result, Err(pi_ai::auth::AuthError::Aborted)));
}

fn bedrock_login(answers: Vec<PromptAnswer>) -> (ApiKeyCredential, Arc<TestInteraction>) {
    let provider = amazon_bedrock_provider();
    let auth = provider.auth().api_key.clone().expect("api key auth");
    let interaction = TestInteraction::new(answers);
    let credential = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login(interaction.clone()).expect("login"))
        .expect("credential");
    (credential, interaction)
}

#[test]
fn bedrock_login_covers_bearer_profile_and_chain_flows() {
    let (bearer, bearer_interaction) = bedrock_login(vec![
        PromptAnswer::Text("bearer-token".to_owned()),
        PromptAnswer::Text("bedrock-token".to_owned()),
    ]);
    assert_eq!(bearer, api_key_credential("bedrock-token"));
    assert!(matches!(
        bearer_interaction.prompts()[0].kind,
        AuthPromptKind::Select { .. }
    ));
    assert!(bearer_interaction.events().is_empty());

    let (profile, profile_interaction) = bedrock_login(vec![
        PromptAnswer::Text("aws-profile".to_owned()),
        PromptAnswer::Text("work".to_owned()),
    ]);
    assert_eq!(
        profile,
        ApiKeyCredential {
            key: None,
            env: Some(ProviderEnv::from([("AWS_PROFILE".to_owned(), "work".to_owned())])),
        }
    );
    assert_eq!(
        profile_interaction.events(),
        vec![AuthEvent::Info {
            message: "Amazon Bedrock supports AWS profiles, IAM credentials, and role-based credentials.".to_owned(),
            links: Some(vec![AuthInfoLink {
                url: "https://docs.aws.amazon.com/sdkref/latest/guide/standardized-credentials.html".to_owned(),
                label: Some("AWS credential provider chain".to_owned()),
            }]),
        }]
    );

    let (chain, chain_interaction) = bedrock_login(vec![
        PromptAnswer::Text("credential-chain".to_owned()),
        PromptAnswer::Text(String::new()),
    ]);
    assert_eq!(chain, ApiKeyCredential { key: None, env: None });
    assert!(matches!(
        chain_interaction.prompts()[1].kind,
        AuthPromptKind::Text { ref message, .. }
            if message == "Configure AWS credentials, then press Enter to continue"
    ));

    let provider = amazon_bedrock_provider();
    let auth = provider.auth().api_key.clone().expect("api key auth");
    let interaction = TestInteraction::new(vec![PromptAnswer::Text("unknown".to_owned())]);
    let error = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login(interaction).expect("login"))
        .expect_err("unknown method");
    assert_eq!(error.detail(), Some("Unknown Amazon Bedrock auth method: unknown"));
}

#[test]
fn bedrock_resolve_detects_stored_and_ambient_credentials() {
    let provider = amazon_bedrock_provider();

    let stored = resolve(
        provider.as_ref(),
        auth_context(&[], &[]),
        Some(api_key_credential("token")),
    );
    assert_eq!(stored.auth, pi_ai::auth::ModelAuth::default());
    assert_eq!(stored.source.as_deref(), Some("stored credential"));

    let bearer = resolve(
        provider.as_ref(),
        auth_context(&[("AWS_BEARER_TOKEN_BEDROCK", "token")], &[]),
        None,
    );
    assert_eq!(bearer.source.as_deref(), Some("AWS_BEARER_TOKEN_BEDROCK"));

    let profile = resolve(provider.as_ref(), auth_context(&[("AWS_PROFILE", "dev")], &[]), None);
    assert_eq!(profile.auth, pi_ai::auth::ModelAuth::default());
    assert_eq!(profile.source.as_deref(), Some("AWS_PROFILE"));

    let stored_profile = resolve(
        provider.as_ref(),
        auth_context(&[], &[]),
        Some(ApiKeyCredential {
            key: None,
            env: Some(ProviderEnv::from([("AWS_PROFILE".to_owned(), "work".to_owned())])),
        }),
    );
    assert_eq!(stored_profile.source.as_deref(), Some("stored credential"));
    assert_eq!(
        stored_profile.env,
        Some(ProviderEnv::from([("AWS_PROFILE".to_owned(), "work".to_owned())]))
    );

    let keys = resolve(
        provider.as_ref(),
        auth_context(&[("AWS_ACCESS_KEY_ID", "id"), ("AWS_SECRET_ACCESS_KEY", "secret")], &[]),
        None,
    );
    assert_eq!(keys.source.as_deref(), Some("AWS access keys"));

    let ecs = resolve(
        provider.as_ref(),
        auth_context(&[("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI", "/v2/creds")], &[]),
        None,
    );
    assert_eq!(ecs.source.as_deref(), Some("ECS task role"));

    let web_identity = resolve(
        provider.as_ref(),
        auth_context(&[("AWS_WEB_IDENTITY_TOKEN_FILE", "/tmp/token")], &[]),
        None,
    );
    assert_eq!(web_identity.source.as_deref(), Some("web identity token"));

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
    assert_eq!(result.auth.api_key.as_deref(), Some("cf-key"));
    assert_eq!(
        result.env,
        Some(ProviderEnv::from([(
            "CLOUDFLARE_ACCOUNT_ID".to_owned(),
            "account-id".to_owned()
        )]))
    );
    assert_eq!(result.source.as_deref(), Some("CLOUDFLARE_API_KEY"));
}

#[test]
fn cloudflare_workers_ai_merges_partial_stored_credential_with_env() {
    let provider = cloudflare_workers_ai_provider();
    let result = resolve(
        provider.as_ref(),
        auth_context(&[("CLOUDFLARE_ACCOUNT_ID", "env-account")], &[]),
        Some(ApiKeyCredential {
            key: Some("stored-key".to_owned()),
            env: None,
        }),
    );
    assert_eq!(result.auth.api_key.as_deref(), Some("stored-key"));
    assert_eq!(
        result.env,
        Some(ProviderEnv::from([(
            "CLOUDFLARE_ACCOUNT_ID".to_owned(),
            "env-account".to_owned()
        )]))
    );
    assert_eq!(result.source.as_deref(), Some("stored credential"));
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
    assert_eq!(
        headers.get("cf-aig-authorization"),
        Some(&Some("Bearer cf-key".to_owned()))
    );
    assert_eq!(headers.get("Authorization"), Some(&None));
    assert_eq!(headers.get("x-api-key"), Some(&None));
    assert_eq!(result.auth.api_key, None);
    assert_eq!(
        result.env,
        Some(ProviderEnv::from([
            ("CLOUDFLARE_ACCOUNT_ID".to_owned(), "account-id".to_owned()),
            ("CLOUDFLARE_GATEWAY_ID".to_owned(), "gateway-id".to_owned()),
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
        .block_on(auth.login(interaction.clone()).expect("login"))
        .expect("credential");
    assert_eq!(
        credential,
        ApiKeyCredential {
            key: Some("cf-key".to_owned()),
            env: Some(ProviderEnv::from([
                ("CLOUDFLARE_ACCOUNT_ID".to_owned(), "account-id".to_owned()),
                ("CLOUDFLARE_GATEWAY_ID".to_owned(), "gateway-id".to_owned()),
            ])),
        }
    );
    let prompts: Vec<String> = interaction
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
fn vertex_login_covers_key_adc_and_service_account_flows() {
    let provider = google_vertex_provider();
    let auth = provider.auth().api_key.clone().expect("api key auth");

    let key_interaction = TestInteraction::new(vec![
        PromptAnswer::Text("api-key".to_owned()),
        PromptAnswer::Text("vertex-key".to_owned()),
    ]);
    let key = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login(key_interaction).expect("login"))
        .expect("credential");
    assert_eq!(key, api_key_credential("vertex-key"));

    let adc_interaction = TestInteraction::new(vec![
        PromptAnswer::Text("adc".to_owned()),
        PromptAnswer::Text("proj".to_owned()),
        PromptAnswer::Text("us-central1".to_owned()),
    ]);
    let adc = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login(adc_interaction.clone()).expect("login"))
        .expect("credential");
    assert_eq!(
        adc,
        ApiKeyCredential {
            key: None,
            env: Some(ProviderEnv::from([
                ("GOOGLE_CLOUD_PROJECT".to_owned(), "proj".to_owned()),
                ("GOOGLE_CLOUD_LOCATION".to_owned(), "us-central1".to_owned()),
            ])),
        }
    );
    assert!(matches!(adc_interaction.events()[0], AuthEvent::Info { .. }));

    let service_account_interaction = TestInteraction::new(vec![
        PromptAnswer::Text("service-account".to_owned()),
        PromptAnswer::Text("proj".to_owned()),
        PromptAnswer::Text("us-central1".to_owned()),
        PromptAnswer::Text("/tmp/sa.json".to_owned()),
    ]);
    let service_account = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login(service_account_interaction).expect("login"))
        .expect("credential");
    assert_eq!(
        service_account
            .env
            .as_ref()
            .and_then(|env| env.get("GOOGLE_APPLICATION_CREDENTIALS")),
        Some(&"/tmp/sa.json".to_owned())
    );

    let unknown = TestInteraction::new(vec![PromptAnswer::Text("unknown".to_owned())]);
    let error = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(auth.login(unknown).expect("login"))
        .expect_err("unknown method");
    assert_eq!(error.detail(), Some("Unknown Google Vertex AI auth method: unknown"));
}

#[test]
fn vertex_resolve_prefers_api_key_then_adc_with_project_and_location() {
    let provider = google_vertex_provider();

    let keyed = resolve(
        provider.as_ref(),
        auth_context(&[("GOOGLE_CLOUD_API_KEY", "vertex-key")], &[]),
        None,
    );
    assert_eq!(keyed.auth.api_key.as_deref(), Some("vertex-key"));
    assert_eq!(keyed.source.as_deref(), Some("GOOGLE_CLOUD_API_KEY"));

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
    assert_eq!(adc.source.as_deref(), Some("gcloud application default credentials"));

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
    assert_eq!(keyed_over_adc.auth.api_key.as_deref(), Some("vertex-key"));

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
        gcloud_project.source.as_deref(),
        Some("gcloud application default credentials")
    );
}

#[test]
fn vertex_stored_credential_env_drives_adc_resolution() {
    let provider = google_vertex_provider();
    let credential = ApiKeyCredential {
        key: None,
        env: Some(ProviderEnv::from([
            ("GOOGLE_CLOUD_PROJECT".to_owned(), "proj".to_owned()),
            ("GOOGLE_CLOUD_LOCATION".to_owned(), "us-central1".to_owned()),
            ("GOOGLE_APPLICATION_CREDENTIALS".to_owned(), "/tmp/sa.json".to_owned()),
        ])),
    };
    let result = resolve(
        provider.as_ref(),
        auth_context(&[], &["/tmp/sa.json"]),
        Some(credential.clone()),
    );
    assert_eq!(result.auth, pi_ai::auth::ModelAuth::default());
    assert_eq!(result.source.as_deref(), Some("stored credential"));
    assert_eq!(result.env, credential.env);
}

fn oauth_credential(extra: Value) -> Credential {
    let mut credential = OAuthCredential::new("refresh", "access", 0.0);
    let Value::Object(extra) = extra else { panic!("object") };
    credential.extra = extra;
    Credential::OAuth(credential)
}

fn copilot_available_ids(credential: Option<&Credential>) -> Vec<String> {
    let provider = github_copilot_provider();
    provider
        .filter_models(provider.get_models(), credential)
        .into_iter()
        .map(|model| model.id)
        .collect()
}

#[test]
fn github_copilot_filters_models_to_available_model_ids() {
    let provider = github_copilot_provider();
    let all: Vec<String> = provider.get_models().into_iter().map(|model| model.id).collect();
    assert!(all.contains(&"gpt-5.4".to_owned()));

    let filtered = copilot_available_ids(Some(&oauth_credential(json!({
        "availableModelIds": ["gpt-5.4", "claude-opus-4.7"]
    }))));
    assert_eq!(filtered, vec!["claude-opus-4.7".to_owned(), "gpt-5.4".to_owned()]);

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
fn github_copilot_available_models_follow_the_stored_oauth_credential() {
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
                AuthOperationOptions::default(),
            )
            .await
            .expect("store credential");
    });

    let models = create_models(CreateModelsOptions {
        credentials: Some(store as Arc<dyn CredentialStore>),
        ..Default::default()
    });
    models.set_provider(github_copilot_provider());
    let available = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(models.get_available(Some("github-copilot"), AuthOperationOptions::default()));
    let ids: Vec<String> = available
        .expect("available")
        .into_iter()
        .map(|model| model.id)
        .collect();
    assert_eq!(ids, vec!["gpt-5.4".to_owned()]);
}

#[test]
fn builtin_models_exposes_provider_owned_auth() {
    let models = create_models(CreateModelsOptions {
        auth_context: Some(auth_context(&[("ANTHROPIC_AUTH_TOKEN", "auth-token")], &[])),
        ..Default::default()
    });
    models.set_provider(anthropic_provider());
    let auth = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(models.get_auth("anthropic", Default::default()))
        .expect("auth")
        .expect("resolved");
    let headers = auth.auth.headers.expect("headers");
    assert_eq!(
        headers.get("Authorization"),
        Some(&Some("Bearer auth-token".to_owned()))
    );
    assert_eq!(auth.source.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
}
