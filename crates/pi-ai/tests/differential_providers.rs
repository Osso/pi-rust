//! Differential test for `packages/ai/src/providers/*.ts`, `providers/all.ts`,
//! and the provider-owned auth flows against the real TypeScript modules.

use std::collections::BTreeMap;
use std::sync::Arc;

use pi_ai::auth::{ApiKeyResolveInput, AuthContext, AuthModel, AuthPromptKind, Credential};
use pi_ai::models::{CreateModelsOptions, Models, MutableModels, Provider, create_models};
use pi_ai::providers::all::{builtin_providers, get_builtin_model, get_builtin_models, get_builtin_providers};
use pi_ai::request::{BoxFuture, Fetch, ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model};
use serde_json::{Map, Value, json};

use crate::shared_oracle as oracle;

use crate::shared_oauth as oauth_support;

use oauth_support::PromptAnswer;
use oauth_support::TestInteraction;
use oauth_support::recording_fetch::{RecordingFetch, ScriptedResponse};

const SOURCE_MODULE: &str = "packages/ai/src/providers/all.ts";

struct FakeAuthContext {
    env: BTreeMap<String, String>,
    files: Vec<String>,
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
        path: &pi_js_compat::js_json::JsString,
    ) -> BoxFuture<'_, Result<bool, pi_ai::auth::AuthError>> {
        let exists = self.files.iter().any(|file| *path == *file.as_str());
        Box::pin(async move { Ok(exists) })
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
        "models": provider.get_models().unwrap().into_iter().map(|model| model.id).collect::<Vec<_>>(),
        "pickerModels": provider.get_models().unwrap().into_iter().map(|model| model.id).collect::<Vec<_>>(),
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

    assert_eq!(ts_ids, rust_ids);
    let mut mismatches = Vec::new();
    for rust_provider in &rust_providers {
        let id = rust_provider["id"].as_str().expect("id");
        let ts_provider = ts_providers
            .iter()
            .find(|provider| provider["id"] == id)
            .unwrap_or_else(|| panic!("missing TS provider {id}"));
        if rust_provider != ts_provider {
            mismatches.push((id, rust_provider, ts_provider));
        }
    }
    assert!(mismatches.is_empty(), "provider mismatches: {mismatches:#?}");
}

#[test]
fn catalog_matches_the_generated_ts_modules() {
    let mut ts = oracle::TsOracle::spawn("providers-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "catalog"}));
    assert_eq!(response["ok"], true, "{response}");
    let output = &response["output"];

    let ts_providers: Vec<String> = string_list(&output["providers"]);
    assert_eq!(get_builtin_providers(), ts_providers);
    // Common-pin all.ts exposes literal model records, not a generation timestamp.

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
        // Common-pin all.ts:79–117 no longer registers radius.
        json!({"provider": "mistral", "env": {"MISTRAL_API_KEY": "mistral-key"}}),
        json!({"provider": "google", "env": {"GEMINI_API_KEY": "gemini-key"}}),
        json!({"provider": "moonshotai", "env": {"MOONSHOT_API_KEY": "moonshot-key"}}),
    ]
    .into_iter()
    .map(|mut case| {
        // models.ts:216 takes an actual model, never a provider ID.
        let provider = provider_by_id(case["provider"].as_str().expect("provider"));
        case["model"] = json!(provider.get_models().unwrap().first().expect("fixture model"));
        case
    })
    .collect()
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
            .block_on(models.get_auth(&serde_json::from_value(case["model"].clone()).expect("model")))
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
                        value.insert("id".to_owned(), json!(option.id));
                        value.insert("label".to_owned(), json!(option.label));
                        if let Some(description) = &option.description {
                            value.insert("description".to_owned(), json!(description));
                        }
                        Value::Object(value)
                    })
                    .collect(),
            ),
        ),
    }
}

fn prompt_entry(
    kind: &str,
    message: &pi_js_compat::js_json::JsString,
    placeholder: &Option<pi_js_compat::js_json::JsString>,
    options: Option<Vec<Value>>,
) -> Value {
    let mut value = Map::new();
    value.insert("type".to_owned(), Value::String(kind.to_owned()));
    value.insert("message".to_owned(), json!(message));
    if let Some(placeholder) = placeholder {
        value.insert("placeholder".to_owned(), json!(placeholder));
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
        // Bedrock's common-pin auth is ambient-only (amazon-bedrock.ts:12–30).
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let result = auth.login().map(|login| runtime.block_on(login(interaction_handle)));

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
            Some(Ok(credential)) => {
                assert_eq!(
                    serde_json::to_value(Credential::ApiKey(credential)).expect("credential"),
                    ts_result["credential"],
                    "credential {provider_id} answers {:?}",
                    case["answers"]
                );
                assert_eq!(ts_result["error"], Value::Null, "ts error {provider_id}");
            }
            Some(Err(error)) => {
                assert_eq!(
                    json!({"name": "Error", "message": error.detail()}),
                    ts_result["error"],
                    "error {provider_id} answers {:?}",
                    case["answers"]
                );
            }
            None => assert_eq!(ts_result["credential"], Value::Null, "ambient-only {provider_id}"),
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
    // cloudflare-auth.ts:22–50 now materializes auth.baseUrl, not a stream wrapper.
    let configured =
        json!({"CLOUDFLARE_API_KEY":"cf-key", "CLOUDFLARE_ACCOUNT_ID":"account", "CLOUDFLARE_GATEWAY_ID":"gateway"});
    let mut workers_model = model.clone();
    workers_model["provider"] = json!("cloudflare-workers-ai");
    workers_model["baseUrl"] =
        json!("https://api.cloudflare.com/{CLOUDFLARE_ACCOUNT_ID}/{CLOUDFLARE_ACCOUNT_ID}/{CLOUDFLARE_GATEWAY_ID}");
    let mut cases = vec![
        json!({"model": model, "env": {}}),
        json!({"model": model, "env": {"CLOUDFLARE_API_KEY":"cf-key"}}),
        json!({"model": model, "env": {"CLOUDFLARE_API_KEY":"cf-key", "CLOUDFLARE_ACCOUNT_ID":"account"}}),
        json!({"model": model, "env": configured}),
        json!({"model": model, "env": configured, "credential":{"type":"api_key", "key":"stored"}}),
        json!({"model": model, "env": configured, "credential":{"type":"api_key", "key":"stored", "env":{"CLOUDFLARE_ACCOUNT_ID":"saved", "CLOUDFLARE_GATEWAY_ID":"saved-gateway"}}}),
        json!({"model": model, "env": {"CLOUDFLARE_API_KEY":"", "CLOUDFLARE_ACCOUNT_ID":"account", "CLOUDFLARE_GATEWAY_ID":"gateway"}}),
        json!({"model": model, "env": {"CLOUDFLARE_API_KEY":"cf-key", "CLOUDFLARE_ACCOUNT_ID":"", "CLOUDFLARE_GATEWAY_ID":"gateway"}}),
        json!({"model": model, "env": {"CLOUDFLARE_API_KEY":"cf-key", "CLOUDFLARE_ACCOUNT_ID":"account", "CLOUDFLARE_GATEWAY_ID":""}}),
        json!({"model": workers_model, "env":configured}),
    ];
    let vertex_model = json!(provider_by_id("google-vertex").get_models().unwrap().remove(0));
    for credential in [
        json!({"type":"api_key", "key":"stored"}),
        json!({"type":"api_key", "key":""}),
        json!({"type":"api_key"}),
    ] {
        cases.push(json!({
            "model": vertex_model,
            "env": {"GOOGLE_CLOUD_API_KEY":"ambient"},
            "credential": credential,
        }));
    }
    let mut ts = oracle::TsOracle::spawn("providers-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "op": "cloudflare", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    let ts_results = response["output"]["results"].as_array().expect("results").clone();

    for (case, ts_result) in cases.iter().zip(ts_results) {
        let model: Model = serde_json::from_value(case["model"].clone()).expect("model");
        let provider = provider_by_id(&model.provider);
        let auth = provider.auth().api_key.clone().expect("model-aware provider auth");
        let input = ApiKeyResolveInput {
            model: AuthModel::Chat(model),
            ctx: auth_context(&string_map(&case["env"]), &[]),
            credential: case
                .get("credential")
                .map(|value| serde_json::from_value(value.clone()).expect("credential")),
        };
        let resolved = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(auth.resolve(input))
            .expect("auth");
        assert_eq!(json!(resolved), ts_result, "model-aware provider {case}");
    }
}

const SDK_APIS: [&str; 3] = ["bedrock-converse-stream", "google-generative-ai", "google-vertex"];

fn dispatch_cases() -> Vec<(String, Model)> {
    let mut cases = Vec::new();
    for provider in builtin_providers() {
        let mut seen: Vec<String> = Vec::new();
        for model in provider.get_models().unwrap() {
            // These APIs go through vendor SDKs that bypass `globalThis.fetch`;
            // their request construction is covered by
            // `differential_bedrock_converse_stream` and `differential_google_api`.
            // `opencode-go`'s unmapped google-generative-ai model is the
            // exception: it fails before any request on both sides.
            let unmapped_source_discrepancy = provider.id() == "opencode-go" && model.api == "google-generative-ai";
            if SDK_APIS.contains(&model.api.as_str()) && !unmapped_source_discrepancy {
                continue;
            }
            if seen.contains(&model.api) {
                continue;
            }
            seen.push(model.api.clone());
            cases.push((provider.id().to_owned(), model));
        }
    }
    // Common-pin opencode-go.ts supports only Anthropic and Completions.
    let mut unsupported = provider_by_id("opencode-go").get_models().unwrap().remove(0);
    unsupported.api = "openai-responses".to_owned();
    cases.push(("opencode-go".to_owned(), unsupported));
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
        .map(|(provider, model)| json!({"provider": provider, "modelId": model.id, "api":model.api}))
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
                    api_key: Some(("test-key".to_owned()).into()),
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
                .expect("acquisition")
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
