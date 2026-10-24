#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::auth::{
    AuthContext, AuthError, AuthInteraction, AuthModel, AuthOperationOptions, Credential, CredentialInfo,
    CredentialModifier, CredentialStore, InMemoryCredentialStore, ModelAuth, OAuthAuth, OAuthCredential, ProviderAuth,
    resolve_provider_auth,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct ObservedStore {
    inner: InMemoryCredentialStore,
    reads: AtomicUsize,
    modifies: AtomicUsize,
    modify_failure: Option<pi_ai::auth::ModelsError>,
}
impl CredentialStore for ObservedStore {
    fn read(&self, id: &str, options: AuthOperationOptions) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.read(id, options)
    }
    fn list(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>> {
        self.inner.list(options)
    }
    fn modify(
        &self,
        id: &str,
        modifier: CredentialModifier,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.modifies.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = &self.modify_failure {
            let callback = error.clone().into_callback();
            return Box::pin(async move { Err(AuthError::from(callback)) });
        }
        self.inner.modify(id, modifier, options)
    }
    fn delete(&self, id: &str, options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        self.inner.delete(id, options)
    }
}
struct EmptyContext;
impl AuthContext for EmptyContext {
    fn env(&self, _: &str) -> BoxFuture<'_, Option<JsString>> {
        Box::pin(async { None })
    }
    fn file_exists(&self, _: &JsString) -> BoxFuture<'_, bool> {
        Box::pin(async { false })
    }
}
struct RefreshFixture {
    count: Arc<AtomicUsize>,
    expires: f64,
}
impl OAuthAuth for RefreshFixture {
    fn name(&self) -> &str {
        "Fixture"
    }
    fn login(&self, _: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        panic!("must not login")
    }
    fn refresh(
        &self,
        mut credential: OAuthCredential,
        _: AbortSignal,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        self.count.fetch_add(1, Ordering::SeqCst);
        credential.access = "new".into();
        credential.expires = self.expires;
        Box::pin(async move { Ok(credential) })
    }
    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
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
                Default::default(),
            )
            .await
            .unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let auth = ProviderAuth::oauth(Arc::new(RefreshFixture {
            count: count.clone(),
            expires: future_expiry,
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
        let stored = store.inner.read("p1", Default::default()).await.unwrap();
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
        modify_failure: Some(pi_ai::auth::ModelsError::new(
            pi_ai::auth::ModelsErrorCode::Auth,
            message.clone(),
        )),
        ..Default::default()
    });
    store
        .inner
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(Credential::OAuth(OAuthCredential::new("refresh", "old", 0.0)))) })),
            Default::default(),
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
            })),
            api: Arc::new(NeverGenerate(calls.clone())),
        },
    ));
    let mut source = oracle::TsOracle::spawn("auth-resolve-contract-oracle.ts", "packages/ai/src/auth/resolve.ts");
    let expected =
        source.request(&pi_js_compat::js_json!({"operation":"imageMessage","message":message,"model":model}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let before = pi_ai::request::now_millis();
    let result = models
        .generate_images(model, pi_ai::types::ImagesContext { input: vec![] }, Default::default())
        .await;
    let after = pi_ai::request::now_millis();
    assert!(result.timestamp >= before && result.timestamp <= after);
    let mut actual = pi_js_compat::js_json::to_value(&result).unwrap();
    actual
        .as_object_mut()
        .unwrap()
        .shift_remove(&JsString::from("timestamp"));
    assert_eq!(actual, expected["output"]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
