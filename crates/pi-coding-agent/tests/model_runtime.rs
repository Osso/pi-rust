//! Ports `packages/coding-agent/test/model-runtime-*.test.ts` and runtime cases
//! from `model-registry.test.ts`.

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use pi_ai::auth::InMemoryCredentialStore;
use pi_ai::auth::resolve::AuthResolutionOverrides;
use pi_ai::auth::types::{AuthOperationOptions, Credential, CredentialModifier, CredentialStore};
use pi_ai::models::Provider;
use pi_ai::request::{SimpleStreamOptions, StreamOptions, error_stream};
use pi_ai::types::{Context, Model, ProviderEnv};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{
    CreateModelRuntimeOptions, CredentialSynchronizationOperation, ModelRuntime, ModelRuntimeError,
};
use pi_coding_agent::core::provider_composer::{AuthStatusSource, ExtensionOAuthConfig, ProviderConfigInput};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-runtime-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn storage_with(entries: &[(&str, Value)]) -> Arc<AuthStorage> {
    let mut data = Map::new();
    for (provider, credential) in entries {
        data.insert((*provider).to_owned(), credential.clone());
    }
    Arc::new(AuthStorage::in_memory(data))
}

fn create_options(credentials: Arc<AuthStorage>) -> CreateModelRuntimeOptions {
    CreateModelRuntimeOptions {
        credentials: Some(credentials as Arc<dyn CredentialStore>),
        models_path: Some(None),
        ..Default::default()
    }
}

fn test_model(id: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "api": "openai-completions",
        "provider": "placeholder",
        "baseUrl": "https://example.test/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 10000,
        "maxTokens": 1000,
    }))
    .unwrap()
}

#[tokio::test]
async fn accepts_a_pi_ai_credential_store() {
    let credentials = InMemoryCredentialStore::new();
    let modifier: CredentialModifier = Arc::new(|_current| {
        Box::pin(async {
            Ok(Some(Credential::ApiKey(pi_ai::auth::types::ApiKeyCredential {
                key: Some("stored-key".to_owned().into()),
                env: None,
            })))
        })
    });
    credentials
        .modify("anthropic", modifier, AuthOperationOptions::default())
        .await
        .unwrap();
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(credentials)),
        models_path: Some(None),
        ..Default::default()
    })
    .await
    .unwrap();
    let auth = runtime
        .get_auth("anthropic", AuthResolutionOverrides::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(auth.auth.api_key.as_ref(), Some(&JsString::from("stored-key")));
}

#[tokio::test]
async fn projects_provider_owned_names_and_auth_methods() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    let providers = runtime.get_providers();
    let ids: Vec<&str> = providers.iter().map(|provider| provider.id()).collect();
    for expected in [
        "amazon-bedrock",
        "google-vertex",
        "anthropic",
        "cloudflare-ai-gateway",
        "cloudflare-workers-ai",
        "openai-codex",
        "openai-codex-gc",
    ] {
        assert!(ids.contains(&expected), "missing {expected}");
    }
    let anthropic = runtime.get_provider("anthropic").unwrap();
    assert_eq!(anthropic.name(), "Anthropic");
    assert!(anthropic.auth().oauth.is_some());
    let bedrock = runtime.get_provider("amazon-bedrock").unwrap();
    assert_eq!(
        bedrock.auth().api_key.as_ref().map(|method| method.name()),
        Some("AWS credentials or bearer token")
    );
    let vertex = runtime.get_provider("google-vertex").unwrap();
    assert_eq!(
        vertex.auth().api_key.as_ref().map(|method| method.name()),
        Some("Google Cloud credentials")
    );
    let codex = runtime.get_provider("openai-codex").unwrap();
    assert!(codex.auth().oauth.is_some());
    assert!(codex.auth().api_key.is_none());
    assert!(runtime.get_error().is_none());
}

#[tokio::test]
async fn distinguishes_subscription_oauth_from_generic_oauth() {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;
    let storage = storage_with(&[
        (
            "anthropic",
            json!({"type": "oauth", "access": "anthropic-access", "refresh": "anthropic-refresh", "expires": now + 3_600_000.0}),
        ),
        (
            "openrouter",
            json!({"type": "oauth", "access": "openrouter-key", "refresh": "", "expires": 9_007_199_254_740_991.0}),
        ),
    ]);
    let runtime = ModelRuntime::create(create_options(storage)).await.unwrap();
    assert!(runtime.is_using_oauth("anthropic"));
    assert!(runtime.is_using_subscription("anthropic"));
    assert!(runtime.is_using_oauth("openrouter"));
    assert!(!runtime.is_using_subscription("openrouter"));
}

#[tokio::test]
async fn constructs_an_api_key_method_for_an_extension_provider() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    runtime
        .register_provider(
            "extension-api-key",
            ProviderConfigInput {
                name: Some("Extension API Key".to_owned()),
                base_url: Some("https://example.test/v1".to_owned().into()),
                api_key: Some("$EXTENSION_TEST_API_KEY".to_owned().into()),
                api: Some("openai-completions".to_owned()),
                models: Some(vec![test_model("extension-model")]),
                ..Default::default()
            },
        )
        .unwrap();
    let provider = runtime.get_provider("extension-api-key").unwrap();
    assert_eq!(provider.name(), "Extension API Key");
    assert_eq!(
        provider.auth().api_key.as_ref().map(|method| method.name()),
        Some("API key")
    );
    assert!(provider.auth().oauth.is_none());
}

#[tokio::test]
async fn resolves_configured_auth_from_request_scoped_env_overrides() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    runtime
        .register_provider(
            "request-env-provider",
            ProviderConfigInput {
                base_url: Some("https://example.test/v1".to_owned().into()),
                api_key: Some("$REQUEST_SCOPED_API_KEY".to_owned().into()),
                headers: Some(
                    [("x-request-value".to_owned(), JsString::from("$REQUEST_SCOPED_HEADER"))]
                        .into_iter()
                        .collect(),
                ),
                api: Some("openai-completions".to_owned()),
                models: Some(vec![test_model("request-env-model")]),
                ..Default::default()
            },
        )
        .unwrap();
    let model = runtime.get_model("request-env-provider", "request-env-model").unwrap();
    let mut env = ProviderEnv::new();
    env.insert("REQUEST_SCOPED_API_KEY".to_owned(), "request-key".to_owned().into());
    env.insert("REQUEST_SCOPED_HEADER".to_owned(), "request-header".to_owned().into());
    let auth = runtime
        .get_auth_for_model(
            &model,
            AuthResolutionOverrides {
                env: Some(env),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(auth.auth.api_key.as_ref(), Some(&JsString::from("request-key")));
    assert_eq!(
        auth.auth
            .headers
            .as_ref()
            .and_then(|headers| headers.get("x-request-value"))
            .cloned()
            .flatten()
            .as_ref(),
        Some(&JsString::from("request-header"))
    );
}

fn capturing_provider() -> (ProviderConfigInput, Arc<Mutex<Option<pi_ai::types::ProviderHeaders>>>) {
    let captured = Arc::new(Mutex::new(None));
    let sink = Arc::clone(&captured);
    let config = ProviderConfigInput {
        base_url: Some("https://example.test/v1".to_owned().into()),
        api_key: Some("generated-key".to_owned().into()),
        auth_header: Some(true),
        headers: Some(
            [("x-provider".to_owned(), JsString::from("provider"))]
                .into_iter()
                .collect(),
        ),
        api: Some("openai-completions".to_owned()),
        stream_simple: Some(Arc::new(move |model, _context, options| {
            *sink.lock().unwrap() = options.and_then(|options| options.stream.request.headers);
            error_stream(&model, "captured")
        })),
        models: Some(vec![test_model("header-model")]),
        ..Default::default()
    };
    (config, captured)
}

#[tokio::test]
async fn explicit_authorization_header_overrides_auth_header_case_insensitively() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    let (mut config, captured) = capturing_provider();
    config.headers = None;
    config.models = Some(vec![test_model("auth-header-model")]);
    runtime.register_provider("auth-header-provider", config).unwrap();
    let model = runtime.get_model("auth-header-provider", "auth-header-model").unwrap();
    let mut headers = pi_ai::types::ProviderHeaders::new();
    headers.insert("authorization".to_owned(), Some("Explicit token".to_owned().into()));
    let mut options = SimpleStreamOptions::default();
    options.stream.request.headers = Some(headers);
    let _ = runtime
        .complete_simple(
            model,
            Context {
                system_prompt: None,
                messages: Vec::new(),
                tools: None,
            },
            options,
        )
        .await;
    let captured = captured.lock().unwrap().clone().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured.get("authorization").cloned().flatten().as_ref(),
        Some(&JsString::from("Explicit token"))
    );
    assert!(!captured.contains_key("Authorization"));
}

#[tokio::test]
async fn transforms_fully_assembled_headers_once() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    let (config, captured) = capturing_provider();
    runtime.register_provider("header-provider", config).unwrap();
    let mut model = runtime.get_model("header-provider", "header-model").unwrap();
    model.headers = Some([("x-model".to_owned(), JsString::from("model"))].into());
    let transforms = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&transforms);
    let mut options = SimpleStreamOptions::default();
    options.stream.request.headers = Some(
        [("x-explicit".to_owned(), Some(JsString::from("explicit")))]
            .into_iter()
            .collect(),
    );
    options.stream.request.transform_headers = Some(Arc::new(move |headers| {
        let counter = Arc::clone(&counter);
        Box::pin(async move {
            counter.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                headers.get("Authorization").cloned().flatten().as_ref(),
                Some(&JsString::from("Bearer generated-key"))
            );
            assert_eq!(
                headers.get("x-provider").cloned().flatten().as_ref(),
                Some(&JsString::from("provider"))
            );
            assert_eq!(
                headers.get("x-model").cloned().flatten().as_ref(),
                Some(&JsString::from("model"))
            );
            assert_eq!(
                headers.get("x-explicit").cloned().flatten().as_ref(),
                Some(&JsString::from("explicit"))
            );
            let mut transformed = headers;
            transformed.insert("x-transformed".to_owned(), Some("yes".to_owned().into()));
            Ok(transformed)
        })
    }));
    let _ = runtime
        .complete_simple(
            model,
            Context {
                system_prompt: None,
                messages: Vec::new(),
                tools: None,
            },
            options,
        )
        .await;
    assert_eq!(transforms.load(Ordering::SeqCst), 1);
    let captured = captured.lock().unwrap().clone().unwrap();
    assert_eq!(
        captured.get("x-transformed").cloned().flatten().as_ref(),
        Some(&JsString::from("yes"))
    );
}

#[tokio::test]
async fn forwards_cancellation_to_extension_oauth_refresh() {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;
    let storage = storage_with(&[(
        "extension-oauth",
        json!({"type": "oauth", "access": "expired", "refresh": "refresh", "expires": 0.0}),
    )]);
    let runtime = ModelRuntime::create(create_options(storage)).await.unwrap();
    let captured = Arc::new(Mutex::new(None::<CancellationToken>));
    let sink = Arc::clone(&captured);
    runtime
        .register_provider(
            "extension-oauth",
            ProviderConfigInput {
                name: Some("Extension OAuth".to_owned()),
                base_url: Some("https://example.test/v1".to_owned().into()),
                api: Some("openai-completions".to_owned()),
                oauth: Some(ExtensionOAuthConfig {
                    name: "Extension subscription".to_owned(),
                    is_subscription: Some(true),
                    uses_callback_server: None,
                    login: Arc::new(|_callbacks| {
                        Box::pin(async { Err(pi_ai::auth::types::AuthError::failed("unused")) })
                    }),
                    refresh_token: Arc::new(move |credential, signal| {
                        *sink.lock().unwrap() = Some(signal.clone());
                        Box::pin(async move {
                            let mut refreshed = credential;
                            refreshed.expires = now + 60_000.0;
                            Ok(refreshed)
                        })
                    }),
                    get_api_key: Arc::new(|credential| credential.access.to_utf8().expect("scalar fixture key").into()),
                    modify_models: None,
                }),
                models: Some(vec![test_model("extension-model")]),
                ..Default::default()
            },
        )
        .unwrap();
    let model = runtime.get_model("extension-oauth", "extension-model").unwrap();
    let controller = CancellationToken::new();
    let auth = runtime
        .get_auth_for_model(
            &model,
            AuthResolutionOverrides {
                signal: Some(controller.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(auth.is_some());
    let refresh_signal = captured.lock().unwrap().clone().unwrap();
    assert!(!refresh_signal.is_cancelled());
    controller.cancel();
    assert!(refresh_signal.is_cancelled());
}

#[tokio::test]
async fn runtime_api_keys_mask_stored_credentials_and_report_status() {
    let storage = storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]);
    let runtime = ModelRuntime::create(create_options(storage)).await.unwrap();
    runtime
        .set_runtime_api_key(
            "anthropic",
            &JsString::from("runtime-key"),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let auth = runtime
        .get_auth("anthropic", AuthResolutionOverrides::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(auth.auth.api_key.as_ref(), Some(&JsString::from("runtime-key")));
    let status = runtime.get_provider_auth_status("anthropic");
    assert_eq!(status.source, Some(AuthStatusSource::Runtime));

    runtime
        .remove_runtime_api_key("anthropic", AuthOperationOptions::default())
        .await
        .unwrap();
    let status = runtime.get_provider_auth_status("anthropic");
    assert_eq!(status.source, Some(AuthStatusSource::Stored));
}

#[tokio::test]
async fn credential_sync_failure_is_reported_as_a_typed_error() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    runtime
        .register_provider(
            "failing-provider",
            ProviderConfigInput {
                base_url: Some("https://example.test/v1".to_owned().into()),
                api_key: Some("key".to_owned().into()),
                api: Some("openai-completions".to_owned()),
                models: Some(vec![test_model("model")]),
                refresh_models: Some(Arc::new(|_context| {
                    Box::pin(async { Err(pi_ai::request::CallbackError::new("refresh failed")) })
                })),
                ..Default::default()
            },
        )
        .unwrap();
    let refresh = runtime
        .refresh(pi_ai::models::ModelsRefreshOptions {
            allow_network: Some(false),
            providers: Some(vec!["failing-provider".to_owned()]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(refresh.errors.is_empty());
    let error = runtime
        .set_runtime_api_key(
            "failing-provider",
            &JsString::from("runtime-key"),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap_err();
    match error {
        ModelRuntimeError::CredentialSync(error) => {
            assert_eq!(error.provider_id, "failing-provider");
            assert_eq!(error.operation, CredentialSynchronizationOperation::SetRuntimeApiKey);
            assert_eq!(
                error.to_string(),
                "Credential setRuntimeApiKey committed for failing-provider, but local synchronization failed"
            );
            assert!(error.credential.is_some());
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn refresh_reloads_models_json_from_disk() {
    let dir = temp_dir("refresh-models");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {"anthropic": {"baseUrl": "https://first.example.com/v1"}}}).to_string(),
    )
    .unwrap();
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(storage_with(&[]) as Arc<dyn CredentialStore>),
        models_path: Some(Some(path.display().to_string().into())),
        ..Default::default()
    })
    .await
    .unwrap();
    let models = runtime.get_models(Some("anthropic"));
    assert!(!models.is_empty());
    assert!(
        models
            .iter()
            .all(|model| model.base_url == "https://first.example.com/v1")
    );

    fs::write(
        &path,
        json!({"providers": {"anthropic": {"baseUrl": "https://second.example.com/v1"}}}).to_string(),
    )
    .unwrap();
    runtime.refresh(Default::default()).await.unwrap();
    let models = runtime.get_models(Some("anthropic"));
    assert!(
        models
            .iter()
            .all(|model| model.base_url == "https://second.example.com/v1")
    );
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn reports_every_provider_composition_error() {
    let dir = temp_dir("composition-errors");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {
            "broken-one": {"api": "openai-completions", "models": [{"id": "one"}]},
            "broken-two": {"api": "openai-completions", "models": [{"id": "two"}]}
        }})
        .to_string(),
    )
    .unwrap();
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(storage_with(&[]) as Arc<dyn CredentialStore>),
        models_path: Some(Some(path.display().to_string().into())),
        ..Default::default()
    })
    .await
    .unwrap();
    let error = runtime.get_error().unwrap();
    assert!(error.contains("Provider \"broken-one\""), "{error:?}");
    assert!(error.contains("Provider \"broken-two\""), "{error:?}");
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn unregister_provider_removes_the_registration() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    runtime
        .register_provider(
            "extension",
            ProviderConfigInput {
                base_url: Some("https://example.test/v1".to_owned().into()),
                api_key: Some("key".to_owned().into()),
                api: Some("openai-completions".to_owned()),
                models: Some(vec![test_model("model")]),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(runtime.get_provider("extension").is_some());
    assert_eq!(runtime.get_registered_provider_ids(), vec!["extension"]);
    runtime.unregister_provider("extension");
    assert!(runtime.get_provider("extension").is_none());
    assert!(runtime.get_registered_provider_ids().is_empty());
}

#[tokio::test]
async fn registered_models_json_overrides_apply_above_extension_models() {
    let dir = temp_dir("override-above-extension");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {"extension": {"modelOverrides": {"model": {"name": "Overridden"}}}}}).to_string(),
    )
    .unwrap();
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(storage_with(&[]) as Arc<dyn CredentialStore>),
        models_path: Some(Some(path.display().to_string().into())),
        ..Default::default()
    })
    .await
    .unwrap();
    runtime
        .register_provider(
            "extension",
            ProviderConfigInput {
                base_url: Some("https://example.test/v1".to_owned().into()),
                api_key: Some("key".to_owned().into()),
                api: Some("openai-completions".to_owned()),
                models: Some(vec![test_model("model")]),
                ..Default::default()
            },
        )
        .unwrap();
    let model = runtime.get_model("extension", "model").unwrap();
    assert_eq!(model.name, "Overridden");
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn provider_auth_status_reports_models_json_sources() {
    let dir = temp_dir("statuses");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {
            "demo": {"baseUrl": "https://example.com/v1", "apiKey": "!echo key", "models": [{"id": "demo-model", "api": "openai-completions"}]},
            "env-demo": {"baseUrl": "https://example.com/v1", "apiKey": "$RUNTIME_STATUS_KEY", "models": [{"id": "env-model", "api": "openai-completions"}]}
        }})
        .to_string(),
    )
    .unwrap();
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(storage_with(&[]) as Arc<dyn CredentialStore>),
        models_path: Some(Some(path.display().to_string().into())),
        ..Default::default()
    })
    .await
    .unwrap();
    let demo = runtime.get_provider_auth_status("demo");
    assert_eq!(demo.source, Some(AuthStatusSource::ModelsJsonCommand));
    let env_demo = runtime.get_provider_auth_status("env-demo");
    assert_eq!(env_demo.source, None);
    assert!(!env_demo.configured);
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn get_available_filters_by_configured_auth() {
    let storage = storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]);
    let runtime = ModelRuntime::create(create_options(storage)).await.unwrap();
    let available = runtime
        .get_available(None, AuthOperationOptions::default())
        .await
        .unwrap();
    assert!(!available.is_empty());
    assert!(available.iter().all(|model| model.provider == "anthropic"));
    assert!(runtime.has_configured_auth("anthropic"));
    assert!(!runtime.has_configured_auth("openai"));
}

#[tokio::test]
async fn extension_oauth_only_provider_has_no_api_key_method() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    runtime
        .register_provider(
            "extension-oauth",
            ProviderConfigInput {
                name: Some("Extension OAuth".to_owned()),
                base_url: Some("https://example.test/v1".to_owned().into()),
                api: Some("openai-completions".to_owned()),
                oauth: Some(ExtensionOAuthConfig {
                    name: "Extension subscription".to_owned(),
                    is_subscription: Some(true),
                    uses_callback_server: None,
                    login: Arc::new(|_callbacks| {
                        Box::pin(async { Err(pi_ai::auth::types::AuthError::failed("unused")) })
                    }),
                    refresh_token: Arc::new(|credential, _signal| Box::pin(async move { Ok(credential) })),
                    get_api_key: Arc::new(|credential| credential.access.to_utf8().expect("scalar fixture key").into()),
                    modify_models: None,
                }),
                models: Some(vec![test_model("extension-model")]),
                ..Default::default()
            },
        )
        .unwrap();
    let provider = runtime.get_provider("extension-oauth").unwrap();
    assert!(provider.auth().api_key.is_none());
    let oauth = provider.auth().oauth.as_ref().unwrap();
    assert_eq!(oauth.name(), "Extension subscription");
    assert!(oauth.is_subscription());
}

#[tokio::test]
async fn compatibility_request_config_is_exposed_for_unconfigured_models() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    let mut model = test_model("ghost");
    model.provider = "missing-provider".to_owned();
    model.headers = Some([("X-Static-Model".to_owned(), JsString::from("static-value"))].into());
    let config = runtime.get_compatibility_request_config(&model).unwrap();
    assert!(!config.auth_header);
    assert_eq!(
        config
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Static-Model"))
            .cloned()
            .flatten()
            .as_ref(),
        Some(&JsString::from("static-value"))
    );
}

#[tokio::test]
async fn stream_errors_for_unknown_providers_use_the_source_message() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    let model = test_model("ghost");
    let stream = runtime.stream(
        model,
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        StreamOptions::default(),
    );
    let message = stream.result().await.unwrap();
    assert_eq!(
        message.error_message.as_ref(),
        Some(&JsString::from("Unknown provider: placeholder"))
    );
    assert_eq!(message.stop_reason, pi_ai::types::StopReason::Error);
}

#[tokio::test]
async fn get_auth_merges_configured_headers_with_resolved_auth() {
    let dir = temp_dir("auth-headers");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {"anthropic": {"headers": {"X-Configured": "yes"}}}}).to_string(),
    )
    .unwrap();
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(
            storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]) as Arc<dyn CredentialStore>,
        ),
        models_path: Some(Some(path.display().to_string().into())),
        ..Default::default()
    })
    .await
    .unwrap();
    let model = runtime.get_model("anthropic", "claude-opus-4-8").unwrap();
    let auth = runtime
        .get_auth_for_model(&model, AuthResolutionOverrides::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        auth.auth
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Configured"))
            .cloned()
            .flatten()
            .as_ref(),
        Some(&JsString::from("yes"))
    );
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn native_provider_registration_keeps_its_auth_implementation() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    let base = runtime.get_provider("anthropic").unwrap();
    let provider: Arc<dyn Provider> = base;
    runtime.register_native_provider(Arc::clone(&provider));
    let registered = runtime.get_registered_native_provider("anthropic").unwrap();
    assert!(registered.auth().oauth.is_some());
    assert_eq!(runtime.get_registered_provider_ids(), vec!["anthropic".to_owned()]);
}

#[tokio::test]
async fn list_credentials_includes_runtime_overrides() {
    let runtime = ModelRuntime::create(create_options(storage_with(&[]))).await.unwrap();
    runtime
        .set_runtime_api_key(
            "anthropic",
            &JsString::from("runtime-key"),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let credentials = runtime.list_credentials(AuthOperationOptions::default()).await.unwrap();
    assert!(credentials.iter().any(|entry| entry.provider_id == "anthropic"));
}
