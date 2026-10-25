#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_oracle as oracle;

use pi_coding_agent::core::{auth_storage::AuthStorage, model_registry::ModelRegistry};
use pi_js_compat::{js_json, js_json::JsValue};
use std::path::Path;

const REQUEST_PATH: &str = "PI_REGISTRY_CONSTRUCTOR_REQUESTS";

fn write_models(path: &Path, name: &str) {
    std::fs::write(path,format!(r#"{{"providers":{{"constructor-fixture":{{"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{{"id":"one","name":"{name}"}}]}}}}}}"#)).unwrap();
}
fn registry_snapshot(registry: &ModelRegistry) -> JsValue {
    let models = registry
        .get_all()
        .into_iter()
        .filter(|model| model.provider == "constructor-fixture")
        .collect::<Vec<_>>();
    let models = js_json::to_value(&models).unwrap();
    js_json!({"error":registry.get_error(),"models":models,"serializedModels":js_json::stringify_lossless(&models)})
}

#[tokio::test]
async fn constructor_normalization_error_boundary_and_path_lifetime_match_frozen() {
    let directory = tempfile::tempdir().unwrap();
    let home_a = directory.path().join("home-a");
    let home_b = directory.path().join("home-b");
    std::fs::create_dir(&home_a).unwrap();
    std::fs::create_dir(&home_b).unwrap();
    let models_a = home_a.join("models.json");
    write_models(&models_a, "Home A");
    write_models(&home_b.join("models.json"), "Home B");
    let invalid = home_a.join("invalid.json");
    std::fs::write(
        &invalid,
        r#"{"providers":{"constructor-fixture":{"models":[{"id":"one","api":"openai-completions"}]}}}"#,
    )
    .unwrap();
    let syntax = home_a.join("syntax.json");
    std::fs::write(&syntax, "{").unwrap();
    let mut requests = vec![
        js_json!({"name":"tilde-home-change","path":"~/models.json"}),
        js_json!({"name":"absolute-home-control","path":models_a.to_str().unwrap()}),
        js_json!({"name":"default-path-control","agentDir":home_a.to_str().unwrap()}),
        js_json!({"name":"empty-path-control","path":"","agentDir":home_a.to_str().unwrap()}),
        js_json!({"name":"file-url-validation-error","path":url::Url::from_file_path(invalid).unwrap().to_string()}),
        js_json!({"name":"file-url-syntax-control","path":url::Url::from_file_path(syntax).unwrap().to_string()}),
        js_json!({"name":"file-url-directory-read-error","path":url::Url::from_file_path(&home_a).unwrap().to_string()}),
        js_json!({"name":"malformed-file-url","path":"file://%"}),
        js_json!({"name":"nonlocal-file-url","path":"file://server/models.json"}),
        js_json!({"name":"encoded-file-url-separator","path":"file:///tmp/a%2Fb"}),
    ];
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let mut expected = Vec::new();
    for request in &mut requests {
        request["operation"] = JsValue::String("constructor_lifetime".into());
        request["home"] = JsValue::String(home_a.to_str().unwrap().into());
        request["nextHome"] = JsValue::String(home_b.to_str().unwrap().into());
        let result = source.request(request);
        assert_eq!(result["ok"], JsValue::Bool(true), "{result:?}");
        expected.push(js_json!({"name":request["name"],"output":result["output"]}));
    }
    let path = directory.path().join("requests.json");
    std::fs::write(&path, js_json::stringify_lossless(&JsValue::Array(requests))).unwrap();
    let output = env_probe::run_probe(
        "isolated_registry_constructor_matrix",
        &[(REQUEST_PATH, path.into_os_string())],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let wire = stdout
        .lines()
        .find_map(|line| line.strip_prefix("REGISTRY_CONSTRUCTOR_RESULT:"))
        .expect("native constructor output");
    let actual = js_json::parse_lossless(wire).unwrap();
    let expected = JsValue::Array(expected);
    let differences = actual
        .as_array()
        .unwrap()
        .iter()
        .zip(expected.as_array().unwrap())
        .filter(|(actual, expected)| actual != expected)
        .map(|(actual, expected)| js_json!({"actual":actual,"expected":expected}))
        .collect::<Vec<_>>();
    let proof = std::env::temp_dir().join(format!("registry-constructor-{}.json", std::process::id()));
    std::fs::write(&proof, js_json::stringify_lossless(&JsValue::Array(differences))).unwrap();
    assert_eq!(actual, expected, "{}", proof.display());
}

#[tokio::test]
#[ignore = "private environment/process constructor matrix"]
async fn isolated_registry_constructor_matrix() {
    assert!(env_probe::in_probe(), "private fixture only");
    let requests =
        js_json::parse_lossless(std::fs::read_to_string(std::env::var_os(REQUEST_PATH).unwrap()).unwrap()).unwrap();
    let mut outputs = Vec::new();
    for request in requests.as_array().unwrap() {
        pi_js_compat::process_env::set("HOME", &request["home"].as_string().unwrap().to_utf8().unwrap());
        pi_js_compat::process_env::remove("PI_CODING_AGENT_DIR");
        if let Some(dir) = request.get("agentDir").and_then(JsValue::as_string) {
            pi_js_compat::process_env::set("PI_CODING_AGENT_DIR", &dir.to_utf8().unwrap());
        }
        let path = request.get("path").and_then(JsValue::as_string).cloned();
        let outcome = match ModelRegistry::create(AuthStorage::in_memory(serde_json::Map::new()), path) {
            Ok(registry) => {
                let before = registry_snapshot(&registry);
                pi_js_compat::process_env::set("HOME", &request["nextHome"].as_string().unwrap().to_utf8().unwrap());
                registry.refresh().unwrap();
                js_json!({"created":true,"before":before,"after":registry_snapshot(&registry)})
            }
            Err(error) => js_json!({"created":false,"error":error.thrown_value().value()}),
        };
        outputs.push(js_json!({"name":request["name"],"output":outcome}));
    }
    println!(
        "\nREGISTRY_CONSTRUCTOR_RESULT:{}",
        js_json::stringify_lossless(&JsValue::Array(outputs))
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
