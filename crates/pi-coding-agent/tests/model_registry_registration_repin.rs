#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::types::{CompatFields, Model};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::{ModelRegistry, ProviderConfigInput, ProviderModelInput};
use pi_js_compat::{js_json, js_json::JsValue};
use serde_json::Value;

const PROVIDER: &str = "registry-model-input";

fn model_definition() -> JsValue {
    js_json!({"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32})
}

fn full_model_input(definition: &JsValue) -> Model {
    // A complete Model is a valid structural source input too. This case uses
    // an explicit endpoint/API and deliberately carries additional root fields.
    let mut wire = js_json!({"id":definition["id"],"name":definition["name"],"api":definition.get("api").unwrap().clone(),"provider":"ignored-input-provider","baseUrl":definition.get("baseUrl").unwrap().clone(),"reasoning":definition["reasoning"],"input":definition["input"],"cost":definition["cost"],"contextWindow":definition["contextWindow"],"maxTokens":definition["maxTokens"]});
    for (key, value) in definition.as_object().unwrap() {
        if !wire.as_object().unwrap().contains_key(key) {
            wire.as_object_mut().unwrap().insert(key.clone(), value.clone());
        }
    }
    js_json::from_value(&wire).unwrap()
}

#[tokio::test]
async fn registered_model_inputs_preserve_frozen_optional_fields_and_reconstruction() {
    let mut oracle = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let mut differences = Vec::new();
    for name in [
        "inherit-endpoint",
        "explicit-empty-endpoint",
        "model-endpoint",
        "root-reconstruction",
        "empty-model-api",
    ] {
        let mut definition = model_definition();
        match name {
            "explicit-empty-endpoint" => definition["baseUrl"] = JsValue::String("".into()),
            "model-endpoint" => definition["baseUrl"] = JsValue::String("https://model.invalid".into()),
            "root-reconstruction" => {
                definition["api"] = JsValue::String("openai-completions".into());
                definition["baseUrl"] = JsValue::String("https://model.invalid".into());
                definition["samplingParams"] = js_json!({"temperature":0.4});
                definition["autoCompactionThreshold"] = JsValue::Number(0.5);
                definition["unknownRoot"] = js_json!({"extra":"source drops this"});
            }
            "empty-model-api" => definition["api"] = JsValue::String("".into()),
            "inherit-endpoint" => {}
            _ => unreachable!(),
        }
        let config = js_json!({"api":"openai-completions","baseUrl":"https://provider.invalid","apiKey":"fixture-key","models":[definition]});
        let expected: JsValue = oracle
            .request(&js_json!({"id":name,"operation":"registration_snapshot","provider":PROVIDER,"config":config}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{name}: {expected:?}");
        let registry = ModelRegistry::in_memory(AuthStorage::in_memory(Vec::<(String, Value)>::new()));
        let error = registry
            .register_provider(
                PROVIDER,
                ProviderConfigInput {
                    api: Some("openai-completions".into()),
                    base_url: Some("https://provider.invalid".into()),
                    api_key: Some("fixture-key".into()),
                    models: Some(vec![if name == "root-reconstruction" {
                        ProviderModelInput::from(full_model_input(&definition))
                    } else {
                        js_json::from_value::<ProviderModelInput>(&definition).unwrap()
                    }]),
                    ..Default::default()
                },
            )
            .err()
            .map(|error| error.message().clone());
        let models = registry
            .get_all()
            .into_iter()
            .filter(|model| model.provider == PROVIDER)
            .collect::<Vec<_>>();
        let models = js_json::to_value(&models).unwrap();
        let actual = js_json!({"error":error,"serializedModels":js_json::stringify_lossless(&models),"models":models});
        if actual != expected["output"] {
            differences.push(js_json!({"case":name,"actual":actual,"expected":expected["output"]}));
        }
    }
    let path = std::env::temp_dir().join(format!("q15e-registry-registration-{}.json", std::process::id()));
    std::fs::write(&path, js_json::stringify_lossless(&JsValue::Array(differences.clone()))).unwrap();
    assert!(
        differences.is_empty(),
        "{} registration differences; {}",
        differences.len(),
        path.display()
    );
}

#[tokio::test]
async fn registering_a_loaded_model_copies_native_compat_before_json_projection() {
    // This is valid models.json ingress: another optional compat schema accepts
    // supportsStore:1e400. Source preserves Infinity until output serialization.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models.json");
    std::fs::write(&path,r#"{"providers":{"source-fixture":{"api":"openai-completions","baseUrl":"https://source.invalid","models":[{"id":"one","compat":{"supportsStore":1e400,"unknownHuman":"\ud800"}}]}}}"#).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let expected:JsValue=oracle.request(&js_json!({"id":"copy-native-values","operation":"registration_from_existing","modelsPath":path.to_str().unwrap(),"sourceProvider":"source-fixture","sourceModel":"one","provider":PROVIDER,"config":{"api":"openai-completions","baseUrl":"https://provider.invalid","apiKey":"fixture-key"}}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(
        expected["output"]["numberState"],
        js_json!({"type":"number","finite":false,"positiveInfinity":true})
    );
    let registry = ModelRegistry::create(
        AuthStorage::in_memory(Vec::<(String, Value)>::new()),
        Some(path.to_str().unwrap().into()),
    )
    .unwrap();
    let input = registry.find("source-fixture", "one").unwrap();
    registry
        .register_provider(
            PROVIDER,
            ProviderConfigInput {
                api: Some("openai-completions".into()),
                base_url: Some("https://provider.invalid".into()),
                api_key: Some("fixture-key".into()),
                models: Some(vec![input.into()]),
                ..Default::default()
            },
        )
        .unwrap();
    let models = registry
        .get_all()
        .into_iter()
        .filter(|model| model.provider == PROVIDER)
        .collect::<Vec<_>>();
    let value = models[0]
        .compat
        .as_ref()
        .unwrap()
        .raw("supportsStore")
        .unwrap()
        .as_f64()
        .unwrap();
    let number_state = js_json!({"type":"number","finite":value.is_finite(),"positiveInfinity":value==f64::INFINITY});
    let models = js_json::to_value(&models).unwrap();
    let actual = js_json!({"error":null,"models":models,"serializedModels":js_json::stringify_lossless(&models),"numberState":number_state});
    assert_eq!(actual, expected["output"]);
}
