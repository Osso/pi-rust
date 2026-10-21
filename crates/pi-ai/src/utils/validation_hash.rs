//! Frozen TypeBox uniqueItems hashes through TextEncoder, including replacement of unmatched surrogates.
//! This lossy hash is source behavior, not the stored/returned value representation.
use pi_js_compat::js_json::{JsString, JsValue};

pub(super) fn hash(value: &JsValue) -> u64 {
    let mut accumulator = 14_695_981_039_346_656_037;
    hash_value(&mut accumulator, value);
    accumulator
}

fn hash_byte(accumulator: &mut u64, byte: u8) {
    *accumulator = (*accumulator ^ u64::from(byte)).wrapping_mul(1_099_511_628_211);
}

fn hash_string(accumulator: &mut u64, text: &JsString) {
    hash_byte(accumulator, 10);
    for byte in pi_js_compat::js_json::encode_node_utf8(text).bytes() {
        hash_byte(accumulator, byte);
    }
}

fn hash_value(accumulator: &mut u64, value: &JsValue) {
    match value {
        JsValue::Null => hash_byte(accumulator, 6),
        JsValue::Bool(flag) => {
            hash_byte(accumulator, 2);
            hash_byte(accumulator, u8::from(*flag));
        }
        JsValue::Number(number) => {
            hash_byte(accumulator, 7);
            for byte in number.to_le_bytes() {
                hash_byte(accumulator, byte);
            }
        }
        JsValue::String(text) => hash_string(accumulator, text),
        JsValue::Array(items) => {
            hash_byte(accumulator, 0);
            for item in items {
                hash_value(accumulator, item);
            }
        }
        JsValue::Object(object) => {
            hash_byte(accumulator, 8);
            let mut entries: Vec<_> = object.iter().filter(|(key, _)| *key != "constructor").collect();
            entries.sort_by(|(left, _), (right, _)| left.units().cmp(right.units()));
            for (key, value) in entries {
                hash_string(accumulator, key);
                hash_value(accumulator, value);
            }
        }
    }
}
