// Reuse the full existing scalar/error mutation corpus without copying fixtures.
#[path = "../../pi-coding-agent/tests/differential_js_json.rs"]
mod legacy;

use crate::shared_oracle_2 as oracle;

use oracle::TsOracle;
use pi_js_compat::js_json::{
    JsValue, object_entries_lossless, parse_lossless, stringify_lossless, stringify_lossless_pretty,
};
use serde_json::{Value, json};

const INPUTS: &[&str] = &[
    r#""\ud800""#,
    r#""\udfff""#,
    "\"�\"",
    r#"["\ud800",["\udfff","�"],{"\ud800":"\udfff"}]"#,
    r#"{"\ud800":1,"\udfff":2,"�":3}"#,
    r#"{"\ud800":1,"x":2,"\ud800":3}"#,
    r#"{"\ud800":1,"�":2,"\ud800":3,"\udfff":4}"#,
    r#"{"😀":1,"a":2,"\ud83d\ude00":3}"#,
    r#"["\ud83d\ude00","😀","\ud800\ud800\udfff\udfff"]"#,
    r#"{"10":1,"2":2,"0":3,"01":4,"4294967294":5,"4294967295":6,"-1":7,"00":8,"1.0":9,"\ud800":10,"3":11}"#,
    r#""\"\\\/\b\f\n\r\t\u0000\u001f\u007f é中""#,
    r#"{"outer":[{"\ud800":["\udfff",true,null,{},[]]}]}"#,
    "null",
    "true",
    "false",
    "[]",
    "{}",
    " 123.456e-7 ",
    "[1e400,-1e-400,9007199254740993,-0.0,5e-324]",
    "",
    "[",
    "{",
    "{\"a\"",
    "{\"a\":1",
    "{\"a\":1,}",
    "[1 2]",
    "[1,]",
    "true x",
    "-",
    "1.e5",
    "1e+",
    "[-01]",
    "\"abc",
    "\"a\\",
    "\"a\\x\"",
    "\"a\nb\"",
    "\"\\u12\"",
    "NaN",
    "undefined",
    "Infinity",
    "[object Object]",
    "[nulL]",
    "{\r\"a\":1\r\r,x}",
    "[1,\r\n2 3]",
    "{\"😀\": [1 x]}",
];

fn rust_result(text: &str) -> Value {
    match parse_lossless(text) {
        Ok(value) => {
            let keys = match &value {
                JsValue::Object(object) => Some(
                    object_entries_lossless(object)
                        .into_iter()
                        .map(|(key, _)| key.units().to_vec())
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            };
            json!({"compact": stringify_lossless(&value), "pretty": stringify_lossless_pretty(&value), "keys": keys})
        }
        Err(error) => json!({"error": error.to_string()}),
    }
}

#[test]
fn lossless_json_matches_node_bytes_and_key_identity() {
    let mut oracle = TsOracle::spawn("js-json-lossless-oracle.ts", "packages/coding-agent/package.json");
    let response = oracle.request(&json!({"id": 1, "texts": INPUTS}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected = response["output"].as_array().unwrap();
    assert_eq!(expected.len(), INPUTS.len());
    let mismatches: Vec<_> = INPUTS
        .iter()
        .zip(expected)
        .filter_map(|(text, expected)| {
            let actual = rust_result(text);
            (actual != *expected).then(|| format!("{text:?}: rust {actual} vs node {expected}"))
        })
        .collect();
    assert!(
        mismatches.is_empty(),
        "{} differences:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}
