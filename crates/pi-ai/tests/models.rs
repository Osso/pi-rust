use indexmap::IndexMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::auth::{
    ApiKeyAuth, ApiKeyCredential, ApiKeyResolveInput, AuthError, AuthInteraction, AuthResult, Credential,
    CredentialModifier, CredentialStore, InMemoryCredentialStore, ModelAuth, OAuthAuth, OAuthCredential, ProviderAuth,
};
use pi_ai::models::{
    CreateModelsOptions, CreateProviderOptions, Models, ModelsErrorCode, MutableModels, Provider, ProviderApi,
    calculate_cost, clamp_thinking_level, create_models, create_provider, get_supported_thinking_levels, has_api,
    models_are_equal,
};
use pi_ai::request::{
    BoxFuture, CallbackError, ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, Message, Modality, Model,
    ModelCost, ModelCostRates, ModelThinkingLevel, StopReason, TextContent, Usage, UsageCost,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, StreamFailure, create_assistant_message_event_stream};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::JsString;
use serde_json::json;

fn test_model(provider: &str, id: &str) -> Model {
    Model {
        properties: Default::default(),
        id: id.to_owned(),
        name: id.into(),
        api: "test-api".to_owned(),
        provider: provider.to_owned(),
        base_url: ("https://example.test/v1".to_owned()).into(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            properties: Default::default(),
            rates: ModelCostRates::default(),
        },
        context_window: 10_000.0,
        auto_compaction_threshold: None,
        max_tokens: 1_000.0,
        headers: None,
        compat: None,
    }
}

fn done_message(model: &Model, text: &str) -> AssistantMessage {
    let mut message = AssistantMessage::empty(&model.api, &model.provider, &model.id, 1.0);
    message.content = vec![AssistantContent::Text(TextContent::new(text))];
    message.stop_reason = StopReason::Stop;
    message
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![Message::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text(("hi".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 1.0,
        })],
        tools: None,
    }
}

#[path = "support/oracle.rs"]
mod oracle;

#[tokio::test]
async fn request_auth_receives_the_actual_model_not_provider_status_context() {
    struct ModelAwareAuth;
    impl ApiKeyAuth for ModelAwareAuth {
        fn name(&self) -> JsString {
            "model-aware".into()
        }
        fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async move {
                let model = input.model;
                let pi_ai::auth::AuthModel::Chat(model) = model else {
                    panic!("chat model")
                };
                Ok(Some(AuthResult {
                    auth: ModelAuth {
                        api_key: Some(format!("{}:{}", model.id, model.base_url.to_utf8().unwrap()).into()),
                        ..Default::default()
                    },
                    env: None,
                    source: None,
                }))
            })
        }
    }
    let models = models();
    let model = test_model("model-aware", "requested-2");
    models.set_provider(provider(
        TestProvider::new("model-aware", vec![test_model("model-aware", "first-1")])
            .with_auth(ProviderAuth::api_key(Arc::new(ModelAwareAuth))),
    ));
    let auth = models.get_auth(&model).await.unwrap().unwrap();
    let mut oracle = oracle::TsOracle::spawn("auth-request-model-oracle.ts", "packages/ai/src/auth/resolve.ts");
    assert_eq!(
        serde_json::to_value(&auth).unwrap(),
        oracle.request(&json!({"model": model}))
    );
    assert_eq!(auth.auth.api_key, Some("requested-2:https://example.test/v1".into()));
    let result = models
        .complete_simple(model.clone(), context(), Default::default())
        .await
        .unwrap();
    assert_eq!(result.model, model.id);
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64
}

struct AmbientAuth;

impl ApiKeyAuth for AmbientAuth {
    fn name(&self) -> JsString {
        "Ambient".into()
    }

    fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async {
            Ok(Some(AuthResult {
                auth: ModelAuth::default(),
                env: None,
                source: None,
            }))
        })
    }
}

fn ambient_auth() -> ProviderAuth {
    ProviderAuth::api_key(Arc::new(AmbientAuth))
}

fn env_key_auth(key: Option<&str>) -> Arc<dyn ApiKeyAuth> {
    struct EnvKeyAuth {
        key: Option<String>,
    }
    impl ApiKeyAuth for EnvKeyAuth {
        fn name(&self) -> JsString {
            "Test API key".into()
        }
        fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            let stored = input.credential.and_then(|credential| credential.key);
            let source = if stored.is_some() { "stored" } else { "env" };
            let key = stored.or_else(|| (self.key.clone()).map(pi_js_compat::js_json::JsString::from));
            Box::pin(async move {
                Ok(key.map(|key| AuthResult {
                    auth: ModelAuth {
                        api_key: Some(key),
                        headers: None,
                        base_url: None,
                    },
                    env: None,
                    source: Some((source.to_owned()).into()),
                }))
            })
        }
    }
    Arc::new(EnvKeyAuth {
        key: key.map(str::to_owned),
    })
}

#[derive(Clone)]
struct ProviderCall {
    model: Model,
    options: StreamOptions,
}

struct TestProvider {
    id: String,
    models: Arc<Mutex<Vec<Model>>>,
    auth: ProviderAuth,
    calls: Option<Arc<Mutex<Vec<ProviderCall>>>>,
}

impl TestProvider {
    fn new(id: &str, models: Vec<Model>) -> Self {
        Self {
            id: id.to_owned(),
            models: Arc::new(Mutex::new(models)),
            auth: ambient_auth(),
            calls: None,
        }
    }

    fn with_auth(mut self, auth: ProviderAuth) -> Self {
        self.auth = auth;
        self
    }

    fn with_calls(mut self, calls: Arc<Mutex<Vec<ProviderCall>>>) -> Self {
        self.calls = Some(calls);
        self
    }

    fn respond(&self, model: Model, options: StreamOptions) -> AssistantMessageEventStream {
        if let Some(calls) = &self.calls {
            calls.lock().unwrap().push(ProviderCall {
                model: model.clone(),
                options,
            });
        }
        let (sender, stream) = create_assistant_message_event_stream();
        let message = done_message(&model, "ok");
        sender.push(AssistantMessageEvent::Start {
            partial: message.clone(),
        });
        sender.push(AssistantMessageEvent::Done {
            reason: DoneReason::Stop,
            message: message.clone(),
        });
        sender.end(Some(message));
        stream
    }
}

impl Provider for TestProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.id
    }

    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }

    fn base_url(&self) -> Option<&pi_js_compat::js_json::JsString> {
        None
    }
    fn headers(&self) -> Option<&pi_ai::types::ProviderHeaders> {
        None
    }
    fn get_models(&self) -> Result<Vec<Model>, CallbackError> {
        Ok(self.models.lock().unwrap().clone())
    }
    fn refresh_models(&self) -> Option<pi_ai::models::RefreshModelsFn> {
        None
    }

    fn stream(
        &self,
        model: Model,
        _context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        Ok(self.respond(model, options))
    }

    fn stream_simple(
        &self,
        model: Model,
        _context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        Ok(self.respond(model, options.stream))
    }
}

fn provider(provider: TestProvider) -> Arc<dyn Provider> {
    Arc::new(provider)
}

fn models() -> pi_ai::models::ModelsCollection {
    create_models(CreateModelsOptions::default())
}

fn models_with_credentials(credentials: Arc<dyn CredentialStore>) -> pi_ai::models::ModelsCollection {
    create_models(CreateModelsOptions {
        credentials: Some(credentials),
        ..CreateModelsOptions::default()
    })
}

fn oauth_credential(access: &str, expires: f64) -> Credential {
    Credential::OAuth(OAuthCredential::new("refresh", access, expires))
}

type TestRefreshFn =
    Arc<dyn Fn(OAuthCredential, AbortSignal) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> + Send + Sync>;

struct TestOAuth {
    refresh: TestRefreshFn,
}

impl OAuthAuth for TestOAuth {
    fn name(&self) -> JsString {
        "Test OAuth".into()
    }

    fn login(&self, _interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async { Err(AuthError::failed("not used")) })
    }

    fn refresh(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let signal = AbortSignal::default();
        (self.refresh)(credential, signal)
    }

    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async move {
            Ok(ModelAuth {
                api_key: Some(credential.access),
                headers: None,
                base_url: None,
            })
        })
    }
}

fn test_oauth(refresh: TestRefreshFn) -> Arc<TestOAuth> {
    Arc::new(TestOAuth { refresh })
}

#[test]
fn registers_replaces_and_deletes_providers() {
    let models = models();
    models.set_provider(provider(TestProvider::new("p1", vec![test_model("p1", "model-a")])));
    models.set_provider(provider(TestProvider::new("p2", vec![test_model("p2", "model-a")])));
    assert_eq!(
        models
            .get_providers()
            .iter()
            .map(|provider| provider.id())
            .collect::<Vec<_>>(),
        vec!["p1", "p2"]
    );

    let replacement = provider(TestProvider::new("p1", vec![test_model("p1", "model-b")]));
    models.set_provider(Arc::clone(&replacement));
    assert!(Arc::ptr_eq(&models.get_provider("p1").unwrap(), &replacement));
    assert_eq!(models.get_providers().len(), 2);

    models.delete_provider("p1");
    assert!(models.get_provider("p1").is_none());
    models.clear_providers();
    assert!(models.get_providers().is_empty());
}

#[test]
fn lists_and_finds_models_per_provider() {
    let models = models();
    models.set_provider(provider(TestProvider::new(
        "p1",
        vec![test_model("p1", "m1"), test_model("p1", "m2")],
    )));
    models.set_provider(provider(TestProvider::new("p2", vec![test_model("p2", "m3")])));

    assert_eq!(
        models
            .get_models(None)
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2", "m3"]
    );
    assert_eq!(
        models
            .get_models(Some("p1"))
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2"]
    );
    assert!(models.get_models(Some("nope")).is_empty());
    assert_eq!(models.get_model("p2", "m3").unwrap().id, "m3");
    assert!(models.get_model("p2", "missing").is_none());
    assert!(has_api(&models.get_model("p2", "m3").unwrap(), "test-api"));
    assert!(!has_api(&models.get_model("p2", "m3").unwrap(), "openai-completions"));
}

#[tokio::test]
async fn resolves_auth_stored_credential_owns_the_provider() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    let models = models_with_credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>);
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "model-a")]).with_auth(ProviderAuth {
            api_key: Some(env_key_auth(Some("env-key"))),
            oauth: Some(test_oauth(Arc::new(|credential, _signal| {
                Box::pin(async move { Ok(credential) })
            }))),
        }),
    ));
    let model = test_model("p1", "model-a");

    assert_eq!(
        models.get_auth(&model).await.unwrap().unwrap().auth.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("env-key"))
    );
    assert_eq!(
        models
            .get_auth(&test_model("p1", "m1"))
            .await
            .unwrap()
            .unwrap()
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("env-key"))
    );
    assert_eq!(
        pi_ai::auth::resolve_provider_auth(
            "p1",
            models.get_provider("p1").unwrap().auth(),
            pi_ai::auth::AuthModel::Chat(model.clone()),
            credentials.clone(),
            pi_ai::auth::default_provider_auth_context(),
            pi_ai::auth::AuthResolutionOverrides {
                api_key: Some(("explicit-key".to_owned()).into()),
                ..Default::default()
            }
        )
        .await
        .unwrap()
        .unwrap()
        .auth
        .api_key
        .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("explicit-key"))
    );

    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("oauth-token", now_ms() + 600_000.0))) })),
        )
        .await
        .unwrap();
    let resolution = models.get_auth(&test_model("p1", "m1")).await.unwrap().unwrap();
    assert_eq!(
        resolution.auth.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("oauth-token"))
    );
    assert_eq!(
        resolution.source.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("OAuth"))
    );

    credentials
        .modify(
            "p1",
            Arc::new(|_| {
                Box::pin(async {
                    Ok(Some(Credential::ApiKey(ApiKeyCredential {
                        key: Some(("stored-key".to_owned()).into()),
                        env: None,
                    })))
                })
            }),
        )
        .await
        .unwrap();
    let resolution = models.get_auth(&test_model("p1", "m1")).await.unwrap().unwrap();
    assert_eq!(
        resolution.auth.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("stored-key"))
    );
    assert_eq!(
        resolution.source.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("stored"))
    );
}

#[tokio::test]
async fn stored_credential_without_matching_handler_blocks_ambient_fallback() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    let models = models_with_credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>);
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")])
            .with_auth(ProviderAuth::api_key(env_key_auth(Some("env-key")))),
    ));
    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("a", 0.0))) })),
        )
        .await
        .unwrap();
    assert_eq!(models.get_auth(&test_model("p1", "m1")).await.unwrap(), None);
}

#[tokio::test]
async fn oauth_refresh_failure_returns_oauth_code_and_preserves_credential() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("old", 0.0))) })),
        )
        .await
        .unwrap();
    let models = models_with_credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>);
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")]).with_auth(ProviderAuth::oauth(test_oauth(Arc::new(
            |_credential, _signal| {
                Box::pin(async { Err(AuthError::failed("token refresh failed (400): invalid_grant")) })
            },
        )))),
    ));
    let error = models.get_auth(&test_model("p1", "m1")).await.unwrap_err();
    assert_eq!(error.code, ModelsErrorCode::Oauth);
    assert_eq!(error.message, "OAuth refresh failed for p1");
    assert_eq!(
        error.cause.as_deref().unwrap().as_error().unwrap().message,
        "token refresh failed (400): invalid_grant"
    );
    assert_eq!(
        credentials
            .read("p1")
            .await
            .unwrap()
            .unwrap()
            .as_oauth()
            .unwrap()
            .access
            .to_utf8()
            .unwrap(),
        "old"
    );
}

#[tokio::test]
async fn unexpired_oauth_tokens_never_lock_or_refresh_even_when_short_lived() {
    let credentials = Arc::new(CountingStore {
        inner: InMemoryCredentialStore::new(),
        modifies: AtomicUsize::new(0),
    });
    credentials
        .inner
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("valid", now_ms() + 600_000.0))) })),
        )
        .await
        .unwrap();
    let refreshes = Arc::new(AtomicUsize::new(0));
    let oauth = test_oauth({
        let refreshes = Arc::clone(&refreshes);
        Arc::new(move |credential, _signal| {
            let refreshes = Arc::clone(&refreshes);
            Box::pin(async move {
                refreshes.fetch_add(1, Ordering::SeqCst);
                Ok(OAuthCredential::new(
                    credential.refresh,
                    "new-token",
                    now_ms() + 3_600_000.0,
                ))
            })
        })
    });
    let models = models_with_credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>);
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")])
            .with_auth(ProviderAuth::oauth(Arc::clone(&oauth) as Arc<dyn OAuthAuth>)),
    ));
    assert_eq!(
        models
            .get_auth(&test_model("p1", "m1"))
            .await
            .unwrap()
            .unwrap()
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("valid"))
    );
    assert_eq!(credentials.modifies.load(Ordering::SeqCst), 0);

    credentials
        .inner
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("short", now_ms() + 60_000.0))) })),
        )
        .await
        .unwrap();
    assert_eq!(
        models
            .get_auth(&test_model("p1", "m1"))
            .await
            .unwrap()
            .unwrap()
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("short"))
    );
    assert_eq!(credentials.modifies.load(Ordering::SeqCst), 0);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
}

struct CountingStore {
    inner: InMemoryCredentialStore,
    modifies: AtomicUsize,
}

impl CredentialStore for CountingStore {
    fn read(&self, provider_id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.inner.read(provider_id)
    }

    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.modifies.fetch_add(1, Ordering::SeqCst);
        self.inner.modify(provider_id, modifier)
    }
    fn delete(&self, provider_id: &str) -> BoxFuture<'_, Result<(), AuthError>> {
        self.inner.delete(provider_id)
    }
}

#[tokio::test]
async fn serializes_concurrent_oauth_refreshes() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("old", 0.0))) })),
        )
        .await
        .unwrap();
    let refreshes = Arc::new(AtomicUsize::new(0));
    let models = models_with_credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>);
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")]).with_auth(ProviderAuth::oauth(test_oauth({
            let refreshes = Arc::clone(&refreshes);
            Arc::new(move |_credential, _signal| {
                let refreshes = Arc::clone(&refreshes);
                Box::pin(async move {
                    let count = refreshes.fetch_add(1, Ordering::SeqCst) + 1;
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    Ok(OAuthCredential::new(
                        "r2",
                        format!("new-{count}"),
                        now_ms() + 3_600_000.0,
                    ))
                })
            })
        }))),
    ));

    let (left, right) = tokio::join!(
        models.get_auth(&test_model("p1", "m1")),
        models.get_auth(&test_model("p1", "m1"))
    );
    assert_eq!(
        left.unwrap().unwrap().auth.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("new-1"))
    );
    assert_eq!(
        right.unwrap().unwrap().auth.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("new-1"))
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn wraps_credential_store_and_api_key_failures() {
    struct FailingStore;
    impl CredentialStore for FailingStore {
        fn read(&self, _provider_id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
            Box::pin(async { Err(AuthError::failed("disk on fire")) })
        }

        fn modify(
            &self,
            _provider_id: &str,
            _modifier: CredentialModifier,
        ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
            Box::pin(async { Ok(None) })
        }
        fn delete(&self, _provider_id: &str) -> BoxFuture<'_, Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
    }
    let models = models_with_credentials(Arc::new(FailingStore));
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")])
            .with_auth(ProviderAuth::api_key(env_key_auth(Some("env-key")))),
    ));
    let error = models.get_auth(&test_model("p1", "m1")).await.unwrap_err();
    assert_eq!(error.code, ModelsErrorCode::Auth);
    assert_eq!(error.message, "Credential store read failed for p1");
    assert_eq!(
        error.cause.as_deref().unwrap().as_error().unwrap().message,
        "disk on fire"
    );

    struct FailingKey;
    impl ApiKeyAuth for FailingKey {
        fn name(&self) -> JsString {
            "Failing".into()
        }
        fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async { Err(AuthError::failed("nope")) })
        }
    }
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")]).with_auth(ProviderAuth::api_key(Arc::new(FailingKey))),
    ));
    let error = models.get_auth(&test_model("p1", "m1")).await.unwrap_err();
    assert_eq!(error.code, ModelsErrorCode::Auth);
    assert_eq!(error.message, "API key auth failed for provider p1");
    assert_eq!(error.cause.as_deref().unwrap().as_error().unwrap().message, "nope");
}

#[tokio::test]
async fn explicit_request_api_key_and_env_resolve_provider_auth() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    struct ScopedAuth;
    impl ApiKeyAuth for ScopedAuth {
        fn name(&self) -> JsString {
            "Scoped".into()
        }
        fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            let ctx = input.ctx;
            let credential = input.credential;
            Box::pin(async move {
                let account = credential
                    .as_ref()
                    .and_then(|credential| credential.env.as_ref())
                    .and_then(|env| env.get("ACCOUNT_ID").cloned())
                    .or(ctx.env("ACCOUNT_ID").await?);
                let Some(account) = account else {
                    return Ok(None);
                };
                let Some(key) = credential.and_then(|credential| credential.key) else {
                    return Ok(None);
                };
                Ok(Some(AuthResult {
                    auth: ModelAuth {
                        api_key: Some(key),
                        headers: None,
                        base_url: Some(pi_js_compat::js_format!("https://example.test/{}", account)),
                    },
                    env: Some(IndexMap::from([("ACCOUNT_ID".to_owned(), account)])),
                    source: None,
                }))
            })
        }
    }
    let models = models();
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")])
            .with_auth(ProviderAuth::api_key(Arc::new(ScopedAuth)))
            .with_calls(Arc::clone(&calls)),
    ));
    let model = test_model("p1", "model-a");
    models
        .complete_simple(
            model,
            context(),
            SimpleStreamOptions {
                stream: StreamOptions {
                    request: ProviderRequestOptions {
                        api_key: Some(("explicit-key".to_owned()).into()),
                        env: Some(IndexMap::from([("ACCOUNT_ID".into(), "acct".into())])),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].model.base_url, "https://example.test/acct");
    assert_eq!(
        calls[0].options.request.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("explicit-key"))
    );
    assert_eq!(
        calls[0].options.request.env,
        Some(IndexMap::from([("ACCOUNT_ID".into(), "acct".into())]))
    );
}

#[tokio::test]
async fn merges_auth_options_and_keeps_model_headers_for_the_api_adapter() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    struct HeaderAuth;
    impl ApiKeyAuth for HeaderAuth {
        fn name(&self) -> JsString {
            "Test".into()
        }
        fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async {
                Ok(Some(AuthResult {
                    auth: ModelAuth {
                        api_key: Some(("resolved-key".to_owned()).into()),
                        headers: Some(IndexMap::from([
                            ("Authorization".into(), Some("Bearer resolved-key".into())),
                            ("x-a".into(), Some("auth".into())),
                            ("x-b".into(), Some("auth".into())),
                        ])),
                        base_url: Some(("https://auth.test/v1".to_owned()).into()),
                    },
                    env: None,
                    source: None,
                }))
            })
        }
    }
    let models = models();
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")])
            .with_auth(ProviderAuth::api_key(Arc::new(HeaderAuth)))
            .with_calls(Arc::clone(&calls)),
    ));
    let mut model = test_model("p1", "model-a");
    model.headers = Some(IndexMap::from([
        ("x-model".into(), "model".into()),
        ("x-shared".into(), "model".into()),
    ]));

    let result = models
        .complete_simple(
            model.clone(),
            context(),
            SimpleStreamOptions {
                stream: StreamOptions {
                    request: ProviderRequestOptions {
                        api_key: Some(("explicit-key".to_owned()).into()),
                        headers: Some(IndexMap::from([
                            ("authorization".into(), Some("Explicit token".into())),
                            ("x-b".into(), Some("explicit".into())),
                        ])),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Stop);
    {
        let calls = calls.lock().unwrap();
        assert_eq!(
            calls[0].options.request.api_key.as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("explicit-key"))
        );
        let headers = calls[0].options.request.headers.as_ref().unwrap();
        assert_eq!(
            headers.get("authorization").unwrap().as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("Explicit token"))
        );
        assert_eq!(
            headers.get("x-a").unwrap().as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("auth"))
        );
        assert_eq!(
            headers.get("x-b").unwrap().as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("explicit"))
        );
        assert_eq!(calls[0].model.base_url, "https://auth.test/v1");
    }

    models
        .complete_simple(
            model,
            context(),
            SimpleStreamOptions {
                stream: StreamOptions {
                    request: ProviderRequestOptions {
                        headers: Some(IndexMap::from([
                            ("x-explicit".into(), Some("explicit".into())),
                            ("X-Shared".into(), Some("explicit".into())),
                        ])),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let calls = calls.lock().unwrap();
    let headers = calls[1].options.request.headers.as_ref().unwrap();
    // models.ts:249–253 merges auth/explicit options; model headers stay on model.
    assert!(!headers.contains_key("x-model"));
    assert_eq!(
        calls[1].model.headers.as_ref().unwrap().get("x-model"),
        Some(&"model".into())
    );
    assert_eq!(
        headers.get("x-explicit").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("explicit"))
    );
    assert_eq!(
        headers.get("X-Shared").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("explicit"))
    );
}

#[tokio::test]
async fn unknown_provider_becomes_an_error_stream() {
    let models = models();
    let result = models
        .complete_simple(
            test_model("ghost", "model-a"),
            context(),
            SimpleStreamOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert!(
        result
            .error_message
            .as_ref()
            .is_some_and(|message| message.contains("Unknown provider: ghost"))
    );
}

#[tokio::test]
async fn streams_through_the_provider() {
    let models = models();
    models.set_provider(provider(TestProvider::new("p1", vec![test_model("p1", "model-a")])));
    let mut stream = models.stream_simple(test_model("p1", "model-a"), context(), SimpleStreamOptions::default());
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    assert_eq!(events.len(), 2);
    assert_eq!(stream.result().await.unwrap().stop_reason, StopReason::Stop);
}

struct RecordingStreams {
    label: String,
    calls: Arc<Mutex<Vec<String>>>,
}

impl ProviderStreams for RecordingStreams {
    fn stream(
        &self,
        model: Model,
        _context: Context,
        _options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        self.calls.lock().unwrap().push(format!("{}:{}", self.label, model.id));
        let (sender, stream) = create_assistant_message_event_stream();
        let message = done_message(&model, "ok");
        sender.push(AssistantMessageEvent::Start {
            partial: message.clone(),
        });
        sender.push(AssistantMessageEvent::Done {
            reason: DoneReason::Stop,
            message: message.clone(),
        });
        sender.end(Some(message));
        Ok(stream)
    }
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        self.stream(model, context, options.stream)
    }
}

#[tokio::test]
async fn create_provider_dispatches_on_model_api_and_reports_missing_implementations() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = create_provider(CreateProviderOptions {
        id: "mixed".to_owned(),
        name: None,
        base_url: None,
        headers: None,
        auth: ambient_auth(),
        models: vec![test_model("mixed", "model-a"), test_model("mixed", "model-b")],
        refresh_models: None,
        api: ProviderApi::Map(IndexMap::from([
            (
                "test-api".to_owned(),
                Arc::new(RecordingStreams {
                    label: "a".to_owned(),
                    calls: Arc::clone(&calls),
                }) as Arc<dyn ProviderStreams>,
            ),
            (
                "api-b".to_owned(),
                Arc::new(RecordingStreams {
                    label: "b".to_owned(),
                    calls: Arc::clone(&calls),
                }) as Arc<dyn ProviderStreams>,
            ),
        ])),
    });
    let models = models();
    models.set_provider(provider.clone());
    let mut model_b = test_model("mixed", "model-b");
    model_b.api = "api-b".to_owned();
    models
        .complete_simple(
            test_model("mixed", "model-a"),
            context(),
            SimpleStreamOptions::default(),
        )
        .await
        .unwrap();
    models
        .complete_simple(model_b, context(), SimpleStreamOptions::default())
        .await
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), vec!["a:model-a", "b:model-b"]);

    let mut ghost = test_model("mixed", "model-x");
    ghost.api = "api-ghost".to_owned();
    let result = models
        .complete_simple(ghost, context(), SimpleStreamOptions::default())
        .await
        .unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert!(
        result
            .error_message
            .as_ref()
            .is_some_and(|message| message.contains("no API implementation"))
    );
}

#[tokio::test]
async fn applies_resolved_env_to_stream_options() {
    struct EnvStreams {
        options: Arc<Mutex<Option<StreamOptions>>>,
    }
    impl ProviderStreams for EnvStreams {
        fn stream(
            &self,
            model: Model,
            _context: Context,
            options: StreamOptions,
        ) -> Result<AssistantMessageEventStream, CallbackError> {
            *self.options.lock().unwrap() = Some(options);
            let (sender, stream) = create_assistant_message_event_stream();
            let message = done_message(&model, "ok");
            sender.push(AssistantMessageEvent::Start {
                partial: message.clone(),
            });
            sender.push(AssistantMessageEvent::Done {
                reason: DoneReason::Stop,
                message: message.clone(),
            });
            sender.end(Some(message));
            Ok(stream)
        }
        fn stream_simple(
            &self,
            model: Model,
            context: Context,
            options: SimpleStreamOptions,
        ) -> Result<AssistantMessageEventStream, CallbackError> {
            self.stream(model, context, options.stream)
        }
    }
    let captured = Arc::new(Mutex::new(None));
    struct EnvAuth;
    impl ApiKeyAuth for EnvAuth {
        fn name(&self) -> JsString {
            "Test".into()
        }
        fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async {
                Ok(Some(AuthResult {
                    auth: ModelAuth {
                        api_key: Some(("provider-key".to_owned()).into()),
                        headers: None,
                        base_url: None,
                    },
                    env: Some(IndexMap::from([
                        ("PROVIDER_ONLY".into(), "provider".into()),
                        ("SHARED".into(), "provider".into()),
                    ])),
                    source: None,
                }))
            })
        }
    }
    let provider = create_provider(CreateProviderOptions {
        id: "env-provider".to_owned(),
        name: None,
        base_url: None,
        headers: None,
        auth: ProviderAuth::api_key(Arc::new(EnvAuth)),
        models: vec![test_model("env-provider", "model-a")],
        refresh_models: None,
        api: ProviderApi::Single(Arc::new(EnvStreams {
            options: Arc::clone(&captured),
        })),
    });
    let models = models();
    models.set_provider(provider);
    let env = IndexMap::from([
        (
            "REQUEST_ONLY".to_owned(),
            pi_js_compat::js_json::JsString::from("request"),
        ),
        ("SHARED".to_owned(), pi_js_compat::js_json::JsString::from("request")),
    ]);
    models
        .complete_simple(
            test_model("env-provider", "model-a"),
            context(),
            SimpleStreamOptions {
                stream: StreamOptions {
                    request: ProviderRequestOptions {
                        api_key: Some(("request-key".to_owned()).into()),
                        env: Some(env),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let options = captured.lock().unwrap().clone().unwrap();
    assert_eq!(
        options.request.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("request-key"))
    );
    assert_eq!(
        options.request.env,
        Some(IndexMap::from([
            ("PROVIDER_ONLY".into(), "provider".into()),
            ("REQUEST_ONLY".into(), "request".into()),
            ("SHARED".into(), "request".into()),
        ]))
    );
}

#[test]
fn calculates_cost_from_base_rates_even_with_old_tier_metadata() {
    let mut model = test_model("openai", "gpt-5.6-sol");
    model.cost = serde_json::from_value(json!({
        "input": 5.0, "output": 30.0, "cacheRead": 0.5, "cacheWrite": 6.25,
        "tiers": [{ "input": 10.0, "output": 45.0, "cacheRead": 1.0, "cacheWrite": 12.5,
            "inputTokensAbove": 272_000.0 }]
    }))
    .unwrap();
    let usage = |cache_write: f64| Usage {
        input: 200_000.0,
        output: 100_000.0,
        cache_read: 72_000.0,
        cache_write,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: 372_000.0 + cache_write,
        cost: UsageCost::default(),
    };
    let short = calculate_cost(&model, &usage(0.0));
    assert_eq!(short.input, 1.0);
    assert_eq!(short.output, 3.0);
    assert!((short.cache_read - 0.036).abs() < 1e-12);
    assert_eq!(short.cache_write, 0.0);

    let long = calculate_cost(&model, &usage(1.0));
    // Frozen models.ts:385–395 reads base rates only; tiers is old-only metadata.
    assert_eq!(long.input, 1.0);
    assert_eq!(long.output, 3.0);
    assert!((long.cache_read - 0.036).abs() < 1e-12);
    assert!((long.cache_write - 0.00000625).abs() < 1e-15);
}

#[test]
fn thinking_levels_clamp_to_supported_levels() {
    let mut model = test_model("p", "m");
    model.reasoning = true;
    assert_eq!(
        get_supported_thinking_levels(&model),
        vec![
            ModelThinkingLevel::Off,
            ModelThinkingLevel::Minimal,
            ModelThinkingLevel::Low,
            ModelThinkingLevel::Medium,
            ModelThinkingLevel::High,
        ]
    );
    assert_eq!(
        clamp_thinking_level(&model, ModelThinkingLevel::Max),
        ModelThinkingLevel::High
    );

    model.thinking_level_map = Some(
        [
            (ModelThinkingLevel::Xhigh, Some("x".into())),
            (ModelThinkingLevel::Max, None),
            (ModelThinkingLevel::High, None),
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(
        get_supported_thinking_levels(&model),
        vec![
            ModelThinkingLevel::Off,
            ModelThinkingLevel::Minimal,
            ModelThinkingLevel::Low,
            ModelThinkingLevel::Medium,
            ModelThinkingLevel::Xhigh,
        ]
    );
    assert_eq!(
        clamp_thinking_level(&model, ModelThinkingLevel::Ultra),
        ModelThinkingLevel::Xhigh
    );
    assert_eq!(
        clamp_thinking_level(&model, ModelThinkingLevel::High),
        ModelThinkingLevel::Xhigh
    );

    let mut non_reasoning = test_model("p", "m");
    non_reasoning.reasoning = false;
    assert_eq!(
        get_supported_thinking_levels(&non_reasoning),
        vec![ModelThinkingLevel::Off]
    );
}

#[test]
fn compares_models_by_provider_and_id() {
    let a = test_model("p", "m");
    let b = test_model("p", "m");
    let c = test_model("other", "m");
    assert!(models_are_equal(Some(&a), Some(&b)));
    assert!(!models_are_equal(Some(&a), Some(&c)));
    assert!(!models_are_equal(None, Some(&a)));
}

#[test]
fn stream_failure_is_reported_as_error() {
    let failure = StreamFailure::new("boom");
    assert_eq!(failure.message(), "boom");
    let _ = json!({});
}
