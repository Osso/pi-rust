//! Actual frozen callback throws, including a throw from the request-auth catch itself.
use crate::shared_oracle as oracle;
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

fn at<'a>(value: &'a JsValue, path: &[&str]) -> &'a JsValue {
    path.iter()
        .fold(value, |value, key| value.get(key).expect("oracle field"))
}
#[test]
fn frozen_oauth_get_api_key_rejects_exact_values_and_request_auth_coerces_them() {
    let mut oracle = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let raw = JsString::from_utf16(vec![0x71, 0xd800, 0x31, 0xdc00]);
    let mut results = Vec::new();
    for kind in ["value", "error"] {
        let response = oracle.request(
            &js_json!({"id":kind,"operation":"oauth_throw","throwKind":kind,"value":raw,"detail":{"native":true}}),
        );
        assert_eq!(at(&response, &["ok"]), &JsValue::Bool(true));
        let output = at(&response, &["output"]);
        assert_eq!(at(output, &["key", "ok"]), &JsValue::Bool(false));
        let value = at(output, &["key", "rejection", "value"]);
        if kind == "error" {
            assert_eq!(at(value, &["message"]), &JsValue::String(raw.clone()));
            assert_eq!(at(value, &["code"]), &JsValue::String("E_Q15".into()));
        } else {
            assert_eq!(value, &JsValue::String(raw.clone()));
        }
        assert_eq!(at(output, &["auth", "ok"]), &JsValue::Bool(true));
        assert_eq!(at(output, &["auth", "value", "error"]), &JsValue::String(raw.clone()));
        results.push(response);
    }
    let response =
        oracle.request(&js_json!({"id":"shadow","operation":"oauth_throw","throwKind":"value","value":{"toString":0}}));
    assert_eq!(at(&response, &["ok"]), &JsValue::Bool(true));
    assert_eq!(at(&response, &["output", "auth", "ok"]), &JsValue::Bool(false));
    assert_eq!(
        at(&response, &["output", "auth", "rejection", "value", "name"]),
        &JsValue::String("TypeError".into())
    );
    assert_eq!(
        at(&response, &["output", "auth", "rejection", "value", "message"]),
        &JsValue::String("Cannot convert object to primitive value".into())
    );
    results.push(response);
    println!("{}", js_json::stringify_lossless(&JsValue::Array(results)));
}
