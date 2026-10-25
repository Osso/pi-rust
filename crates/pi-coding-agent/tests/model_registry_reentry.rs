#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::{
    auth_storage::AuthStorage,
    model_registry::{ExtensionOAuthConfig, ModelRegistry, ProviderConfigInput, ProviderModelInput},
};
use pi_js_compat::{js_json, js_json::JsValue};
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
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let directory = tempfile::tempdir().unwrap();
    let expected_path = directory.path().join("expected.json");
    std::fs::write(&expected_path, js_json::stringify_lossless(&expected["output"])).unwrap();
    let log_path = directory.path().join("child.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "isolated_reentrant_registry_probe",
            "--ignored",
            "--nocapture",
        ])
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
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(auth.as_object().unwrap().clone()));
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
            models
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
    registry.refresh();
    let after_refresh = snapshot(&registry);
    registry.unregister_provider(PROVIDER);
    let after_unregister = snapshot(&registry);
    registry.unregister_provider(NESTED);
    let actual = js_json!({"observations":observations.lock().unwrap().clone(),"afterRefresh":after_refresh,"afterUnregister":after_unregister});
    assert_eq!(actual, expected);
}
