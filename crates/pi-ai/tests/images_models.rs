use indexmap::IndexMap;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_ai::auth::{
    ApiKeyAuth, ApiKeyResolveInput, AuthContext, AuthError, AuthResult, ModelAuth, ProviderAuth, env_api_key_auth,
};
use pi_ai::images_models::{
    CreateImagesProviderOptions, ImagesModels, MutableImagesModels, create_images_models, create_images_provider,
};
use pi_ai::request::{BoxFuture, CallbackError, ImagesOptions, ImagesRequestOptions, ProviderImages};
use pi_ai::types::{AssistantImages, ImagesContext, ImagesModel, ImagesStopReason, Modality, TextOrImageContent};
use serde_json::json;

fn test_model(provider: &str, id: &str) -> ImagesModel {
    ImagesModel {
        properties: Default::default(),
        id: id.to_owned(),
        name: id.into(),
        api: "test-images".to_owned(),
        provider: provider.to_owned(),
        base_url: ("https://example.test/v1".to_owned()).into(),
        thinking_level_map: None,
        input: vec![Modality::Text],
        output: vec![Modality::Image],
        cost: Default::default(),
        auto_compaction_threshold: None,
        headers: None,
    }
}

fn context() -> ImagesContext {
    ImagesContext {
        input: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new("a red circle"))],
    }
}

fn ok_result(model: &ImagesModel) -> AssistantImages {
    AssistantImages {
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        output: vec![TextOrImageContent::Image(pi_ai::types::ImageContent::new(
            "aGk=",
            "image/png",
        ))],
        response_id: None,
        usage: None,
        stop_reason: ImagesStopReason::Stop,
        error_message: None,
        timestamp: 1.0,
    }
}

struct FakeAuthContext {
    env: IndexMap<String, String>,
}

impl AuthContext for FakeAuthContext {
    fn env(
        &self,
        name: &str,
    ) -> BoxFuture<'_, Result<Option<pi_js_compat::js_json::JsString>, pi_ai::auth::AuthError>> {
        let value = self.env.get(name).cloned();
        Box::pin(async move { Ok(value.map(pi_js_compat::js_json::JsString::from)) })
    }

    fn file_exists(
        &self,
        _path: &pi_js_compat::js_json::JsString,
    ) -> BoxFuture<'_, Result<bool, pi_ai::auth::AuthError>> {
        Box::pin(async { Ok(false) })
    }
}

fn fake_auth_context(pairs: &[(&str, &str)]) -> Arc<dyn AuthContext> {
    Arc::new(FakeAuthContext {
        env: pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    })
}

struct StaticAuth {
    name: String,
    result: Option<AuthResult>,
}

impl ApiKeyAuth for StaticAuth {
    fn name(&self) -> JsString {
        self.name.clone().into()
    }

    fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        let result = self.result.clone();
        Box::pin(async move { Ok(result) })
    }
}

fn static_auth(result: Option<AuthResult>) -> Arc<dyn ApiKeyAuth> {
    Arc::new(StaticAuth {
        name: "Test key".to_owned(),
        result,
    })
}

#[derive(Clone)]
struct GenerateCall {
    model: ImagesModel,
    options: ImagesOptions,
}

struct TestApi {
    calls: Arc<Mutex<Vec<GenerateCall>>>,
}

impl ProviderImages for TestApi {
    fn generate_images(
        &self,
        model: ImagesModel,
        _context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'_, Result<AssistantImages, CallbackError>> {
        self.calls.lock().expect("calls lock").push(GenerateCall {
            model: model.clone(),
            options,
        });
        Box::pin(async move { Ok(ok_result(&model)) })
    }
}

fn test_provider(
    id: &str,
    models: Vec<ImagesModel>,
    env_var: Option<&str>,
    calls: Option<Arc<Mutex<Vec<GenerateCall>>>>,
) -> Arc<dyn pi_ai::images_models::ImagesProvider> {
    let auth = match env_var {
        Some(name) => ProviderAuth::api_key(Arc::new(env_api_key_auth("Test key", &[name]))),
        None => ProviderAuth::api_key(static_auth(Some(AuthResult {
            auth: ModelAuth::default(),
            env: None,
            source: None,
        }))),
    };
    create_images_provider(CreateImagesProviderOptions {
        id: id.to_owned(),
        name: None,
        auth,
        models,
        refresh_models: None,
        api: Arc::new(TestApi {
            calls: calls.unwrap_or_else(|| Arc::new(Mutex::new(Vec::new()))),
        }),
    })
}

#[path = "support/oracle.rs"]
mod oracle;

#[tokio::test]
async fn image_request_auth_receives_the_actual_image_model() {
    struct ImageAwareAuth;
    impl ApiKeyAuth for ImageAwareAuth {
        fn name(&self) -> JsString {
            "image-aware".into()
        }
        fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
            Box::pin(async move {
                let pi_ai::auth::AuthModel::Images(model) = input.model else {
                    panic!("image request model")
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
    let calls = Arc::new(Mutex::new(Vec::new()));
    let models = create_images_models(Default::default());
    models.set_provider(create_images_provider(CreateImagesProviderOptions {
        id: "p1".into(),
        name: None,
        auth: ProviderAuth::api_key(Arc::new(ImageAwareAuth)),
        models: vec![test_model("p1", "first")],
        refresh_models: None,
        api: Arc::new(TestApi {
            calls: Arc::clone(&calls),
        }),
    }));
    let model = test_model("p1", "actual-image");
    let auth = models.get_auth(&model).await.unwrap().unwrap();
    let mut oracle = oracle::TsOracle::spawn("auth-request-model-oracle.ts", "packages/ai/src/auth/resolve.ts");
    assert_eq!(
        serde_json::to_value(&auth).unwrap(),
        oracle.request(&json!({"model": model}))
    );
    let result = models
        .generate_images(model, context(), Default::default())
        .await
        .expect("image generation");
    assert_eq!(result.stop_reason, ImagesStopReason::Stop);
    assert_eq!(
        calls.lock().unwrap()[0].options.request.api_key,
        Some("actual-image:https://example.test/v1".into())
    );
}

#[test]
fn registers_providers_and_reads_models_synchronously() {
    let models = create_images_models(Default::default());
    models.set_provider(test_provider(
        "p1",
        vec![test_model("p1", "m1"), test_model("p1", "m2")],
        None,
        None,
    ));
    models.set_provider(test_provider("p2", vec![test_model("p2", "m3")], None, None));

    let provider_ids: Vec<String> = models.get_providers().iter().map(|p| p.id().to_owned()).collect();
    assert_eq!(provider_ids, ["p1", "p2"]);
    let model_ids: Vec<String> = models.get_models(None).into_iter().map(|m| m.id).collect();
    assert_eq!(model_ids, ["m1", "m2", "m3"]);
    let provider_models: Vec<String> = models.get_models(Some("p1")).into_iter().map(|m| m.id).collect();
    assert_eq!(provider_models, ["m1", "m2"]);
    assert_eq!(models.get_model("p2", "m3").map(|m| m.id), Some("m3".to_owned()));
    assert_eq!(models.get_model("p2", "missing"), None);
    assert!(models.get_models(Some("missing")).is_empty());

    models.delete_provider("p1");
    assert!(models.get_provider("p1").is_none());
}

#[tokio::test]
async fn resolves_auth_and_merges_it_into_requests_with_explicit_options_winning() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let models = create_images_models(pi_ai::models::CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(&[("TEST_KEY", "env-key")])),
    });
    models.set_provider(test_provider(
        "p1",
        vec![test_model("p1", "model-a")],
        Some("TEST_KEY"),
        Some(Arc::clone(&calls)),
    ));
    let model = models.get_model("p1", "model-a").expect("registered model");

    assert_eq!(
        models
            .get_auth(&model)
            .await
            .expect("auth resolves")
            .expect("configured")
            .auth
            .api_key
            .as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("env-key"))
    );
    let result = models
        .generate_images(model.clone(), context(), ImagesOptions::default())
        .await
        .expect("image generation");
    assert_eq!(result.stop_reason, ImagesStopReason::Stop);
    assert_eq!(
        calls.lock().expect("calls lock")[0].options.request.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("env-key"))
    );

    let mut explicit = ImagesOptions::default();
    explicit.request.api_key = Some(("explicit".to_owned()).into());
    models
        .generate_images(model, context(), explicit)
        .await
        .expect("image generation");
    assert_eq!(
        calls.lock().expect("calls lock")[1].options.request.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("explicit"))
    );
}

#[tokio::test]
async fn merges_provider_resolved_env_into_image_options() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let models = create_images_models(Default::default());
    let auth = static_auth(Some(AuthResult {
        auth: ModelAuth {
            api_key: Some(("provider-key".to_owned()).into()),
            headers: None,
            base_url: None,
        },
        env: Some(IndexMap::from([
            ("zeta".into(), "provider".into()),
            ("SHARED".into(), "provider".into()),
        ])),
        source: None,
    }));
    models.set_provider(create_images_provider(CreateImagesProviderOptions {
        id: "p1".to_owned(),
        name: None,
        auth: ProviderAuth::api_key(auth),
        models: vec![test_model("p1", "model-a")],
        refresh_models: None,
        api: Arc::new(TestApi {
            calls: Arc::clone(&calls),
        }),
    }));
    let model = models.get_model("p1", "model-a").expect("registered model");

    let options = ImagesOptions {
        request: ImagesRequestOptions {
            api_key: Some(("request-key".to_owned()).into()),
            env: Some(IndexMap::from([
                ("alpha".into(), "request".into()),
                ("SHARED".into(), "request".into()),
            ])),
            ..Default::default()
        },
        ..Default::default()
    };
    models
        .generate_images(model, context(), options)
        .await
        .expect("image generation");

    let recorded = &calls.lock().expect("calls lock")[0].options;
    assert_eq!(
        recorded.request.api_key.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("request-key"))
    );
    let env = recorded.request.env.as_ref().expect("merged env");
    let keys: Vec<&str> = env.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["zeta", "SHARED", "alpha"]);
    assert_eq!(
        env.get("SHARED"),
        Some(&pi_js_compat::js_json::JsString::from("request"))
    );
}

#[tokio::test]
async fn applies_auth_base_url_and_headers_to_the_request_model() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let models = create_images_models(Default::default());
    let auth = static_auth(Some(AuthResult {
        auth: ModelAuth {
            api_key: Some(("key".to_owned()).into()),
            headers: Some(IndexMap::from([("x-auth".into(), Some("auth".into()))])),
            base_url: Some(("https://auth.example/v1".to_owned()).into()),
        },
        env: None,
        source: None,
    }));
    models.set_provider(create_images_provider(CreateImagesProviderOptions {
        id: "p1".to_owned(),
        name: None,
        auth: ProviderAuth::api_key(auth),
        models: vec![test_model("p1", "model-a")],
        refresh_models: None,
        api: Arc::new(TestApi {
            calls: Arc::clone(&calls),
        }),
    }));
    let model = models.get_model("p1", "model-a").expect("registered model");
    let options = ImagesOptions {
        request: ImagesRequestOptions {
            headers: Some(IndexMap::from([
                ("x-request".into(), Some("request".into())),
                ("x-auth".into(), Some("request".into())),
            ])),
            ..Default::default()
        },
        ..Default::default()
    };
    models
        .generate_images(model, context(), options)
        .await
        .expect("image generation");

    let recorded = &calls.lock().expect("calls lock")[0];
    assert_eq!(recorded.model.base_url, "https://auth.example/v1");
    assert_eq!(
        recorded.options.request.headers,
        Some(IndexMap::from([
            (
                "x-auth".to_owned(),
                Some(pi_js_compat::js_json::JsString::from("request"))
            ),
            (
                "x-request".to_owned(),
                Some(pi_js_compat::js_json::JsString::from("request"))
            ),
        ]))
    );
}

#[tokio::test]
async fn returns_error_results_for_unknown_providers_and_still_dispatches_unconfigured_auth() {
    let models = create_images_models(pi_ai::models::CreateModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(&[])),
    });
    let ghost = models
        .generate_images(test_model("ghost", "m"), context(), ImagesOptions::default())
        .await
        .expect("image generation");
    assert_eq!(ghost.stop_reason, ImagesStopReason::Error);
    assert!(
        ghost
            .error_message
            .as_ref()
            .expect("error")
            .contains("Unknown provider: ghost")
    );

    let calls = Arc::new(Mutex::new(Vec::new()));
    models.set_provider(test_provider(
        "p1",
        vec![test_model("p1", "model-a")],
        Some("MISSING"),
        Some(Arc::clone(&calls)),
    ));
    let model = models.get_model("p1", "model-a").expect("registered model");
    assert!(models.get_auth(&model).await.expect("auth resolves").is_none());
    models
        .generate_images(model, context(), ImagesOptions::default())
        .await
        .expect("image generation");
    assert_eq!(calls.lock().expect("calls lock")[0].options.request.api_key, None);
}

type RefreshFn = Arc<dyn Fn() -> BoxFuture<'static, Result<Vec<ImagesModel>, CallbackError>> + Send + Sync>;

fn dynamic_provider(id: &str, refresh: RefreshFn) -> Arc<dyn pi_ai::images_models::ImagesProvider> {
    create_images_provider(CreateImagesProviderOptions {
        id: id.to_owned(),
        name: None,
        auth: ProviderAuth::api_key(static_auth(Some(AuthResult {
            auth: ModelAuth::default(),
            env: None,
            source: None,
        }))),
        models: Vec::new(),
        refresh_models: Some(refresh),
        api: Arc::new(TestApi {
            calls: Arc::new(Mutex::new(Vec::new())),
        }),
    })
}

#[tokio::test]
async fn refreshes_dynamic_providers_with_in_flight_dedupe() {
    let fetches = Arc::new(Mutex::new(0_u32));
    let counter = Arc::clone(&fetches);
    let provider = dynamic_provider(
        "dyn",
        Arc::new(move || {
            let counter = Arc::clone(&counter);
            Box::pin(async move {
                *counter.lock().expect("counter lock") += 1;
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                Ok(vec![test_model("dyn", "listed")])
            })
        }),
    );
    let models = create_images_models(Default::default());
    models.set_provider(provider);
    assert!(models.get_models(Some("dyn")).is_empty());

    let (first, second) = tokio::join!(models.refresh(Some("dyn")), models.refresh(Some("dyn")));
    first.expect("first refresh");
    second.expect("second refresh");
    assert_eq!(*fetches.lock().expect("fetches lock"), 1);
    assert!(models.get_model("dyn", "listed").is_some());

    models.set_provider(dynamic_provider(
        "flaky",
        Arc::new(|| Box::pin(async { Err(CallbackError::new("fetch failed")) })),
    ));
    let error = models
        .refresh(Some("flaky"))
        .await
        .expect_err("single-provider failure rejects");
    assert_eq!(error.code, pi_ai::auth::ModelsErrorCode::ModelSource);
    assert_eq!(error.message, "Model refresh failed for flaky");
    assert_eq!(
        error.cause.as_deref().unwrap().as_error().unwrap().message,
        "fetch failed"
    );
    models.refresh(None).await.expect("refresh-all never rejects");
    models
        .refresh(Some("missing"))
        .await
        .expect("unknown provider is a no-op");
}

#[test]
fn create_images_provider_defaults_name_to_id_and_keeps_auth() {
    let provider = create_images_provider(CreateImagesProviderOptions {
        id: "p1".to_owned(),
        name: None,
        auth: ProviderAuth::api_key(static_auth(None)),
        models: vec![test_model("p1", "m1")],
        refresh_models: None,
        api: Arc::new(TestApi {
            calls: Arc::new(Mutex::new(Vec::new())),
        }),
    });
    assert_eq!(provider.name(), "p1");
    assert!(provider.auth().api_key.is_some());
    assert_eq!(provider.get_models().len(), 1);
}

#[test]
fn options_metadata_and_extra_are_forwarded() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let models = create_images_models(Default::default());
    models.set_provider(test_provider(
        "p1",
        vec![test_model("p1", "m1")],
        None,
        Some(Arc::clone(&calls)),
    ));
    let model = models.get_model("p1", "m1").expect("model");
    let options = ImagesOptions {
        metadata: Some(
            pi_js_compat::js_json!({"trace": "abc"})
                .as_object()
                .expect("object")
                .clone(),
        ),
        extra: serde_json::Map::from_iter([("custom".to_owned(), json!(true))]),
        ..Default::default()
    };
    block_on(models.generate_images(model, context(), options)).expect("image generation");
    let recorded = &calls.lock().expect("calls lock")[0].options;
    assert_eq!(
        recorded.metadata.as_ref().expect("metadata")["trace"],
        pi_js_compat::js_json!("abc")
    );
    assert_eq!(recorded.extra["custom"], json!(true));
}

fn block_on<T>(future: BoxFuture<'static, T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}
