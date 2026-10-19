#[path = "../support/llama_fetch.rs"]
mod llama_fetch;
#[path = "../support/oracle.rs"]
mod oracle;

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{
    ApiKeyCheckInput, ApiKeyCredential, ApiKeyResolveInput, AuthContext, AuthInteraction, AuthPrompt, AuthPromptKind,
    Credential,
};
use pi_ai::models::{ModelsPublication, PersistAction, RefreshModelsContext};
use pi_ai::models_store::ModelsStoreEntry;
use pi_ai::request::{BoxFuture, CallbackError};
use pi_ai::types::{Model, ProviderEnv};
use pi_coding_agent::bundled_extensions::llama::client::LlamaModelInfo;
use pi_coding_agent::bundled_extensions::llama::provider::{create_llama_provider, create_llama_provider_with_fetch};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use llama_fetch::{HandlerFetch, json_response};

const SOURCE_MODULE: &str = "packages/coding-agent/src/extensions/llama/provider.ts";
const RUST_BASE: &str = "http://127.0.0.1:8080";

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("llama-provider-oracle.ts", SOURCE_MODULE);
    compare_catalog(&mut oracle);
    compare_auth(&mut oracle);
    compare_login(&mut oracle);
    compare_refresh(&mut oracle);
}

struct JsNumber(f64);

impl serde::Serialize for JsNumber {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        pi_js_compat::number::serialize(&self.0, serializer)
    }
}

fn js_number(value: f64) -> Value {
    serde_json::to_value(JsNumber(value)).unwrap()
}

/// Serializes a model in the `provider.ts` literal key order (the compat literal
/// puts `supportsStrictMode` before `maxTokensField`, unlike the shared type).
fn model_json(model: &Model) -> Value {
    let mut value = json!({
        "id": model.id,
        "name": model.name,
        "api": model.api,
        "provider": model.provider,
        "baseUrl": model.base_url,
        "reasoning": model.reasoning,
        "input": model.input,
        "cost": model.cost,
        "contextWindow": js_number(model.context_window),
        "maxTokens": js_number(model.max_tokens),
    });
    if let Some(compat) = model.compat.as_ref() {
        let compat = match compat {
            pi_ai::types::ModelCompat::OpenAICompletions(compat) => json!({
                "supportsStore": compat.supports_store,
                "supportsDeveloperRole": compat.supports_developer_role,
                "supportsReasoningEffort": compat.supports_reasoning_effort,
                "supportsUsageInStreaming": compat.supports_usage_in_streaming,
                "supportsStrictMode": compat.supports_strict_mode,
                "maxTokensField": compat.max_tokens_field.map(|field| match field {
                    pi_ai::types::MaxTokensField::MaxCompletionTokens => "max_completion_tokens",
                    pi_ai::types::MaxTokensField::MaxTokens => "max_tokens",
                }),
            }),
            _ => panic!("llama models use openai-completions compat"),
        };
        value["compat"] = compat;
    }
    value
}

fn models_json(models: &[Model]) -> Value {
    Value::Array(models.iter().map(model_json).collect())
}

fn catalog(entries: &[Value]) -> Vec<LlamaModelInfo> {
    entries
        .iter()
        .map(|entry| serde_json::from_value(entry.clone()).unwrap())
        .collect()
}

fn compare_catalog(oracle: &mut oracle::TsOracle) {
    let models = json!([
        {
            "id": "loaded",
            "status": {"value": "loaded", "args": ["llama-server", "-c", "4096"]},
            "architecture": {"input_modalities": ["text", "image"]},
            "meta": {"n_ctx": 65536, "n_ctx_train": 131072}
        },
        {"id": "unloaded", "status": {"value": "unloaded"}},
        {"id": "loading", "status": {"value": "loading"}},
        {"id": "text", "status": {"value": "loaded"}, "meta": {"n_ctx_train": 4096}}
    ]);
    let response = oracle.request(&json!({
        "id": "catalog",
        "op": "catalog",
        "serverUrl": "http://localhost:8080",
        "models": models,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let controller = create_llama_provider();
    controller
        .set_catalog(&catalog(models.as_array().unwrap()), "http://localhost:8080")
        .unwrap();
    let rust = models_json(&controller.provider().get_models());
    assert_eq!(
        serde_json::to_string(&response["output"]["models"]).unwrap(),
        serde_json::to_string(&rust).unwrap()
    );
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

fn auth_json(check: Option<pi_ai::auth::types::AuthCheck>, resolved: Option<pi_ai::auth::types::AuthResult>) -> Value {
    let check = check.map(|check| {
        json!({
            "type": check.auth_type.as_str(),
            "source": check.source,
        })
    });
    json!({ "check": check, "resolve": resolved })
}

fn compare_auth(oracle: &mut oracle::TsOracle) {
    let cases = json!([
        {"env": {}},
        {"env": {"LLAMA_BASE_URL": "http://env:8080"}},
        {"env": {"LLAMA_BASE_URL": "http://env:8080", "LLAMA_API_KEY": "env-key"}},
        {"env": {}, "credential": {"type": "api_key", "key": "secret", "env": {"LLAMA_BASE_URL": "http://127.0.0.1:9999/v1/"}}},
        {"env": {}, "credential": {"type": "api_key", "key": "", "env": {"LLAMA_BASE_URL": "http://127.0.0.1:9999"}}}
    ]);
    let response = oracle.request(&json!({"id": "auth", "op": "auth", "cases": cases}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected = response["output"]["results"].as_array().unwrap().clone();

    let controller = create_llama_provider();
    let auth = controller.provider().auth().api_key.clone().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for (case, expected) in cases.as_array().unwrap().iter().zip(expected) {
        let env: BTreeMap<String, String> = case["env"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_owned()))
            .collect();
        let credential = case.get("credential").map(|credential| ApiKeyCredential {
            key: credential.get("key").and_then(Value::as_str).map(str::to_owned),
            env: credential.get("env").map(|env| {
                env.as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_owned()))
                    .collect()
            }),
        });
        let ctx: Arc<dyn AuthContext> = Arc::new(EnvContext(env));
        let signal = CancellationToken::new();
        let check = runtime
            .block_on(
                auth.check(ApiKeyCheckInput {
                    ctx: Arc::clone(&ctx),
                    credential: credential.clone(),
                    signal: signal.clone(),
                })
                .unwrap(),
            )
            .unwrap();
        let resolved = runtime
            .block_on(auth.resolve(ApiKeyResolveInput {
                ctx,
                credential,
                signal,
            }))
            .unwrap();
        let rust = serde_json::to_value(auth_json(check, resolved)).unwrap();
        assert_eq!(
            serde_json::to_string(&expected).unwrap(),
            serde_json::to_string(&rust).unwrap()
        );
    }
    assert_eq!(response["output"]["name"], "llama.cpp server");
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

fn prompt_json(prompt: &AuthPrompt) -> Value {
    let (prompt_type, message, placeholder) = match &prompt.kind {
        AuthPromptKind::Text { message, placeholder } => ("text", message, placeholder),
        AuthPromptKind::Secret { message, placeholder } => ("secret", message, placeholder),
        other => panic!("unexpected prompt {other:?}"),
    };
    let mut value = json!({"type": prompt_type, "message": message});
    if let Some(placeholder) = placeholder {
        value["placeholder"] = json!(placeholder);
    }
    value
}

fn compare_login(oracle: &mut oracle::TsOracle) {
    let response = oracle.request(&json!({
        "id": "login",
        "op": "login",
        "answers": ["\u{0}url", " secret "],
        "catalog": {"data": []},
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let output = &response["output"];
    let oracle_base = output["baseUrl"].as_str().unwrap();

    let fetch = HandlerFetch::new(Arc::new(|_| json_response(200, &json!({"data": []}))));
    let controller = create_llama_provider_with_fetch(fetch.clone());
    let auth = controller.provider().auth().api_key.clone().unwrap();
    let interaction = Arc::new(ScriptedInteraction {
        signal: CancellationToken::new(),
        answers: Mutex::new(VecDeque::from([format!("{RUST_BASE}/v1/"), " secret ".to_owned()])),
        prompts: Mutex::new(Vec::new()),
    });
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let credential = runtime.block_on(auth.login(interaction.clone()).unwrap()).unwrap();
    let credential = json!({
        "type": "api_key",
        "key": credential.key,
        "env": credential.env,
    });
    let credential = replace_base(&credential, RUST_BASE);
    let expected_credential = replace_base(&output["credential"], oracle_base);
    assert_eq!(
        serde_json::to_string(&expected_credential).unwrap(),
        serde_json::to_string(&credential).unwrap()
    );

    let prompts = interaction
        .prompts
        .lock()
        .unwrap()
        .iter()
        .map(prompt_json)
        .collect::<Vec<_>>();
    assert_eq!(
        serde_json::to_string(&output["prompts"]).unwrap(),
        serde_json::to_string(&prompts).unwrap()
    );

    let requests = fetch
        .requests()
        .iter()
        .map(|request| {
            json!({
                "url": request.url.strip_prefix(RUST_BASE).unwrap_or(&request.url),
                "authorization": llama_fetch::header(request, "authorization"),
            })
        })
        .collect::<Vec<_>>();
    let expected_requests = output["requests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|request| {
            json!({
                "url": request["url"],
                "authorization": request["authorization"],
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        serde_json::to_string(&expected_requests).unwrap(),
        serde_json::to_string(&requests).unwrap()
    );
}

fn replace_base(value: &Value, base: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(base, "<base>")),
        Value::Array(values) => Value::Array(values.iter().map(|value| replace_base(value, base)).collect()),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), replace_base(value, base)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn publication_json(publication: &ModelsPublication) -> Value {
    match &publication.persist {
        PersistAction::Unchanged | PersistAction::Delete => Value::Null,
        PersistAction::Write(entry) => json!({
            "models": models_json(&entry.models),
            "checkedAt": 0,
        }),
    }
}

fn compare_refresh(oracle: &mut oracle::TsOracle) {
    let catalog_json = json!({"data": [
        {"id": "loaded", "status": {"value": "loaded"}, "meta": {"n_ctx": 32768}},
        {"id": "unloaded", "status": {"value": "unloaded"}}
    ]});
    let stored_models = json!([
        {
            "id": "loaded", "name": "loaded", "api": "openai-completions", "provider": "llama.cpp",
            "baseUrl": "http://stored/v1", "reasoning": false, "input": ["text"],
            "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
            "contextWindow": 32768, "maxTokens": 32768
        },
        {
            "id": "other", "name": "other", "api": "openai-completions", "provider": "other",
            "baseUrl": "http://other/v1", "reasoning": false, "input": ["text"],
            "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
            "contextWindow": 1000, "maxTokens": 1000
        }
    ]);
    let scenarios = [
        json!({"stored": null, "allowNetwork": true, "publishResult": true, "catalog": catalog_json}),
        json!({"stored": {"models": stored_models}, "allowNetwork": false, "publishResult": true, "catalog": catalog_json}),
        json!({"stored": {"models": stored_models}, "allowNetwork": true, "publishResult": false, "catalog": catalog_json}),
    ];
    for scenario in scenarios {
        let response = oracle.request(&json!({
            "id": "refresh",
            "op": "refresh",
            "credentialEnv": "<base>",
            "stored": scenario["stored"],
            "allowNetwork": scenario["allowNetwork"],
            "publishResult": scenario["publishResult"],
            "catalog": scenario["catalog"],
        }));
        assert_eq!(response["ok"], true, "oracle failed: {response}");
        let output = &response["output"];
        let oracle_base = output["baseUrl"].as_str().unwrap();

        let fetch = HandlerFetch::new(Arc::new({
            let catalog = scenario["catalog"].clone();
            move |_| json_response(200, &catalog)
        }));
        let controller = create_llama_provider_with_fetch(fetch.clone());
        let stored = scenario["stored"].is_object().then(|| ModelsStoreEntry {
            models: serde_json::from_value::<Vec<Model>>(scenario["stored"]["models"].clone()).unwrap(),
            last_modified: None,
            checked_at: Some(1.0),
            etag: None,
        });
        let publications = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&publications);
        let publish_result = scenario["publishResult"].as_bool().unwrap();
        let publish: pi_ai::models::PublishFn = Arc::new(move |publication: ModelsPublication| {
            let sink = Arc::clone(&sink);
            Box::pin(async move {
                sink.lock().unwrap().push(publication_json(&publication));
                if let Some(update) = &publication.update {
                    update();
                }
                Ok(publish_result)
            })
        });
        let mut env = ProviderEnv::new();
        env.insert("LLAMA_BASE_URL".to_owned(), RUST_BASE.to_owned());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(
                controller
                    .provider()
                    .refresh_models(RefreshModelsContext {
                        credential: Some(Credential::ApiKey(ApiKeyCredential {
                            key: Some("local".to_owned()),
                            env: Some(env),
                        })),
                        stored,
                        publish,
                        allow_network: scenario["allowNetwork"].as_bool().unwrap(),
                        force: None,
                        signal: CancellationToken::new(),
                    })
                    .unwrap(),
            )
            .unwrap();

        let models = models_json(&controller.provider().get_models());
        assert_eq!(
            serde_json::to_string(&replace_base(&output["models"], oracle_base)).unwrap(),
            serde_json::to_string(&replace_base(&models, RUST_BASE)).unwrap()
        );
        let expected_publications = replace_base(&output["publications"], oracle_base);
        let rust_publications = Value::Array(
            publications
                .lock()
                .unwrap()
                .iter()
                .map(|publication| replace_base(publication, RUST_BASE))
                .collect(),
        );
        assert_eq!(
            serde_json::to_string(&expected_publications).unwrap(),
            serde_json::to_string(&rust_publications).unwrap()
        );
        let requests = fetch
            .requests()
            .iter()
            .map(|request| {
                json!({
                    "url": request.url.strip_prefix(RUST_BASE).unwrap_or(&request.url),
                    "authorization": llama_fetch::header(request, "authorization"),
                })
            })
            .collect::<Vec<_>>();
        let expected_requests = output["requests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|request| {
                json!({
                    "url": request["url"],
                    "authorization": request["authorization"],
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::to_string(&expected_requests).unwrap(),
            serde_json::to_string(&requests).unwrap()
        );
    }
}
