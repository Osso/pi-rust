//! Behavioral tests for `extensions/llama/provider.ts`.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{
    ApiKeyCheckInput, ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthInteraction, AuthPrompt, AuthPromptKind,
    Credential,
};
use pi_ai::models::{ModelsPublication, PersistAction, RefreshModelsContext};
use pi_ai::models_store::ModelsStoreEntry;
use pi_ai::request::{BoxFuture, CallbackError};
use pi_ai::types::{Modality, ProviderEnv};
use pi_coding_agent::bundled_extensions::llama::client::LlamaModelInfo;
use pi_coding_agent::bundled_extensions::llama::provider::{
    DEFAULT_LLAMA_SERVER_URL, LLAMA_PROVIDER_ID, create_llama_provider, create_llama_provider_with_fetch,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[path = "support/llama_fetch.rs"]
mod llama_fetch;

use llama_fetch::{HandlerFetch, json_response};

fn catalog(entries: &[Value]) -> Vec<LlamaModelInfo> {
    entries
        .iter()
        .map(|entry| serde_json::from_value(entry.clone()).unwrap())
        .collect()
}

#[test]
fn registers_the_native_provider_metadata() {
    let controller = create_llama_provider();
    let provider = controller.provider();
    assert_eq!(provider.id(), LLAMA_PROVIDER_ID);
    assert_eq!(provider.name(), "llama.cpp");
    assert_eq!(
        provider.base_url(),
        Some("http://127.0.0.1:8080/v1"),
        "default server URL {DEFAULT_LLAMA_SERVER_URL}"
    );
    let auth = provider.auth();
    assert_eq!(auth.api_key.as_ref().unwrap().name(), "llama.cpp server");
    assert!(auth.oauth.is_none());
    assert!(provider.get_models().is_empty());
}

#[test]
fn exposes_only_loaded_models_with_router_metadata() {
    let controller = create_llama_provider();
    controller
        .set_catalog(
            &catalog(&[
                json!({
                    "id": "loaded",
                    "status": {"value": "loaded", "args": ["llama-server", "--n-gpu-layers", "999"]},
                    "architecture": {"input_modalities": ["text", "image"]},
                    "meta": {"n_ctx": 65536, "n_ctx_train": 131072}
                }),
                json!({"id": "unloaded", "status": {"value": "unloaded"}}),
                json!({"id": "loading", "status": {"value": "loading"}}),
            ]),
            "http://localhost:8080",
        )
        .unwrap();
    let models = controller.provider().get_models();
    assert_eq!(models.len(), 1);
    let model = &models[0];
    assert_eq!(model.id, "loaded");
    assert_eq!(model.name, "loaded");
    assert_eq!(model.api, "openai-completions");
    assert_eq!(model.provider, LLAMA_PROVIDER_ID);
    assert_eq!(model.base_url, "http://localhost:8080/v1");
    assert!(!model.reasoning);
    assert_eq!(model.input, vec![Modality::Text, Modality::Image]);
    assert_eq!(model.context_window, 65536.0);
    assert_eq!(model.max_tokens, 65536.0);
    let compat = serde_json::to_value(model.compat.as_ref().unwrap()).unwrap();
    assert_eq!(
        compat,
        json!({
            "supportsStore": false,
            "supportsDeveloperRole": false,
            "supportsReasoningEffort": false,
            "supportsUsageInStreaming": true,
            "supportsStrictMode": false,
            "maxTokensField": "max_tokens"
        })
    );
    let cost = serde_json::to_value(&model.cost).unwrap();
    assert_eq!(cost, json!({"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}));
}

#[test]
fn falls_back_to_128k_and_text_only_without_metadata() {
    let controller = create_llama_provider();
    controller
        .set_catalog(
            &catalog(&[json!({"id": "m", "status": {"value": "loaded"}})]),
            "http://localhost:8080",
        )
        .unwrap();
    let model = &controller.provider().get_models()[0];
    assert_eq!(model.context_window, 128000.0);
    assert_eq!(model.input, vec![Modality::Text]);
}

struct EnvContext(BTreeMap<String, String>);

impl AuthContext for EnvContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<String>> {
        let value = self.0.get(name).cloned();
        Box::pin(async move { value })
    }

    fn file_exists(&self, _path: &str) -> BoxFuture<'_, bool> {
        Box::pin(async { false })
    }
}

fn context(env: &[(&str, &str)]) -> Arc<dyn AuthContext> {
    Arc::new(EnvContext(
        env.iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
    ))
}

#[tokio::test]
async fn stays_dormant_until_configured() {
    let controller = create_llama_provider();
    let auth = controller.provider().auth().api_key.clone().unwrap();
    let empty = context(&[]);
    let signal = CancellationToken::new();
    assert!(
        auth.check(ApiKeyCheckInput {
            ctx: Arc::clone(&empty),
            credential: None,
            signal: signal.clone(),
        })
        .unwrap()
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        auth.resolve(ApiKeyResolveInput {
            ctx: empty,
            credential: None,
            signal,
        })
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn resolves_stored_credentials_and_environment() {
    let controller = create_llama_provider();
    let auth = controller.provider().auth().api_key.clone().unwrap();
    let credential = ApiKeyCredential {
        key: Some("secret".to_owned()),
        env: Some(ProviderEnv::from([(
            "LLAMA_BASE_URL".to_owned(),
            "http://127.0.0.1:9999/v1/".to_owned(),
        )])),
    };
    let signal = CancellationToken::new();
    let check = auth
        .check(ApiKeyCheckInput {
            ctx: context(&[]),
            credential: Some(credential.clone()),
            signal: signal.clone(),
        })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(check.source.as_deref(), Some("stored credential"));

    let resolved = auth
        .resolve(ApiKeyResolveInput {
            ctx: context(&[]),
            credential: Some(credential),
            signal: signal.clone(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.auth.api_key.as_deref(), Some("secret"));
    assert_eq!(resolved.auth.base_url.as_deref(), Some("http://127.0.0.1:9999/v1"));
    assert_eq!(resolved.source.as_deref(), Some("stored credential"));
    assert_eq!(
        resolved.env.unwrap().get("LLAMA_BASE_URL").map(String::as_str),
        Some("http://127.0.0.1:9999")
    );

    let resolved = auth
        .resolve(ApiKeyResolveInput {
            ctx: context(&[("LLAMA_BASE_URL", "http://env:8080"), ("LLAMA_API_KEY", "env-key")]),
            credential: None,
            signal,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.auth.api_key.as_deref(), Some("env-key"));
    assert_eq!(resolved.auth.base_url.as_deref(), Some("http://env:8080/v1"));
    assert_eq!(resolved.source.as_deref(), Some("LLAMA_BASE_URL"));

    let resolved = auth
        .resolve(ApiKeyResolveInput {
            ctx: context(&[("LLAMA_BASE_URL", "http://env:8080")]),
            credential: None,
            signal: CancellationToken::new(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.auth.api_key.as_deref(), Some("local"));
}

struct ScriptedInteraction {
    signal: CancellationToken,
    answers: Mutex<VecDeque<String>>,
    prompts: Mutex<Vec<AuthPrompt>>,
}

impl AuthInteraction for ScriptedInteraction {
    fn signal(&self) -> CancellationToken {
        self.signal.clone()
    }

    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<String, CallbackError>> {
        self.prompts.lock().unwrap().push(prompt);
        let answer = self.answers.lock().unwrap().pop_front().unwrap_or_default();
        Box::pin(async move { Ok(answer) })
    }

    fn notify(&self, _event: pi_ai::auth::types::AuthEvent) {}
}

#[tokio::test]
async fn logs_in_with_url_and_optional_key() {
    let fetch = HandlerFetch::new(Arc::new(|request| {
        assert_eq!(request.url, "http://127.0.0.1:8080/models");
        assert_eq!(
            llama_fetch::header(request, "authorization").as_deref(),
            Some("Bearer secret")
        );
        json_response(200, &json!({"data": []}))
    }));
    let controller = create_llama_provider_with_fetch(fetch.clone());
    let auth = controller.provider().auth().api_key.clone().unwrap();
    let interaction = Arc::new(ScriptedInteraction {
        signal: CancellationToken::new(),
        answers: Mutex::new(VecDeque::from([
            "http://127.0.0.1:8080/v1/".to_owned(),
            " secret ".to_owned(),
        ])),
        prompts: Mutex::new(Vec::new()),
    });
    let credential = auth.login(interaction.clone()).unwrap().await.unwrap();
    assert_eq!(credential.key.as_deref(), Some("secret"));
    assert_eq!(
        credential
            .env
            .as_ref()
            .unwrap()
            .get("LLAMA_BASE_URL")
            .map(String::as_str),
        Some("http://127.0.0.1:8080")
    );
    let prompts = interaction.prompts.lock().unwrap();
    assert_eq!(
        prompts[0].kind,
        AuthPromptKind::Text {
            message: "llama.cpp server URL".to_owned(),
            placeholder: Some(DEFAULT_LLAMA_SERVER_URL.to_owned()),
        }
    );
    assert_eq!(
        prompts[1].kind,
        AuthPromptKind::Secret {
            message: "API key (optional)".to_owned(),
            placeholder: None,
        }
    );
}

fn entry(models: Vec<pi_ai::types::Model>) -> ModelsStoreEntry {
    ModelsStoreEntry {
        models,
        last_modified: None,
        checked_at: Some(1.0),
        etag: None,
    }
}

#[tokio::test]
async fn restores_stored_models_without_network() {
    let controller = create_llama_provider();
    controller
        .set_catalog(
            &catalog(&[json!({"id": "loaded", "status": {"value": "loaded"}})]),
            "http://127.0.0.1:8080",
        )
        .unwrap();
    let stored = entry(controller.provider().get_models());
    let published = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&published);
    let publish: pi_ai::models::PublishFn = Arc::new(move |publication: ModelsPublication| {
        let sink = Arc::clone(&sink);
        Box::pin(async move {
            sink.lock().unwrap().push(publication.persist.clone());
            if let Some(update) = &publication.update {
                update();
            }
            Ok(true)
        })
    });
    controller
        .provider()
        .refresh_models(RefreshModelsContext {
            credential: Some(Credential::ApiKey(ApiKeyCredential {
                key: Some("local".to_owned()),
                env: None,
            })),
            stored: Some(stored),
            publish,
            allow_network: false,
            force: None,
            signal: CancellationToken::new(),
        })
        .unwrap()
        .await
        .unwrap();
    assert_eq!(controller.provider().get_models().len(), 1);
    assert_eq!(published.lock().unwrap().len(), 1);
    assert!(matches!(published.lock().unwrap()[0], PersistAction::Unchanged));
}

#[tokio::test]
async fn refreshes_loaded_models_and_persists_the_catalog() {
    let fetch = HandlerFetch::new(Arc::new(|request| {
        assert_eq!(request.url, "http://127.0.0.1:8080/models");
        json_response(
            200,
            &json!({"data": [
                {"id": "loaded", "status": {"value": "loaded"}, "meta": {"n_ctx": 32768}},
                {"id": "unloaded", "status": {"value": "unloaded"}}
            ]}),
        )
    }));
    let controller = create_llama_provider_with_fetch(fetch.clone());
    let published = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&published);
    let publish: pi_ai::models::PublishFn = Arc::new(move |publication: ModelsPublication| {
        let sink = Arc::clone(&sink);
        Box::pin(async move {
            sink.lock().unwrap().push(publication.persist.clone());
            if let Some(update) = &publication.update {
                update();
            }
            Ok(true)
        })
    });
    let mut env = ProviderEnv::new();
    env.insert("LLAMA_BASE_URL".to_owned(), "http://127.0.0.1:8080".to_owned());
    controller
        .provider()
        .refresh_models(RefreshModelsContext {
            credential: Some(Credential::ApiKey(ApiKeyCredential {
                key: Some("local".to_owned()),
                env: Some(env),
            })),
            stored: None,
            publish,
            allow_network: true,
            force: None,
            signal: CancellationToken::new(),
        })
        .unwrap()
        .await
        .unwrap();
    let models = controller.provider().get_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "loaded");
    assert_eq!(models[0].context_window, 32768.0);
    let persisted = published.lock().unwrap();
    match &persisted[0] {
        PersistAction::Write(entry) => {
            assert_eq!(entry.models.len(), 1);
            assert!(entry.checked_at.is_some());
        }
        _ => panic!("expected a persisted catalog"),
    }
}

#[tokio::test]
async fn skips_network_refresh_when_disabled() {
    let fetch = HandlerFetch::new(Arc::new(|_| panic!("network must not be used")));
    let controller = create_llama_provider_with_fetch(fetch.clone());
    let publish: pi_ai::models::PublishFn = Arc::new(|_| Box::pin(async { Ok(true) }));
    controller
        .provider()
        .refresh_models(RefreshModelsContext {
            credential: Some(Credential::ApiKey(ApiKeyCredential {
                key: Some("local".to_owned()),
                env: None,
            })),
            stored: None,
            publish,
            allow_network: false,
            force: None,
            signal: CancellationToken::new(),
        })
        .unwrap()
        .await
        .unwrap();
    assert!(fetch.requests().is_empty());
}
