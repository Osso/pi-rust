#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::js_json::{JsValue, is_truthy, parse_lossless, stringify_lossless, to_js_string};
use serde_json::json;

#[test]
fn json_coercions_match_node_without_losing_code_units() {
    let mut ts = oracle::TsOracle::spawn(
        "image-generator-coercion-oracle.ts",
        "packages/ai/scripts/generate-image-models.ts",
    );
    for value in [
        "null",
        "false",
        "true",
        "0",
        "-0",
        "1.5",
        "1e21",
        r#""""#,
        r#""\ud800""#,
        "[]",
        "[null,1,[2,3],{}]",
        "{}",
        r#"{"toString":null}"#,
        r#"{"valueOf":1}"#,
    ] {
        let parsed = parse_lossless(value).unwrap();
        let actual = match to_js_string(&parsed) {
            Ok(text) => json!({"truthy":is_truthy(&parsed),"text":stringify_lossless(&JsValue::String(text))}),
            Err(error) => json!({"name":"TypeError","message":error.to_string()}),
        };
        assert_eq!(actual, ts.request(&json!({"value":value})), "{value}");
    }
}
