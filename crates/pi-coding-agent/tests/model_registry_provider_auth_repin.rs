#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_oracle as oracle;

use pi_coding_agent::core::{
    auth_storage::{AuthStatus, AuthStorage},
    model_registry::{ModelRegistry, ProviderConfigInput, ResolvedRequestAuth},
};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};
use std::path::Path;

const PROVIDER: &str = "registry-provider-auth";
const REQUEST_PATH: &str = "PI_REGISTRY_PROVIDER_REQUESTS";

fn cases(witness: &Path) -> Vec<JsValue> {
    let raw = JsString::from("raw-") + JsString::from_utf16(vec![0xd800]);
    vec![
        js_json!({"name":"no-config"}),
        js_json!({"name":"empty-key-without-headers","config":{"apiKey":""}}),
        js_json!({"name":"empty-key-with-headers","config":{"apiKey":"","headers":{}}}),
        js_json!({"name":"literal-key","config":{"apiKey":"fixture-key","authHeader":true,"name":"Literal"}}),
        js_json!({"name":"whitespace-key-empty-name","config":{"apiKey":" ","authHeader":true,"name":""}}),
        js_json!({"name":"raw-key-name-header","config":{"apiKey":raw,"authHeader":true,"name":raw,"headers":{"X-Raw":raw}}}),
        js_json!({"name":"command-key","witness":witness.to_str().unwrap(),"config":{"apiKey":format!("!printf x >> '{}'; printf fixture-command-key",witness.display()),"authHeader":true}}),
        js_json!({"name":"missing-env","config":{"apiKey":"$PI_REGISTRY_PROVIDER_KEY"}}),
        js_json!({"name":"present-env","envValue":"fixture-env-key","config":{"apiKey":"$PI_REGISTRY_PROVIDER_KEY","authHeader":true}}),
        js_json!({"name":"empty-env","envValue":"","config":{"apiKey":"$PI_REGISTRY_PROVIDER_KEY"}}),
        js_json!({"name":"stored-key","auth":{PROVIDER:{"type":"api_key","key":raw}},"config":{"apiKey":"config-key","authHeader":true}}),
        js_json!({"name":"empty-stored-key","auth":{PROVIDER:{"type":"api_key","key":""}},"config":{"apiKey":"config-key","authHeader":true}}),
        js_json!({"name":"runtime-key","runtimeKey":"runtime-key","config":{"apiKey":"config-key","authHeader":true}}),
        js_json!({"name":"empty-runtime-key","runtimeKey":"","config":{"apiKey":"config-key","authHeader":true}}),
        js_json!({"name":"unknown-oauth-provider","auth":{PROVIDER:{"type":"oauth","access":"access","refresh":"refresh","expires":9999999999999_f64}},"config":{"apiKey":"config-key","authHeader":true}}),
        js_json!({"name":"stored-env-precedence","envValue":"global-key","auth":{PROVIDER:{"type":"api_key","key":"$PI_REGISTRY_PROVIDER_ABSENT","env":{"PI_REGISTRY_PROVIDER_KEY":"stored-key"}}},"config":{"apiKey":"$PI_REGISTRY_PROVIDER_KEY","authHeader":true,"headers":{"X-Env":"$PI_REGISTRY_PROVIDER_KEY"}}}),
    ]
}

#[tokio::test]
async fn full_provider_status_key_and_request_auth_matrix_matches_frozen() {
    let directory = tempfile::tempdir().unwrap();
    let source_cases = cases(&directory.path().join("source-command"));
    let native_cases = cases(&directory.path().join("native-command"));
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let mut expected = Vec::new();
    for mut request in source_cases {
        request["operation"] = JsValue::String("provider_auth_snapshot".into());
        let result = source.request(&request);
        assert_eq!(
            result["ok"],
            JsValue::Bool(true),
            "{}: {result:?}",
            js_json::stringify_lossless(&request["name"])
        );
        if request["name"].as_string().unwrap() == "command-key" {
            assert_eq!(result["output"]["before"]["commandCount"], JsValue::Number(0.0));
            assert_eq!(result["output"]["commandCount"], JsValue::Number(2.0));
        }
        expected.push(js_json!({"name":request["name"],"output":result["output"]}));
    }
    let path = directory.path().join("requests.json");
    std::fs::write(&path, js_json::stringify_lossless(&JsValue::Array(native_cases))).unwrap();
    let output = env_probe::run_probe(
        "isolated_provider_auth_matrix",
        &[(REQUEST_PATH, path.into_os_string())],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let wire = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PROVIDER_AUTH_RESULT:"))
        .expect("native matrix output");
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
    let proof = std::env::temp_dir().join(format!("registry-provider-auth-{}.json", std::process::id()));
    std::fs::write(&proof, js_json::stringify_lossless(&JsValue::Array(differences))).unwrap();
    assert_eq!(actual, expected, "{}", proof.display());
}

fn native_config(value: &JsValue) -> ProviderConfigInput {
    ProviderConfigInput {
        name: value.get("name").and_then(JsValue::as_string).cloned(),
        api_key: value.get("apiKey").and_then(JsValue::as_string).cloned(),
        headers: value
            .get("headers")
            .map(|headers| js_json::from_value(headers).unwrap()),
        auth_header: value.get("authHeader").and_then(JsValue::as_bool),
        ..Default::default()
    }
}
fn status_value(status: AuthStatus) -> JsValue {
    let mut value = js_json!({"configured":status.configured});
    if let Some(source) = status.source {
        value["source"] = js_json::to_value(&source).unwrap();
    }
    if let Some(label) = status.label {
        value["label"] = js_json::to_value(&label).unwrap();
    }
    value
}
fn auth_value(auth: ResolvedRequestAuth) -> JsValue {
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
fn read_command_count(request: &JsValue) -> usize {
    request
        .get("witness")
        .and_then(JsValue::as_string)
        .map(|path| {
            let path = std::path::PathBuf::from(path.to_utf8().unwrap());
            if path.exists() {
                std::fs::read(path).unwrap().len()
            } else {
                0
            }
        })
        .unwrap_or(0)
}

#[tokio::test]
#[ignore = "private environment/process matrix entry"]
async fn isolated_provider_auth_matrix() {
    assert!(env_probe::in_probe(), "private fixture only");
    let requests =
        js_json::parse_lossless(std::fs::read_to_string(std::env::var_os(REQUEST_PATH).unwrap()).unwrap()).unwrap();
    let mut outputs = Vec::new();
    for request in requests.as_array().unwrap() {
        pi_js_compat::process_env::remove("PI_REGISTRY_PROVIDER_KEY");
        pi_js_compat::process_env::remove("PI_REGISTRY_PROVIDER_ABSENT");
        if let Some(value) = request.get("envValue").and_then(JsValue::as_string) {
            pi_js_compat::process_env::set("PI_REGISTRY_PROVIDER_KEY", &value.to_utf8().unwrap());
        }
        let data = request
            .get("auth")
            .and_then(JsValue::as_object)
            .cloned()
            .unwrap_or_default();
        let storage = AuthStorage::in_memory(data);
        let registry = ModelRegistry::in_memory(storage.clone()).unwrap();
        if let Some(config) = request.get("config") {
            registry.register_provider(PROVIDER, native_config(config)).unwrap();
        }
        if let Some(key) = request.get("runtimeKey").and_then(JsValue::as_string) {
            storage.set_runtime_api_key(PROVIDER, key);
        }
        let mut model = registry.find("anthropic", "claude-opus-4-8").unwrap();
        model.provider = PROVIDER.into();
        let before = js_json!({"configured":registry.has_configured_auth(&model).unwrap(),"status":status_value(registry.get_provider_auth_status(PROVIDER)),"display":registry.get_provider_display_name(PROVIDER),"oauth":registry.is_using_oauth(&model),"commandCount":read_command_count(request)});
        let key = registry.get_api_key_for_provider(PROVIDER).await.unwrap();
        let auth = auth_value(registry.get_api_key_and_headers(&model).await.unwrap());
        outputs.push(js_json!({"name":request["name"],"output":{"before":before,"key":key,"auth":auth,"commandCount":read_command_count(request)}}));
    }
    println!(
        "\nPROVIDER_AUTH_RESULT:{}",
        js_json::stringify_lossless(&JsValue::Array(outputs))
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
