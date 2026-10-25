//! Same-bit Node built-ins: literal text for formatting, binary64 for serde values.

use crate::shared_oracle_2 as oracle;

use pi_js_compat::js_json::{self, JsValue};
use pi_js_compat::number::{js_number_to_string, js_number_value};
use serde::Serialize;
use serde_json::json;

fn number_bits() -> Vec<u64> {
    let mut bits = vec![
        1_000_000_000_000_000_128.0_f64.to_bits(),
        // Exact halfway decimal: shortest-digit tie must choose the even digit.
        2.9802322387695312e-8_f64.to_bits(),
        0,
        1 << 63,
        1,
        (1 << 63) | 1,
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        f64::NAN.to_bits(),
        0xfff0_0000_0000_0001,
        f64::MAX.to_bits(),
        f64::MIN_POSITIVE.to_bits(),
    ];
    for value in [1e18_f64, 2_f64.powi(53), 2_f64.powi(64), 1e20, 1e21, 1e-6, 1e-7] {
        for offset in -8_i64..=8 {
            let adjacent = value.to_bits().checked_add_signed(offset).unwrap();
            bits.extend([adjacent, adjacent | (1 << 63)]);
        }
    }
    // Fixed xorshift64 state: no filtering, sorting, decimal conversion or rounding.
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..4096 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bits.push(state);
    }
    bits
}

#[test]
fn shared_formatter_and_json_consumers_match_node_same_bits() {
    let bits = number_bits();
    let hex: Vec<_> = bits.iter().map(|bits| format!("{bits:016x}")).collect();
    let mut oracle = oracle::TsOracle::spawn("js-compat-oracle.ts", "packages/coding-agent/src/index.ts");
    let response = oracle.request(&json!({"id": 20, "operation": "number_bits", "bits": hex}));
    assert_eq!(response["ok"], true, "{response}");
    let expected = response["output"].as_array().expect("Node output array");
    assert_eq!(expected.len(), bits.len());
    for (bits, expected) in bits.into_iter().zip(expected) {
        let value = f64::from_bits(bits);
        let string = expected["string"].as_str().unwrap();
        let json = expected["json"].as_str().unwrap();
        assert_eq!(js_number_to_string(value), string, "String bits={bits:016x}");
        let number = JsValue::Number(value);
        assert_eq!(js_json::stringify_lossless(&number), json, "JSON bits={bits:016x}");
        assert_eq!(
            js_json::stringify_lossless_pretty(&number),
            json,
            "pretty bits={bits:016x}"
        );
        assert_eq!(
            js_json::to_js_string(&number).unwrap().to_utf8().unwrap(),
            string,
            "coercion bits={bits:016x}"
        );
        assert_eq!(
            js_json::stringify(&js_number_value(value)),
            json,
            "serde writer bits={bits:016x}"
        );
        if value.is_finite() {
            let parsed = js_json::parse_lossless(json).unwrap();
            assert_eq!(
                js_json::stringify_lossless(&parsed),
                json,
                "parse/write bits={bits:016x}"
            );
        }
    }
}

#[derive(Serialize)]
struct NumericFields {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    scalar: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    optional: Option<f64>,
}

#[test]
fn generic_serde_preserves_finite_numeric_meaning_without_range_errors() {
    for bits in number_bits() {
        let value = f64::from_bits(bits);
        let fields = NumericFields {
            scalar: value,
            optional: Some(value),
        };
        let encoded = serde_json::to_value(&fields).unwrap_or_else(|error| panic!("bits={bits:016x}: {error}"));
        for field in ["scalar", "optional"] {
            if value.is_finite() {
                let expected = if value == 0.0 { 0.0 } else { value };
                assert_eq!(
                    encoded[field].as_f64().unwrap().to_bits(),
                    expected.to_bits(),
                    "{field} bits={bits:016x}"
                );
            } else {
                assert!(encoded[field].is_null(), "{field} bits={bits:016x}");
            }
        }
        let stored = js_number_value(value);
        if value.is_finite() {
            let expected = if value == 0.0 { 0.0 } else { value };
            assert_eq!(
                stored.as_f64().unwrap().to_bits(),
                expected.to_bits(),
                "stored bits={bits:016x}"
            );
        } else {
            assert!(stored.is_null());
        }
    }
    // Keep the exact integer storage value, not the shortest decimal spelling
    // (1000000000000000100), which only round-trips to the same binary64.
    assert_eq!(
        js_number_value(1_000_000_000_000_000_128.0).as_i64(),
        Some(1_000_000_000_000_000_128)
    );
    let absent = serde_json::to_value(NumericFields {
        scalar: 0.0,
        optional: None,
    })
    .unwrap();
    assert!(absent["optional"].is_null());
}
