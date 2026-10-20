#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::{get_models_dev_cost, process_baseten_models, process_fireworks_models};
use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
use serde_json::json;

#[test]
fn raw_provider_errors_and_lone_surrogates_are_not_typed_away() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for (operation, input) in [
        ("getModelsDevCost", r#"{"tiers":{}}"#),
        ("getModelsDevCost", r#"{"tiers":[null]}"#),
        ("processBasetenModels", r#"{"models":{"bad":null}}"#),
        ("processBasetenModels", r#"{"models":{"bad":{"reasoning_options":{}}}}"#),
        (
            "processBasetenModels",
            r#"{"models":{"bad":{"reasoning_options":[null]}}}"#,
        ),
        ("processFireworksModels", r#"{"models":{"bad":null}}"#),
        (
            "processFireworksModels",
            r#"{"models":{"bad":{"tool_call":true,"modalities":{"input":2}}}}"#,
        ),
        (
            "processBasetenModels",
            r#"{"models":{"\ud800":{"name":"\ud801"},"\ud801":{"name":"\ufffd"},"\ufffd":{"name":"\ud800"}}}"#,
        ),
    ] {
        let expected = source.request(&json!({"operation":operation,"argsJson":format!("[{input}]")}));
        let input = parse_lossless(input).unwrap();
        let actual = match operation {
            "getModelsDevCost" => get_models_dev_cost(&input),
            "processBasetenModels" => process_baseten_models(&input),
            _ => process_fireworks_models(&input),
        };
        if let Some(error) = expected.get("error") {
            let actual = actual.unwrap_err();
            assert_eq!(actual.name, error["name"].as_str().unwrap());
            assert_eq!(actual.message.to_utf8().unwrap(), error["message"].as_str().unwrap());
        } else {
            assert_eq!(stringify_lossless(&actual.unwrap()), expected["json"].as_str().unwrap());
        }
    }
}
