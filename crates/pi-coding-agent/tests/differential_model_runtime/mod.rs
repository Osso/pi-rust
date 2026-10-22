#[path = "../support/oracle.rs"]
mod oracle;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use pi_ai::auth::resolve::AuthResolutionOverrides;
use pi_ai::auth::types::CredentialStore;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use serde_json::{Map, Value, json};

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-runtime-oracle-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn storage_with(entries: &Value) -> Arc<AuthStorage> {
    let mut data = Map::new();
    if let Value::Object(object) = entries {
        for (provider, credential) in object {
            data.insert(provider.clone(), credential.clone());
        }
    }
    Arc::new(AuthStorage::in_memory(data))
}

struct Case {
    name: &'static str,
    providers: Value,
    auth: Value,
    auth_providers: Vec<&'static str>,
    model_refs: Vec<(&'static str, &'static str)>,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "empty",
            providers: json!({}),
            auth: json!({}),
            auth_providers: vec!["anthropic", "openai"],
            model_refs: vec![("anthropic", "claude-opus-4-8")],
        },
        Case {
            name: "stored-keys",
            providers: json!({}),
            auth: json!({
                "anthropic": {"type": "api_key", "key": "anthropic-key"},
                "openai": {"type": "api_key", "key": "openai-key"},
                "openrouter": {"type": "api_key", "key": "openrouter-key"}
            }),
            auth_providers: vec!["anthropic", "openai", "openrouter"],
            model_refs: vec![("anthropic", "claude-opus-4-8"), ("openai", "gpt-5.5")],
        },
        Case {
            name: "base-url-headers",
            providers: json!({"providers": {"anthropic": {
                "baseUrl": "https://proxy.example.com/v1",
                "headers": {"X-Custom-Header": "custom-value"}
            }}}),
            auth: json!({"anthropic": {"type": "api_key", "key": "anthropic-key"}}),
            auth_providers: vec!["anthropic"],
            model_refs: vec![("anthropic", "claude-opus-4-8")],
        },
        Case {
            name: "custom-models-overrides",
            providers: json!({"providers": {"anthropic": {
                "baseUrl": "https://merged.example.com/v1",
                "models": [{
                    "id": "claude-custom",
                    "name": "Custom Claude",
                    "reasoning": true,
                    "input": ["text", "image"],
                    "contextWindow": 64000,
                    "maxTokens": 4096
                }],
                "modelOverrides": {
                    "claude-opus-4-8": {
                        "name": "Renamed Opus",
                        "cost": {"input": 9},
                        "samplingParams": {"temperature": 0.5},
                        "headers": {"x-override": "override"}
                    }
                }
            }}}),
            auth: json!({"anthropic": {"type": "api_key", "key": "anthropic-key"}}),
            auth_providers: vec!["anthropic"],
            model_refs: vec![("anthropic", "claude-custom"), ("anthropic", "claude-opus-4-8")],
        },
        Case {
            name: "compat",
            providers: json!({"providers": {"demo": {
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
            }}}),
            auth: json!({}),
            auth_providers: vec!["demo"],
            model_refs: vec![("demo", "demo-model")],
        },
        Case {
            name: "broken-providers",
            providers: json!({"providers": {
                "broken-one": {"api": "openai-completions", "models": [{"id": "one"}]},
                "broken-two": {"api": "openai-completions", "models": [{"id": "two"}]}
            }}),
            auth: json!({}),
            auth_providers: vec!["broken-one", "broken-two"],
            model_refs: vec![("broken-one", "one")],
        },
        Case {
            name: "auth-header-missing-key",
            providers: json!({"providers": {"custom-provider": {
                "baseUrl": "https://example.test/v1",
                "api": "openai-completions",
                "authHeader": true,
                "headers": {"x-configured": "value"},
                "models": [{"id": "test-model"}]
            }}}),
            auth: json!({}),
            auth_providers: vec!["custom-provider"],
            model_refs: vec![("custom-provider", "test-model")],
        },
        Case {
            name: "oauth-stored",
            providers: json!({}),
            auth: json!({
                "anthropic": {"type": "oauth", "access": "access", "refresh": "refresh", "expires": 9999999999999.0}
            }),
            auth_providers: vec!["anthropic"],
            model_refs: vec![("anthropic", "claude-opus-4-8")],
        },
    ]
}

fn normalize_auth(resolution: Option<pi_ai::auth::types::AuthResult>) -> Value {
    match resolution {
        Some(resolution) => json!({
            "apiKey": resolution.auth.api_key,
            "headers": resolution.auth.headers,
            "baseUrl": resolution.auth.base_url,
            "env": resolution.env,
            "source": resolution.source,
        }),
        None => Value::Null,
    }
}

fn rust_output(models_path: &str, case: &Case) -> Value {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
            credentials: Some(storage_with(&case.auth) as Arc<dyn CredentialStore>),
            models_path: Some(Some(models_path.to_owned().into())),
            ..Default::default()
        })
        .await
        .unwrap();
        let providers = runtime.get_providers();
        let models = runtime.get_models(None);
        let auth_statuses: Vec<Value> = providers
            .iter()
            .map(|provider| {
                let status = runtime.get_provider_auth_status(provider.id());
                json!({
                    "provider": provider.id(),
                    "configured": status.configured,
                    "source": status.source.map(|source| source.as_str()),
                    "label": status.label,
                })
            })
            .collect();
        let mut auth = Map::new();
        for provider in &case.auth_providers {
            let resolution = runtime
                .get_auth(provider, AuthResolutionOverrides::default())
                .await
                .unwrap();
            auth.insert((*provider).to_owned(), normalize_auth(resolution));
        }
        let mut compatibility = Map::new();
        for (provider, id) in &case.model_refs {
            let key = format!("{provider}/{id}");
            match runtime.get_model(*provider, *id) {
                Some(model) => {
                    let config = runtime.get_compatibility_request_config(&model).unwrap();
                    compatibility.insert(
                        key,
                        json!({"headers": config.headers, "authHeader": config.auth_header}),
                    );
                }
                None => {
                    compatibility.insert(key, Value::Null);
                }
            }
        }
        json!({
            "providers": providers
                .iter()
                .map(|provider| json!({
                    "id": provider.id(),
                    "name": provider.name(),
                    "apiKeyName": provider.auth().api_key.as_ref().map(|method| method.name()),
                    "oauthName": provider.auth().oauth.as_ref().map(|method| method.name()),
                    "oauthSubscription": provider.auth().oauth.as_ref().is_some_and(|method| method.is_subscription()),
                }))
                .collect::<Vec<_>>(),
            "models": models
                .iter()
                .map(|model| serde_json::to_value(model).unwrap())
                .collect::<Vec<_>>(),
            "error": runtime.get_error(),
            "authStatuses": auth_statuses,
            "available": runtime
                .get_available_snapshot()
                .iter()
                .map(|model| json!({"provider": model.provider, "id": model.id}))
                .collect::<Vec<_>>(),
            "registeredProviderIds": runtime.get_registered_provider_ids(),
            "auth": auth,
            "compatibility": compatibility,
        })
    })
}

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "model-runtime-oracle.ts",
        "packages/coding-agent/src/core/model-runtime.ts",
    );
    for (index, case) in cases().into_iter().enumerate() {
        let oracle_dir = temp_dir(&format!("oracle-{}", case.name));
        let rust_dir = temp_dir(&format!("rust-{}", case.name));
        let content =
            json!({"providers": case.providers.get("providers").cloned().unwrap_or_else(|| json!({}))}).to_string();
        let oracle_path = oracle_dir.join("models.json");
        let rust_path = rust_dir.join("models.json");
        fs::write(&oracle_path, &content).unwrap();
        fs::write(&rust_path, &content).unwrap();
        let request = json!({
            "id": index,
            "modelsPath": oracle_path.display().to_string(),
            "auth": case.auth,
            "authProviders": case.auth_providers,
            "modelRefs": case.model_refs,
        });
        let expected = oracle.request(&request);
        assert!(expected["ok"].as_bool().unwrap(), "{}: {expected}", case.name);
        let actual = rust_output(&rust_path.display().to_string(), &case);
        assert_eq!(actual, expected["output"], "case {}", case.name);
        fs::remove_dir_all(oracle_dir).unwrap();
        fs::remove_dir_all(rust_dir).unwrap();
    }
}
