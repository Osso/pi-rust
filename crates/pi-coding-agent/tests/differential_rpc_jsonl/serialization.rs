use pi_coding_agent::modes::rpc::serialize_json_line;
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 9000;

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    compare_shared_outputs(oracle);
    compare_approved_discrepancies(oracle);
}

fn compare_shared_outputs(oracle: &mut TsOracle) {
    let fixtures = [
        ("null", Value::Null),
        ("boolean", json!(true)),
        ("integer", json!(9)),
        ("array", json!([1, "x", null])),
        ("empty object", json!({})),
        (
            "escaped and Unicode separators",
            json!({"text": "a\nb\u{2028}\u{2029}"}),
        ),
    ];

    for (offset, (name, value)) in fixtures.into_iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&serialize_request(id, value.clone()));
        let serialized = serialize_json_line(&value).expect("shared JSON value must serialize");
        let actual = serialize_response(id, &serialized);

        assert_eq!(actual, expected, "shared serialization mismatch: {name}");
    }
}

fn compare_approved_discrepancies(oracle: &mut TsOracle) {
    let fixtures = [
        ("integral float", json!(1.0), "1\n", "1.0\n"),
        ("negative zero", json!(-0.0), "0\n", "-0.0\n"),
        ("exponent threshold", json!(1e20), "100000000000000000000\n", "1e+20\n"),
        (
            "integer beyond JavaScript safe range",
            json!(9_007_199_254_740_993u64),
            "9007199254740992\n",
            "9007199254740993\n",
        ),
    ];

    for (offset, (name, value, source_output, rust_output)) in fixtures.into_iter().enumerate() {
        let id = FIRST_REQUEST_ID + 6 + u64::try_from(offset).expect("fixture offset must fit u64");
        let source_response = oracle.request(&serialize_request(id, value.clone()));
        let rust_serialized = serialize_json_line(&value).expect("discrepancy value must serialize");

        assert_eq!(
            source_response,
            serialize_response(id, source_output),
            "source discrepancy changed: {name}"
        );
        assert_eq!(rust_serialized, rust_output, "Rust discrepancy changed: {name}");
        assert_ne!(
            rust_serialized, source_output,
            "approved discrepancy unexpectedly disappeared: {name}"
        );
    }
}

fn serialize_request(id: u64, value: Value) -> Value {
    json!({"id": id, "operation": "serialize_json_line", "value": value})
}

fn serialize_response(id: u64, serialized: &str) -> Value {
    json!({"id": id, "ok": true, "output": {"serialized": serialized}})
}
