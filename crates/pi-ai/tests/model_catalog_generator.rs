#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::model_catalog_generator::{get_models_dev_cost, process_baseten_models, process_fireworks_models};
use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
use serde_json::json;

#[test]
fn source_cost_tiers_and_provider_transforms_preserve_values_and_utf16() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for text in [
        "null",
        "{}",
        r#"{"input":2,"output":0,"cache_read":0.25,"cache_write":3,"tiers":[{"tier":{"type":"context","size":0},"input":0.8},{"tier":{"type":"image","size":10}},{"tier":{"type":"context"}},{"tier":{"type":"context","size":1000},"output":4}]}"#,
    ] {
        let expected = source.request(
            &json!({"operation":"getModelsDevCost","args":[serde_json::from_str::<serde_json::Value>(text).unwrap()]}),
        );
        assert_eq!(
            stringify_lossless(&get_models_dev_cost(&parse_lossless(text).unwrap())),
            expected["json"].as_str().unwrap()
        );
    }
    for provider in [
        "null",
        "{}",
        r#"{"models":{"zai-org/GLM-5.2":{"name":"GLM","reasoning":true},"zai-org/GLM-5.2-Fast":{"name":"","tool_call":true},"skip":{"status":"deprecated","tool_call":true},"accounts/glm-5p2":{"tool_call":true},"accounts/kimi-k3":{"tool_call":true,"modalities":{"input":["image"]}},"plain":{"tool_call":true,"cost":{"input":0.1},"reasoning_options":[{"type":"effort","values":["low","high"]}]},"toggle":{"reasoning_options":[{"type":"toggle"}]},"both":{"reasoning_options":[{"type":"toggle"},{"type":"effort","values":["max"]}]}}}"#,
    ] {
        let input = parse_lossless(provider).unwrap();
        for (name, result) in [
            ("processBasetenModels", process_baseten_models(&input)),
            ("processFireworksModels", process_fireworks_models(&input)),
        ] {
            let expected = source.request(
                &json!({"operation":name,"args":[serde_json::from_str::<serde_json::Value>(provider).unwrap()]}),
            );
            assert_eq!(
                stringify_lossless(&result),
                expected["json"].as_str().unwrap(),
                "{name}: {provider}; response {expected}"
            );
        }
    }
}
