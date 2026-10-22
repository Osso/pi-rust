//! Ports provider-composition cases from `packages/coding-agent/test/model-registry.test.ts`.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use pi_ai::auth::types::{
    ApiKeyAuth, ApiKeyCheckInput, ApiKeyResolveInput, AuthCheck, AuthError, AuthResult, Credential, ModelAuth,
    ProviderAuth,
};
use pi_ai::models::{Provider, RefreshModelsContext};
use pi_ai::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{Context, DeferredHandle, Model, ModelCompat, ProviderHeaders};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_coding_agent::core::model_config::ModelConfig;
use pi_coding_agent::core::provider_composer::{
    AuthStatus, AuthStatusSource, ProviderConfigInput, compose_model_provider, configured_request_auth_status,
    models_json_provider, resolve_compatibility_request_config, resolve_configured_model_headers,
    validate_extension_provider,
};
use serde_json::{Value, json};

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-composer-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_models(dir: &std::path::Path, providers: Value) -> String {
    let path = dir.join("models.json");
    fs::write(&path, json!({"providers": providers}).to_string()).unwrap();
    path.to_string_lossy().into_owned()
}

fn model(provider: &str, id: &str, api: &str, base_url: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "api": api,
        "provider": provider,
        "baseUrl": base_url,
        "reasoning": true,
        "input": ["text"],
        "cost": {"input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 1},
        "contextWindow": 128000,
        "maxTokens": 8192,
    }))
    .unwrap()
}

struct StubAuth;

impl ApiKeyAuth for StubAuth {
    fn name(&self) -> &str {
        "Stub key"
    }

    fn resolve(&self, _input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async {
            Ok(Some(AuthResult {
                auth: ModelAuth {
                    api_key: Some("base-key".to_owned().into()),
                    headers: None,
                    base_url: None,
                },
                env: None,
                source: Some("stub".to_owned().into()),
            }))
        })
    }

    fn check(&self, _input: ApiKeyCheckInput) -> Option<BoxFuture<'static, Result<Option<AuthCheck>, AuthError>>> {
        Some(Box::pin(async {
            Ok(Some(AuthCheck {
                source: Some("stub".to_owned().into()),
                auth_type: pi_ai::auth::types::AuthType::ApiKey,
            }))
        }))
    }
}

struct TestProvider {
    id: String,
    models: Vec<Model>,
    auth: ProviderAuth,
}

impl TestProvider {
    fn boxed(id: &str, models: Vec<Model>) -> Arc<dyn Provider> {
        Arc::new(Self {
            id: id.to_owned(),
            models,
            auth: ProviderAuth::api_key(Arc::new(StubAuth)),
        })
    }

    fn oauth_only(id: &str, models: Vec<Model>) -> Arc<dyn Provider> {
        struct StubOAuth;
        impl pi_ai::auth::types::OAuthAuth for StubOAuth {
            fn name(&self) -> &str {
                "Stub OAuth"
            }
            fn login(
                &self,
                _interaction: Arc<dyn pi_ai::auth::types::AuthInteraction>,
            ) -> BoxFuture<'static, Result<pi_ai::auth::types::OAuthCredential, AuthError>> {
                Box::pin(async { Err(AuthError::failed("unused")) })
            }
            fn refresh(
                &self,
                credential: pi_ai::auth::types::OAuthCredential,
                _signal: tokio_util::sync::CancellationToken,
            ) -> BoxFuture<'static, Result<pi_ai::auth::types::OAuthCredential, AuthError>> {
                Box::pin(async move { Ok(credential) })
            }
            fn to_auth(
                &self,
                _credential: pi_ai::auth::types::OAuthCredential,
            ) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
                Box::pin(async { Ok(ModelAuth::default()) })
            }
        }
        Arc::new(Self {
            id: id.to_owned(),
            models,
            auth: ProviderAuth::oauth(Arc::new(StubOAuth)),
        })
    }
}

impl Provider for TestProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.id
    }

    fn base_url(&self) -> Option<&JsString> {
        None
    }

    fn headers(&self) -> Option<&ProviderHeaders> {
        None
    }

    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }

    fn get_models(&self) -> Vec<Model> {
        self.models.clone()
    }

    fn stream(&self, _model: Model, _context: Context, _options: StreamOptions) -> AssistantMessageEventStream {
        unimplemented!("stream is not exercised by composition tests")
    }

    fn stream_simple(
        &self,
        _model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        unimplemented!("stream is not exercised by composition tests")
    }

    fn fetch_deferred(
        &self,
        _model: Model,
        _handle: DeferredHandle,
        _options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        unimplemented!()
    }

    fn cancel_deferred(
        &self,
        _model: Model,
        _handle: DeferredHandle,
        _options: DeferredCancelOptions,
    ) -> BoxFuture<'static, Result<(), CallbackError>> {
        Box::pin(async { Ok(()) })
    }
}

fn anthropic_base() -> Arc<dyn Provider> {
    TestProvider::boxed(
        "anthropic",
        vec![
            model(
                "anthropic",
                "claude-opus-4-8",
                "anthropic-messages",
                "https://api.anthropic.com",
            ),
            model(
                "anthropic",
                "claude-sonnet-4-5",
                "anthropic-messages",
                "https://api.anthropic.com",
            ),
        ],
    )
}

#[test]
fn base_url_override_keeps_models_and_rewrites_urls() {
    let dir = temp_dir("base-url");
    let path = write_models(
        &dir,
        json!({"anthropic": {"baseUrl": "https://my-proxy.example.com/v1"}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("anthropic", Some(anthropic_base()), &config, None).unwrap();
    let models = provider.get_models();
    assert_eq!(models.len(), 2);
    assert!(
        models
            .iter()
            .all(|model| model.base_url == "https://my-proxy.example.com/v1")
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn base_url_override_does_not_affect_other_providers() {
    let dir = temp_dir("other-provider");
    let path = write_models(&dir, json!({"anthropic": {"baseUrl": "https://proxy.example.com"}}));
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let google = TestProvider::boxed(
        "google",
        vec![model(
            "google",
            "gemini-3.1-pro-preview",
            "google-generative-ai",
            "https://google.example.com",
        )],
    );
    let provider = compose_model_provider("google", Some(google), &config, None).unwrap();
    assert_eq!(provider.get_models()[0].base_url, "https://google.example.com");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_models_inherit_api_and_base_url_from_builtin_models() {
    let dir = temp_dir("inherit");
    let path = write_models(
        &dir,
        json!({"anthropic": {"models": [{"id": "claude-custom", "reasoning": true, "input": ["text"]}]}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("anthropic", Some(anthropic_base()), &config, None).unwrap();
    let custom = provider
        .get_models()
        .into_iter()
        .find(|model| model.id == "claude-custom")
        .unwrap();
    assert_eq!(custom.api, "anthropic-messages");
    assert_eq!(custom.base_url, "https://api.anthropic.com");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_models_for_unknown_provider_require_base_url() {
    let dir = temp_dir("missing-base-url");
    let path = write_models(
        &dir,
        json!({"my-custom-provider": {"apiKey": "test-key", "models": [{"id": "my-model", "api": "openai-completions", "reasoning": false, "input": ["text"]}]}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let error = compose_model_provider("my-custom-provider", None, &config, None)
        .err()
        .expect("expected composition error");
    assert!(error.to_string().contains("baseUrl"), "{error:?}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provider_without_any_configuration_reports_the_expected_error() {
    let dir = temp_dir("empty-provider");
    let path = write_models(&dir, json!({"empty": {}}));
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let error = compose_model_provider("empty", None, &config, None)
        .err()
        .expect("expected composition error");
    assert_eq!(
        error.to_string(),
        "Provider empty: must specify \"baseUrl\", \"headers\", \"compat\", \"modelOverrides\", or \"models\"."
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provider_level_base_url_applies_to_builtin_and_custom_models() {
    let dir = temp_dir("merged-url");
    let path = write_models(
        &dir,
        json!({"anthropic": {"baseUrl": "https://merged-proxy.example.com/v1", "apiKey": "test-key", "models": [{"id": "claude-custom"}]}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("anthropic", Some(anthropic_base()), &config, None).unwrap();
    let models = provider.get_models();
    assert_eq!(models.len(), 3);
    assert!(
        models
            .iter()
            .all(|model| model.base_url == "https://merged-proxy.example.com/v1")
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn custom_model_with_same_id_replaces_builtin_model() {
    let dir = temp_dir("replace");
    let path = write_models(
        &dir,
        json!({"anthropic": {"baseUrl": "https://my-proxy.example.com/v1", "models": [{"id": "claude-opus-4-8"}]}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("anthropic", Some(anthropic_base()), &config, None).unwrap();
    let models = provider.get_models();
    assert_eq!(models.len(), 2);
    let opus: Vec<_> = models.iter().filter(|model| model.id == "claude-opus-4-8").collect();
    assert_eq!(opus.len(), 1);
    assert_eq!(opus[0].base_url, "https://my-proxy.example.com/v1");
    fs::remove_dir_all(dir).unwrap();
}

fn compat_of(model: &Model) -> &pi_ai::types::OpenAICompletionsCompat {
    match model.compat.as_ref().unwrap() {
        ModelCompat::OpenAICompletions(compat) => compat,
        other => panic!("unexpected compat: {other:?}"),
    }
}

#[test]
fn provider_level_compat_applies_to_custom_and_builtin_models() {
    let dir = temp_dir("compat");
    let path = write_models(
        &dir,
        json!({
            "demo": {
                "baseUrl": "https://example.com/v1",
                "apiKey": "DEMO_KEY",
                "api": "openai-completions",
                "compat": {"supportsUsageInStreaming": false, "maxTokensField": "max_tokens"},
                "models": [{"id": "demo-model", "reasoning": false, "input": ["text"]}]
            }
        }),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("demo", None, &config, None).unwrap();
    let custom = provider
        .get_models()
        .into_iter()
        .find(|model| model.id == "demo-model")
        .unwrap();
    let compat = compat_of(&custom);
    assert_eq!(compat.supports_usage_in_streaming, Some(false));
    assert_eq!(compat.max_tokens_field, Some(pi_ai::types::MaxTokensField::MaxTokens));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn model_level_compat_overrides_provider_level_compat() {
    let dir = temp_dir("compat-override");
    let path = write_models(
        &dir,
        json!({
            "demo": {
                "baseUrl": "https://example.com/v1",
                "apiKey": "DEMO_KEY",
                "api": "openai-completions",
                "compat": {"supportsUsageInStreaming": false, "maxTokensField": "max_tokens"},
                "models": [{
                    "id": "demo-model",
                    "reasoning": false,
                    "input": ["text"],
                    "compat": {"supportsUsageInStreaming": true, "maxTokensField": "max_completion_tokens"}
                }]
            }
        }),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("demo", None, &config, None).unwrap();
    let custom = provider
        .get_models()
        .into_iter()
        .find(|model| model.id == "demo-model")
        .unwrap();
    let compat = compat_of(&custom);
    assert_eq!(compat.supports_usage_in_streaming, Some(true));
    assert_eq!(
        compat.max_tokens_field,
        Some(pi_ai::types::MaxTokensField::MaxCompletionTokens)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn model_overrides_apply_to_single_models_and_merge_nested_compat() {
    let dir = temp_dir("overrides");
    let path = write_models(
        &dir,
        json!({
            "anthropic": {
                "modelOverrides": {
                    "claude-opus-4-8": {
                        "name": "Renamed",
                        "cost": {"input": 9},
                        "samplingParams": {"temperature": 0.5}
                    }
                }
            }
        }),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("anthropic", Some(anthropic_base()), &config, None).unwrap();
    let models = provider.get_models();
    let opus = models.iter().find(|model| model.id == "claude-opus-4-8").unwrap();
    let sonnet = models.iter().find(|model| model.id == "claude-sonnet-4-5").unwrap();
    assert_eq!(opus.name, "Renamed");
    assert_eq!(opus.cost.rates.input, 9.0);
    assert_eq!(opus.cost.rates.output, 2.0);
    assert_eq!(
        opus.sampling_params
            .as_ref()
            .and_then(|params| params.get("temperature")),
        Some(&json!(0.5))
    );
    assert_eq!(sonnet.name, "claude-sonnet-4-5");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn model_override_for_missing_model_is_ignored() {
    let dir = temp_dir("override-missing");
    let path = write_models(
        &dir,
        json!({"anthropic": {"modelOverrides": {"missing": {"name": "Nope"}}}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("anthropic", Some(anthropic_base()), &config, None).unwrap();
    assert!(provider.get_models().iter().all(|model| model.id != "missing"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn configured_headers_resolve_at_request_time() {
    let dir = temp_dir("headers");
    let path = write_models(
        &dir,
        json!({"anthropic": {"baseUrl": "https://proxy.example.com", "headers": {"X-Custom-Header": "custom-value"}}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("anthropic", Some(anthropic_base()), &config, None).unwrap();
    let typed = models_json_provider(&config, "anthropic").unwrap();
    for model in provider.get_models() {
        let headers = resolve_configured_model_headers(&model, typed.as_ref(), None, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            headers.get("X-Custom-Header").cloned(),
            Some(JsString::from("custom-value"))
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn model_level_headers_merge_over_provider_headers() {
    let dir = temp_dir("model-headers");
    let path = write_models(
        &dir,
        json!({
            "demo": {
                "baseUrl": "https://example.com/v1",
                "apiKey": "DEMO_KEY",
                "api": "openai-completions",
                "headers": {"x-shared": "provider"},
                "models": [{"id": "demo-model", "headers": {"x-model": "model", "x-shared": "model"}}]
            }
        }),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("demo", None, &config, None).unwrap();
    let typed = models_json_provider(&config, "demo").unwrap();
    let model = provider
        .get_models()
        .into_iter()
        .find(|model| model.id == "demo-model")
        .unwrap();
    let headers = resolve_configured_model_headers(&model, typed.as_ref(), None, None)
        .unwrap()
        .unwrap();
    assert_eq!(headers.get("x-model").cloned(), Some(JsString::from("model")));
    assert_eq!(headers.get("x-shared").cloned(), Some(JsString::from("model")));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn compatibility_request_config_includes_static_model_headers() {
    let model = {
        let mut model = model(
            "missing-provider",
            "ghost",
            "openai-completions",
            "https://example.test",
        );
        model.headers = Some([("X-Static-Model".to_owned(), JsString::from("static-value"))].into());
        model
    };
    let compatibility = resolve_compatibility_request_config(&model, None, None).unwrap();
    assert!(!compatibility.auth_header);
    let headers = compatibility.headers.unwrap();
    assert_eq!(
        headers.get("X-Static-Model"),
        Some(&Some(JsString::from("static-value".to_owned())))
    );
}

#[test]
fn configured_auth_status_reports_env_and_literal_sources() {
    let provider = |api_key: &str| pi_coding_agent::core::provider_composer::ModelsJsonProvider {
        api_key: Some(api_key.to_owned().into()),
        ..Default::default()
    };

    let status = configured_request_auth_status(Some(&provider("$MISSING_COMPOSER_KEY")), None).unwrap();
    assert_eq!(
        status,
        AuthStatus {
            configured: false,
            source: None,
            label: None
        }
    );

    let status = configured_request_auth_status(Some(&provider("literal-key")), None).unwrap();
    assert_eq!(status.source, Some(AuthStatusSource::ModelsJsonKey));

    let status = configured_request_auth_status(Some(&provider("!echo key")), None).unwrap();
    assert_eq!(status.source, Some(AuthStatusSource::ModelsJsonCommand));

    let extension = ProviderConfigInput {
        api_key: Some("literal".to_owned().into()),
        ..Default::default()
    };
    let status = configured_request_auth_status(None, Some(&extension)).unwrap();
    assert_eq!(status.source, Some(AuthStatusSource::Fallback));
}

#[test]
fn extension_stream_simple_requires_api() {
    let extension = ProviderConfigInput {
        stream_simple: Some(Arc::new(|_model, _context, _options| unimplemented!("not called"))),
        ..Default::default()
    };
    let error = match validate_extension_provider("ext", None, None, &extension) {
        Ok(()) => panic!("expected composition error"),
        Err(error) => error,
    };
    assert_eq!(
        error.to_string(),
        "Provider ext: \"api\" is required when registering streamSimple."
    );
}

#[test]
fn extension_models_override_provider_and_base_url() {
    let extension = ProviderConfigInput {
        base_url: Some("https://extension.example.com".to_owned().into()),
        api: Some("openai-completions".to_owned()),
        models: Some(vec![model(
            "ignored",
            "ext-model",
            "openai-completions",
            "https://model.example.com",
        )]),
        ..Default::default()
    };
    validate_extension_provider("ext", None, None, &extension).unwrap();

    let dir = temp_dir("extension");
    let path = write_models(&dir, json!({}));
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("ext", None, &config, Some(&extension)).unwrap();
    let models = provider.get_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].provider, "ext");
    assert_eq!(models[0].base_url, "https://model.example.com");
    assert!(models[0].headers.is_none());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn oauth_only_extension_has_no_api_key_method() {
    let dir = temp_dir("oauth-only");
    let path = write_models(&dir, json!({}));
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let extension = ProviderConfigInput {
        name: Some("Extension OAuth".to_owned()),
        base_url: Some("https://example.test/v1".to_owned().into()),
        api: Some("openai-completions".to_owned()),
        oauth: Some(pi_coding_agent::core::provider_composer::ExtensionOAuthConfig {
            name: "Extension subscription".to_owned(),
            is_subscription: Some(true),
            uses_callback_server: None,
            login: Arc::new(|_callbacks| Box::pin(async { Err(AuthError::failed("unused")) })),
            refresh_token: Arc::new(|credential, _signal| Box::pin(async move { Ok(credential) })),
            get_api_key: Arc::new(|credential| credential.access.to_utf8().expect("scalar fixture key").into()),
            modify_models: None,
        }),
        models: Some(vec![model(
            "ignored",
            "extension-model",
            "openai-completions",
            "https://example.test/v1",
        )]),
        ..Default::default()
    };
    let provider = compose_model_provider("extension-oauth", None, &config, Some(&extension)).unwrap();
    assert!(provider.auth().api_key.is_none());
    assert!(provider.auth().oauth.is_some());
    assert_eq!(provider.name(), "Extension OAuth");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn composed_provider_keeps_base_oauth_when_no_extension() {
    let dir = temp_dir("base-oauth");
    let path = write_models(&dir, json!({}));
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let base = TestProvider::oauth_only(
        "anthropic",
        vec![model(
            "anthropic",
            "claude-opus-4-8",
            "anthropic-messages",
            "https://api.anthropic.com",
        )],
    );
    let provider = compose_model_provider("anthropic", Some(base), &config, None).unwrap();
    assert!(provider.auth().oauth.is_some());
}

#[test]
fn provider_without_configured_key_still_gets_a_synthetic_api_key_method() {
    let dir = temp_dir("synthetic-key");
    let path = write_models(
        &dir,
        json!({"demo": {"baseUrl": "https://example.com/v1", "models": [{"id": "demo-model", "api": "openai-completions"}]}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("demo", None, &config, None).unwrap();
    assert!(provider.auth().api_key.is_some());
    assert!(provider.auth().oauth.is_none());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn provider_credentials_are_not_read_during_composition() {
    let dir = temp_dir("credential-blind");
    let path = write_models(
        &dir,
        json!({"demo": {"baseUrl": "https://example.com/v1", "apiKey": "!false", "models": [{"id": "demo-model", "api": "openai-completions"}]}}),
    );
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let provider = compose_model_provider("demo", None, &config, None).unwrap();
    assert_eq!(provider.get_models().len(), 1);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn headers_map_helper_is_available_for_fixtures() {
    let mut headers = HashMap::new();
    headers.insert("x".to_owned(), "y".to_owned().into());
    let converted = pi_coding_agent::core::provider_composer::provider_headers(headers);
    assert_eq!(converted.get("x"), Some(&Some(JsString::from("y".to_owned()))));
    assert!(matches!(
        pi_coding_agent::core::provider_composer::modality_from_str("image"),
        Some(pi_ai::types::Modality::Image)
    ));
    assert_eq!(
        pi_coding_agent::core::provider_composer::thinking_level_key("xhigh"),
        Some(pi_ai::types::ModelThinkingLevel::Xhigh)
    );
    let credential = Credential::ApiKey(pi_ai::auth::types::ApiKeyCredential::default());
    let _ = credential;
}

#[test]
fn refresh_models_publishes_extension_results() {
    use pi_ai::models::{ModelsPublication, PublishFn};
    use pi_ai::models_store::ModelsStoreEntry;
    use tokio_util::sync::CancellationToken;

    let dir = temp_dir("refresh");
    let path = write_models(&dir, json!({}));
    let config = ModelConfig::load(Some(&JsString::from(&path)));
    let published = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let extension = ProviderConfigInput {
        base_url: Some("https://example.test/v1".to_owned().into()),
        api: Some("openai-completions".to_owned()),
        api_key: Some("key".to_owned().into()),
        models: Some(vec![model(
            "ignored",
            "initial",
            "openai-completions",
            "https://example.test/v1",
        )]),
        refresh_models: Some(Arc::new(|_context: RefreshModelsContext| {
            Box::pin(async {
                Ok(vec![model(
                    "ignored",
                    "refreshed",
                    "openai-completions",
                    "https://example.test/v1",
                )])
            })
        })),
        ..Default::default()
    };
    let provider = compose_model_provider("ext", None, &config, Some(&extension)).unwrap();
    assert_eq!(provider.get_models()[0].id, "initial");

    let publish: PublishFn = {
        let published = Arc::clone(&published);
        Arc::new(move |publication: ModelsPublication| {
            let published = Arc::clone(&published);
            Box::pin(async move {
                if let Some(update) = &publication.update {
                    update();
                    published.lock().unwrap().push("updated".to_owned());
                }
                Ok(true)
            })
        })
    };
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    runtime.block_on(async {
        let refresh = provider.refresh_models(RefreshModelsContext {
            credential: None,
            stored: None::<ModelsStoreEntry>,
            publish,
            allow_network: false,
            force: None,
            signal: CancellationToken::new(),
        });
        refresh.unwrap().await.unwrap();
    });
    assert_eq!(provider.get_models()[0].id, "refreshed");
    assert_eq!(published.lock().unwrap().len(), 2);
    fs::remove_dir_all(dir).unwrap();
}
