//! JavaScript number conversions: `Number(string)`, `Number.parseInt`,
//! `Number.parseFloat`, and ECMA-262 `Number::toString`.

use serde::Serializer;

use crate::string::is_js_whitespace_char;

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
pub fn parse_int(input: &str, radix: u32) -> f64 {
    let trimmed = input.trim_start_matches(is_js_whitespace_char);
    let (negative, unsigned) = match trimmed.as_bytes().first() {
        Some(b'+') => (false, &trimmed[1..]),
        Some(b'-') => (true, &trimmed[1..]),
        _ => (false, trimmed),
    };
    let (digits, radix) = match radix {
        0 => match unsigned.strip_prefix("0x").or_else(|| unsigned.strip_prefix("0X")) {
            Some(hex) => (hex, 16),
            None => (unsigned, 10),
        },
        16 => match unsigned.strip_prefix("0x").or_else(|| unsigned.strip_prefix("0X")) {
            Some(hex) => (hex, 16),
            None => (unsigned, 16),
        },
        radix @ 2..=36 => (unsigned, radix),
        _ => return f64::NAN,
    };

    let mut value = 0.0;
    let mut seen_digit = false;
    for character in digits.chars() {
        let Some(digit) = character.to_digit(radix) else {
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

/// ECMA-262 `Number::toString` for the finite values representable in JSON.
pub fn js_number_to_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity"
        } else {
            "Infinity"
        }
        .to_owned();
    }

    let sign = if value.is_sign_negative() { "-" } else { "" };
    let (digits, point) = shortest_decimal_digits(value.abs());
    format_decimal_digits(&digits, point, sign)
}

/// Returns the shortest round-trip digits and the decimal point position.
fn shortest_decimal_digits(value: f64) -> (String, i32) {
    let rendered = value.to_string();
    let (integer, fraction) = rendered.split_once('.').unwrap_or((&rendered, ""));
    let raw_digits = format!("{integer}{fraction}");
    let leading_zeros = raw_digits.len() - raw_digits.trim_start_matches('0').len();
    let digits = raw_digits.trim_start_matches('0').trim_end_matches('0');
    let point = integer.len() as i32 - leading_zeros as i32;
    (digits.to_owned(), point)
}

fn format_decimal_digits(digits: &str, point: i32, sign: &str) -> String {
    let digit_count = digits.len() as i32;
    if digit_count <= point && point <= 21 {
        return format!("{sign}{digits}{}", "0".repeat((point - digit_count) as usize));
    }
    if 0 < point && point <= 21 {
        let (head, tail) = digits.split_at(point as usize);
        return format!("{sign}{head}.{tail}");
    }
    if -6 < point && point <= 0 {
        return format!("{sign}0.{}{digits}", "0".repeat((-point) as usize));
    }

    let (head, tail) = digits.split_at(1);
    let exponent = point - 1;
    let exponent_sign = if exponent >= 0 { "+" } else { "-" };
    if tail.is_empty() {
        format!("{sign}{head}e{exponent_sign}{}", exponent.abs())
    } else {
        format!("{sign}{head}.{tail}e{exponent_sign}{}", exponent.abs())
    }
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
    let sign = if value.is_sign_negative() { "-" } else { "" };
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

#[cfg(test)]
mod to_fixed_tests {
    use super::to_fixed;

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
}
