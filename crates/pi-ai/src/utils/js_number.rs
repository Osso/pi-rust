/// ECMA-262 `Number::toString` for the finite values representable in JSON.
pub(crate) fn js_number_to_string(value: f64) -> String {
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
