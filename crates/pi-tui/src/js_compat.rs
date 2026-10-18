//! ECMAScript string semantics shared by the TUI leaf modules.

/// ECMAScript `\s` / `trim()` whitespace code units.
pub(crate) fn is_js_whitespace_unit(unit: u16) -> bool {
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

pub(crate) fn is_js_whitespace_char(ch: char) -> bool {
    ch.len_utf16() == 1 && is_js_whitespace_unit(ch as u16)
}

/// ECMAScript `String.prototype.trim`.
pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(is_js_whitespace_char)
}
