//! Exact JSON comparison used by differential tests.
//!
//! `serde_json::Value` equality ignores object key order, so differential tests
//! that only use `assert_eq!` cannot detect a Rust serializer that emits keys in
//! a different order than the TypeScript source. This helper compares object
//! keys positionally.

use serde_json::Value;

/// Compares JSON values exactly, including object key order.
pub fn assert_json_eq_ordered(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            assert_eq!(actual.as_f64(), expected.as_f64(), "number mismatch at {path}");
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "array length mismatch at {path}");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_json_eq_ordered(actual, expected, &format!("{path}[{index}]"));
            }
        }
        (Value::Object(actual), Value::Object(expected)) => {
            let actual_keys: Vec<&String> = actual.keys().collect();
            let expected_keys: Vec<&String> = expected.keys().collect();
            assert_eq!(
                actual_keys, expected_keys,
                "object keys or key order mismatch at {path}"
            );
            for (key, expected) in expected {
                assert_json_eq_ordered(&actual[key], expected, &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "value mismatch at {path}"),
    }
}
