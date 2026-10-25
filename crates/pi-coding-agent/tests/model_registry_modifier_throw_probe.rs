//! Actual frozen behavior only. Native parity is blocked by infallible modifier APIs.
#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

#[tokio::test]
async fn frozen_modifier_throw_preserves_brand_value_and_partial_state_at_each_entrypoint() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let raw = JsString::from("modifier-") + JsString::from_utf16(vec![0xd800]);
    let config = js_json!({"api":"openai-completions","baseUrl":"https://fixture.invalid","models":[{"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32}]});
    for stage in ["register", "refresh", "constructor"] {
        for kind in ["error", "value"] {
            let result = source.request(
                &js_json!({"operation":"modifier_throw","stage":stage,"throwKind":kind,"value":raw,"config":config}),
            );
            assert_eq!(result["ok"], JsValue::Bool(true), "{result:?}");
            let rejection = if kind == "error" {
                js_json!({"kind":"error","value":{"name":"TypeError","message":raw,"code":"E_MODIFIER","detail":{"stage":stage}}})
            } else {
                js_json!({"kind":"value","value":raw})
            };
            let display = if stage == "register" {
                "Modifier Fixture"
            } else {
                "Registered Modifier"
            };
            assert_eq!(
                result["output"],
                js_json!({"rejection":rejection,"ids":["one"],"display":display}),
                "{stage}/{kind}"
            );
        }
    }
}
