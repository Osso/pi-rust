use crate::shared_oracle as oracle;

use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_registry::{
        ExtensionOAuthConfig, ModelRegistry, ProviderConfigInput, ProviderModelInput, ResolvedRequestAuth,
    },
};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};
use std::{
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const PROVIDER: &str = "registry-reentrant";
const NESTED: &str = "registry-reentrant-nested";
const EXPECTED_PATH: &str = "PI_REGISTRY_REENTRY_EXPECTED";

fn input() -> JsValue {
    js_json!({"api":"openai-completions","baseUrl":"https://provider.invalid","models":[{"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32}]})
}
fn snapshot(registry: &ModelRegistry) -> JsValue {
    let ids = registry
        .get_all()
        .into_iter()
        .filter(|model| model.provider == PROVIDER)
        .map(|model| model.id)
        .collect::<Vec<_>>();
    js_json!({"ids":ids,"display":registry.get_provider_display_name(PROVIDER),"nestedDisplay":registry.get_provider_display_name(NESTED)})
}

#[tokio::test]
async fn oauth_model_callback_can_read_and_register_during_registration_and_refresh() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let expected:JsValue=source.request(&js_json!({"operation":"registration_reentry","config":input(),"auth":{PROVIDER:{"type":"oauth","access":"fixture-access","refresh":"fixture-refresh","expires":9999999999999_f64}}}));
    compare_private_probe(&expected, "isolated_reentrant_registry_probe");
}

fn compare_private_probe(expected: &JsValue, probe: &str) {
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let directory = tempfile::tempdir().unwrap();
    let expected_path = directory.path().join("expected.json");
    std::fs::write(&expected_path, js_json::stringify_lossless(&expected["output"])).unwrap();
    let log_path = directory.path().join("child.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", probe, "--ignored", "--nocapture"])
        .env(EXPECTED_PATH, expected_path)
        .stdin(Stdio::null())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "registry OAuth callback deadlocked: {}",
                std::fs::read_to_string(log_path).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        status.success(),
        "private registry probe failed: {}",
        std::fs::read_to_string(log_path).unwrap()
    );
}

#[test]
#[ignore = "private subprocess entry; run by the differential parent"]
fn isolated_reentrant_registry_probe() {
    let expected_path = std::env::var_os(EXPECTED_PATH).expect("private fixture path");
    let expected = js_json::parse_lossless(std::fs::read_to_string(expected_path).unwrap()).unwrap();
    let auth = js_json!({PROVIDER:{"type":"oauth","access":"fixture-access","refresh":"fixture-refresh","expires":9999999999999_f64}});
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(auth.as_object().unwrap().clone())).unwrap();
    let reader = registry.clone();
    let observations = Arc::new(Mutex::new(Vec::<JsValue>::new()));
    let callback_observations = observations.clone();
    let oauth = ExtensionOAuthConfig {
        name: "OAuth Fixture".into(),
        uses_callback_server: None,
        login: Arc::new(|_| Box::pin(async { unreachable!("unused login") })),
        refresh_token: Arc::new(|credential, _| Box::pin(async move { Ok(credential) })),
        get_api_key: Arc::new(|credential| Ok(credential.access.clone())),
        modify_models: Some(Arc::new(move |models, credential| {
            let mut view = snapshot(&reader);
            view["access"] = js_json::to_value(&credential.access).unwrap();
            let count = {
                let mut values = callback_observations.lock().unwrap();
                values.push(view);
                values.len()
            };
            reader
                .register_provider(
                    NESTED,
                    ProviderConfigInput {
                        name: Some(format!("Nested {count}").into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            Ok(models)
        })),
    };
    registry
        .register_provider(
            PROVIDER,
            ProviderConfigInput {
                name: Some("Registered Fixture".into()),
                api: Some("openai-completions".into()),
                base_url: Some("https://provider.invalid".into()),
                oauth: Some(oauth),
                models: Some(vec![
                    js_json::from_value::<ProviderModelInput>(&input()["models"][0]).unwrap(),
                ]),
                ..Default::default()
            },
        )
        .unwrap();
    registry.refresh().unwrap();
    let after_refresh = snapshot(&registry);
    registry.unregister_provider(PROVIDER).unwrap();
    let after_unregister = snapshot(&registry);
    registry.unregister_provider(NESTED).unwrap();
    let actual = js_json!({"observations":observations.lock().unwrap().clone(),"afterRefresh":after_refresh,"afterUnregister":after_unregister});
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn request_auth_reads_model_headers_after_reentrant_key_resolution() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let expected=source.request(&js_json!({"operation":"auth_reentry","config":input(),"auth":{"registry-auth-reentrant":{"type":"oauth","access":"fixture-access","refresh":"fixture-refresh","expires":9999999999999_f64}}}));
    compare_private_probe(&expected, "isolated_request_auth_reentry_probe");
}

fn resolved_auth_value(auth: ResolvedRequestAuth) -> JsValue {
    match auth {
        ResolvedRequestAuth::Err { error } => js_json!({"ok":false,"error":error}),
        ResolvedRequestAuth::Ok { api_key, headers, env } => {
            let mut value = js_json!({"ok":true});
            if let Some(key) = api_key {
                value["apiKey"] = js_json::to_value(&key).unwrap();
            }
            if let Some(headers) = headers {
                value["headers"] = js_json::to_value(&headers).unwrap();
            }
            if let Some(env) = env {
                value["env"] = js_json::to_value(&env).unwrap();
            }
            value
        }
    }
}

fn request_headers(prefix: &str, count: usize, unit: u16) -> indexmap::IndexMap<String, JsString> {
    let value = JsString::from(format!("{}-{count}", prefix.to_ascii_lowercase()));
    [
        (format!("X-{prefix}"), value.clone() + JsString::from_utf16(vec![unit])),
        ("X-Same".into(), value),
    ]
    .into()
}

#[tokio::test]
#[ignore = "private subprocess entry; run by the differential parent"]
async fn isolated_request_auth_reentry_probe() {
    const AUTH_PROVIDER: &str = "registry-auth-reentrant";
    let expected_path = std::env::var_os(EXPECTED_PATH).expect("private fixture path");
    let expected = js_json::parse_lossless(std::fs::read_to_string(expected_path).unwrap()).unwrap();
    let auth = js_json!({AUTH_PROVIDER:{"type":"oauth","access":"fixture-access","refresh":"fixture-refresh","expires":9999999999999_f64}});
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(auth.as_object().unwrap().clone())).unwrap();
    let reader = registry.clone();
    let count = Arc::new(Mutex::new(0));
    let callback_count = count.clone();
    let mut definition = js_json::from_value::<ProviderModelInput>(&input()["models"][0]).unwrap();
    let updated_definition = definition.clone();
    definition.headers = Some(request_headers("Model", 0, 0xd800));
    registry
        .register_provider(
            AUTH_PROVIDER,
            ProviderConfigInput {
                api: Some("openai-completions".into()),
                base_url: Some("https://provider.invalid".into()),
                headers: Some(request_headers("Provider", 0, 0xdc00)),
                models: Some(vec![definition]),
                oauth: Some(ExtensionOAuthConfig {
                    name: "OAuth Header Fixture".into(),
                    uses_callback_server: None,
                    login: Arc::new(|_| Box::pin(async { unreachable!("unused login") })),
                    refresh_token: Arc::new(|credential, _| Box::pin(async move { Ok(credential) })),
                    get_api_key: Arc::new(move |credential| {
                        let current_count = {
                            let mut count = callback_count.lock().unwrap();
                            *count += 1;
                            *count
                        };
                        let mut definition = updated_definition.clone();
                        definition.headers = Some(request_headers("Model", current_count, 0xd800));
                        reader.register_provider(
                            AUTH_PROVIDER,
                            ProviderConfigInput {
                                api: Some("openai-completions".into()),
                                base_url: Some("https://provider.invalid".into()),
                                api_key: Some("updated-provider-key".into()),
                                auth_header: Some(true),
                                headers: Some(request_headers("Provider", current_count, 0xdc00)),
                                models: Some(vec![definition]),
                                ..Default::default()
                            },
                        )?;
                        Ok(credential.access.clone())
                    }),
                    modify_models: None,
                }),
                ..Default::default()
            },
        )
        .unwrap();
    let mut model = registry.find(AUTH_PROVIDER, "one").unwrap();
    model.headers = Some(
        [
            ("X-Static".into(), "static".into()),
            ("X-Same".into(), "static".into()),
            ("Authorization".into(), "static-auth".into()),
        ]
        .into(),
    );
    let first = resolved_auth_value(registry.get_api_key_and_headers(&model).await.unwrap());
    let second = resolved_auth_value(registry.get_api_key_and_headers(&model).await.unwrap());
    registry.unregister_provider(AUTH_PROVIDER).unwrap();
    let actual = js_json!({"first":first,"second":second,"count":*count.lock().unwrap()});
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn refresh_visits_a_provider_appended_by_a_modifier() {
    compare_live_registration("append");
}
#[tokio::test]
async fn refresh_skips_a_provider_deleted_by_a_modifier() {
    compare_live_registration("delete");
}
#[tokio::test]
async fn refresh_visits_the_new_slot_after_provider_reinsertion() {
    compare_live_registration("reinsert");
}
fn compare_live_registration(mode: &str) {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let expected = source.request(&js_json!({"operation":"registration_live_map","mode":mode,"config":input()}));
    compare_private_probe(&expected, "isolated_registry_live_map_probe");
}
fn side_config(name: &str) -> ProviderConfigInput {
    let mut model = js_json::from_value::<ProviderModelInput>(&input()["models"][0]).unwrap();
    model.id = "side".into();
    model.name = name.into();
    ProviderConfigInput {
        name: Some(name.into()),
        api: Some("openai-completions".into()),
        base_url: Some("https://provider.invalid".into()),
        api_key: Some("side-key".into()),
        models: Some(vec![model]),
        ..Default::default()
    }
}
fn live_map_snapshot(registry: &ModelRegistry) -> JsValue {
    let models = registry
        .get_all()
        .into_iter()
        .filter(|model| model.provider == "registry-live-parent" || model.provider == "registry-live-side")
        .collect::<Vec<_>>();
    let models = js_json::to_value(&models).unwrap();
    js_json!({"models":models,"serializedModels":js_json::stringify_lossless(&models),"sideDisplay":registry.get_provider_display_name("registry-live-side")})
}
#[test]
#[ignore = "private live-map callback process entry"]
fn isolated_registry_live_map_probe() {
    const LIVE: &str = "registry-live-parent";
    const SIDE: &str = "registry-live-side";
    let expected_path = std::env::var_os(EXPECTED_PATH).expect("private fixture path");
    let expected = js_json::parse_lossless(std::fs::read_to_string(expected_path).unwrap()).unwrap();
    let mode = expected["mode"].as_string().unwrap().to_utf8().unwrap();
    let callback_mode = mode.clone();
    let storage = AuthStorage::in_memory(serde_json::Map::new());
    let registry = ModelRegistry::in_memory(storage.clone()).unwrap();
    let reader = registry.clone();
    let calls = Arc::new(Mutex::new(0));
    let callback_calls = calls.clone();
    registry
        .register_provider(
            LIVE,
            ProviderConfigInput {
                api: Some("openai-completions".into()),
                base_url: Some("https://provider.invalid".into()),
                models: Some(vec![
                    js_json::from_value::<ProviderModelInput>(&input()["models"][0]).unwrap(),
                ]),
                oauth: Some(ExtensionOAuthConfig {
                    name: "Live Parent".into(),
                    uses_callback_server: None,
                    login: Arc::new(|_| Box::pin(async { unreachable!("unused login") })),
                    refresh_token: Arc::new(|credential, _| Box::pin(async move { Ok(credential) })),
                    get_api_key: Arc::new(|credential| Ok(credential.access.clone())),
                    modify_models: Some(Arc::new(move |models, _| {
                        let count = {
                            let mut count = callback_calls.lock().unwrap();
                            *count += 1;
                            *count
                        };
                        if count == 1 {
                            if callback_mode != "append" {
                                reader.unregister_provider(SIDE)?;
                            }
                            if callback_mode != "delete" {
                                reader.register_provider(SIDE, side_config("Side new"))?;
                            }
                        }
                        Ok(models)
                    })),
                }),
                ..Default::default()
            },
        )
        .unwrap();
    if mode != "append" {
        registry.register_provider(SIDE, side_config("Side old")).unwrap();
    }
    storage
        .set(
            LIVE,
            js_json!({"type":"oauth","access":"access","refresh":"refresh","expires":9999999999999_f64}),
        )
        .unwrap();
    registry.refresh().unwrap();
    let first = live_map_snapshot(&registry);
    registry.refresh().unwrap();
    let second = live_map_snapshot(&registry);
    registry.unregister_provider(LIVE).unwrap();
    registry.unregister_provider(SIDE).unwrap();
    let actual = js_json!({"mode":mode,"first":first,"second":second,"calls":*calls.lock().unwrap()});
    assert_eq!(actual, expected);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
