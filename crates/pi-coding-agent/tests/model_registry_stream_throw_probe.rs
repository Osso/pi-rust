//! Actual frozen callback contract. Native synchronous rejection channel is not yet represented.
#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

#[tokio::test]
async fn frozen_registered_raw_and_simple_dispatch_propagate_callback_rejection_synchronously() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let raw = JsString::from("stream-") + JsString::from_utf16(vec![0xd800]);
    let config = js_json!({"baseUrl":"https://fixture.invalid","models":[{"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32}]});
    for method in ["stream", "streamSimple"] {
        for kind in ["error", "value"] {
            let result=source.request(&js_json!({"operation":"registration_stream_throw","method":method,"kind":kind,"value":raw,"config":config}));
            assert_eq!(result["ok"], JsValue::Bool(true), "{result:?}");
            let rejection = if kind == "error" {
                js_json!({"kind":"error","value":{"name":"TypeError","message":raw,"code":"E_STREAM","detail":{"method":method}}})
            } else {
                js_json!({"kind":"value","value":raw})
            };
            assert_eq!(
                result["output"],
                js_json!({"callbackCount":1,"rejection":rejection}),
                "{method}/{kind}"
            );
        }
    }
}
