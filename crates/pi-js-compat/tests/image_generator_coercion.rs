use crate::shared_oracle_2 as oracle;
use pi_js_compat::js_json::{JsValue, is_truthy, parse_lossless, stringify_lossless, to_js_string};
use serde_json::json;

#[test]
fn lossless_numbers_retain_overflow_and_negative_zero_until_json_boundary() {
    let JsValue::Number(negative_zero) = parse_lossless("-0").unwrap() else {
        panic!("number");
    };
    assert!(negative_zero.is_sign_negative());
    for source in ["1e400", "-1e400"] {
        let parsed = parse_lossless(source).unwrap();
        let JsValue::Number(number) = &parsed else {
            panic!("number");
        };
        assert!(number.is_infinite());
        assert_eq!(stringify_lossless(&parsed), "null");
        assert_eq!(parsed.into_serde_lossy(), serde_json::Value::Null);
        assert_eq!(pi_js_compat::js_json::parse(source).unwrap(), serde_json::Value::Null);
    }
}

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
        "1e400",
        "-1e400",
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
