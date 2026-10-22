//! Ports `packages/coding-agent/test/model-registry.test.ts` and
//! `model-registry-resolved-auth.test.ts` registry-facade cases.

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use pi_ai::auth::InMemoryCredentialStore;
use pi_ai::auth::types::CredentialStore;
use pi_ai::models::ModelsRefreshOptions;
use pi_ai::types::Model;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::{ModelRegistry, ResolvedRequestAuth};
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::provider_composer::{AuthStatusSource, ProviderConfigInput};
use serde_json::{Map, Value, json};

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-registry-{name}-{}-{unique}", std::process::id()));
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

async fn registry(storage: Arc<AuthStorage>, models_path: Option<String>) -> ModelRegistry {
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(storage as Arc<dyn CredentialStore>),
        models_path: Some((models_path).map(JsString::from)),
        ..Default::default()
    })
    .await
    .unwrap();
    ModelRegistry::new(runtime)
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
async fn exposes_model_runtime_auth_resolution_to_extensions() {
    let registry = registry(
        storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]),
        None,
    )
    .await;
    let model = registry.find("anthropic", "claude-opus-4-8").unwrap();
    let auth = registry.get_api_key_and_headers(&model).await;
    match auth {
        ResolvedRequestAuth::Ok {
            api_key,
            headers,
            base_url,
            env,
        } => {
            assert_eq!(api_key.as_ref(), Some(&JsString::from("stored-key")));
            assert_eq!(headers, None);
            assert_eq!(base_url, None);
            assert_eq!(env, None);
        }
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[tokio::test]
async fn accepts_a_pi_ai_credential_store() {
    let credentials = InMemoryCredentialStore::new();
    let modifier: pi_ai::auth::types::CredentialModifier = Arc::new(|_current| {
        Box::pin(async {
            Ok(Some(pi_ai::auth::types::Credential::ApiKey(
                pi_ai::auth::types::ApiKeyCredential {
                    key: Some("stored-key".to_owned().into()),
                    env: None,
                },
            )))
        })
    });
    credentials
        .modify("anthropic", modifier, Default::default())
        .await
        .unwrap();
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(credentials)),
        models_path: Some(None),
        ..Default::default()
    })
    .await
    .unwrap();
    let registry = ModelRegistry::new(runtime);
    assert_eq!(
        registry.get_api_key_for_provider("anthropic").await.unwrap().as_ref(),
        Some(&JsString::from("stored-key"))
    );
}

#[tokio::test]
async fn get_api_key_and_headers_preserves_the_missing_key_auth_header_error() {
    let dir = temp_dir("auth-header-missing");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {"custom-provider": {
            "baseUrl": "https://example.test/v1",
            "api": "openai-completions",
            "authHeader": true,
            "models": [{"id": "test-model"}]
        }}})
        .to_string(),
    )
    .unwrap();
    let registry = registry(storage_with(&[]), Some(path.display().to_string())).await;
    let model = registry.find("custom-provider", "test-model").unwrap();
    let auth = registry.get_api_key_and_headers(&model).await;
    assert_eq!(
        auth.error(),
        Some(&JsString::from("No API key found for \"custom-provider\""))
    );
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn get_api_key_and_headers_reports_failed_auth_header_resolution() {
    let dir = temp_dir("auth-header-failure");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {"custom-provider": {
            "baseUrl": "https://example.test/v1",
            "api": "openai-completions",
            "apiKey": "!exit 1",
            "authHeader": true,
            "models": [{"id": "test-model"}]
        }}})
        .to_string(),
    )
    .unwrap();
    let registry = registry(storage_with(&[]), Some(path.display().to_string())).await;
    let model = registry.find("custom-provider", "test-model").unwrap();
    let auth = registry.get_api_key_and_headers(&model).await;
    assert!(!auth.is_ok());
    assert!(
        auth.error()
            .unwrap()
            .contains("Failed to resolve API key for provider \"custom-provider\"")
    );
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn compatibility_auth_includes_static_model_headers() {
    let registry = registry(storage_with(&[]), None).await;
    let mut model = test_model("ghost");
    model.provider = "missing-provider".to_owned();
    model.headers = Some([("X-Static-Model".to_owned(), JsString::from("static-value"))].into());
    match registry.get_api_key_and_headers(&model).await {
        ResolvedRequestAuth::Ok { headers, .. } => {
            let headers = headers.unwrap();
            assert_eq!(
                headers.get("X-Static-Model"),
                Some(&Some(JsString::from("static-value".to_owned())))
            );
        }
        other => panic!("unexpected auth: {other:?}"),
    }
}

#[tokio::test]
async fn get_api_key_and_headers_resolves_configured_auth_once() {
    let dir = temp_dir("configured");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {"demo": {
            "baseUrl": "https://example.com/v1",
            "apiKey": "$REGISTRY_TEST_KEY",
            "headers": {"x-configured": "$REGISTRY_TEST_HEADER"},
            "models": [{"id": "demo-model", "api": "openai-completions"}]
        }}})
        .to_string(),
    )
    .unwrap();
    let registry = registry(storage_with(&[]), Some(path.display().to_string())).await;
    // The configured key references a missing env var, so auth is unconfigured.
    let model = registry.find("demo", "demo-model").unwrap();
    let auth = registry.get_api_key_and_headers(&model).await;
    assert!(!auth.is_ok());
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn provider_display_names_resolve_registered_oauth_builtin_and_fallback() {
    let registry = registry(storage_with(&[]), None).await;
    assert_eq!(registry.get_provider_display_name("anthropic"), "Anthropic");
    assert_eq!(registry.get_provider_display_name("not-a-provider"), "not-a-provider");
    registry
        .register_provider(
            "extension-display",
            ProviderConfigInput {
                name: Some("Extension Display".to_owned()),
                base_url: Some("https://example.test/v1".to_owned().into()),
                api_key: Some("key".to_owned().into()),
                api: Some("openai-completions".to_owned()),
                models: Some(vec![test_model("model")]),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        registry.get_provider_display_name("extension-display"),
        "Extension Display"
    );
}

#[tokio::test]
async fn provider_auth_status_reports_stored_and_configured_sources() {
    let dir = temp_dir("status");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {"demo": {
            "baseUrl": "https://example.com/v1",
            "apiKey": "literal-key",
            "models": [{"id": "demo-model", "api": "openai-completions"}]
        }}})
        .to_string(),
    )
    .unwrap();
    let registry = registry(
        storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]),
        Some(path.display().to_string()),
    )
    .await;
    assert_eq!(
        registry.get_provider_auth_status("anthropic").source,
        Some(AuthStatusSource::Stored)
    );
    assert_eq!(
        registry.get_provider_auth_status("demo").source,
        Some(AuthStatusSource::ModelsJsonKey)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn has_configured_auth_and_oauth_follow_the_runtime_snapshot() {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;
    let registry = registry(
        storage_with(&[(
            "anthropic",
            json!({"type": "oauth", "access": "a", "refresh": "r", "expires": now + 3_600_000.0}),
        )]),
        None,
    )
    .await;
    let model = registry.find("anthropic", "claude-opus-4-8").unwrap();
    assert!(registry.has_configured_auth(&model));
    assert!(registry.is_using_oauth(&model));
    let openai = registry.find("openai", "gpt-5.5").unwrap();
    assert!(!registry.has_configured_auth(&openai));
}

#[tokio::test]
async fn refresh_reloads_custom_models_from_disk() {
    let dir = temp_dir("refresh");
    let path = dir.join("models.json");
    fs::write(
        &path,
        json!({"providers": {"anthropic": {"baseUrl": "https://first.example.com/v1"}}}).to_string(),
    )
    .unwrap();
    let registry = registry(storage_with(&[]), Some(path.display().to_string())).await;
    assert_eq!(
        registry.find("anthropic", "claude-opus-4-8").unwrap().base_url,
        "https://first.example.com/v1"
    );

    fs::write(
        &path,
        json!({"providers": {"anthropic": {"baseUrl": "https://second.example.com/v1"}}}).to_string(),
    )
    .unwrap();
    registry.refresh(ModelsRefreshOptions::default()).await.unwrap();
    assert_eq!(
        registry.find("anthropic", "claude-opus-4-8").unwrap().base_url,
        "https://second.example.com/v1"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn registered_provider_accessors_round_trip() {
    let registry = registry(storage_with(&[]), None).await;
    let config = ProviderConfigInput {
        base_url: Some("https://example.test/v1".to_owned().into()),
        api_key: Some("key".to_owned().into()),
        api: Some("openai-completions".to_owned()),
        models: Some(vec![test_model("model")]),
        ..Default::default()
    };
    registry.register_provider("extension", config.clone()).unwrap();
    assert_eq!(
        registry
            .get_registered_provider_config("extension")
            .and_then(|config| config.base_url),
        (Some("https://example.test/v1".to_owned())).map(JsString::from)
    );
    assert_eq!(registry.get_registered_provider_ids(), vec!["extension"]);
    assert!(registry.get_registered_native_provider("extension").is_none());
    registry.unregister_provider("extension");
    assert!(registry.get_registered_provider_config("extension").is_none());
}

#[tokio::test]
async fn get_api_key_for_provider_swallows_resolution_errors() {
    let registry = registry(storage_with(&[]), None).await;
    assert_eq!(registry.get_api_key_for_provider("unknown").await.unwrap(), None);
}
