use crate::shared_oracle as oracle;
#[path = "../examples/extensions/provider_payload.rs"]
mod provider_payload;
use crate::shared_extensions as support;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::model_registry::ModelRegistry;
use serde_json::{Map, Value, json};

async fn run_native(request: &Value) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().display().to_string();
    let log_path = temp
        .path()
        .join(pi_coding_agent::config::config_dir_name().to_utf8().unwrap())
        .join("provider-payload.log");
    if request["directory"] == true {
        std::fs::create_dir(log_path.parent().unwrap()).unwrap();
    }
    if let Some(initial) = request["initial"].as_str() {
        std::fs::write(&log_path, initial).unwrap();
    }
    let runtime = create_extension_runtime();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = provider_payload::provider_payload_extension(api);
        Box::pin(async move { result })
    });
    let mut registry = ExtensionRegistry::new();
    registry.register("<example:provider-payload>", factory);
    let extension = load_extension_from_factory(
        registry
            .get(&JsString::from("<example:provider-payload>"))
            .unwrap()
            .clone(),
        &JsString::from(&cwd),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<example:provider-payload>"),
    )
    .await
    .unwrap();
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(Map::new())).unwrap();
    let mut runner = support::runner_for(vec![extension], &cwd, runtime, Arc::new(model_registry));
    runner
        .bind_core(support::actions(), support::context_actions(), None)
        .unwrap();
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let error_log = errors.clone();
    let _subscription = runner.on_error(Arc::new(move |error| {
        error_log.lock().unwrap().push(error.event.clone());
    }));
    let mut payloads = Vec::new();
    for event in request["events"].as_array().unwrap() {
        if event["type"] == "before_provider_request" {
            payloads.push(
                runner
                    .emit_before_provider_request(JsValue::from(event["payload"].clone()))
                    .await,
            );
        } else {
            let mut event: ExtensionEvent = serde_json::from_value(event.clone()).unwrap();
            runner.emit(&mut event).await;
        }
    }
    let content = match std::fs::read_to_string(log_path) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("read log: {error}"),
    };
    let errors = errors.lock().unwrap().clone();
    json!({"content":content, "errors":errors, "payloads":payloads})
}

fn response(status: u16, headers: Value) -> Value {
    json!({"type":"after_provider_response", "status":status, "headers":headers})
}

#[tokio::test]
async fn pinned_provider_payload_example_matches_appends_and_missing_directory_errors() {
    let mut oracle = oracle::TsOracle::spawn(
        "example-provider-payload-oracle.ts",
        "packages/coding-agent/examples/extensions/provider-payload.ts",
    );
    let events = json!([
        {"type":"before_provider_request", "payload":{"z":"λ🚀\n\t", "a":[true,null,1e-7,1e21], "nested":{"2":2,"1":1}}},
        response(201, json!({"z-first":"last alphabetically","a-second":"first alphabetically"})),
        {"type":"before_provider_request", "payload":null},
        response(204, json!({})),
        {"type":"before_provider_request", "payload":[{"key":"value"},false]}
    ]);
    for request in [
        json!({"directory":true,"initial":"existing\n","events":events}),
        json!({"directory":true,"initial":null,"events":events}),
        json!({"directory":false,"initial":null,"events":events}),
    ] {
        let expected = oracle.request(&request);
        if request["directory"] == false {
            assert_eq!(expected["errors"].as_array().unwrap().len(), 5);
            assert_eq!(expected["content"], Value::Null);
        } else {
            let content = expected["content"].as_str().unwrap();
            assert!(
                content
                    .contains("[201] {\"z-first\":\"last alphabetically\",\"a-second\":\"first alphabetically\"}\n\n")
            );
            assert!(content.contains("null\n\n[204] {}\n\n"));
            assert_eq!(expected["errors"], json!([]));
        }
        assert_eq!(run_native(&request).await, expected);
    }
}
