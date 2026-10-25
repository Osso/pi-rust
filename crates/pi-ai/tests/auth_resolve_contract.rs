#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::auth::{
    AuthContext, AuthError, AuthInteraction, AuthModel, Credential, CredentialModifier, CredentialStore,
    InMemoryCredentialStore, ModelAuth, OAuthAuth, OAuthCredential, ProviderAuth, resolve_provider_auth,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone)]
enum LockRecheck {
    LoggedOut,
    Replaced(Credential),
}

#[derive(Default)]
struct ObservedStore {
    inner: InMemoryCredentialStore,
    reads: AtomicUsize,
    modifies: AtomicUsize,
    read_failure: Option<AuthError>,
    modify_failure: Option<AuthError>,
    lock_recheck: Option<LockRecheck>,
    read_barrier: Option<Arc<tokio::sync::Barrier>>,
}
impl CredentialStore for ObservedStore {
    fn read(&self, id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.read_failure.clone() {
            return Box::pin(async move { Err(error) });
        }
        let id = id.to_owned();
        Box::pin(async move {
            let current = self.inner.read(&id).await?;
            if let Some(barrier) = &self.read_barrier {
                barrier.wait().await;
            }
            Ok(current)
        })
    }

    fn modify(&self, id: &str, modifier: CredentialModifier) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.modifies.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.modify_failure.clone() {
            return Box::pin(async move { Err(error) });
        }
        let id = id.to_owned();
        Box::pin(async move {
            match self.lock_recheck.clone() {
                Some(LockRecheck::LoggedOut) => self.inner.delete(&id).await?,
                Some(LockRecheck::Replaced(credential)) => {
                    self.inner
                        .modify(
                            &id,
                            Arc::new(move |_| {
                                let credential = credential.clone();
                                Box::pin(async move { Ok(Some(credential)) })
                            }),
                        )
                        .await?;
                }
                None => {}
            }
            self.inner.modify(&id, modifier).await
        })
    }
    fn delete(&self, id: &str) -> BoxFuture<'_, Result<(), AuthError>> {
        self.inner.delete(id)
    }
}
struct EmptyContext;
impl AuthContext for EmptyContext {
    fn env(&self, _: &str) -> BoxFuture<'_, Result<Option<JsString>, pi_ai::auth::AuthError>> {
        Box::pin(async { Ok(None) })
    }
    fn file_exists(&self, _: &JsString) -> BoxFuture<'_, Result<bool, pi_ai::auth::AuthError>> {
        Box::pin(async { Ok(false) })
    }
}
#[derive(Default)]
struct RefreshFixture {
    count: Arc<AtomicUsize>,
    expires: f64,
    refresh_failure: Option<AuthError>,
    derive_failure: Option<AuthError>,
    refresh_delay_ms: u64,
}
impl OAuthAuth for RefreshFixture {
    fn name(&self) -> JsString {
        "Fixture".into()
    }
    fn login(&self, _: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        panic!("must not login")
    }
    fn refresh(&self, mut credential: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        self.count.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.refresh_failure.clone() {
            return Box::pin(async move { Err(error) });
        }
        credential.access = "new".into();
        credential.expires = self.expires;
        let delay = std::time::Duration::from_millis(self.refresh_delay_ms);
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            Ok(credential)
        })
    }
    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        if let Some(error) = self.derive_failure.clone() {
            return Box::pin(async move { Err(error) });
        }
        Box::pin(async move {
            Ok(ModelAuth {
                api_key: Some(credential.access),
                ..Default::default()
            })
        })
    }
}
#[tokio::test]
async fn oauth_refreshes_only_after_expiry_not_within_a_minimum_validity_window() {
    let mut oracle = oracle::TsOracle::spawn("auth-resolve-contract-oracle.ts", "packages/ai/src/auth/resolve.ts");
    let future_expiry = pi_ai::request::now_millis() as f64 + 60_000.0;
    for expires in [future_expiry, 0.0] {
        let store = Arc::new(ObservedStore::default());
        let credential = Credential::OAuth(OAuthCredential::new("refresh", "old", expires));
        store
            .inner
            .modify(
                "p1",
                Arc::new(move |_| {
                    let credential = credential.clone();
                    Box::pin(async move { Ok(Some(credential)) })
                }),
            )
            .await
            .unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let auth = ProviderAuth::oauth(Arc::new(RefreshFixture {
            count: count.clone(),
            expires: future_expiry,
            ..Default::default()
        }));
        let mut model = pi_ai::providers::all::get_builtin_model("openai", "gpt-5").unwrap();
        model.provider = "p1".into();
        let source =
            oracle.request(&pi_js_compat::js_json!({"expires":expires,"refreshedExpires":future_expiry,"model":model}));
        assert_eq!(source["ok"], JsValue::Bool(true), "{source:?}");
        let result = resolve_provider_auth(
            "p1",
            &auth,
            AuthModel::Chat(model),
            store.clone(),
            Arc::new(EmptyContext),
            Default::default(),
        )
        .await
        .unwrap();
        let stored = store.inner.read("p1").await.unwrap();
        let actual = pi_js_compat::js_json!({"result":result,"reads":store.reads.load(Ordering::SeqCst),
            "modifies":store.modifies.load(Ordering::SeqCst),"refreshes":count.load(Ordering::SeqCst),"stored":stored});
        assert_eq!(actual, source["output"], "expires={expires}");
    }
}

struct NeverGenerate(Arc<AtomicUsize>);
impl pi_ai::request::ProviderImages for NeverGenerate {
    fn generate_images(
        &self,
        _: pi_ai::types::ImagesModel,
        _: pi_ai::types::ImagesContext,
        _: pi_ai::request::ImagesOptions,
    ) -> BoxFuture<'static, Result<pi_ai::types::AssistantImages, pi_ai::request::CallbackError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(pi_ai::request::CallbackError::new("must not generate")) })
    }
}

#[tokio::test]
async fn image_auth_failure_reads_the_exact_native_models_error_message() {
    use pi_ai::images_models::{ImagesModels, MutableImagesModels};
    let message = JsString::from_utf16(vec![78, 0xd800, 77, 0xdc00]);
    let store = Arc::new(ObservedStore {
        modify_failure: Some(AuthError::from(
            pi_ai::auth::ModelsError::new(pi_ai::auth::ModelsErrorCode::Auth, message.clone()).into_callback(),
        )),
        ..Default::default()
    });
    store
        .inner
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(Credential::OAuth(OAuthCredential::new("refresh", "old", 0.0)))) })),
        )
        .await
        .unwrap();
    let models = pi_ai::images_models::create_images_models(pi_ai::models::CreateModelsOptions {
        credentials: Some(store),
        auth_context: Some(Arc::new(EmptyContext)),
    });
    let model: pi_ai::types::ImagesModel = serde_json::from_value(serde_json::json!({
        "id":"image","name":"Image","api":"fixture","provider":"p1","baseUrl":"https://example.test/v1",
        "input":["text"],"output":["image"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}
    }))
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    models.set_provider(pi_ai::images_models::create_images_provider(
        pi_ai::images_models::CreateImagesProviderOptions {
            id: "p1".into(),
            name: None,
            models: vec![model.clone()],
            refresh_models: None,
            auth: ProviderAuth::oauth(Arc::new(RefreshFixture {
                count: Arc::new(AtomicUsize::new(0)),
                expires: 0.0,
                ..Default::default()
            })),
            api: Arc::new(NeverGenerate(calls.clone())),
        },
    ));
    let mut source = oracle::TsOracle::spawn("auth-resolve-contract-oracle.ts", "packages/ai/src/auth/resolve.ts");
    let expected =
        source.request(&pi_js_compat::js_json!({"operation":"imageMessage","message":message,"model":model}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let before = pi_ai::request::now_millis() as f64;
    let result = models
        .generate_images(model, pi_ai::types::ImagesContext { input: vec![] }, Default::default())
        .await;
    let after = pi_ai::request::now_millis() as f64;
    assert!(result.timestamp >= before && result.timestamp <= after);
    let mut actual = pi_js_compat::js_json::to_value(&result).unwrap();
    actual
        .as_object_mut()
        .unwrap()
        .shift_remove(&JsString::from("timestamp"));
    assert_eq!(actual, expected["output"]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

async fn seed_credential(store: &InMemoryCredentialStore, credential: Credential) {
    store
        .modify(
            "p1",
            Arc::new(move |_| {
                let credential = credential.clone();
                Box::pin(async move { Ok(Some(credential)) })
            }),
        )
        .await
        .unwrap();
}
fn requested_model() -> pi_ai::types::Model {
    let mut model = pi_ai::providers::all::get_builtin_model("openai", "gpt-5").unwrap();
    model.provider = "model-owner".into();
    model.id = "requested-model".into();
    model
}
async fn observe_resolution(
    store: &ObservedStore,
    refreshes: &AtomicUsize,
    result: Result<Option<pi_ai::auth::AuthResult>, pi_ai::auth::ModelsError>,
) -> JsValue {
    let mut output = pi_js_compat::js_json::JsObject::new();
    match result {
        Ok(Some(result)) => {
            output.insert("result".into(), pi_js_compat::js_json::to_value(&result).unwrap());
        }
        Ok(None) => {}
        Err(error) => {
            output.insert(
                "error".into(),
                pi_js_compat::js_json!({"kind":"error", "value":error.into_callback().thrown_value().value()}),
            );
        }
    }
    output.insert(
        "reads".into(),
        JsValue::Number(store.reads.load(Ordering::SeqCst) as f64),
    );
    output.insert(
        "modifies".into(),
        JsValue::Number(store.modifies.load(Ordering::SeqCst) as f64),
    );
    output.insert(
        "refreshes".into(),
        JsValue::Number(refreshes.load(Ordering::SeqCst) as f64),
    );
    if let Some(stored) = store.inner.read("p1").await.unwrap() {
        output.insert("stored".into(), pi_js_compat::js_json::to_value(&stored).unwrap());
    }
    JsValue::Object(output)
}

#[tokio::test]
async fn oauth_authoritative_recheck_accepts_other_refresh_logout_and_type_change() {
    let mut source = oracle::TsOracle::spawn("auth-resolve-contract-oracle.ts", "packages/ai/src/auth/resolve.ts");
    let future = pi_ai::request::now_millis() as f64 + 60_000.0;
    let cases = [
        (
            "refreshed",
            LockRecheck::Replaced(Credential::OAuth(OAuthCredential::new("refresh", "elsewhere", future))),
        ),
        ("loggedOut", LockRecheck::LoggedOut),
        (
            "apiKey",
            LockRecheck::Replaced(Credential::ApiKey(pi_ai::auth::ApiKeyCredential {
                key: Some("changed".into()),
                env: None,
            })),
        ),
    ];
    for (name, recheck) in cases {
        let store = Arc::new(ObservedStore {
            lock_recheck: Some(recheck),
            ..Default::default()
        });
        seed_credential(
            &store.inner,
            Credential::OAuth(OAuthCredential::new("refresh", "old", 0.0)),
        )
        .await;
        let refreshes = Arc::new(AtomicUsize::new(0));
        let auth = ProviderAuth::oauth(Arc::new(RefreshFixture {
            count: refreshes.clone(),
            expires: future,
            ..Default::default()
        }));
        let model = requested_model();
        let expected = source
            .request(&pi_js_compat::js_json!({"expires":0,"refreshedExpires":future,"model":model,"lockState":name}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let result = resolve_provider_auth(
            "p1",
            &auth,
            AuthModel::Chat(model),
            store.clone(),
            Arc::new(EmptyContext),
            Default::default(),
        )
        .await;
        assert_eq!(
            observe_resolution(&store, &refreshes, result).await,
            expected["output"],
            "{name}"
        );
    }
}

struct FailureApiKey(AuthError);
impl pi_ai::auth::ApiKeyAuth for FailureApiKey {
    fn name(&self) -> JsString {
        "Fixture".into()
    }
    fn resolve(
        &self,
        _: pi_ai::auth::ApiKeyResolveInput,
    ) -> BoxFuture<'static, Result<Option<pi_ai::auth::AuthResult>, AuthError>> {
        let error = self.0.clone();
        Box::pin(async move { Err(error) })
    }
}
fn failure_cases() -> Vec<(AuthError, JsValue)> {
    use pi_ai::request::CallbackError;
    use pi_js_compat::abort::{AbortController, AbortReason};
    use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
    let raw = JsString::from_utf16(vec![69, 0xd800, 0xdc00, 0xdc00]);
    let mut cases: Vec<_> = [
        JsValue::Bool(false),
        JsValue::Null,
        JsValue::String(raw.clone()),
        pi_js_compat::js_json!({"toString":0,"raw":raw}),
    ]
    .into_iter()
    .map(|value| {
        (
            AuthError::from(CallbackError::from_rejection(AbortReason::Value(value.clone()))),
            pi_js_compat::js_json!({"kind":"raw","value":value}),
        )
    })
    .collect();
    let native = NativeNodeError::from_constructor(NativeErrorClass::TypeError, raw.clone())
        .with_property("code", JsValue::String("E_TEST".into()))
        .with_cause(AbortReason::Value(JsValue::Bool(false)));
    cases.push((
        AuthError::from(CallbackError::from(native)),
        pi_js_compat::js_json!({"kind":"native","message":raw}),
    ));
    let existing = pi_ai::auth::ModelsError::with_cause(
        pi_ai::auth::ModelsErrorCode::Stream,
        raw.clone(),
        Some(Box::new(AbortReason::Value(JsValue::Bool(false)))),
    );
    cases.push((
        AuthError::from(existing.clone().into_callback()),
        pi_js_compat::js_json!({"kind":"models","message":raw}),
    ));
    let spoof = NativeNodeError::new("ModelsError", raw.clone())
        .with_property("code", JsValue::String("stream".into()))
        .with_cause(AbortReason::Value(JsValue::Bool(false)));
    cases.push((
        AuthError::from(CallbackError::from(spoof)),
        pi_js_compat::js_json!({"kind":"spoof","message":raw}),
    ));
    let nested = NativeNodeError::new("Error", "outer").with_cause(existing.into_callback().thrown_value());
    cases.push((
        AuthError::from(CallbackError::from(nested)),
        pi_js_compat::js_json!({"kind":"nested","message":raw}),
    ));
    let controller = AbortController::new();
    controller.abort(None);
    cases.push((
        AuthError::from_abort_reason(controller.signal().reason().unwrap()),
        pi_js_compat::js_json!({"kind":"defaultAbort"}),
    ));
    let controller = AbortController::new();
    controller.abort(Some(AbortReason::Value(JsValue::Bool(false))));
    cases.push((
        AuthError::from_abort_reason(controller.signal().reason().unwrap()),
        pi_js_compat::js_json!({"kind":"abortedFalse"}),
    ));
    cases
}

#[tokio::test]
async fn every_auth_catch_preserves_raw_native_brands_causes_and_existing_error_rules() {
    let mut source = oracle::TsOracle::spawn("auth-resolve-contract-oracle.ts", "packages/ai/src/auth/resolve.ts");
    let future = pi_ai::request::now_millis() as f64 + 60_000.0;
    for boundary in ["read", "modify", "refresh", "derive", "api"] {
        for (failure, descriptor) in failure_cases() {
            let mut store = ObservedStore::default();
            match boundary {
                "read" => store.read_failure = Some(failure.clone()),
                "modify" => store.modify_failure = Some(failure.clone()),
                _ => {}
            }
            let store = Arc::new(store);
            let expires = if boundary == "derive" { future } else { 0.0 };
            let credential = if boundary == "api" {
                Credential::ApiKey(pi_ai::auth::ApiKeyCredential {
                    key: Some("stored".into()),
                    env: None,
                })
            } else {
                Credential::OAuth(OAuthCredential::new("refresh", "old", expires))
            };
            seed_credential(&store.inner, credential).await;
            let refreshes = Arc::new(AtomicUsize::new(0));
            let mut oauth = RefreshFixture {
                count: refreshes.clone(),
                expires: future,
                ..Default::default()
            };
            match boundary {
                "refresh" => oauth.refresh_failure = Some(failure.clone()),
                "derive" => oauth.derive_failure = Some(failure.clone()),
                _ => {}
            }
            let auth = if boundary == "api" {
                ProviderAuth::api_key(Arc::new(FailureApiKey(failure)))
            } else {
                ProviderAuth::oauth(Arc::new(oauth))
            };
            let model = requested_model();
            let expected = source.request(&pi_js_compat::js_json!({"expires":expires,"refreshedExpires":future,
                "model":model,"boundary":boundary,"failure":descriptor}));
            assert_eq!(expected["ok"], JsValue::Bool(true), "{boundary}: {expected:?}");
            let result = resolve_provider_auth(
                "p1",
                &auth,
                AuthModel::Chat(model),
                store.clone(),
                Arc::new(EmptyContext),
                Default::default(),
            )
            .await;
            assert_eq!(
                observe_resolution(&store, &refreshes, result).await,
                expected["output"],
                "{boundary}: {descriptor:?}"
            );
        }
    }
}

#[tokio::test]
async fn simultaneous_expired_reads_refresh_once_under_the_real_store_lock() {
    let store = Arc::new(ObservedStore {
        read_barrier: Some(Arc::new(tokio::sync::Barrier::new(2))),
        ..Default::default()
    });
    seed_credential(
        &store.inner,
        Credential::OAuth(OAuthCredential::new("refresh", "old", 0.0)),
    )
    .await;
    let future = pi_ai::request::now_millis() as f64 + 60_000.0;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let auth = ProviderAuth::oauth(Arc::new(RefreshFixture {
        count: refreshes.clone(),
        expires: future,
        ..Default::default()
    }));
    let model = requested_model();
    let mut source = oracle::TsOracle::spawn("auth-resolve-contract-oracle.ts", "packages/ai/src/auth/resolve.ts");
    let expected = source
        .request(&pi_js_compat::js_json!({"expires":0,"refreshedExpires":future,"model":model,"concurrent":true}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let first = resolve_provider_auth(
        "p1",
        &auth,
        AuthModel::Chat(model.clone()),
        store.clone(),
        Arc::new(EmptyContext),
        Default::default(),
    );
    let second = resolve_provider_auth(
        "p1",
        &auth,
        AuthModel::Chat(model),
        store.clone(),
        Arc::new(EmptyContext),
        Default::default(),
    );
    let (first, second) = tokio::join!(first, second);
    let mut actual = observe_resolution(&store, &refreshes, Ok(None)).await;
    actual.as_object_mut().unwrap().insert(
        "result".into(),
        pi_js_compat::js_json!([first.unwrap(), second.unwrap()]),
    );
    assert_eq!(actual, expected["output"]);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
}

#[derive(Default)]
struct RoutingContext {
    env: pi_ai::types::ProviderEnv,
    env_reads: std::sync::Mutex<Vec<String>>,
    file_reads: std::sync::Mutex<Vec<JsString>>,
    expected_path: JsString,
}
impl AuthContext for RoutingContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Result<Option<JsString>, pi_ai::auth::AuthError>> {
        self.env_reads.lock().unwrap().push(name.to_owned());
        let value = self.env.get(name).cloned();
        Box::pin(async move { Ok(value) })
    }
    fn file_exists(&self, path: &JsString) -> BoxFuture<'_, Result<bool, pi_ai::auth::AuthError>> {
        self.file_reads.lock().unwrap().push(path.clone());
        let exists = path == &self.expected_path;
        Box::pin(async move { Ok(exists) })
    }
}
struct InspectApiKey;
impl pi_ai::auth::ApiKeyAuth for InspectApiKey {
    fn name(&self) -> JsString {
        "Fixture".into()
    }
    fn resolve(
        &self,
        input: pi_ai::auth::ApiKeyResolveInput,
    ) -> BoxFuture<'static, Result<Option<pi_ai::auth::AuthResult>, AuthError>> {
        Box::pin(async move {
            let AuthModel::Chat(model) = input.model else {
                panic!("routing fixture requires actual chat model");
            };
            let key = match input.credential.as_ref().and_then(|credential| credential.key.clone()) {
                Some(key) => Some(key),
                None => input.ctx.env("MAIN").await?,
            };
            let extra = input.ctx.env("EXTRA").await?.unwrap_or_else(|| "<missing>".into());
            let exists = input.ctx.file_exists(&model.base_url).await?;
            let mut headers = pi_ai::types::ProviderHeaders::from([
                ("x-model".into(), Some(model.id.into())),
                ("x-name".into(), Some(model.name)),
                ("x-base".into(), Some(model.base_url)),
                ("x-extra".into(), Some(extra)),
                ("x-file".into(), Some(if exists { "yes".into() } else { "no".into() })),
            ]);
            if let Some(credential) = &input.credential {
                let value = pi_js_compat::js_json::to_value(&Credential::ApiKey(credential.clone())).unwrap();
                headers.insert(
                    "x-credential".into(),
                    Some(pi_js_compat::js_json::stringify_lossless(&value).into()),
                );
            }
            Ok(Some(pi_ai::auth::AuthResult {
                auth: ModelAuth {
                    api_key: key,
                    headers: Some(headers),
                    base_url: None,
                },
                env: input.credential.and_then(|credential| credential.env),
                source: Some("Fixture".into()),
            }))
        })
    }
}
fn overrides_snapshot(overrides: &pi_ai::auth::AuthResolutionOverrides) -> JsValue {
    let mut value = pi_js_compat::js_json::JsObject::new();
    if let Some(key) = &overrides.api_key {
        value.insert("apiKey".into(), JsValue::String(key.clone()));
    }
    if let Some(env) = &overrides.env {
        value.insert("env".into(), pi_js_compat::js_json::to_value(env).unwrap());
    }
    JsValue::Object(value)
}
fn routing_cases(
    future: f64,
) -> Vec<(
    &'static str,
    Option<Credential>,
    pi_ai::auth::AuthResolutionOverrides,
    ProviderAuth,
)> {
    use pi_ai::auth::{ApiKeyCredential, AuthResolutionOverrides};
    let raw = JsString::from_utf16(vec![86, 0xd800]);
    let api: Arc<dyn pi_ai::auth::ApiKeyAuth> = Arc::new(InspectApiKey);
    let oauth: Arc<dyn OAuthAuth> = Arc::new(RefreshFixture::default());
    let both = ProviderAuth {
        api_key: Some(api.clone()),
        oauth: Some(oauth.clone()),
    };
    let stored_oauth = Credential::OAuth(OAuthCredential::new("refresh", raw.clone(), future));
    let stored = Credential::ApiKey(ApiKeyCredential {
        key: Some("stored".into()),
        env: Some(pi_ai::types::ProviderEnv::from([
            ("MAIN".into(), "stored-env".into()),
            ("EXTRA".into(), "old".into()),
            ("KEEP".into(), raw.clone()),
            ("2".into(), raw.clone()),
        ])),
    });
    vec![
        ("ambient", None, Default::default(), both.clone()),
        (
            "explicit-empty",
            Some(stored_oauth.clone()),
            AuthResolutionOverrides {
                api_key: Some("".into()),
                env: Some(pi_ai::types::ProviderEnv::from([
                    ("MAIN".into(), raw.clone()),
                    ("EXTRA".into(), raw.clone()),
                ])),
            },
            both.clone(),
        ),
        (
            "stored-merged",
            Some(stored),
            AuthResolutionOverrides {
                api_key: None,
                env: Some(pi_ai::types::ProviderEnv::from([
                    ("EXTRA".into(), raw.clone()),
                    ("ADD".into(), raw.clone()),
                    ("1".into(), raw.clone()),
                ])),
            },
            both.clone(),
        ),
        (
            "empty-overlay-falls-back",
            Some(Credential::ApiKey(ApiKeyCredential {
                key: None,
                env: Some(pi_ai::types::ProviderEnv::from([("EXTRA".into(), raw.clone())])),
            })),
            AuthResolutionOverrides {
                api_key: None,
                env: Some(pi_ai::types::ProviderEnv::from([
                    ("MAIN".into(), "".into()),
                    ("EXTRA".into(), "".into()),
                ])),
            },
            both.clone(),
        ),
        (
            "empty-env-map",
            Some(Credential::ApiKey(ApiKeyCredential::default())),
            AuthResolutionOverrides {
                api_key: None,
                env: Some(Default::default()),
            },
            both.clone(),
        ),
        (
            "stored-oauth-no-handler",
            Some(stored_oauth.clone()),
            Default::default(),
            ProviderAuth::api_key(api),
        ),
        (
            "stored-apikey-no-handler",
            Some(Credential::ApiKey(ApiKeyCredential {
                key: Some("stored".into()),
                env: None,
            })),
            Default::default(),
            ProviderAuth::oauth(oauth.clone()),
        ),
        (
            "override-without-api-handler",
            Some(stored_oauth),
            AuthResolutionOverrides {
                api_key: Some("ignored".into()),
                env: Some(pi_ai::types::ProviderEnv::from([("EXTRA".into(), raw.clone())])),
            },
            ProviderAuth::oauth(oauth),
        ),
        (
            "no-handlers",
            None,
            AuthResolutionOverrides {
                api_key: Some("ignored".into()),
                env: Some(Default::default()),
            },
            ProviderAuth::default(),
        ),
        (
            "stored-empty-key",
            Some(Credential::ApiKey(ApiKeyCredential {
                key: Some("".into()),
                env: None,
            })),
            Default::default(),
            both.clone(),
        ),
        (
            "ambient-empty-overlay",
            None,
            AuthResolutionOverrides {
                api_key: None,
                env: Some(pi_ai::types::ProviderEnv::from([
                    ("MAIN".into(), "".into()),
                    ("EXTRA".into(), raw),
                ])),
            },
            both.clone(),
        ),
        (
            "explicit-no-env",
            None,
            AuthResolutionOverrides {
                api_key: Some("explicit".into()),
                env: None,
            },
            both,
        ),
    ]
}

#[tokio::test]
async fn dispatch_env_overlay_and_file_delegation_match_source_with_exact_utf16_values() {
    let mut source = oracle::TsOracle::spawn("auth-resolve-contract-oracle.ts", "packages/ai/src/auth/resolve.ts");
    let future = pi_ai::request::now_millis() as f64 + 60_000.0;
    for (name, credential, overrides, auth) in routing_cases(future) {
        let store = Arc::new(ObservedStore::default());
        if let Some(credential) = &credential {
            seed_credential(&store.inner, credential.clone()).await;
        }
        let mut model = requested_model();
        model.name = JsString::from_utf16(vec![77, 0xd800]);
        model.base_url = JsString::from_utf16(vec![80, 0xdc00]);
        let context = Arc::new(RoutingContext {
            expected_path: model.base_url.clone(),
            env: pi_ai::types::ProviderEnv::from([
                ("MAIN".into(), JsString::from_utf16(vec![66, 0xdc00])),
                ("EXTRA".into(), JsString::from_utf16(vec![69, 0xd800])),
            ]),
            ..Default::default()
        });
        let expected = source.request(&pi_js_compat::js_json!({"operation":"routing","credential":credential,
            "overrides":overrides_snapshot(&overrides),"apiKeyHandler":auth.api_key.is_some(),"oauthHandler":auth.oauth.is_some(),
            "model":model,"baseEnv":context.env}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{name}: {expected:?}");
        let result = resolve_provider_auth(
            "p1",
            &auth,
            AuthModel::Chat(model),
            store.clone(),
            context.clone(),
            overrides,
        )
        .await;
        let mut actual = observe_resolution(&store, &AtomicUsize::new(0), result).await;
        let output = actual.as_object_mut().unwrap();
        output.insert(
            "envReads".into(),
            pi_js_compat::js_json::to_value(&*context.env_reads.lock().unwrap()).unwrap(),
        );
        output.insert(
            "fileReads".into(),
            pi_js_compat::js_json::to_value(&*context.file_reads.lock().unwrap()).unwrap(),
        );
        assert_eq!(actual, expected["output"], "{name}");
    }
}

#[tokio::test]
async fn refresh_beyond_the_retired_fifteen_second_deadline_still_persists_and_resolves() {
    const REFRESH_DELAY_MS: u64 = 16_000;
    let future = pi_ai::request::now_millis() as f64 + 120_000.0;
    let store = Arc::new(ObservedStore::default());
    seed_credential(
        &store.inner,
        Credential::OAuth(OAuthCredential::new("refresh", "old", 0.0)),
    )
    .await;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let auth = ProviderAuth::oauth(Arc::new(RefreshFixture {
        count: refreshes.clone(),
        expires: future,
        refresh_delay_ms: REFRESH_DELAY_MS,
        ..Default::default()
    }));
    let model = requested_model();
    let request =
        pi_js_compat::js_json!({"expires":0,"refreshedExpires":future,"model":model,"refreshDelayMs":REFRESH_DELAY_MS});
    let source = tokio::task::spawn_blocking(move || {
        let mut source = oracle::TsOracle::spawn("auth-resolve-contract-oracle.ts", "packages/ai/src/auth/resolve.ts");
        source.request(&request)
    });
    let result = resolve_provider_auth(
        "p1",
        &auth,
        AuthModel::Chat(model),
        store.clone(),
        Arc::new(EmptyContext),
        Default::default(),
    )
    .await;
    let expected = source.await.unwrap();
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(observe_resolution(&store, &refreshes, result).await, expected["output"]);
}
