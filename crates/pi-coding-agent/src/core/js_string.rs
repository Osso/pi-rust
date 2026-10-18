//! JavaScript `String.prototype.trim` character set, shared by modules that
//! mirror the source's trim semantics.

/// Characters removed by JavaScript `trim`, `trimStart`, and `trimEnd`.
pub(crate) fn is_javascript_trim_character(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'..='\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// JavaScript `trim()`.
pub(crate) fn trim_javascript(value: &str) -> &str {
    value.trim_matches(is_javascript_trim_character)
}
