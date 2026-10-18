//! Glob escaping helpers (`escape`, `unescape`, pattern validation).

use crate::string::is_js_whitespace_char;

const MAX_PATTERN_LENGTH: usize = 1024 * 64;

/// `assertValidPattern`: Node counts UTF-16 code units.
pub(crate) fn is_valid_pattern(pattern: &str) -> bool {
    pattern.encode_utf16().count() <= MAX_PATTERN_LENGTH
}

fn replace_brackets(value: &str, exclude: fn(char) -> bool) -> String {
    let characters: Vec<char> = value.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        let preceded_by_backslash = index > 0 && characters[index - 1] == '\\';
        if character == '[' && !preceded_by_backslash && index + 2 < characters.len() && characters[index + 2] == ']' {
            let inner = characters[index + 1];
            if inner != '/' && inner != '\\' && !exclude(inner) {
                output.push(inner);
                index += 3;
                continue;
            }
        }
        output.push(character);
        index += 1;
    }
    output
}

fn remove_escapes(value: &str, exclude: fn(char) -> bool) -> String {
    let characters: Vec<char> = value.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '\\'
            && let Some(next) = characters.get(index + 1).copied()
            && next != '/'
            && !exclude(next)
        {
            output.push(next);
            index += 2;
            continue;
        }
        output.push(character);
        index += 1;
    }
    output
}

/// `unescape(s)` with the default `magicalBraces: true`.
pub(crate) fn unescape(value: &str) -> String {
    let replaced = replace_brackets(value, |_| false);
    remove_escapes(&replaced, |_| false)
}

/// Escape a character for use inside a generated Rust regular expression.
pub(crate) fn regexp_escape_char(character: char) -> String {
    if is_js_whitespace_char(character) {
        return character.to_string();
    }
    if matches!(
        character,
        '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
    ) {
        return format!("\\{character}");
    }
    character.to_string()
}

pub(crate) fn regexp_escape(value: &str) -> String {
    value.chars().map(regexp_escape_char).collect()
}

/// Escape a character for use inside a character class.
pub(crate) fn brace_escape(character: char) -> String {
    if matches!(character, '[' | '\\' | ']' | '-') {
        format!("\\{character}")
    } else {
        character.to_string()
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
