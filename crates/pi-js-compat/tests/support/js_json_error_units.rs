#[path = "oracle.rs"]
mod oracle;

use super::{JsString, parse_lossless};
use oracle::TsOracle;
use serde_json::json;

fn error_sources() -> Vec<(&'static str, JsString)> {
    let raw = |units: &[u16]| JsString::from_utf16(units.to_vec());
    vec![
        ("raw high token", raw(&[0xd800])),
        ("raw low token", raw(&[0xdc00])),
        ("emoji split token", JsString::from("😀")),
        ("short raw excerpt", raw(&[0x5b, 0xd800, 0xdc00, 0x2c, 0xdc01, 0x5d])),
        ("head cutoff splits pair", JsString::from("x12345678😀abcdefghijklmnop")),
        (
            "middle cutoff splits pair",
            JsString::from("[          x12345678😀abcdefghijklmnop]"),
        ),
        ("tail cutoff splits pair", JsString::from("[\"aaaaaaaa😀bbbbbbb\",x]")),
        (
            "middle start splits pair",
            JsString::from("[\"aaaaaaaa😀bbbbbbb\",x123456789abcdefghijklmnop]"),
        ),
        (
            "multiline raw token",
            JsString::from("[\r\n  ") + raw(&[0xdfff]) + "\n]",
        ),
        ("multiline position", JsString::from("{\r\n\"😀\": 1,\r\n\"x\" 2}")),
        ("end of input", JsString::from("[")),
        ("special source", JsString::from("undefined")),
    ]
}

#[test]
fn json_error_messages_match_node_utf16_units() {
    let sources = error_sources();
    let inputs: Vec<_> = sources.iter().map(|(_, source)| source.units()).collect();
    let mut oracle = TsOracle::spawn("js-json-error-units-oracle.ts", "packages/coding-agent/package.json");
    let response = oracle.request(&json!({"id": 1, "sources": inputs}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected = response["output"].as_array().unwrap();
    assert_eq!(expected.len(), sources.len());
    let mismatches: Vec<_> = sources
        .iter()
        .zip(expected)
        .filter_map(|((name, source), expected)| {
            let error = parse_lossless(source).unwrap_err();
            let actual = json!({
                "name": "SyntaxError",
                "message": error.message().units(),
                "display": error.to_string(),
            });
            assert_eq!(
                error.clone().into_message(),
                *error.message(),
                "{name}: owned message differs"
            );
            (actual != *expected).then(|| format!("{name}: rust {actual} vs Node {expected}"))
        })
        .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
