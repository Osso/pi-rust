//! JavaScript number conversions: `JSON.stringify` serialization and
//! `Number(string)` parsing.
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

/// JavaScript `Number(string)`: `NaN` when the text is not a numeric literal.
pub fn parse_number(input: &str) -> f64 {
    let trimmed = input.trim_matches(is_js_whitespace);
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

/// ECMAScript `WhiteSpace` and `LineTerminator` code points stripped by `Number`.
fn is_js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{9}' | '\u{A}' | '\u{B}' | '\u{C}' | '\u{D}' | '\u{20}' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}
