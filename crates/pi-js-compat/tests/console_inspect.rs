use crate::shared_oracle_0 as oracle;
use pi_js_compat::{
    js_json::{JsString, JsValue},
    node_fs::inspect_console_value,
};

#[test]
fn sdk_console_values_match_node_inspect_without_json_projection() {
    let mut source = oracle::TsOracle::spawn(
        "node-inspect-console-oracle.ts",
        "packages/coding-agent/examples/sdk/02-custom-model.ts",
    );
    let values = vec![
        JsValue::Null,
        JsValue::Bool(true),
        JsValue::Number(123.5),
        JsValue::String(JsString::from_utf16(vec![39, 34, 96, 10, 0xd800, 0xdc00, 0xdc00, 27])),
        pi_js_compat::js_json!([]),
        pi_js_compat::js_json!({}),
        pi_js_compat::js_json!(["browser-local", "search-local", "my-skill"]),
        pi_js_compat::js_json!(["a", "bb", "ccc", "dddd", "eeeee", "ffffff", "ggggggg", "h"]),
        pi_js_compat::js_json!([1, 20, 300, 4000, 5, 60, 700, 8000]),
        pi_js_compat::js_json!(["日本", "日本語", "😀", "λ", "é", "z", "中", "long name"]),
        pi_js_compat::js_json!([{"type":"warning","message":"short","path":"/fixture/SKILL.md","sourceInfo":{"source":"sdk","scope":"temporary","locator":{"path":"/deep","tags":[]}}}]),
        pi_js_compat::js_json!({"quoted key":"value","$tag":true,"nested":{"deeper":{"empty":{},"list":[1,2],"value":{"hidden":true}}}}),
        JsValue::Array(
            (0..105)
                .map(|n| JsValue::String(format!("faux/model-{n}").into()))
                .collect(),
        ),
        JsValue::String("x".repeat(10001).into()),
    ];
    let mut failures = Vec::new();
    for (index, value) in values.into_iter().enumerate() {
        let expected = source.request(&pi_js_compat::js_json!({"value":value}));
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let actual = JsValue::String(inspect_console_value(&value));
        if actual != expected["output"]["text"] {
            failures.push(format!(
                "case={index}: native={actual:?}; source={:?}",
                expected["output"]["text"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
