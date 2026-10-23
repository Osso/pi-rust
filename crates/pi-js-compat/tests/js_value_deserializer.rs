//! Typed decode must not introduce a JSON.stringify projection before visiting values.
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Numbers {
    value: f64,
    optional: Option<f64>,
}

#[test]
fn native_numbers_reach_float_and_optional_fields_without_null_projection() {
    for number in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, -0.0, 0.25] {
        let value = JsValue::Object(JsObject::from([
            ("value".into(), JsValue::Number(number)),
            ("optional".into(), JsValue::Number(number)),
        ]));
        let decoded: Numbers = js_json::from_value(&value).unwrap();
        for observed in [decoded.value, decoded.optional.expect("number must remain present")] {
            if number.is_nan() {
                assert!(observed.is_nan());
            } else {
                assert_eq!(observed, number);
                assert_eq!(observed.is_sign_negative(), number.is_sign_negative());
            }
        }
    }
}

#[test]
fn integer_targets_reject_fractional_nonfinite_and_out_of_range_values() {
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.25, -0.25] {
        let value = JsValue::Number(value);
        assert!(js_json::from_value::<i8>(&value).is_err());
        assert!(js_json::from_value::<u64>(&value).is_err());
        assert!(js_json::from_value::<i128>(&value).is_err());
        assert!(js_json::from_value::<u128>(&value).is_err());
    }
    assert_eq!(js_json::from_value::<i8>(&JsValue::Number(-128.0)).unwrap(), -128);
    assert!(js_json::from_value::<i8>(&JsValue::Number(128.0)).is_err());
    assert!(js_json::from_value::<i64>(&JsValue::Number(2_f64.powi(63))).is_err());
    assert!(js_json::from_value::<u64>(&JsValue::Number(2_f64.powi(64))).is_err());
    assert_eq!(js_json::from_value::<u64>(&JsValue::Number(-0.0)).unwrap(), 0);
}

#[test]
fn native_keys_strings_and_values_decode_without_repair_or_number_projection() {
    let key = JsString::from_utf16(vec![0xd800]);
    let text = JsString::from_utf16(vec![0xdc00]);
    let value = JsValue::Object(JsObject::from([(
        key.clone(),
        JsValue::Array(vec![JsValue::String(text.clone()), JsValue::Number(f64::INFINITY)]),
    )]));
    let decoded: JsValue = js_json::from_value(&value).unwrap();
    let object = decoded.as_object().unwrap();
    assert!(object.contains_key(&key));
    let array = object.get(&key).unwrap().as_array().unwrap();
    assert_eq!(array[0].as_string().unwrap(), &text);
    assert_eq!(array[1].as_f64().unwrap(), f64::INFINITY);
    let decoded: std::collections::BTreeMap<JsString, JsValue> = js_json::from_value(&value).unwrap();
    assert_eq!(
        decoded.get(&key).unwrap().as_array().unwrap()[0].as_string().unwrap(),
        &text
    );
    assert!(js_json::from_value::<String>(&JsValue::String(text)).is_err());
}

#[derive(Debug, Deserialize, PartialEq)]
enum Record {
    Unit,
    Pair(u32, String),
    Named { label: JsString },
}

#[test]
fn typed_sequences_enums_and_json_map_keys_keep_existing_acceptance() {
    let values = JsValue::Array(vec![JsValue::Number(2.0), JsValue::String("x".into())]);
    assert_eq!(js_json::from_value::<(u32, String)>(&values).unwrap(), (2, "x".into()));
    for (value, expected) in [
        (JsValue::String("Unit".into()), Record::Unit),
        (
            JsValue::Object(JsObject::from([("Pair".into(), values)])),
            Record::Pair(2, "x".into()),
        ),
        (
            JsValue::Object(JsObject::from([(
                "Named".into(),
                JsValue::Object(JsObject::from([(
                    "label".into(),
                    JsValue::String(JsString::from_utf16(vec![0xd800])),
                )])),
            )])),
            Record::Named {
                label: JsString::from_utf16(vec![0xd800]),
            },
        ),
    ] {
        assert_eq!(js_json::from_value::<Record>(&value).unwrap(), expected);
    }
    let value = JsValue::Object(JsObject::from([
        ("9007199254740993".into(), JsValue::Number(2.0)),
        ("-2".into(), JsValue::Number(3.0)),
    ]));
    let numeric: std::collections::BTreeMap<i64, u32> = js_json::from_value(&value).unwrap();
    assert_eq!(
        numeric,
        std::collections::BTreeMap::from([(9007199254740993, 2), (-2, 3)])
    );
    let value = JsValue::Object(JsObject::from([("true".into(), JsValue::Number(2.0))]));
    let boolean: std::collections::BTreeMap<bool, u32> = js_json::from_value(&value).unwrap();
    assert_eq!(boolean, std::collections::BTreeMap::from([(true, 2)]));
}

#[test]
fn fixed_tuple_decoders_reject_unconsumed_elements() {
    let value = JsValue::Array(vec![JsValue::Number(1.0), JsValue::Number(2.0), JsValue::Number(3.0)]);
    assert!(js_json::from_value::<(u32, u32)>(&value).is_err());
    let value = JsValue::Object(JsObject::from([(
        "Pair".into(),
        JsValue::Array(vec![
            JsValue::Number(1.0),
            JsValue::String("x".into()),
            JsValue::Bool(true),
        ]),
    )]));
    assert!(js_json::from_value::<Record>(&value).is_err());
}
