//! Frozen generate-models.ts:328–589 metadata, actual module via AST exposure.
#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::apply_model_metadata;
use pi_js_compat::{js_json, js_json::JsValue};

#[test]
fn metadata_matches_frozen_common_pin_without_old_only_fields() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let models = js_json!([
        {"provider":"deepseek","id":"deepseek-v4-flash","api":"openai-completions","baseUrl":"https://api.deepseek.com","reasoning":true,"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0}},
        {"provider":"openai-codex-team","id":"gpt-6-astra","api":"openai-codex-responses","baseUrl":"https://chatgpt.com/backend-api","reasoning":true,"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0}},
        {"provider":"anthropic","id":"claude-sonnet-5-5","api":"anthropic-messages","baseUrl":"https://api.anthropic.com","reasoning":true,"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0}}
    ]);
    let expected = source.request(&js_json!({"operation":"metadata","models":models}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut actual = models;
    for model in actual.as_array_mut().unwrap() {
        apply_model_metadata(model).unwrap();
    }
    assert_eq!(actual, expected["result"]);
}
