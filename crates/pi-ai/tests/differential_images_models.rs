#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_ai::auth::{ApiKeyAuth, ApiKeyResolveInput, AuthContext, AuthError, AuthResult, ModelAuth, ProviderAuth};
use pi_ai::images_models::{
    CreateImagesProviderOptions, ImagesModels, MutableImagesModels, create_images_models, create_images_provider,
};
use pi_ai::request::{BoxFuture, CallbackError, ImagesOptions, ProviderImages};
use pi_ai::types::{AssistantImages, ImagesContext, ImagesModel, ImagesStopReason, Modality, TextOrImageContent};
use serde_json::{Value, json};

fn test_model(provider: &str, id: &str) -> ImagesModel {
    ImagesModel {
        id: id.to_owned(),
        name: id.to_owned(),
        api: "test-images".to_owned(),
        provider: provider.to_owned(),
        base_url: ("https://example.test/v1".to_owned()).into(),
        thinking_level_map: None,
        input: vec![Modality::Text],
        output: vec![Modality::Image],
        cost: Default::default(),
        auto_compaction_threshold: None,
        sampling_params: None,
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
        timestamp: 0.0,
    }
}

struct NoAuthContext;

impl AuthContext for NoAuthContext {
    fn env(
        &self,
        _name: &str,
    ) -> BoxFuture<'_, Result<Option<pi_js_compat::js_json::JsString>, pi_ai::auth::AuthError>> {
        Box::pin(async { Ok(None) })
    }

    fn file_exists(
        &self,
        _path: &pi_js_compat::js_json::JsString,
    ) -> BoxFuture<'_, Result<bool, pi_ai::auth::AuthError>> {
        Box::pin(async { Ok(false) })
    }
}

struct StaticAuth {
    result: Option<AuthResult>,
}

impl ApiKeyAuth for StaticAuth {
    fn name(&self) -> JsString {
        "Test key".into()
    }

    fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        let result = self.result.clone();
        Box::pin(async move { Ok(result) })
    }
}

#[derive(Clone)]
struct Call {
    model: ImagesModel,
    options: ImagesOptions,
}

struct RecordingApi {
    calls: Arc<Mutex<Vec<Call>>>,
}

impl ProviderImages for RecordingApi {
    fn generate_images(
        &self,
        model: ImagesModel,
        _context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'_, Result<AssistantImages, CallbackError>> {
        self.calls.lock().expect("calls lock").push(Call {
            model: model.clone(),
            options,
        });
        Box::pin(async move { Ok(ok_result(&model)) })
    }
}

#[tokio::test]
async fn image_refresh_preserves_an_existing_models_error() {
    use pi_ai::auth::resolve::{ModelsError, ModelsErrorCode};
    let models = create_images_models(Default::default());
    let original = ModelsError::with_cause(
        ModelsErrorCode::Auth,
        "catalog permission denied",
        Some(Box::new(pi_js_compat::abort::AbortReason::Value(
            pi_js_compat::js_json::JsValue::String("cause".into()),
        ))),
    );
    let failure = original.clone();
    models.set_provider(create_images_provider(CreateImagesProviderOptions {
        id: "p1".into(),
        name: None,
        auth: ProviderAuth::api_key(Arc::new(StaticAuth { result: None })),
        models: vec![],
        refresh_models: Some(Arc::new(move || {
            let failure = failure.clone();
            Box::pin(async move { Err(failure.into_callback()) })
        })),
        api: Arc::new(RejectingApi),
    }));
    assert_eq!(models.refresh(Some("p1")).await.unwrap_err(), original);
}

struct RejectingApi;

impl ProviderImages for RejectingApi {
    fn generate_images(
        &self,
        _model: ImagesModel,
        _context: ImagesContext,
        _options: ImagesOptions,
    ) -> BoxFuture<'_, Result<AssistantImages, CallbackError>> {
        Box::pin(async {
            Err(pi_js_compat::node_error::NativeNodeError::new("TypeError", "image request failed").into())
        })
    }
}

#[test]
fn unauthenticated_image_rejection_matches_frozen_return_without_await() {
    let mut oracle = oracle::TsOracle::spawn("images-models-oracle.ts", "packages/ai/src/images-models.ts");
    let expected = oracle.request(&json!({"id": 1, "operation": "generate", "reject": true}));
    let models = create_images_models(Default::default());
    models.set_provider(create_images_provider(CreateImagesProviderOptions {
        id: "p1".into(),
        name: None,
        auth: ProviderAuth {
            api_key: Some(Arc::new(StaticAuth { result: None })),
            oauth: None,
        },
        models: vec![test_model("p1", "m1")],
        refresh_models: None,
        api: Arc::new(RejectingApi),
    }));
    let result = block_on(models.generate_images(test_model("p1", "m1"), context(), Default::default()));
    let actual = match result {
        Ok(output) => json!({"id": 1, "ok": true, "output": output}),
        Err(error) => json!({"id": 1, "ok": false, "error": {"name": error.name, "message": error.message}}),
    };
    assert_eq!(actual, expected);
}

fn auth_from_request(value: &Value) -> Option<AuthResult> {
    let auth = value.get("auth")?.as_object()?;
    Some(AuthResult {
        auth: ModelAuth {
            api_key: (auth
                .get("apiKey")
                .and_then(Value::as_str)
                .map(pi_js_compat::js_json::JsString::from)),
            headers: auth.get("headers").and_then(headers_from_value),
            base_url: (auth
                .get("baseUrl")
                .and_then(Value::as_str)
                .map(pi_js_compat::js_json::JsString::from)),
        },
        env: auth.get("env").and_then(env_from_value),
        source: None,
    })
}

fn headers_from_value(value: &Value) -> Option<indexmap::IndexMap<String, Option<pi_js_compat::js_json::JsString>>> {
    let object = value.as_object()?;
    Some(
        object
            .iter()
            .map(|(name, value)| (name.clone(), value.as_str().map(pi_js_compat::js_json::JsString::from)))
            .collect(),
    )
}

fn env_from_value(value: &Value) -> Option<indexmap::IndexMap<String, pi_js_compat::js_json::JsString>> {
    let object = value.as_object()?;
    Some(
        object
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    pi_js_compat::js_json::JsString::from(value.as_str().expect("string")),
                )
            })
            .collect(),
    )
}

fn call_value(call: &Call) -> Value {
    json!({
        "modelId": call.model.id,
        "baseUrl": call.model.base_url,
        "apiKey": call.options.request.api_key,
        "headers": call.options.request.headers,
        "env": call.options.request.env,
    })
}

fn observed(request: &Value) -> Value {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let models = create_images_models(pi_ai::models::CreateModelsOptions {
        credentials: None,
        auth_context: Some(Arc::new(NoAuthContext)),
    });
    models.set_provider(create_images_provider(CreateImagesProviderOptions {
        id: "p1".to_owned(),
        name: None,
        auth: ProviderAuth::api_key(Arc::new(StaticAuth {
            result: auth_from_request(request),
        })),
        models: vec![test_model("p1", "m1"), test_model("p1", "m2")],
        refresh_models: None,
        api: Arc::new(RecordingApi {
            calls: Arc::clone(&calls),
        }),
    }));

    match request["operation"].as_str().expect("operation") {
        "read" => json!({
            "id": request["id"],
            "ok": true,
            "output": {
                "providers": models.get_providers().iter().map(|p| p.id().to_owned()).collect::<Vec<_>>(),
                "modelsAll": models.get_models(None).into_iter().map(|m| m.id).collect::<Vec<_>>(),
                "modelsP1": models.get_models(Some("p1")).into_iter().map(|m| m.id).collect::<Vec<_>>(),
                "modelFound": models.get_model("p1", "m1").map(|m| m.id),
                "modelMissing": models.get_model("p1", "missing"),
                "modelsMissing": models.get_models(Some("missing")).into_iter().map(|m| m.id).collect::<Vec<_>>(),
            }
        }),
        "unknown" => {
            let result = block_on(models.generate_images(
                test_model("ghost", "m"),
                ImagesContext { input: Vec::new() },
                ImagesOptions::default(),
            ))
            .expect("image generation");
            json!({
                "id": request["id"],
                "ok": true,
                "output": {
                    "stopReason": result.stop_reason,
                    "errorMessage": result.error_message,
                }
            })
        }
        "generate" => {
            let mut options = ImagesOptions::default();
            if let Some(option_values) = request.get("options").and_then(Value::as_object) {
                options.request.api_key = option_values
                    .get("apiKey")
                    .and_then(Value::as_str)
                    .map(pi_js_compat::js_json::JsString::from);
                options.request.headers = option_values.get("headers").and_then(headers_from_value);
                options.request.env = option_values.get("env").and_then(env_from_value);
            }
            let result =
                block_on(models.generate_images(test_model("p1", "m1"), context(), options)).expect("image generation");
            let calls = calls.lock().expect("calls lock");
            json!({
                "id": request["id"],
                "ok": true,
                "output": {
                    "result": serde_json::to_value(result).expect("result serializes"),
                    "calls": calls.iter().map(call_value).collect::<Vec<_>>(),
                }
            })
        }
        other => panic!("unexpected operation {other}"),
    }
}

fn block_on<T>(future: BoxFuture<'static, T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

#[test]
fn images_models_matches_the_pinned_typescript_module() {
    let requests = vec![
        json!({"id": 0, "operation": "read"}),
        json!({"id": 1, "operation": "unknown"}),
        json!({
            "id": 2,
            "operation": "generate",
            "auth": {"apiKey": "auth-key", "env": {"PROVIDER_ONLY": "provider", "SHARED": "provider"}},
            "options": {"apiKey": "request-key", "env": {"REQUEST_ONLY": "request", "SHARED": "request"}}
        }),
        json!({
            "id": 3,
            "operation": "generate",
            "auth": {
                "apiKey": "auth-key",
                "headers": {"x-auth": "auth", "x-shared": "auth"},
                "baseUrl": "https://auth.example/v1"
            },
            "options": {"headers": {"x-request": "request", "x-shared": "request"}}
        }),
        json!({"id": 4, "operation": "generate", "auth": null}),
        json!({"id": 6, "operation": "generate", "auth": {"baseUrl": ""}}),
        json!({"id": 5, "operation": "generate", "auth": {"apiKey": "auth-key"}, "options": {"apiKey": "explicit"}}),
    ];
    let mut ts = oracle::TsOracle::spawn("images-models-oracle.ts", "packages/ai/src/images-models.ts");
    for request in requests {
        assert_eq!(observed(&request), ts.request(&request), "request {request}");
    }
}
