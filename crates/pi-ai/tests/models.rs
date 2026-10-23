use indexmap::IndexMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::auth::{
    ApiKeyAuth, ApiKeyCheckInput, ApiKeyCredential, ApiKeyResolveInput, AuthCheck, AuthError, AuthEvent,
    AuthInteraction, AuthOperationOptions, AuthPrompt, AuthResult, AuthType, Credential, CredentialModifier,
    CredentialStore, InMemoryCredentialStore, ModelAuth, OAuthAuth, OAuthCredential, ProviderAuth,
};
use pi_ai::models::{
    CreateModelsOptions, CreateProviderOptions, Models, ModelsErrorCode, ModelsPublication, ModelsRefreshOptions,
    MutableModels, PersistAction, Provider, ProviderApi, RefreshModelsContext, calculate_cost, clamp_thinking_level,
    create_models, create_provider, get_supported_thinking_levels, has_api, merge_headers, models_are_equal,
};
use pi_ai::models_store::{InMemoryModelsStore, ModelsStore, ModelsStoreEntry, ModelsStoreOperationOptions};
use pi_ai::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, ProviderRequestOptions, ProviderStreams,
    SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DeferredHandle, DoneReason, Message, Modality,
    Model, ModelCost, ModelCostRates, ModelCostTier, ModelThinkingLevel, StopReason, TextContent, Usage, UsageCost,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, StreamFailure, create_assistant_message_event_stream};
use serde_json::json;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

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
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 10_000.0,
        auto_compaction_threshold: None,
        max_tokens: 1_000.0,
        sampling_params: None,
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
        fn name(&self) -> &str {
            "model-aware"
        }
        fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async move {
                let model = input.model.expect("request model must be present");
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
    let auth = models
        .get_auth_for_model(&model, Default::default())
        .await
        .unwrap()
        .unwrap();
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
    fn name(&self) -> &str {
        "Ambient"
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
        fn name(&self) -> &str {
            "Test API key"
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

type RefreshFn = Arc<dyn Fn(RefreshModelsContext) -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;

struct TestProvider {
    id: String,
    models: Arc<Mutex<Vec<Model>>>,
    auth: ProviderAuth,
    refresh: Option<RefreshFn>,
    calls: Option<Arc<Mutex<Vec<ProviderCall>>>>,
}

impl TestProvider {
    fn new(id: &str, models: Vec<Model>) -> Self {
        Self {
            id: id.to_owned(),
            models: Arc::new(Mutex::new(models)),
            auth: ambient_auth(),
            refresh: None,
            calls: None,
        }
    }

    fn with_auth(mut self, auth: ProviderAuth) -> Self {
        self.auth = auth;
        self
    }

    fn with_refresh(mut self, refresh: RefreshFn) -> Self {
        self.refresh = Some(refresh);
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

    fn get_models(&self) -> Vec<Model> {
        self.models.lock().unwrap().clone()
    }

    fn supports_refresh_models(&self) -> bool {
        self.refresh.is_some()
    }

    fn refresh_models(&self, context: RefreshModelsContext) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        self.refresh.as_ref().map(|refresh| refresh(context))
    }

    fn stream(&self, model: Model, _context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        self.respond(model, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        _context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        self.respond(model, options.stream)
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

type TestRefreshFn = Arc<
    dyn Fn(OAuthCredential, CancellationToken) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> + Send + Sync,
>;

struct TestOAuth {
    refresh: TestRefreshFn,
}

impl OAuthAuth for TestOAuth {
    fn name(&self) -> &str {
        "Test OAuth"
    }

    fn login(&self, _interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async { Err(AuthError::failed("not used")) })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
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
async fn refresh_updates_dynamic_providers_and_reports_failures() {
    let list = Arc::new(Mutex::new(vec![test_model("dyn", "before")]));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let models = models();
    models.set_provider(provider(
        TestProvider::new("dyn", Vec::new())
            .with_refresh({
                let list = Arc::clone(&list);
                let refreshes = Arc::clone(&refreshes);
                Arc::new(move |context: RefreshModelsContext| {
                    let list = Arc::clone(&list);
                    let refreshes = Arc::clone(&refreshes);
                    Box::pin(async move {
                        if !context.allow_network {
                            return Ok(());
                        }
                        refreshes.fetch_add(1, Ordering::SeqCst);
                        let after = vec![test_model("dyn", "after")];
                        (context.publish)(ModelsPublication::new(PersistAction::Unchanged).with_update({
                            let list = Arc::clone(&list);
                            let after = after.clone();
                            move || *list.lock().unwrap() = after.clone()
                        }))
                        .await?;
                        Ok(())
                    })
                })
            })
            .with_models_from(Arc::clone(&list)),
    ));
    models.set_provider(provider(TestProvider::new("static", vec![test_model("static", "s1")])));

    assert!(models.get_model("dyn", "before").is_some());
    let first = models.refresh(ModelsRefreshOptions::default()).await;
    assert!(first.errors.is_empty());
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert!(models.get_model("dyn", "after").is_some());
    assert!(models.get_model("dyn", "before").is_none());

    models.set_provider(provider(TestProvider::new("flaky", Vec::new()).with_refresh(Arc::new(
        |context: RefreshModelsContext| {
            Box::pin(async move {
                if context.allow_network {
                    return Err(CallbackError::new("fetch failed"));
                }
                Ok(())
            })
        },
    ))));
    let second = models.refresh(ModelsRefreshOptions::default()).await;
    assert_eq!(refreshes.load(Ordering::SeqCst), 2);
    assert_eq!(second.errors.get("flaky").unwrap().message, "fetch failed");
}

#[tokio::test]
async fn refresh_reports_errors_in_completion_order() {
    let models = models();
    models.set_provider(provider(TestProvider::new("zeta", Vec::new()).with_refresh(Arc::new(
        |context: RefreshModelsContext| {
            Box::pin(async move {
                if context.allow_network {
                    return Err(CallbackError::new("zeta down"));
                }
                Ok(())
            })
        },
    ))));
    models.set_provider(provider(TestProvider::new("alpha", Vec::new()).with_refresh(Arc::new(
        |context: RefreshModelsContext| {
            Box::pin(async move {
                if context.allow_network {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    return Err(CallbackError::new("alpha down"));
                }
                Ok(())
            })
        },
    ))));
    let result = models.refresh(ModelsRefreshOptions::default()).await;
    let providers: Vec<&str> = result.errors.keys().map(String::as_str).collect();
    assert_eq!(providers, vec!["zeta", "alpha"]);
}

impl TestProvider {
    fn with_models_from(self, list: Arc<Mutex<Vec<Model>>>) -> Self {
        Self { models: list, ..self }
    }
}

#[tokio::test]
async fn restricts_refresh_work_to_selected_providers() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let models = models();
    for id in ["one", "two"] {
        let calls = Arc::clone(&calls);
        let id_owned = id.to_owned();
        models.set_provider(provider(TestProvider::new(id, Vec::new()).with_refresh(Arc::new(
            move |context: RefreshModelsContext| {
                let calls = Arc::clone(&calls);
                let label = format!(
                    "{}:{}",
                    id_owned,
                    if context.allow_network { "network" } else { "cache" }
                );
                Box::pin(async move {
                    calls.lock().unwrap().push(label);
                    Ok(())
                })
            },
        ))));
    }

    let result = models
        .refresh(ModelsRefreshOptions {
            providers: Some(vec!["two".to_owned(), "unknown".to_owned()]),
            ..Default::default()
        })
        .await;
    assert!(result.errors.is_empty());
    assert_eq!(*calls.lock().unwrap(), vec!["two:cache", "two:network"]);
}

#[tokio::test]
async fn restores_cached_models_before_waiting_for_network_auth() {
    let store = Arc::new(InMemoryModelsStore::new());
    store
        .write(
            "dynamic",
            ModelsStoreEntry::new(vec![test_model("dynamic", "cached")]),
            ModelsStoreOperationOptions::default(),
        )
        .await
        .unwrap();
    let auth_started = Arc::new(Notify::new());
    let release_auth = Arc::new(Notify::new());
    let models = create_models(CreateModelsOptions {
        models_store: Some(store),
        ..CreateModelsOptions::default()
    });
    let blocked_auth = {
        let auth_started = Arc::clone(&auth_started);
        let release_auth = Arc::clone(&release_auth);
        struct BlockedAuth {
            started: Arc<Notify>,
            release: Arc<Notify>,
        }
        impl ApiKeyAuth for BlockedAuth {
            fn name(&self) -> &str {
                "Blocked auth"
            }
            fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
                let started = Arc::clone(&self.started);
                let release = Arc::clone(&self.release);
                Box::pin(async move {
                    started.notify_one();
                    release.notified().await;
                    Ok(Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(("key".to_owned()).into()),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: None,
                    }))
                })
            }
        }
        ProviderAuth::api_key(Arc::new(BlockedAuth {
            started: auth_started,
            release: release_auth.clone(),
        }))
    };
    let provider = create_provider(CreateProviderOptions {
        id: "dynamic".to_owned(),
        name: None,
        base_url: None,
        headers: None,
        auth: blocked_auth,
        models: Vec::new(),
        fetch_models: Some(Arc::new(|_context: RefreshModelsContext| {
            Box::pin(async { Err(CallbackError::new("must not fetch")) })
        })),
        filter_models: None,
        api: ProviderApi::Single(Arc::new(EmptyStreams)),
    });
    models.set_provider(provider);
    let signal = CancellationToken::new();
    let pending = models.refresh(ModelsRefreshOptions {
        providers: Some(vec!["dynamic".to_owned()]),
        signal: Some(signal.clone()),
        ..Default::default()
    });
    let handle = tokio::spawn(pending);
    auth_started.notified().await;
    assert!(models.get_model("dynamic", "cached").is_some());
    signal.cancel();
    let result = handle.await.unwrap();
    assert!(result.aborted);
    release_auth.notify_one();
}

#[tokio::test]
async fn persists_dynamic_catalogs_and_restores_them_without_network() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    let models_store = Arc::new(InMemoryModelsStore::new());
    credentials
        .modify(
            "dynamic",
            Arc::new(|_| {
                Box::pin(async {
                    Ok(Some(Credential::ApiKey(ApiKeyCredential {
                        key: Some(("key".to_owned()).into()),
                        env: None,
                    })))
                })
            }),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let create_dynamic = |fetch: bool| {
        create_provider(CreateProviderOptions {
            id: "dynamic".to_owned(),
            name: None,
            base_url: None,
            headers: None,
            auth: ProviderAuth::api_key(env_key_auth(None)),
            models: Vec::new(),
            fetch_models: Some(Arc::new(
                move |_context: RefreshModelsContext| -> BoxFuture<'static, Result<Vec<Model>, CallbackError>> {
                    Box::pin(async move {
                        if fetch {
                            Ok(vec![test_model("dynamic", "fetched")])
                        } else {
                            Err(CallbackError::new("must not fetch"))
                        }
                    })
                },
            ) as pi_ai::models::FetchModelsFn),
            filter_models: None,
            api: ProviderApi::Single(Arc::new(EmptyStreams)),
        })
    };

    let online = create_models(CreateModelsOptions {
        credentials: Some(Arc::clone(&credentials) as Arc<dyn CredentialStore>),
        models_store: Some(Arc::clone(&models_store) as Arc<dyn ModelsStore>),
        ..Default::default()
    });
    online.set_provider(create_dynamic(true));
    assert!(online.refresh(ModelsRefreshOptions::default()).await.errors.is_empty());
    assert!(online.get_model("dynamic", "fetched").is_some());

    let offline = create_models(CreateModelsOptions {
        credentials: Some(Arc::clone(&credentials) as Arc<dyn CredentialStore>),
        models_store: Some(Arc::clone(&models_store) as Arc<dyn ModelsStore>),
        ..Default::default()
    });
    offline.set_provider(create_dynamic(false));
    let result = offline
        .refresh(ModelsRefreshOptions {
            allow_network: Some(false),
            ..Default::default()
        })
        .await;
    assert!(result.errors.is_empty());
    assert!(offline.get_model("dynamic", "fetched").is_some());
}

struct EmptyStreams;

impl ProviderStreams for EmptyStreams {
    fn stream(&self, _model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
        create_assistant_message_event_stream().1
    }
    fn stream_simple(
        &self,
        _model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        create_assistant_message_event_stream().1
    }
}

#[tokio::test]
async fn passes_effective_api_key_and_refresh_options_and_skips_unconfigured_providers() {
    let effective = Arc::new(Mutex::new(None::<Option<Credential>>));
    let force = Arc::new(Mutex::new(None::<Option<bool>>));
    let unconfigured = Arc::new(AtomicUsize::new(0));
    let models = models();
    models.set_provider(provider(
        TestProvider::new("configured", vec![test_model("configured", "m1")])
            .with_auth(ProviderAuth::api_key(env_key_auth(Some("ambient-key"))))
            .with_refresh({
                let effective = Arc::clone(&effective);
                let force = Arc::clone(&force);
                Arc::new(move |context: RefreshModelsContext| {
                    let effective = Arc::clone(&effective);
                    let force = Arc::clone(&force);
                    Box::pin(async move {
                        if context.allow_network {
                            *effective.lock().unwrap() = Some(context.credential.clone());
                            *force.lock().unwrap() = Some(context.force);
                        }
                        Ok(())
                    })
                })
            }),
    ));
    models.set_provider(provider(
        TestProvider::new("unconfigured", vec![test_model("unconfigured", "m1")])
            .with_auth(ProviderAuth::api_key(env_key_auth(None)))
            .with_refresh({
                let unconfigured = Arc::clone(&unconfigured);
                Arc::new(move |context: RefreshModelsContext| {
                    let unconfigured = Arc::clone(&unconfigured);
                    Box::pin(async move {
                        if context.allow_network {
                            unconfigured.fetch_add(1, Ordering::SeqCst);
                        }
                        Ok(())
                    })
                })
            }),
    ));

    models
        .refresh(ModelsRefreshOptions {
            force: Some(true),
            ..Default::default()
        })
        .await;
    let credential = effective.lock().unwrap().clone().unwrap();
    assert_eq!(
        credential,
        Some(Credential::ApiKey(ApiKeyCredential {
            key: Some(("ambient-key".to_owned()).into()),
            env: None,
        }))
    );
    assert_eq!(*force.lock().unwrap(), Some(Some(true)));
    assert_eq!(unconfigured.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn refreshes_expired_oauth_before_refreshing_models() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .modify(
            "oauth-dynamic",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("expired", 0.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let observed = Arc::new(Mutex::new(None::<Option<Credential>>));
    let models = models_with_credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>);
    models.set_provider(provider(
        TestProvider::new("oauth-dynamic", vec![test_model("oauth-dynamic", "m1")])
            .with_auth(ProviderAuth::oauth(test_oauth(Arc::new(|_credential, _signal| {
                Box::pin(async { Ok(OAuthCredential::new("rotated", "fresh", now_ms() + 60_000.0)) })
            }))))
            .with_refresh({
                let observed = Arc::clone(&observed);
                Arc::new(move |context: RefreshModelsContext| {
                    let observed = Arc::clone(&observed);
                    Box::pin(async move {
                        if context.allow_network {
                            *observed.lock().unwrap() = Some(context.credential.clone());
                        }
                        Ok(())
                    })
                })
            }),
    ));

    assert!(models.refresh(ModelsRefreshOptions::default()).await.errors.is_empty());
    let credential = observed.lock().unwrap().clone().unwrap().unwrap();
    assert_eq!(credential.as_oauth().unwrap().access.to_utf8().unwrap(), "fresh");
    assert_eq!(credential.as_oauth().unwrap().refresh.to_utf8().unwrap(), "rotated");
    let stored = credentials
        .read("oauth-dynamic", AuthOperationOptions::default())
        .await
        .unwrap();
    assert_eq!(stored.unwrap().as_oauth().unwrap().access.to_utf8().unwrap(), "fresh");
}

#[tokio::test]
async fn always_gives_providers_a_concrete_signal_and_binds_store_waits_to_it() {
    let signals = Arc::new(Mutex::new(Vec::new()));
    let store = Arc::new(RecordingStore {
        signals: Arc::clone(&signals),
    });
    let models = create_models(CreateModelsOptions {
        models_store: Some(store),
        ..Default::default()
    });
    let provider_signal = Arc::new(Mutex::new(None::<CancellationToken>));
    models.set_provider(provider(
        TestProvider::new("dynamic", vec![test_model("dynamic", "m1")])
            .with_auth(ProviderAuth::api_key(env_key_auth(Some("key"))))
            .with_refresh({
                let provider_signal = Arc::clone(&provider_signal);
                Arc::new(move |context: RefreshModelsContext| {
                    let provider_signal = Arc::clone(&provider_signal);
                    Box::pin(async move {
                        *provider_signal.lock().unwrap() = Some(context.signal.clone());
                        if !context.allow_network {
                            return Ok(());
                        }
                        (context.publish)(ModelsPublication::new(PersistAction::Write(ModelsStoreEntry::new(
                            vec![test_model("dynamic", "fresh")],
                        ))))
                        .await?;
                        Ok(())
                    })
                })
            }),
    ));

    let result = models
        .refresh(ModelsRefreshOptions {
            providers: Some(vec!["dynamic".to_owned()]),
            ..Default::default()
        })
        .await;
    assert!(result.errors.is_empty());
    let storage_signals = signals.lock().unwrap();
    assert_eq!(storage_signals.len(), 3);
    let provider_signal = provider_signal.lock().unwrap().clone().unwrap();
    assert!(storage_signals.iter().all(|signal| !signal.is_cancelled()));
    provider_signal.cancel();
    assert!(storage_signals.iter().all(CancellationToken::is_cancelled));
}

struct RecordingStore {
    signals: Arc<Mutex<Vec<CancellationToken>>>,
}

impl ModelsStore for RecordingStore {
    fn read(
        &self,
        _provider_id: &str,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<Option<ModelsStoreEntry>, AuthError>> {
        self.signals.lock().unwrap().push(options.signal.clone().unwrap());
        Box::pin(async { Ok(None) })
    }
    fn write(
        &self,
        _provider_id: &str,
        _entry: ModelsStoreEntry,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<(), AuthError>> {
        self.signals.lock().unwrap().push(options.signal.clone().unwrap());
        Box::pin(async { Ok(()) })
    }
    fn delete(&self, _provider_id: &str, options: ModelsStoreOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        self.signals.lock().unwrap().push(options.signal.clone().unwrap());
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn returns_aborted_state_without_reporting_cancellation_as_a_provider_error() {
    let controller = CancellationToken::new();
    let models = models();
    models.set_provider(provider(TestProvider::new("dynamic", Vec::new()).with_refresh({
        let controller = controller.clone();
        Arc::new(move |context: RefreshModelsContext| {
            let controller = controller.clone();
            Box::pin(async move {
                controller.cancel();
                let _ = context.signal.is_cancelled();
                Ok(())
            })
        })
    })));

    let result = models
        .refresh(ModelsRefreshOptions {
            signal: Some(controller),
            ..Default::default()
        })
        .await;
    assert!(result.aborted);
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn stops_waiting_on_abort_when_a_provider_ignores_its_signal() {
    let controller = CancellationToken::new();
    let started = Arc::new(Notify::new());
    let models = models();
    models.set_provider(provider(TestProvider::new("dynamic", Vec::new()).with_refresh({
        let started = Arc::clone(&started);
        Arc::new(move |_context: RefreshModelsContext| {
            let started = Arc::clone(&started);
            Box::pin(async move {
                started.notify_one();
                std::future::pending::<()>().await;
                Ok(())
            })
        })
    })));

    let pending = models.refresh(ModelsRefreshOptions {
        signal: Some(controller.clone()),
        ..Default::default()
    });
    let handle = tokio::spawn(pending);
    started.notified().await;
    controller.cancel();
    let result = handle.await.unwrap();
    assert!(result.aborted);
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn rejects_late_publication_from_a_superseded_provider() {
    let store = Arc::new(InMemoryModelsStore::new());
    let state = Arc::new(Mutex::new("initial".to_owned()));
    let first_started = Arc::new(Notify::new());
    let release_first = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let models = create_models(CreateModelsOptions {
        models_store: Some(Arc::clone(&store) as Arc<dyn ModelsStore>),
        ..Default::default()
    });
    models.set_provider(provider(TestProvider::new("dynamic", Vec::new()).with_refresh({
        let state = Arc::clone(&state);
        let first_started = Arc::clone(&first_started);
        let release_first = Arc::clone(&release_first);
        let calls = Arc::clone(&calls);
        Arc::new(move |context: RefreshModelsContext| {
            let state = Arc::clone(&state);
            let first_started = Arc::clone(&first_started);
            let release_first = Arc::clone(&release_first);
            let calls = Arc::clone(&calls);
            Box::pin(async move {
                if !context.allow_network {
                    return Ok(());
                }
                let current = calls.fetch_add(1, Ordering::SeqCst) + 1;
                if current == 1 {
                    first_started.notify_one();
                    release_first.notified().await;
                }
                let value = format!("generation-{current}");
                (context.publish)(
                    ModelsPublication::new(PersistAction::Write(ModelsStoreEntry::new(vec![test_model(
                        "dynamic", &value,
                    )])))
                    .with_update({
                        let state = Arc::clone(&state);
                        let value = value.clone();
                        move || *state.lock().unwrap() = value.clone()
                    }),
                )
                .await?;
                Ok(())
            })
        })
    })));

    let first = models.refresh(ModelsRefreshOptions {
        providers: Some(vec!["dynamic".to_owned()]),
        ..Default::default()
    });
    let first = tokio::spawn(first);
    first_started.notified().await;
    let second = models
        .refresh(ModelsRefreshOptions {
            providers: Some(vec!["dynamic".to_owned()]),
            ..Default::default()
        })
        .await;
    assert!(!second.aborted);
    first.await.unwrap();
    release_first.notify_one();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    assert_eq!(*state.lock().unwrap(), "generation-2");
    let stored = store
        .read("dynamic", ModelsStoreOperationOptions::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.models[0].id, "generation-2");
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
        models
            .get_auth_for_model(&model, Default::default())
            .await
            .unwrap()
            .unwrap()
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("env-key"))
    );
    assert_eq!(
        models
            .get_auth("p1", Default::default())
            .await
            .unwrap()
            .unwrap()
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("env-key"))
    );
    assert_eq!(
        models
            .get_auth_for_model(
                &model,
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
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let resolution = models.get_auth("p1", Default::default()).await.unwrap().unwrap();
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
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let resolution = models.get_auth("p1", Default::default()).await.unwrap().unwrap();
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
async fn checks_auth_without_refreshing_oauth_and_filters_available_models() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    let refreshes = Arc::new(AtomicUsize::new(0));
    let models = models_with_credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>);
    models.set_provider(provider(
        TestProvider::new("ambient", vec![test_model("ambient", "m1")])
            .with_auth(ProviderAuth::api_key(env_key_auth(Some("env-key")))),
    ));
    models.set_provider(provider(
        TestProvider::new("missing", vec![test_model("missing", "m1")])
            .with_auth(ProviderAuth::api_key(env_key_auth(None))),
    ));
    models.set_provider(provider(
        TestProvider::new("oauth", vec![test_model("oauth", "m1")]).with_auth(ProviderAuth::oauth(test_oauth({
            let refreshes = Arc::clone(&refreshes);
            Arc::new(move |credential, _signal| {
                refreshes.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { Ok(credential) })
            })
        }))),
    ));
    credentials
        .modify(
            "oauth",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("expired", 0.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();

    assert_eq!(
        models.check_auth("ambient", Default::default()).await.unwrap(),
        Some(AuthCheck {
            source: Some(("env".to_owned()).into()),
            auth_type: AuthType::ApiKey,
        })
    );
    assert_eq!(models.check_auth("missing", Default::default()).await.unwrap(), None);
    assert_eq!(
        models.check_auth("oauth", Default::default()).await.unwrap(),
        Some(AuthCheck {
            source: Some(("OAuth".to_owned()).into()),
            auth_type: AuthType::Oauth,
        })
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(
        models
            .get_available(None, Default::default())
            .await
            .unwrap()
            .iter()
            .map(|model| model.provider.as_str())
            .collect::<Vec<_>>(),
        vec!["ambient", "oauth"]
    );
    assert_eq!(
        models
            .get_available(Some("ambient"), Default::default())
            .await
            .unwrap()
            .iter()
            .map(|model| model.provider.as_str())
            .collect::<Vec<_>>(),
        vec!["ambient"]
    );
}

struct TestInteraction {
    signal: CancellationToken,
    answer: String,
}

impl AuthInteraction for TestInteraction {
    fn signal(&self) -> CancellationToken {
        self.signal.clone()
    }
    fn prompt(&self, _prompt: AuthPrompt) -> BoxFuture<'_, Result<pi_js_compat::js_json::JsString, CallbackError>> {
        let answer = pi_js_compat::js_json::JsString::from(&self.answer);
        Box::pin(async move { Ok(answer) })
    }
    fn notify(&self, _event: AuthEvent) {}
}

#[tokio::test]
async fn runs_provider_login_and_logout_through_the_credential_store() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    struct LoginAuth;
    impl ApiKeyAuth for LoginAuth {
        fn name(&self) -> &str {
            "Login"
        }
        fn login(
            &self,
            _interaction: Arc<dyn AuthInteraction>,
        ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
            Some(Box::pin(async {
                Ok(ApiKeyCredential {
                    key: Some(("logged-in".to_owned()).into()),
                    env: None,
                })
            }))
        }
        fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async { Ok(None) })
        }
    }
    let models = models_with_credentials(Arc::clone(&credentials) as Arc<dyn CredentialStore>);
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")]).with_auth(ProviderAuth::api_key(Arc::new(LoginAuth))),
    ));

    let credential = models
        .login(
            "p1",
            AuthType::ApiKey,
            Arc::new(TestInteraction {
                signal: CancellationToken::new(),
                answer: "unused".to_owned(),
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        credential,
        Credential::ApiKey(ApiKeyCredential {
            key: Some(("logged-in".to_owned()).into()),
            env: None,
        })
    );
    assert_eq!(
        credentials.read("p1", AuthOperationOptions::default()).await.unwrap(),
        Some(credential)
    );
    models.logout("p1", Default::default()).await.unwrap();
    assert_eq!(
        credentials.read("p1", AuthOperationOptions::default()).await.unwrap(),
        None
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
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(models.get_auth("p1", Default::default()).await.unwrap(), None);
}

#[tokio::test]
async fn oauth_refresh_failure_returns_oauth_code_and_preserves_credential() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("old", 0.0))) })),
            AuthOperationOptions::default(),
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
    let error = models.get_auth("p1", Default::default()).await.unwrap_err();
    let pi_ai::auth::ResolveError::Models(error) = error else {
        panic!("expected ModelsError");
    };
    assert_eq!(error.code, ModelsErrorCode::Oauth);
    assert_eq!(
        error.message,
        "OAuth refresh failed for p1: token refresh failed (400): invalid_grant"
    );
    assert_eq!(
        credentials
            .read("p1", AuthOperationOptions::default())
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
async fn valid_oauth_tokens_do_not_touch_modify_and_short_lived_tokens_refresh() {
    let credentials = Arc::new(CountingStore {
        inner: InMemoryCredentialStore::new(),
        modifies: AtomicUsize::new(0),
    });
    credentials
        .inner
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("valid", now_ms() + 600_000.0))) })),
            AuthOperationOptions::default(),
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
            .get_auth("p1", Default::default())
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
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        models
            .get_auth("p1", Default::default())
            .await
            .unwrap()
            .unwrap()
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("new-token"))
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);

    credentials
        .inner
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("long", now_ms() + 600_000.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        models
            .get_auth(
                "p1",
                pi_ai::auth::AuthResolutionOverrides {
                    min_oauth_validity_ms: Some(1_800_000.0),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .unwrap()
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("new-token"))
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 2);
}

struct CountingStore {
    inner: InMemoryCredentialStore,
    modifies: AtomicUsize,
}

impl CredentialStore for CountingStore {
    fn read(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.inner.read(provider_id, options)
    }
    fn list(
        &self,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Vec<pi_ai::auth::CredentialInfo>, AuthError>> {
        self.inner.list(options)
    }
    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.modifies.fetch_add(1, Ordering::SeqCst);
        self.inner.modify(provider_id, modifier, options)
    }
    fn delete(&self, provider_id: &str, options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        self.inner.delete(provider_id, options)
    }
}

#[tokio::test]
async fn serializes_concurrent_oauth_refreshes() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth_credential("old", 0.0))) })),
            AuthOperationOptions::default(),
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
        models.get_auth("p1", Default::default()),
        models.get_auth("p1", Default::default())
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
        fn read(
            &self,
            _provider_id: &str,
            _options: AuthOperationOptions,
        ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
            Box::pin(async { Err(AuthError::failed("disk on fire")) })
        }
        fn list(
            &self,
            _options: AuthOperationOptions,
        ) -> BoxFuture<'_, Result<Vec<pi_ai::auth::CredentialInfo>, AuthError>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn modify(
            &self,
            _provider_id: &str,
            _modifier: CredentialModifier,
            _options: AuthOperationOptions,
        ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
            Box::pin(async { Ok(None) })
        }
        fn delete(&self, _provider_id: &str, _options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
    }
    let models = models_with_credentials(Arc::new(FailingStore));
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")])
            .with_auth(ProviderAuth::api_key(env_key_auth(Some("env-key")))),
    ));
    let error = models.get_auth("p1", Default::default()).await.unwrap_err();
    let pi_ai::auth::ResolveError::Models(error) = error else {
        panic!("expected ModelsError");
    };
    assert_eq!(error.code, ModelsErrorCode::Auth);

    struct FailingKey;
    impl ApiKeyAuth for FailingKey {
        fn name(&self) -> &str {
            "Failing"
        }
        fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async { Err(AuthError::failed("nope")) })
        }
    }
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")]).with_auth(ProviderAuth::api_key(Arc::new(FailingKey))),
    ));
    let error = models.get_auth("p1", Default::default()).await.unwrap_err();
    let pi_ai::auth::ResolveError::Models(error) = error else {
        panic!("expected ModelsError");
    };
    assert_eq!(error.code, ModelsErrorCode::Auth);
    assert_eq!(error.message, "API key auth failed for provider p1: nope");
}

#[tokio::test]
async fn stops_waiting_for_non_cooperative_auth_callbacks() {
    let started = Arc::new(Notify::new());
    struct BlockedAuth {
        started: Arc<Notify>,
    }
    impl ApiKeyAuth for BlockedAuth {
        fn name(&self) -> &str {
            "Blocked auth"
        }
        fn check(&self, _input: ApiKeyCheckInput) -> Option<BoxFuture<'static, Result<Option<AuthCheck>, AuthError>>> {
            let started = Arc::clone(&self.started);
            Some(Box::pin(async move {
                started.notify_one();
                std::future::pending::<()>().await;
                Ok(None)
            }))
        }
        fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            let started = Arc::clone(&self.started);
            Box::pin(async move {
                started.notify_one();
                std::future::pending::<()>().await;
                Ok(None)
            })
        }
    }
    let models = models();
    models.set_provider(provider(
        TestProvider::new("p1", vec![test_model("p1", "m1")]).with_auth(ProviderAuth::api_key(Arc::new(BlockedAuth {
            started: Arc::clone(&started),
        }))),
    ));

    let available_signal = CancellationToken::new();
    let available = models.get_available(
        None,
        AuthOperationOptions {
            signal: Some(available_signal.clone()),
        },
    );
    let available = tokio::spawn(available);
    started.notified().await;
    available_signal.cancel();
    assert_eq!(available.await.unwrap(), Err(pi_ai::auth::ResolveError::Aborted));

    let auth_signal = CancellationToken::new();
    let auth = models.get_auth(
        "p1",
        pi_ai::auth::AuthResolutionOverrides {
            signal: Some(auth_signal.clone()),
            ..Default::default()
        },
    );
    let auth = tokio::spawn(auth);
    started.notified().await;
    auth_signal.cancel();
    assert_eq!(auth.await.unwrap(), Err(pi_ai::auth::ResolveError::Aborted));
}

#[tokio::test]
async fn explicit_request_api_key_and_env_resolve_provider_auth() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    struct ScopedAuth;
    impl ApiKeyAuth for ScopedAuth {
        fn name(&self) -> &str {
            "Scoped"
        }
        fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            let ctx = input.ctx;
            let credential = input.credential;
            Box::pin(async move {
                let account = credential
                    .as_ref()
                    .and_then(|credential| credential.env.as_ref())
                    .and_then(|env| env.get("ACCOUNT_ID").cloned())
                    .or(ctx.env("ACCOUNT_ID").await);
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
                        base_url: Some((pi_js_compat::js_format!("https://example.test/{}", account)).into()),
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
async fn merges_resolved_auth_into_stream_options_and_applies_model_headers_once() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    struct HeaderAuth;
    impl ApiKeyAuth for HeaderAuth {
        fn name(&self) -> &str {
            "Test"
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

    let transforms = Arc::new(AtomicUsize::new(0));
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
                        transform_headers: Some({
                            let transforms = Arc::clone(&transforms);
                            Arc::new(move |headers| {
                                transforms.fetch_add(1, Ordering::SeqCst);
                                let mut headers = headers;
                                headers.insert("x-transformed".to_owned(), Some(("yes".to_owned()).into()));
                                Box::pin(async move { Ok(headers) })
                            })
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(transforms.load(Ordering::SeqCst), 1);
    let calls = calls.lock().unwrap();
    let headers = calls[1].options.request.headers.as_ref().unwrap();
    assert_eq!(
        headers.get("x-model").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("model"))
    );
    assert_eq!(
        headers.get("x-explicit").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("explicit"))
    );
    assert_eq!(
        headers.get("X-Shared").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("explicit"))
    );
    assert_eq!(
        headers.get("x-transformed").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("yes"))
    );
    assert!(calls[1].options.request.transform_headers.is_none());
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
    fn stream(&self, model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
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
        stream
    }
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
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
        fetch_models: None,
        filter_models: None,
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
        fn stream(&self, model: Model, _context: Context, options: StreamOptions) -> AssistantMessageEventStream {
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
            stream
        }
        fn stream_simple(
            &self,
            model: Model,
            context: Context,
            options: SimpleStreamOptions,
        ) -> AssistantMessageEventStream {
            self.stream(model, context, options.stream)
        }
    }
    let captured = Arc::new(Mutex::new(None));
    struct EnvAuth;
    impl ApiKeyAuth for EnvAuth {
        fn name(&self) -> &str {
            "Test"
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
        fetch_models: None,
        filter_models: None,
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

#[tokio::test]
async fn applies_resolved_options_to_deferred_fetch_and_cancellation() {
    struct DeferredStreams {
        fetched: Arc<Mutex<Option<(Model, DeferredFetchOptions)>>>,
        cancelled: Arc<Mutex<Option<ProviderRequestOptions>>>,
    }
    impl ProviderStreams for DeferredStreams {
        fn stream(&self, _model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
            create_assistant_message_event_stream().1
        }
        fn stream_simple(
            &self,
            _model: Model,
            _context: Context,
            _options: SimpleStreamOptions,
        ) -> AssistantMessageEventStream {
            create_assistant_message_event_stream().1
        }
        fn supports_fetch_deferred(&self) -> bool {
            true
        }
        fn fetch_deferred(
            &self,
            model: Model,
            _handle: DeferredHandle,
            options: DeferredFetchOptions,
        ) -> AssistantMessageEventStream {
            *self.fetched.lock().unwrap() = Some((model.clone(), options));
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
        fn supports_cancel_deferred(&self) -> bool {
            true
        }
        fn cancel_deferred(
            &self,
            _model: Model,
            _handle: DeferredHandle,
            options: DeferredCancelOptions,
        ) -> BoxFuture<'_, Result<(), CallbackError>> {
            *self.cancelled.lock().unwrap() = Some(options);
            Box::pin(async { Ok(()) })
        }
    }

    let fetched = Arc::new(Mutex::new(None));
    let cancelled = Arc::new(Mutex::new(None));
    struct DeferredAuth;
    impl ApiKeyAuth for DeferredAuth {
        fn name(&self) -> &str {
            "Test"
        }
        fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async {
                Ok(Some(AuthResult {
                    auth: ModelAuth {
                        api_key: Some(("provider-key".to_owned()).into()),
                        headers: Some(IndexMap::from([
                            ("Authorization".into(), Some("Bearer provider".into())),
                            ("X-Shared".into(), Some("provider".into())),
                        ])),
                        base_url: Some(("https://resolved.test/v1".to_owned()).into()),
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
        id: "deferred-provider".to_owned(),
        name: None,
        base_url: None,
        headers: None,
        auth: ProviderAuth::api_key(Arc::new(DeferredAuth)),
        models: vec![test_model("deferred-provider", "model-a")],
        fetch_models: None,
        filter_models: None,
        api: ProviderApi::Single(Arc::new(DeferredStreams {
            fetched: Arc::clone(&fetched),
            cancelled: Arc::clone(&cancelled),
        })),
    });
    let models = models();
    models.set_provider(provider);
    let model = test_model("deferred-provider", "model-a");
    let handle = DeferredHandle {
        provider: model.provider.clone(),
        model_id: model.id.clone(),
        api: model.api.clone(),
        id: "response-1".to_owned(),
        expires_at: None,
        poll_after_ms: None,
        data: None,
    };
    models
        .fetch_deferred(
            model.clone(),
            handle.clone(),
            DeferredFetchOptions {
                request: ProviderRequestOptions {
                    api_key: Some(("request-key".to_owned()).into()),
                    headers: Some(IndexMap::from([
                        ("X-Request".into(), Some("request".into())),
                        ("x-shared".into(), Some("request".into())),
                    ])),
                    env: Some(IndexMap::from([
                        ("REQUEST_ONLY".into(), "request".into()),
                        ("SHARED".into(), "request".into()),
                    ])),
                    transform_headers: Some(Arc::new(|mut headers| {
                        headers.insert("X-Transformed".to_owned(), Some(("yes".to_owned()).into()));
                        Box::pin(async move { Ok(headers) })
                    })),
                    ..Default::default()
                },
                wait: Some(50.0),
            },
        )
        .await
        .unwrap();
    {
        let fetched = fetched.lock().unwrap();
        let (fetched_model, options) = fetched.as_ref().unwrap();
        assert_eq!(fetched_model.base_url, "https://resolved.test/v1");
        assert_eq!(options.wait, Some(50.0));
        assert_eq!(
            options.request.api_key.as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("request-key"))
        );
        let headers = options.request.headers.as_ref().unwrap();
        assert_eq!(
            headers.get("Authorization").unwrap().as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("Bearer provider"))
        );
        assert_eq!(
            headers.get("X-Request").unwrap().as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("request"))
        );
        assert_eq!(
            headers.get("x-shared").unwrap().as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("request"))
        );
        assert_eq!(
            headers.get("X-Transformed").unwrap().as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("yes"))
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

    models
        .cancel_deferred(
            model,
            handle,
            ProviderRequestOptions {
                timeout_ms: Some(200.0),
                transform_headers: Some(Arc::new(|mut headers| {
                    headers.insert("X-Cancel".to_owned(), Some(("yes".to_owned()).into()));
                    Box::pin(async move { Ok(headers) })
                })),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let cancelled = cancelled.lock().unwrap();
    let options = cancelled.as_ref().unwrap();
    assert_eq!(options.timeout_ms, Some(200.0));
    assert_eq!(
        options.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("provider-key"))
    );
    let headers = options.headers.as_ref().unwrap();
    assert_eq!(
        headers.get("X-Shared").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("provider"))
    );
    assert_eq!(
        headers.get("X-Cancel").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("yes"))
    );
}

#[tokio::test]
async fn newer_dynamic_refresh_supersedes_older_network_work() {
    let fetches = Arc::new(AtomicUsize::new(0));
    let first_started = Arc::new(Notify::new());
    let release_first = Arc::new(Notify::new());
    let provider = create_provider(CreateProviderOptions {
        id: "dynamic".to_owned(),
        name: None,
        base_url: None,
        headers: None,
        auth: ambient_auth(),
        models: Vec::new(),
        fetch_models: Some({
            let fetches = Arc::clone(&fetches);
            let first_started = Arc::clone(&first_started);
            let release_first = Arc::clone(&release_first);
            Arc::new(move |_context: RefreshModelsContext| {
                let fetches = Arc::clone(&fetches);
                let first_started = Arc::clone(&first_started);
                let release_first = Arc::clone(&release_first);
                Box::pin(async move {
                    let current = fetches.fetch_add(1, Ordering::SeqCst) + 1;
                    if current == 1 {
                        first_started.notify_one();
                        release_first.notified().await;
                    }
                    Ok(vec![test_model("dynamic", &format!("listed-{current}"))])
                })
            })
        }),
        filter_models: None,
        api: ProviderApi::Single(Arc::new(EmptyStreams)),
    });
    let store = Arc::new(InMemoryModelsStore::new());
    let models = create_models(CreateModelsOptions {
        models_store: Some(Arc::clone(&store) as Arc<dyn ModelsStore>),
        ..Default::default()
    });
    models.set_provider(Arc::clone(&provider));
    assert!(provider.get_models().is_empty());

    let first = models.refresh(ModelsRefreshOptions {
        providers: Some(vec!["dynamic".to_owned()]),
        ..Default::default()
    });
    let first = tokio::spawn(first);
    first_started.notified().await;
    let second = models
        .refresh(ModelsRefreshOptions {
            providers: Some(vec!["dynamic".to_owned()]),
            ..Default::default()
        })
        .await;
    assert!(!second.aborted);
    first.await.unwrap();
    assert_eq!(fetches.load(Ordering::SeqCst), 2);
    assert_eq!(
        provider
            .get_models()
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["listed-2"]
    );
    assert_eq!(
        store
            .read("dynamic", ModelsStoreOperationOptions::default())
            .await
            .unwrap()
            .unwrap()
            .models[0]
            .id,
        "listed-2"
    );

    release_first.notify_one();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        provider
            .get_models()
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["listed-2"]
    );
}

#[test]
fn calculates_cost_with_request_wide_tiers() {
    let mut model = test_model("openai", "gpt-5.6-sol");
    model.cost = ModelCost {
        rates: ModelCostRates {
            input: 5.0,
            output: 30.0,
            cache_read: 0.5,
            cache_write: 6.25,
        },
        tiers: Some(vec![ModelCostTier {
            rates: ModelCostRates {
                input: 10.0,
                output: 45.0,
                cache_read: 1.0,
                cache_write: 12.5,
            },
            input_tokens_above: 272_000.0,
        }]),
    };
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
    assert_eq!(long.input, 2.0);
    assert_eq!(long.output, 4.5);
    assert!((long.cache_read - 0.072).abs() < 1e-12);
    assert!((long.cache_write - 0.0000125).abs() < 1e-15);
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

    model.thinking_level_map = Some(IndexMap::from([
        (ModelThinkingLevel::Xhigh, Some("x".to_owned())),
        (ModelThinkingLevel::Max, None),
        (ModelThinkingLevel::High, None),
    ]));
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
fn merges_headers_case_insensitively_and_compares_models() {
    let merged = merge_headers(
        Some(IndexMap::from([
            ("Authorization".into(), Some("Bearer".into())),
            ("x-a".into(), Some("a".into())),
        ])),
        Some(IndexMap::from([
            ("authorization".into(), Some("Explicit".into())),
            ("x-b".into(), None),
        ])),
    )
    .unwrap();
    assert_eq!(
        merged.get("authorization").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("Explicit"))
    );
    assert_eq!(
        merged.get("x-a").unwrap().as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("a"))
    );
    assert!(merged.contains_key("x-b"));
    assert!(!merged.contains_key("Authorization"));
    // Deleting a case-insensitive duplicate and re-inserting moves it to the end.
    let keys: Vec<&str> = merged.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["x-a", "authorization", "x-b"]);
    assert_eq!(merge_headers(None, None), None);

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
