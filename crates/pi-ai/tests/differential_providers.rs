//! Differential test for `packages/ai/src/providers/*.ts`, `providers/all.ts`,
//! and the provider-owned auth flows against the real TypeScript modules.

use std::collections::BTreeMap;
use std::sync::Arc;

use pi_ai::auth::{AuthContext, AuthPromptKind, AuthResolutionOverrides, Credential};
use pi_ai::models::{CreateModelsOptions, Models, MutableModels, Provider, create_models};
use pi_ai::providers::all::{
    builtin_providers, get_builtin_model, get_builtin_model_data_generated_at, get_builtin_models,
    get_builtin_providers,
};
use pi_ai::providers::cloudflare_stream::resolve_cloudflare_model;
use pi_ai::request::{BoxFuture, Fetch, ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model};
use serde_json::{Map, Value, json};

#[path = "support/oracle.rs"]
mod oracle;

#[path = "support/oauth.rs"]
mod oauth_support;

use oauth_support::PromptAnswer;
use oauth_support::TestInteraction;
use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};

const SOURCE_MODULE: &str = "packages/ai/src/providers/all.ts";

struct FakeAuthContext {
    env: BTreeMap<String, String>,
    files: Vec<String>,
}

impl AuthContext for FakeAuthContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<String>> {
        let value = self.env.get(name).cloned();
        Box::pin(async move { value })
    }

    fn file_exists(&self, path: &str) -> BoxFuture<'_, bool> {
        let exists = self.files.iter().any(|file| file == path);
        Box::pin(async move { exists })
    }
}

fn string_map(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(name, value)| (name.clone(), value.as_str().expect("string env").to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().expect("string item").to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn auth_context(env: &BTreeMap<String, String>, files: &[String]) -> Arc<dyn AuthContext> {
    Arc::new(FakeAuthContext {
        env: env.clone(),
        files: files.to_vec(),
    })
}

fn provider_by_id(id: &str) -> Arc<dyn Provider> {
    builtin_providers()
        .into_iter()
        .find(|provider| provider.id() == id)
        .unwrap_or_else(|| panic!("missing provider {id}"))
}

fn describe_provider(provider: &dyn Provider) -> Value {
    let auth = provider.auth();
    json!({
        "id": provider.id(),
        "name": provider.name(),
        "baseUrl": provider.base_url(),
        "headers": provider.headers(),
        "apiKeyName": auth.api_key.as_ref().map(|api_key| api_key.name()),
        "oauthName": auth.oauth.as_ref().map(|oauth| oauth.name()),
        "oauthSubscription": auth.oauth.as_ref().map(|oauth| oauth.is_subscription()),
        "oauthLoginLabel": auth.oauth.as_ref().and_then(|oauth| oauth.login_label()),
        "models": provider.get_models().into_iter().map(|model| model.id).collect::<Vec<_>>(),
    })
}

#[test]
fn providers_match_the_ts_factories() {
    let mut ts = oracle::TsOracle::spawn("providers-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "providers"}));
    assert_eq!(response["ok"], true, "{response}");
    let ts_providers = response["output"]["providers"].as_array().expect("providers").clone();
    let ts_ids: Vec<String> = ts_providers
        .iter()
        .map(|provider| provider["id"].as_str().expect("id").to_owned())
        .collect();

    let rust_providers: Vec<Value> = builtin_providers()
        .iter()
        .map(|provider| describe_provider(provider.as_ref()))
        .collect();
    let rust_ids: Vec<String> = rust_providers
        .iter()
        .map(|provider| provider["id"].as_str().expect("id").to_owned())
        .collect();

    // Only the api-codex slice providers are missing from this slice.
    assert_eq!(
        ts_ids
            .iter()
            .filter(|id| !rust_ids.contains(id))
            .cloned()
            .collect::<Vec<_>>(),
        vec!["openai-codex".to_owned(), "openai-codex-gc".to_owned()]
    );
    for rust_provider in &rust_providers {
        let id = rust_provider["id"].as_str().expect("id");
        let ts_provider = ts_providers
            .iter()
            .find(|provider| provider["id"] == id)
            .unwrap_or_else(|| panic!("missing TS provider {id}"));
        assert_eq!(rust_provider, ts_provider, "provider {id}");
    }
}

#[test]
fn catalog_matches_the_generated_ts_modules() {
    let mut ts = oracle::TsOracle::spawn("providers-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "catalog"}));
    assert_eq!(response["ok"], true, "{response}");
    let output = &response["output"];

    let ts_providers: Vec<String> = string_list(&output["providers"]);
    assert_eq!(get_builtin_providers(), ts_providers);
    assert_eq!(
        get_builtin_model_data_generated_at(),
        output["generatedAt"].as_f64(),
        "manifest generatedAt"
    );

    let ts_models = output["models"].as_object().expect("models");
    assert_eq!(ts_models.len(), get_builtin_providers().len());
    for (provider, ts_models) in ts_models {
        let rust_models = serde_json::to_value(get_builtin_models(provider)).expect("rust models");
        if &rust_models != ts_models {
            let rust_array = rust_models.as_array().expect("rust array");
            let ts_array = ts_models.as_array().expect("ts array");
            for (index, (rust_model, ts_model)) in rust_array.iter().zip(ts_array).enumerate() {
                if rust_model != ts_model {
                    panic!("catalog {provider} model #{index}\nrust: {rust_model}\nts:   {ts_model}");
                }
            }
            panic!("catalog {provider} length {} != {}", rust_array.len(), ts_array.len());
        }

        let first = ts_models
            .as_array()
            .and_then(|models| models.first())
            .expect("first model");
        let model_id = first["id"].as_str().expect("id");
        let rust_model = get_builtin_model(provider, model_id).expect("builtin model");
        assert_eq!(
            serde_json::to_value(rust_model).expect("model"),
            *first,
            "{provider}/{model_id}"
        );
        assert!(get_builtin_model(provider, "not-a-model").is_none());
    }
}

fn auth_cases() -> Vec<Value> {
    vec![
        json!({"provider": "anthropic", "env": {}}),
        json!({"provider": "anthropic", "env": {"ANTHROPIC_AUTH_TOKEN": "auth-token"}}),
        json!({"provider": "anthropic", "env": {"ANTHROPIC_OAUTH_TOKEN": "oauth-token"}}),
        json!({"provider": "anthropic", "env": {"ANTHROPIC_API_KEY": "api-key"}}),
        json!({"provider": "anthropic", "env": {
            "ANTHROPIC_AUTH_TOKEN": "auth-token",
            "ANTHROPIC_OAUTH_TOKEN": "oauth-token",
            "ANTHROPIC_API_KEY": "api-key"
        }}),
        json!({"provider": "amazon-bedrock", "env": {}}),
        json!({"provider": "amazon-bedrock", "env": {"AWS_PROFILE": "dev"}}),
        json!({"provider": "amazon-bedrock", "env": {
            "AWS_ACCESS_KEY_ID": "id", "AWS_SECRET_ACCESS_KEY": "secret"
        }}),
        json!({"provider": "amazon-bedrock", "env": {"AWS_ACCESS_KEY_ID": "id"}}),
        json!({"provider": "amazon-bedrock", "env": {"AWS_BEARER_TOKEN_BEDROCK": "token"}}),
        json!({"provider": "amazon-bedrock", "env": {"AWS_CONTAINER_CREDENTIALS_RELATIVE_URI": "/v2/creds"}}),
        json!({"provider": "amazon-bedrock", "env": {"AWS_CONTAINER_CREDENTIALS_FULL_URI": "http://169.254.170.2"}}),
        json!({"provider": "amazon-bedrock", "env": {"AWS_WEB_IDENTITY_TOKEN_FILE": "/tmp/token"}}),
        json!({"provider": "cloudflare-workers-ai", "env": {"CLOUDFLARE_API_KEY": "cf-key"}}),
        json!({"provider": "cloudflare-workers-ai", "env": {
            "CLOUDFLARE_API_KEY": "cf-key", "CLOUDFLARE_ACCOUNT_ID": "account-id"
        }}),
        json!({"provider": "cloudflare-ai-gateway", "env": {
            "CLOUDFLARE_API_KEY": "cf-key", "CLOUDFLARE_ACCOUNT_ID": "account-id"
        }}),
        json!({"provider": "cloudflare-ai-gateway", "env": {
            "CLOUDFLARE_API_KEY": "cf-key",
            "CLOUDFLARE_ACCOUNT_ID": "account-id",
            "CLOUDFLARE_GATEWAY_ID": "gateway-id"
        }}),
        json!({"provider": "google-vertex", "env": {}}),
        json!({"provider": "google-vertex", "env": {"GOOGLE_CLOUD_API_KEY": "vertex-key"}}),
        json!({"provider": "google-vertex", "env": {
            "GOOGLE_CLOUD_PROJECT": "proj", "GOOGLE_CLOUD_LOCATION": "us-central1"
        }}),
        json!({"provider": "google-vertex", "env": {
            "GOOGLE_CLOUD_PROJECT": "proj", "GOOGLE_CLOUD_LOCATION": "us-central1"
        }, "files": ["~/.config/gcloud/application_default_credentials.json"]}),
        json!({"provider": "google-vertex", "env": {"GOOGLE_CLOUD_PROJECT": "proj"},
            "files": ["~/.config/gcloud/application_default_credentials.json"]}),
        json!({"provider": "google-vertex", "env": {
            "GOOGLE_CLOUD_API_KEY": "vertex-key",
            "GOOGLE_CLOUD_PROJECT": "proj",
            "GOOGLE_CLOUD_LOCATION": "us-central1"
        }, "files": ["~/.config/gcloud/application_default_credentials.json"]}),
        json!({"provider": "google-vertex", "env": {"GCLOUD_PROJECT": "proj", "GOOGLE_CLOUD_LOCATION": "us-central1"},
            "files": ["~/.config/gcloud/application_default_credentials.json"]}),
        json!({"provider": "openai", "env": {}}),
        json!({"provider": "openai", "env": {"OPENAI_API_KEY": "openai-key"}}),
        json!({"provider": "groq", "env": {"GROQ_API_KEY": "groq-key"}}),
        json!({"provider": "github-copilot", "env": {"COPILOT_GITHUB_TOKEN": "copilot-token"}}),
        json!({"provider": "radius", "env": {}}),
        json!({"provider": "radius", "env": {"RADIUS_API_KEY": "radius-key"}}),
        json!({"provider": "mistral", "env": {"MISTRAL_API_KEY": "mistral-key"}}),
        json!({"provider": "google", "env": {"GEMINI_API_KEY": "gemini-key"}}),
        json!({"provider": "moonshotai", "env": {"MOONSHOT_API_KEY": "moonshot-key"}}),
    ]
}

#[test]
fn auth_resolution_matches_ts() {
    let cases = auth_cases();
    let mut ts = oracle::TsOracle::spawn("providers-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "auth", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    let ts_results = response["output"]["results"].as_array().expect("results").clone();
    assert_eq!(ts_results.len(), cases.len());

    for (case, ts_result) in cases.iter().zip(ts_results) {
        let provider_id = case["provider"].as_str().expect("provider");
        let env = string_map(&case["env"]);
        let files = string_list(&case["files"]);
        let models = create_models(CreateModelsOptions {
            auth_context: Some(auth_context(&env, &files)),
            ..Default::default()
        });
        models.set_provider(provider_by_id(provider_id));
        let result = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(models.get_auth(provider_id, AuthResolutionOverrides::default()))
            .expect("auth");
        let rust_result = serde_json::to_value(result).expect("auth result");
        assert_eq!(
            rust_result, ts_result["result"],
            "auth {provider_id} env {:?} files {:?}",
            case["env"], case["files"]
        );
    }
}

fn login_cases() -> Vec<Value> {
    vec![
        json!({"provider": "anthropic", "answers": ["anthropic-key"]}),
        json!({"provider": "openai", "answers": ["openai-key"]}),
        json!({"provider": "amazon-bedrock", "answers": ["bearer-token", "bedrock-token"]}),
        json!({"provider": "amazon-bedrock", "answers": ["aws-profile", "work"]}),
        json!({"provider": "amazon-bedrock", "answers": ["credential-chain", ""]}),
        json!({"provider": "amazon-bedrock", "answers": ["bogus"]}),
        json!({"provider": "google-vertex", "answers": ["api-key", "vertex-key"]}),
        json!({"provider": "google-vertex", "answers": ["adc", "proj", "us-central1"]}),
        json!({"provider": "google-vertex", "answers": ["service-account", "proj", "us-central1", "/tmp/sa.json"]}),
        json!({"provider": "google-vertex", "answers": ["bogus"]}),
        json!({"provider": "cloudflare-workers-ai", "answers": ["cf-key", "account-id"]}),
        json!({"provider": "cloudflare-ai-gateway", "answers": ["cf-key", "account-id", "gateway-id"]}),
    ]
}

fn prompt_json(kind: &AuthPromptKind) -> Value {
    match kind {
        AuthPromptKind::Text { message, placeholder } => prompt_entry("text", message, placeholder, None),
        AuthPromptKind::Secret { message, placeholder } => prompt_entry("secret", message, placeholder, None),
        AuthPromptKind::ManualCode { message, placeholder } => prompt_entry("manual_code", message, placeholder, None),
        AuthPromptKind::Select { message, options } => prompt_entry(
            "select",
            message,
            &None,
            Some(
                options
                    .iter()
                    .map(|option| {
                        let mut value = Map::new();
                        value.insert("id".to_owned(), Value::String(option.id.clone()));
                        value.insert("label".to_owned(), Value::String(option.label.clone()));
                        if let Some(description) = &option.description {
                            value.insert("description".to_owned(), Value::String(description.clone()));
                        }
                        Value::Object(value)
                    })
                    .collect(),
            ),
        ),
    }
}

fn prompt_entry(kind: &str, message: &str, placeholder: &Option<String>, options: Option<Vec<Value>>) -> Value {
    let mut value = Map::new();
    value.insert("type".to_owned(), Value::String(kind.to_owned()));
    value.insert("message".to_owned(), Value::String(message.to_owned()));
    if let Some(placeholder) = placeholder {
        value.insert("placeholder".to_owned(), Value::String(placeholder.clone()));
    }
    if let Some(options) = options {
        value.insert("options".to_owned(), Value::Array(options));
    }
    Value::Object(value)
}

#[test]
fn provider_login_flows_match_ts() {
    let cases = login_cases();
    let mut ts = oracle::TsOracle::spawn("providers-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "login", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    let ts_results = response["output"]["results"].as_array().expect("results").clone();

    for (case, ts_result) in cases.iter().zip(ts_results) {
        let provider_id = case["provider"].as_str().expect("provider");
        let answers: Vec<PromptAnswer> = string_list(&case["answers"])
            .into_iter()
            .map(PromptAnswer::Text)
            .collect();
        let interaction = TestInteraction::new(answers);
        let provider = provider_by_id(provider_id);
        let auth = provider.auth().api_key.clone().expect("api key auth");
        let interaction_handle: Arc<dyn pi_ai::auth::AuthInteraction> = interaction.clone();
        let result = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(auth.login(interaction_handle).expect("login"));

        let prompts: Vec<Value> = interaction
            .prompts()
            .iter()
            .map(|prompt| prompt_json(&prompt.kind))
            .collect();
        let events: Vec<Value> = interaction
            .events()
            .iter()
            .map(|event| serde_json::to_value(event).expect("event"))
            .collect();
        assert_eq!(json!(prompts), ts_result["prompts"], "prompts {provider_id}");
        assert_eq!(json!(events), ts_result["events"], "events {provider_id}");

        match result {
            Ok(credential) => {
                assert_eq!(
                    serde_json::to_value(Credential::ApiKey(credential)).expect("credential"),
                    ts_result["credential"],
                    "credential {provider_id} answers {:?}",
                    case["answers"]
                );
                assert_eq!(ts_result["error"], Value::Null, "ts error {provider_id}");
            }
            Err(error) => {
                assert_eq!(
                    json!({"name": "Error", "message": error.detail()}),
                    ts_result["error"],
                    "error {provider_id} answers {:?}",
                    case["answers"]
                );
            }
        }
    }
}

#[test]
fn cloudflare_model_resolution_matches_ts() {
    let model = json!({
        "id": "model",
        "name": "model",
        "api": "openai-completions",
        "provider": "cloudflare-ai-gateway",
        "baseUrl": "https://gateway.ai.cloudflare.com/v1/{CLOUDFLARE_ACCOUNT_ID}/{CLOUDFLARE_GATEWAY_ID}/openai",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 1000,
        "maxTokens": 100
    });
    let cases = vec![
        json!({"model": model, "env": {}}),
        json!({"model": model, "env": {"CLOUDFLARE_ACCOUNT_ID": "account"}}),
        json!({"model": model, "env": {"CLOUDFLARE_GATEWAY_ID": "gateway"}}),
        json!({"model": model, "env": {"CLOUDFLARE_ACCOUNT_ID": "account", "CLOUDFLARE_GATEWAY_ID": "gateway"}}),
        json!({"model": model}),
    ];
    let mut ts = oracle::TsOracle::spawn("providers-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "cloudflare", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    let ts_results = response["output"]["results"].as_array().expect("results").clone();

    for (case, ts_result) in cases.iter().zip(ts_results) {
        let model: Model = serde_json::from_value(case["model"].clone()).expect("model");
        let env: Option<BTreeMap<String, String>> = case
            .get("env")
            .filter(|value| value.as_object().is_some_and(|object| !object.is_empty()))
            .map(string_map);
        let resolved = resolve_cloudflare_model(&model, env.as_ref());
        assert_eq!(
            json!(resolved.base_url),
            ts_result["baseUrl"],
            "cloudflare {:?}",
            case["env"]
        );
    }
}

const SDK_APIS: [&str; 3] = ["bedrock-converse-stream", "google-generative-ai", "google-vertex"];

fn dispatch_cases() -> Vec<(String, Model)> {
    let mut cases = Vec::new();
    for provider in builtin_providers() {
        let mut seen: Vec<String> = Vec::new();
        for model in provider.get_models() {
            // These APIs go through vendor SDKs that bypass `globalThis.fetch`;
            // their request construction is covered by
            // `differential_bedrock_converse_stream` and `differential_google_api`.
            if SDK_APIS.contains(&model.api.as_str()) {
                continue;
            }
            if seen.contains(&model.api) {
                continue;
            }
            seen.push(model.api.clone());
            cases.push((provider.id().to_owned(), model));
        }
    }
    cases
}

/// Headers whose value depends on the host runtime rather than the port.
const ENV_HEADERS: [&str; 5] = [
    "user-agent",
    "x-stainless-os",
    "x-stainless-arch",
    "x-stainless-runtime",
    "x-stainless-runtime-version",
];

/// Normalizes numbers to `f64` so `65536` and `65536.0` compare equal.
fn normalize_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => {
            if let Some(number) = number.as_f64() {
                *value = Value::from(number);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_numbers),
        Value::Object(map) => map.values_mut().for_each(normalize_numbers),
        _ => {}
    }
}

fn normalize_request(value: &mut Value) {
    if let Some(headers) = value.get_mut("headers").and_then(Value::as_object_mut) {
        for name in ENV_HEADERS {
            if headers.contains_key(name) {
                headers.insert(name.to_owned(), Value::String("<env>".to_owned()));
            }
        }
    }
    if let Some(Value::String(body)) = value.get("body")
        && let Ok(mut parsed) = serde_json::from_str::<Value>(body)
    {
        normalize_numbers(&mut parsed);
        value["body"] = parsed;
    }
}

fn rust_request_json(fetch: &RecordingFetch) -> Vec<Value> {
    fetch
        .requests()
        .into_iter()
        .map(|request| {
            let mut headers: BTreeMap<String, String> = BTreeMap::new();
            for (name, value) in request.headers {
                headers.insert(name.to_lowercase(), value);
            }
            let mut value = json!({
                "url": request.url,
                "method": request.method,
                "headers": headers,
                "body": request.body.map(|body| String::from_utf8_lossy(&body).into_owned()),
            });
            normalize_request(&mut value);
            value
        })
        .collect()
}

#[test]
fn provider_dispatch_matches_ts_requests() {
    let cases = dispatch_cases();
    let request_cases: Vec<Value> = cases
        .iter()
        .map(|(provider, model)| json!({"provider": provider, "modelId": model.id}))
        .collect();
    let mut ts = oracle::TsOracle::spawn("providers-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "dispatch", "cases": request_cases}));
    assert_eq!(response["ok"], true, "{response}");
    let ts_results = response["output"]["results"].as_array().expect("results").clone();
    assert_eq!(ts_results.len(), cases.len());

    for ((provider_id, model), ts_result) in cases.iter().zip(ts_results) {
        let fetch = RecordingFetch::new(vec![ScriptedResponse::json(
            400,
            &json!({"error": {"message": "recorded"}}).to_string(),
        )]);
        let options = SimpleStreamOptions {
            stream: StreamOptions {
                request: ProviderRequestOptions {
                    api_key: Some("test-key".to_owned()),
                    fetch: Some(fetch.clone() as Arc<dyn Fetch>),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let result = tokio::runtime::Runtime::new().expect("runtime").block_on(async {
            provider_by_id(provider_id)
                .stream_simple(
                    model.clone(),
                    Context {
                        system_prompt: None,
                        messages: Vec::new(),
                        tools: None,
                    },
                    options,
                )
                .result()
                .await
        });
        let result = result.expect("stream result");
        let mut ts_requests = ts_result["requests"].clone();
        if let Some(requests) = ts_requests.as_array_mut() {
            requests.iter_mut().for_each(normalize_request);
        }
        assert_eq!(
            json!(rust_request_json(&fetch)),
            ts_requests,
            "requests {provider_id}/{}",
            model.id
        );
        assert_eq!(
            json!(result.stop_reason),
            ts_result["stopReason"],
            "stopReason {provider_id}/{}",
            model.id
        );
        assert_eq!(
            json!(result.error_message),
            ts_result["errorMessage"],
            "errorMessage {provider_id}/{}",
            model.id
        );
    }
}
