//! Glob escaping helpers (`escape`, `unescape`, pattern validation).
use super::units::*;
use crate::js_json::JsString;

use crate::string::is_js_whitespace_unit;

const MAX_PATTERN_LENGTH: usize = 1024 * 64;

/// `assertValidPattern`: Node counts UTF-16 code units.
pub(crate) fn is_valid_pattern(pattern: &JsString) -> bool {
    pattern.len() <= MAX_PATTERN_LENGTH
}

fn replace_brackets(value: &JsString, exclude: fn(u16) -> bool) -> JsString {
    let characters: Vec<u16> = value.units().iter().copied().collect();
    let mut output = JsString::default();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        let preceded_by_backslash = index > 0 && characters[index - 1] == BACKSLASH;
        if character == OPEN_BRACKET
            && !preceded_by_backslash
            && index + 2 < characters.len()
            && characters[index + 2] == CLOSE_BRACKET
        {
            let inner = characters[index + 1];
            if inner != SLASH && inner != BACKSLASH && !exclude(inner) {
                output.push_unit(inner);
                index += 3;
                continue;
            }
        }
        output.push_unit(character);
        index += 1;
    }
    output
}

fn remove_escapes(value: &JsString, exclude: fn(u16) -> bool) -> JsString {
    let characters: Vec<u16> = value.units().iter().copied().collect();
    let mut output = JsString::default();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == BACKSLASH
            && let Some(next) = characters.get(index + 1).copied()
            && next != SLASH
            && !exclude(next)
        {
            output.push_unit(next);
            index += 2;
            continue;
        }
        output.push_unit(character);
        index += 1;
    }
    output
}

/// `unescape(s)` with the default `magicalBraces: true`.
pub(crate) fn unescape(value: impl Into<JsString>) -> JsString {
    let value = value.into();
    let replaced = replace_brackets(&value, |_| false);
    remove_escapes(&replaced, |_| false)
}

/// Escape a character for use inside a generated Rust regular expression.
pub(crate) fn regexp_escape_char(character: u16) -> JsString {
    if is_js_whitespace_unit(character) {
        return JsString::from(character);
    }
    if matches!(
        character,
        BACKSLASH
            | DOT
            | PLUS
            | STAR
            | QUESTION
            | OPEN_PAREN
            | CLOSE_PAREN
            | PIPE
            | OPEN_BRACKET
            | CLOSE_BRACKET
            | OPEN_BRACE
            | CLOSE_BRACE
            | CARET
            | DOLLAR
    ) {
        return JsString::join([JsString::from("\\"), JsString::from(character)], "");
    }
    JsString::from(character)
}

pub(crate) fn regexp_escape(value: &JsString) -> JsString {
    value.units().iter().copied().map(regexp_escape_char).collect()
}

/// Escape a character for use inside a character class.
pub(crate) fn brace_escape(character: u16) -> JsString {
    if matches!(character, OPEN_BRACKET | BACKSLASH | CLOSE_BRACKET | DASH) {
        JsString::join([JsString::from("\\"), JsString::from(character)], "")
    } else {
        JsString::from(character)
    }
}

#[cfg(test)]
mod tests {
    use super::unescape;

    #[test]
    fn unescapes_backslash_and_bracket_escapes() {
        assert_eq!(unescape("a\\*b"), "a*b");
        assert_eq!(unescape("a[*]b"), "a*b");
        assert_eq!(unescape("a[\\]]b"), "a[]]b");
        assert_eq!(unescape("a\\/b"), "a\\/b");
        assert_eq!(unescape("[a]"), "a");
        assert_eq!(unescape("\\[a]"), "[a]");
    }
}
