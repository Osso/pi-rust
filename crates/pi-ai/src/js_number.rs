//! Serializes `f64` the way `JSON.stringify` writes a JavaScript number.
//!
//! serde_json writes `0.0` for an integral `f64`; JavaScript writes `0`. Wire
//! records keep TypeScript `number` fields as `f64` and use these helpers so
//! session files and payloads match the TypeScript output.

use serde::Serializer;

const JS_INTEGER_NOTATION_LIMIT: f64 = 1e21;

/// `serialize_with` helper for `f64` fields.
pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    let value = *value;
    if value.is_finite() && value.fract() == 0.0 && value.abs() < JS_INTEGER_NOTATION_LIMIT {
        // Below 1e21 every integral f64 fits in i128 and JavaScript prints it without exponent.
        return serializer.serialize_i128(value as i128);
    }
    serializer.serialize_f64(value)
}

/// `serialize_with` helper for `Option<f64>` fields (combine with `skip_serializing_if`).
pub fn serialize_option<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serialize(value, serializer),
        None => serializer.serialize_none(),
    }
}
