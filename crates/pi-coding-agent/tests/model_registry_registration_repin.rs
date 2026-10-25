use crate::shared_oracle as oracle;

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
        "empty-provider-override",
        "empty-provider-with-headers",
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
            "inherit-endpoint" | "empty-provider-override" | "empty-provider-with-headers" => {}
            _ => unreachable!(),
        }
        let config = js_json!({"api":"openai-completions","baseUrl":"https://provider.invalid","apiKey":"fixture-key","models":[definition]});
        let update = if name == "empty-provider-override" {
            Some(js_json!({"baseUrl":""}))
        } else if name == "empty-provider-with-headers" {
            Some(js_json!({"baseUrl":"","headers":{}}))
        } else {
            None
        };
        let updates = update.clone().into_iter().collect::<Vec<_>>();
        let expected: JsValue = oracle
            .request(&js_json!({"id":name,"operation":"registration_snapshot","provider":PROVIDER,"config":config,"updates":updates}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{name}: {expected:?}");
        let registry = ModelRegistry::in_memory(AuthStorage::in_memory(Vec::<(String, Value)>::new())).unwrap();
        let mut error = registry
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
            .map(|error| error.error_or_string_message().unwrap());
        if update.is_some() && error.is_none() {
            error = registry
                .register_provider(
                    PROVIDER,
                    ProviderConfigInput {
                        base_url: Some("".into()),
                        headers: (name == "empty-provider-with-headers").then(Default::default),
                        ..Default::default()
                    },
                )
                .err()
                .map(|error| error.error_or_string_message().unwrap());
        }
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

#[tokio::test]
async fn dynamic_validation_order_and_empty_model_guards_match_frozen() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let definition = model_definition();
    let mut model_api = definition.clone();
    model_api["api"] = JsValue::String("registry-validation-api".into());
    let mut second = definition.clone();
    second["id"] = JsValue::String("second".into());
    let cases = vec![
        ("stream-missing-api", js_json!({}), true),
        ("stream-empty-api", js_json!({"api":""}), true),
        (
            "stream-without-models",
            js_json!({"api":"registry-validation-api"}),
            true,
        ),
        ("empty-models", js_json!({"models":[]}), false),
        (
            "missing-provider-url",
            js_json!({"api":"registry-validation-api","apiKey":"fixture","models":[definition]}),
            false,
        ),
        (
            "missing-key-and-oauth",
            js_json!({"api":"registry-validation-api","baseUrl":"https://fixture.invalid","models":[definition]}),
            false,
        ),
        (
            "missing-effective-api",
            js_json!({"baseUrl":"https://fixture.invalid","apiKey":"fixture","models":[definition]}),
            false,
        ),
        (
            "model-api-without-provider-api",
            js_json!({"baseUrl":"https://fixture.invalid","apiKey":"fixture","models":[model_api]}),
            false,
        ),
        (
            "empty-provider-api-model-fallback",
            js_json!({"api":"","baseUrl":"https://fixture.invalid","apiKey":"fixture","models":[model_api]}),
            false,
        ),
        (
            "validate-all-before-apply",
            js_json!({"baseUrl":"https://fixture.invalid","apiKey":"fixture","models":[model_api,second]}),
            false,
        ),
    ];
    for (name, config, stream_callback) in cases {
        let response = source.request(&js_json!({"operation":"registration_snapshot","provider":"registry-validation","config":config,"streamCallback":stream_callback}));
        assert_eq!(response["ok"], JsValue::Bool(true), "{name}: {response:?}");
        let registry = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap();
        let mut input = ProviderConfigInput {
            api: config
                .get("api")
                .and_then(JsValue::as_string)
                .map(|value| value.to_utf8().unwrap()),
            base_url: config.get("baseUrl").and_then(JsValue::as_string).cloned(),
            api_key: config.get("apiKey").and_then(JsValue::as_string).cloned(),
            models: config.get("models").map(|value| js_json::from_value(value).unwrap()),
            ..Default::default()
        };
        if stream_callback {
            input.stream_simple = Some(std::sync::Arc::new(|_, _, _| {
                unreachable!("validation must not stream")
            }));
        }
        let error = registry
            .register_provider("registry-validation", input)
            .err()
            .map(|error| error.error_or_string_message().unwrap());
        let models = registry
            .get_all()
            .into_iter()
            .filter(|model| model.provider == "registry-validation")
            .collect::<Vec<_>>();
        let actual = js_json!({"error":error,"serializedModels":js_json::stringify_lossless(&js_json::to_value(&models).unwrap()),"models":models});
        registry.unregister_provider("registry-validation").unwrap();
        assert_eq!(actual, response["output"], "{name}");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
