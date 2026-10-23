//! Default en-US `Number.prototype.toLocaleString` (maximum three fractional digits).

use crate::{js_json::JsString, number::js_number_to_string};

/// Format with the approved locale default: grouping, half-expand rounding,
/// zero to three fractional digits, and Intl's signs/non-finite spellings.
pub fn to_locale_string(value: f64) -> JsString {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() { "-∞" } else { "∞" }.into();
    }
    let (integer, fraction) = expand_decimal(&js_number_to_string(value.abs()));
    let (integer, mut fraction) = round_fraction(integer, fraction);
    while fraction.ends_with('0') {
        fraction.pop();
    }
    let mut result = String::new();
    if value.is_sign_negative() {
        result.push('-');
    }
    for (index, digit) in integer.bytes().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            result.push(',');
        }
        result.push(char::from(digit));
    }
    if !fraction.is_empty() {
        result.push('.');
        result.push_str(&fraction);
    }
    result.into()
}

/// Intl rounds the shortest decimal quantity, not binary `toFixed`'s exact value.
fn expand_decimal(decimal: &str) -> (String, String) {
    let (mantissa, exponent) = match decimal.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().expect("number exponent")),
        None => (decimal, 0),
    };
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let digits = mantissa.replace('.', "");
    if point <= 0 {
        return ("0".into(), "0".repeat((-point) as usize) + &digits);
    }
    let point = point as usize;
    if point >= digits.len() {
        let zeros = point - digits.len();
        return (digits + &"0".repeat(zeros), String::new());
    }
    (digits[..point].into(), digits[point..].into())
}

fn round_fraction(integer: String, mut fraction: String) -> (String, String) {
    if fraction.len() <= 3 {
        return (integer, fraction);
    }
    let round_up = fraction.as_bytes()[3] >= b'5';
    fraction.truncate(3);
    if !round_up {
        return (integer, fraction);
    }
    let mut digits = integer.into_bytes();
    digits.extend_from_slice(fraction.as_bytes());
    increment_decimal_digits(&mut digits);
    let fraction = digits.split_off(digits.len() - 3);
    (
        String::from_utf8(digits).expect("decimal ASCII digits"),
        String::from_utf8(fraction).expect("fractional ASCII digits"),
    )
}

fn increment_decimal_digits(digits: &mut Vec<u8>) {
    for digit in digits.iter_mut().rev() {
        if *digit < b'9' {
            *digit += 1;
            return;
        }
        *digit = b'0';
    }
    digits.insert(0, b'1');
}
