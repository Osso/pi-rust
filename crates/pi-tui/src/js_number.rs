//! JavaScript `Number(string)` parsing.
//!
//! `pi-tui` does not depend on `pi-ai`, so this mirrors
//! `pi_ai::js_number::parse_number` (the single tested source of truth) for the
//! two environment/terminal-sequence sites that the TypeScript source parses
//! with `Number(...)`.

/// JavaScript `Number(string)`: `NaN` when the text is not a numeric literal.
pub(crate) fn parse_number(input: &str) -> f64 {
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
