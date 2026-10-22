//! JavaScript number conversions: `Number(string)`, `Number.parseInt`,
//! `Number.parseFloat`, and ECMA-262 `Number::toString`.

use serde::Serializer;
use serde_json::Value;

use crate::string::{is_js_whitespace_char, is_js_whitespace_unit};

/// Exact integral binary64 values representable in serde_json's i64/u64 storage.
/// This is a numeric conversion, not JavaScript shortest-decimal formatting.
fn integer_digits(value: f64) -> Option<i128> {
    if value.fract() == 0.0 && (-9_223_372_036_854_775_808.0..18_446_744_073_709_551_616.0).contains(&value) {
        return Some(value as i128);
    }
    None
}

/// `serialize_with` helper for `f64` fields, preserving numeric meaning.
/// Generic serde JSON output is not byte-exact `JSON.stringify`; use the shared
/// `js_json` writer for JavaScript decimal spelling.
pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    match integer_digits(*value) {
        Some(digits) => serializer.serialize_i128(digits),
        None => serializer.serialize_f64(*value),
    }
}

/// `serialize_with` helper for `Option<f64>` fields (combine with `skip_serializing_if`).
pub fn serialize_option<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serialize(value, serializer),
        None => serializer.serialize_none(),
    }
}

/// JavaScript `Number(string)`: `NaN` when the text is not a numeric literal.
pub fn parse_number(input: &str) -> f64 {
    let trimmed = input.trim_matches(is_js_whitespace_char);
    if trimmed.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0b", 2), ("0B", 2), ("0o", 8), ("0O", 8)] {
        if let Some(digits) = trimmed.strip_prefix(prefix) {
            return parse_radix(digits, radix);
        }
    }
    match trimmed {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    // Rust also accepts `inf`/`nan` spellings, which JavaScript rejects.
    if unsigned
        .chars()
        .any(|character| !(character.is_ascii_digit() || matches!(character, '.' | 'e' | 'E' | '+' | '-')))
    {
        return f64::NAN;
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

/// JavaScript `Number.parseInt(string, radix)`: leading whitespace, optional
/// sign, then the longest digit prefix; `NaN` when no digit is present.
pub fn parse_int(input: &[u16], radix: u32) -> f64 {
    let first = input
        .iter()
        .position(|unit| !is_js_whitespace_unit(*unit))
        .unwrap_or(input.len());
    let trimmed = &input[first..];
    let (negative, unsigned) = match trimmed.first() {
        Some(unit) if *unit == u16::from(b'+') => (false, &trimmed[1..]),
        Some(unit) if *unit == u16::from(b'-') => (true, &trimmed[1..]),
        _ => (false, trimmed),
    };
    let has_hex_prefix = unsigned.first() == Some(&u16::from(b'0'))
        && unsigned
            .get(1)
            .is_some_and(|unit| *unit == u16::from(b'x') || *unit == u16::from(b'X'));
    let radix = if radix == 0 {
        if has_hex_prefix { 16 } else { 10 }
    } else {
        radix
    };
    if !(2..=36).contains(&radix) {
        return f64::NAN;
    }
    let digits = if radix == 16 && has_hex_prefix {
        &unsigned[2..]
    } else {
        unsigned
    };
    let mut value = 0.0;
    let mut seen_digit = false;
    for unit in digits {
        let digit = char::from_u32(u32::from(*unit)).and_then(|character| character.to_digit(radix));
        let Some(digit) = digit else {
            break;
        };
        value = value * f64::from(radix) + f64::from(digit);
        seen_digit = true;
    }
    if !seen_digit {
        return f64::NAN;
    }
    if negative { -value } else { value }
}

/// JavaScript `Number.parseFloat(string)`: leading numeric prefix, `NaN` when
/// the text does not start with a numeric literal.
pub fn parse_float(input: &str) -> f64 {
    let trimmed = input.trim_start_matches(is_js_whitespace_char);
    for (prefix, value) in [
        ("Infinity", f64::INFINITY),
        ("+Infinity", f64::INFINITY),
        ("-Infinity", f64::NEG_INFINITY),
    ] {
        if trimmed.starts_with(prefix) {
            return value;
        }
    }

    let bytes = trimmed.as_bytes();
    let mut index = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        index += 1;
    }
    let mut seen_digit = consume_digits(bytes, &mut index);
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        seen_digit |= consume_digits(bytes, &mut index);
    }
    if seen_digit && matches!(bytes.get(index), Some(b'e' | b'E')) {
        let exponent_start = index;
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        if !consume_digits(bytes, &mut index) {
            index = exponent_start;
        }
    }

    if !seen_digit {
        return f64::NAN;
    }
    // The scanned prefix only contains ASCII digits, `.`, `e`, and a sign, so
    // the standard parser accepts it; overflow yields infinity as in JavaScript.
    trimmed[..index].parse::<f64>().unwrap_or(f64::NAN)
}

fn consume_digits(bytes: &[u8], index: &mut usize) -> bool {
    let start = *index;
    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    *index > start
}

fn parse_radix(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    digits
        .chars()
        .try_fold(0.0, |value, character| {
            character
                .to_digit(radix)
                .map(|digit| value * f64::from(radix) + f64::from(digit))
        })
        .unwrap_or(f64::NAN)
}

/// ECMA-262 `Number::toString`, including negative zero and non-finite values.
pub fn js_number_to_string(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}

/// ECMA-262 `Number.prototype.toFixed(fraction_digits)` for finite values below
/// `1e21`, computed from the exact binary value of the double.
///
/// The spec picks the closest multiple of `10^-fraction_digits` to the exact
/// value and the larger one on an exact tie (away from zero after sign
/// extraction). Rust's `{:.n}` formatting rounds half to even, which differs
/// for values such as `1.25` or `9/8`.
pub fn to_fixed(value: f64, fraction_digits: u32) -> String {
    if !value.is_finite() || value.abs() >= 1e21 {
        return js_number_to_string(value);
    }
    // V8 keeps the sign only for values below zero; `(-0).toFixed(1)` is `"0.0"`.
    let sign = if value < 0.0 { "-" } else { "" };
    let magnitude = value.abs();
    let scale = 10_u128
        .checked_pow(fraction_digits)
        .expect("toFixed fraction digits must fit the scale");
    let rounded = round_scaled(magnitude, scale);
    let whole = rounded / scale;
    if fraction_digits == 0 {
        return format!("{sign}{whole}");
    }
    let fraction = rounded % scale;
    format!("{sign}{whole}.{fraction:0width$}", width = fraction_digits as usize)
}

/// `floor(x * scale + 1/2)` on the exact value of `x`.
fn round_scaled(magnitude: f64, scale: u128) -> u128 {
    let bits = magnitude.to_bits();
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction_bits = bits & ((1_u64 << 52) - 1);
    let (mantissa, exponent) = if exponent_bits == 0 {
        (u128::from(fraction_bits), -1074)
    } else {
        (u128::from(fraction_bits | (1_u64 << 52)), exponent_bits - 1075)
    };

    let scaled = mantissa * scale;
    if exponent >= 0 {
        return scaled << exponent;
    }

    let shift = (-exponent) as u32;
    if shift >= 64 {
        return 0;
    }
    (2 * scaled + (1_u128 << shift)) >> (shift + 1)
}

/// JavaScript binary64 numeric meaning in serde storage; non-finite becomes null.
/// Exact integral values use i64/u64 storage when representable, otherwise f64.
/// No shortest-decimal integer substitution: other encodings retain the original
/// binary value. Use `js_json::stringify` for byte-exact JavaScript JSON text.
pub fn js_number_value(value: f64) -> Value {
    if !value.is_finite() {
        return Value::Null;
    }
    if let Some(number) = integer_digits(value).and_then(serde_json::Number::from_i128) {
        return Value::Number(number);
    }
    Value::Number(serde_json::Number::from_f64(value).expect("finite number"))
}

#[cfg(test)]
mod to_fixed_tests {
    use super::{js_number_value, to_fixed};

    #[test]
    fn matches_ecmascript_rounding_of_exact_double_values() {
        // Values where naive `(value * 10).round()` or Rust `{:.1}` differ from the spec.
        assert_eq!(to_fixed(1.15, 1), "1.1");
        assert_eq!(to_fixed(1.25, 1), "1.3");
        assert_eq!(to_fixed(1.35, 1), "1.4");
        assert_eq!(to_fixed(999.95, 1), "1000.0");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(1.5, 0), "2");
        assert_eq!(to_fixed(-1.25, 1), "-1.3");
        assert_eq!(to_fixed(0.0, 1), "0.0");
        assert_eq!(to_fixed(1.0, 2), "1.00");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(1.045, 2), "1.04");
        assert_eq!(to_fixed(0.0001, 2), "0.00");
        assert_eq!(to_fixed(1e21, 1), "1e+21");
        assert_eq!(to_fixed(f64::NAN, 1), "NaN");
    }

    #[test]
    fn js_number_value_matches_json_stringify_numbers() {
        assert_eq!(js_number_value(3000.0), serde_json::json!(3000));
        assert_eq!(js_number_value(-0.0), serde_json::json!(0));
        assert_eq!(js_number_value(42.5), serde_json::json!(42.5));
        assert_eq!(js_number_value(f64::NAN), serde_json::Value::Null);
        assert_eq!(js_number_value(f64::INFINITY), serde_json::Value::Null);
        // Integral values above `MAX_SAFE_INTEGER` keep integer notation.
        assert_eq!(
            js_number_value(9_007_199_254_740_994.0),
            serde_json::json!(9_007_199_254_740_994_u64)
        );
        assert_eq!(
            js_number_value(-9_007_199_254_740_994.0),
            serde_json::json!(-9_007_199_254_740_994_i64)
        );
        // Integral values above `u64::MAX` cannot be stored as `serde_json`
        // integers, so the value round-trips as a float.
        assert_eq!(serde_json::to_string(&js_number_value(1e20)).unwrap(), "1e+20");
        // `1e21` switches to exponent notation in JavaScript.
        assert_eq!(serde_json::to_string(&js_number_value(1e21)).unwrap(), "1e+21");
    }
}
