//! ECMAScript string semantics shared by the ported leaf modules.

/// ECMAScript `\s` / `trim()` whitespace code units.
pub fn is_js_whitespace_unit(unit: u16) -> bool {
    matches!(
        unit,
        0x0009..=0x000d
            | 0x0020
            | 0x00a0
            | 0x1680
            | 0x2000..=0x200a
            | 0x2028
            | 0x2029
            | 0x202f
            | 0x205f
            | 0x3000
            | 0xfeff
    )
}

pub fn is_js_whitespace_char(character: char) -> bool {
    character.len_utf16() == 1 && is_js_whitespace_unit(character as u16)
}

/// ECMAScript `String.prototype.trim`.
pub fn js_trim(value: &str) -> &str {
    value.trim_matches(is_js_whitespace_char)
}

/// ECMAScript `String.prototype.trimStart`.
pub fn js_trim_start(value: &str) -> &str {
    value.trim_start_matches(is_js_whitespace_char)
}

/// ECMAScript `String.prototype.trimEnd`.
pub fn js_trim_end(value: &str) -> &str {
    value.trim_end_matches(is_js_whitespace_char)
}

/// JavaScript `String.prototype.length` for valid UTF-8 input.
pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}
