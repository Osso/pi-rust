use crate::shared_oracle as oracle;

use pi_coding_agent::core::retry_delay::calculate_retry_delay_ms;
use serde_json::{Value, json};

fn normalize(value: f64) -> Value {
    if value.is_nan() {
        json!("NaN")
    } else if value == 0.0 && value.is_sign_negative() {
        json!("-0")
    } else if value.is_infinite() {
        json!(if value.is_sign_positive() {
            "Infinity"
        } else {
            "-Infinity"
        })
    } else {
        json!(value)
    }
}

fn number_argument(value: f64) -> String {
    if value == 0.0 && value.is_sign_negative() {
        "-0".to_owned()
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            "Infinity"
        } else {
            "-Infinity"
        }
        .to_owned()
    } else {
        value.to_string()
    }
}

#[test]
fn matches_frozen_retry_delay_for_caps_jitter_rounding_and_number_edges() {
    let mut cases = Vec::new();
    for attempt in [1.0, 2.0, 3.0, 20.0, 1025.0] {
        for (base, cap) in [(0.0, 0.0), (1.0, 7.0), (5.0, 100.0), (30_000.0, 300_000.0)] {
            for random in [0.0, 0.25, 0.5, 0.75, 1.0] {
                cases.push([attempt, base, cap, random]);
            }
        }
    }
    cases.extend([
        [1.0, 5.0, 100.0, 0.25],
        [1.0, 5.0, 100.0, -2.25],
        [1.0, -0.0, 100.0, f64::NAN],
        [f64::NAN, 5.0, 100.0, 0.5],
        [1.0, 5.0, f64::NAN, 0.5],
        [1.0, 5.0, 100.0, f64::INFINITY],
        [1.0, 5.0, 100.0, f64::NEG_INFINITY],
        [1.0, 5.0, -0.0, 0.5],
        [1.0, 5.0, -0.0, -3.0],
        [1.0, 5.0, -0.0, -2.25],
    ]);
    let mut source = oracle::TsOracle::spawn("retry-delay-oracle.ts", "packages/coding-agent/src/core/retry-delay.ts");
    let response = source.request(&json!({
        "id": 1,
        "cases": cases.iter().map(|args| args.map(number_argument)).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], true, "{response}");
    let expected = response["output"].as_array().unwrap();
    assert_eq!(expected.len(), cases.len());
    for (args, expected) in cases.into_iter().zip(expected) {
        let [attempt, base, cap, random] = args;
        let actual = normalize(calculate_retry_delay_ms(attempt, base, cap, random));
        match (actual.as_f64(), expected.as_f64()) {
            (Some(actual), Some(expected)) => assert_eq!(actual, expected, "arguments: {args:?}"),
            _ => assert_eq!(&actual, expected, "arguments: {args:?}"),
        }
    }
}
