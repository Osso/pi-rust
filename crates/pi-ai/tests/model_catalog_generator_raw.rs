#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::process_baseten_models;
use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
use serde_json::json;

#[test]
fn distinct_lone_surrogate_provider_keys_and_names_keep_source_values() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let input = r#"{"models":{"\ud800":{"name":"\ud801"},"\ud801":{"name":"\ufffd"},"\ufffd":{"name":"\ud800"}}}"#;
    let expected = source.request(&json!({"operation":"processBasetenModels","argsJson":format!("[{input}]")}));
    let actual = process_baseten_models(&parse_lossless(input).unwrap()).unwrap();
    assert_eq!(stringify_lossless(&actual), expected["json"].as_str().unwrap());
}
