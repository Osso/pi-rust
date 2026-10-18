//! JavaScript number conversions: `JSON.stringify` serialization,
//! `Number(string)` / `parseFloat(string)` parsing, and `Date.parse`.
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

/// JavaScript `Number.parseFloat(string)`: leading numeric prefix, `NaN` when
/// the text does not start with a numeric literal.
pub fn parse_float(input: &str) -> f64 {
    let trimmed = input.trim_start_matches(is_js_whitespace);
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

/// JavaScript `Date.parse(string)`: milliseconds since the Unix epoch, or
/// `NaN` when the text is not a recognized date.
///
/// Supports the three HTTP-date formats and the ECMAScript ISO 8601 format.
/// V8's additional legacy formats (`"Jan 1 2020"`, `"1/1/2020"`) are not
/// reproduced. A timestamp without a zone is interpreted as UTC; V8 uses the
/// process-local zone.
pub fn date_parse(input: &str) -> f64 {
    if let Ok(time) = httpdate::parse_http_date(input) {
        return time
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(f64::NAN, |duration| duration.as_millis() as f64);
    }
    parse_iso8601_millis(input).map_or(f64::NAN, |millis| millis as f64)
}

/// Largest absolute timestamp JavaScript dates represent (100,000,000 days).
const MAX_DATE_MILLIS: i64 = 8_640_000_000_000_000;

fn parse_iso8601_millis(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let year = parse_year(bytes, &mut index)?;
    let mut month = 1;
    let mut day = 1;
    if bytes.get(index) == Some(&b'-') {
        index += 1;
        month = parse_short_digits(bytes, &mut index)?;
        if bytes.get(index) == Some(&b'-') {
            index += 1;
            day = parse_short_digits(bytes, &mut index)?;
        }
    }

    let (hour, minute, second, millis, offset_seconds) = if index < bytes.len() {
        parse_iso8601_time(bytes, &mut index)?
    } else {
        (0, 0, 0, 0, 0)
    };
    if index != bytes.len() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let seconds = i128::from(days) * 86_400 + i128::from(hour * 3_600 + minute * 60 + second - offset_seconds);
    let millis = seconds * 1_000 + i128::from(millis);
    (millis.abs() <= i128::from(MAX_DATE_MILLIS)).then_some(millis as i64)
}

fn parse_year(bytes: &[u8], index: &mut usize) -> Option<i64> {
    let sign = match bytes.get(*index) {
        Some(b'+') => 1,
        Some(b'-') => -1,
        _ => 0,
    };
    let digits = if sign == 0 { 4 } else { 6 };
    let start = *index + usize::from(sign != 0);
    let year = parse_fixed_digits(bytes.get(start..start + digits)?)?;
    *index = start + digits;
    Some(if sign == 0 { year } else { sign * year })
}

fn parse_short_digits(bytes: &[u8], index: &mut usize) -> Option<i64> {
    let first = i64::from(bytes.get(*index)?.checked_sub(b'0').filter(|digit| *digit < 10)?);
    *index += 1;
    let Some(second) = bytes
        .get(*index)
        .and_then(|byte| byte.checked_sub(b'0'))
        .filter(|d| *d < 10)
    else {
        return Some(first);
    };
    *index += 1;
    Some(first * 10 + i64::from(second))
}

fn parse_two_digits(bytes: &[u8], index: &mut usize) -> Option<i64> {
    let value = parse_fixed_digits(bytes.get(*index..*index + 2)?)?;
    *index += 2;
    Some(value)
}

fn parse_fixed_digits(bytes: &[u8]) -> Option<i64> {
    bytes.iter().try_fold(0_i64, |value, byte| {
        let digit = byte.checked_sub(b'0').filter(|digit| *digit < 10)?;
        Some(value * 10 + i64::from(digit))
    })
}

fn parse_iso8601_time(bytes: &[u8], index: &mut usize) -> Option<(i64, i64, i64, i64, i64)> {
    if !matches!(bytes.get(*index), Some(b'T' | b't' | b' ')) {
        return None;
    }
    *index += 1;
    let hour = parse_two_digits(bytes, index)?;
    expect_byte(bytes, index, b':')?;
    let minute = parse_two_digits(bytes, index)?;
    let mut second = 0;
    let mut millis = 0;
    if bytes.get(*index) == Some(&b':') {
        *index += 1;
        second = parse_two_digits(bytes, index)?;
    }
    if bytes.get(*index) == Some(&b'.') {
        *index += 1;
        millis = parse_fraction_millis(bytes, index)?;
    }
    let offset_seconds = parse_iso8601_zone(bytes, index)?;
    if hour > 24 || minute > 59 || second > 59 || (hour == 24 && (minute != 0 || second != 0 || millis != 0)) {
        return None;
    }
    Some((hour, minute, second, millis, offset_seconds))
}

fn parse_fraction_millis(bytes: &[u8], index: &mut usize) -> Option<i64> {
    let start = *index;
    while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
        *index += 1;
    }
    let digits = bytes.get(start..*index)?;
    if digits.is_empty() {
        return None;
    }
    let kept = &digits[..digits.len().min(3)];
    let millis = parse_fixed_digits(kept)?;
    Some(millis * 10_i64.pow(3 - kept.len() as u32))
}

fn parse_iso8601_zone(bytes: &[u8], index: &mut usize) -> Option<i64> {
    match bytes.get(*index) {
        None => Some(0),
        Some(b'Z' | b'z') => {
            *index += 1;
            Some(0)
        }
        Some(b'+' | b'-') => {
            let sign = if bytes[*index] == b'-' { -1 } else { 1 };
            *index += 1;
            let hours = parse_two_digits(bytes, index)?;
            if bytes.get(*index) == Some(&b':') {
                *index += 1;
            }
            let minutes = parse_two_digits(bytes, index)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            Some(sign * (hours * 3_600 + minutes * 60))
        }
        Some(_) => None,
    }
}

fn expect_byte(bytes: &[u8], index: &mut usize, expected: u8) -> Option<()> {
    (bytes.get(*index) == Some(&expected)).then(|| *index += 1)
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
/// `day` may exceed the month length; the result rolls into later months.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
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
