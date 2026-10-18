//! Translation of the JavaScript regular-expression dialect used by
//! highlight.js grammars into `fancy-regex` patterns.
//!
//! Differences handled here: JavaScript `\d`, `\w`, `\s`, `.`, `\b`, `\B`,
//! `^`, and `$` are ASCII- or ECMAScript-defined, `\uXXXX` uses a four-digit
//! form, and `\v` is a vertical tab.

const JS_SPACE_CLASS: &str = " \\t\\n\\u{0B}\\u{0C}\\r\\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}";
const WORD: &str = "A-Za-z0-9_";

/// Translates one JavaScript pattern into an equivalent `fancy-regex` pattern.
pub(crate) fn translate_pattern(pattern: &str) -> String {
    let characters = pattern.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(pattern.len() + 32);
    let mut index = 0;
    while index < characters.len() {
        match characters[index] {
            '\\' => {
                index = translate_escape(&characters, index, false, &mut output);
            }
            '[' => {
                index = translate_class(&characters, index, &mut output);
            }
            '.' => {
                output.push_str("[^\\n\\r\\u{2028}\\u{2029}]");
                index += 1;
            }
            '^' => {
                output.push_str("(?:^|(?<=[\\n\\r\\u{2028}\\u{2029}]))");
                index += 1;
            }
            '$' => {
                output.push_str("(?=\\r|\\n|\\u{2028}|\\u{2029}|$)");
                index += 1;
            }
            character => {
                output.push(character);
                index += 1;
            }
        }
    }
    output
}

/// Copies a character class, translating JavaScript shorthand classes inside it.
fn translate_class(characters: &[char], start: usize, output: &mut String) -> usize {
    output.push('[');
    let mut index = start + 1;
    if characters.get(index) == Some(&'^') {
        output.push('^');
        index += 1;
    }
    if characters.get(index) == Some(&']') {
        output.push_str("\\]");
        index += 1;
    }
    while index < characters.len() {
        match characters[index] {
            ']' => {
                output.push(']');
                return index + 1;
            }
            '\\' => {
                index = translate_escape(characters, index, true, output);
            }
            character => {
                output.push(character);
                index += 1;
            }
        }
    }
    output.push(']');
    index
}

/// Translates an escape sequence starting at `start`, returning the next index.
fn translate_escape(characters: &[char], start: usize, in_class: bool, output: &mut String) -> usize {
    let Some(&escaped) = characters.get(start + 1) else {
        output.push('\\');
        return start + 1;
    };
    match escaped {
        'd' => output.push_str(if in_class { "0-9" } else { "[0-9]" }),
        'D' if !in_class => output.push_str("[^0-9]"),
        'w' => output.push_str(if in_class { WORD } else { "[A-Za-z0-9_]" }),
        'W' if !in_class => output.push_str("[^A-Za-z0-9_]"),
        's' => {
            if in_class {
                output.push_str(JS_SPACE_CLASS);
            } else {
                output.push('[');
                output.push_str(JS_SPACE_CLASS);
                output.push(']');
            }
        }
        'S' if !in_class => {
            output.push_str("[^");
            output.push_str(JS_SPACE_CLASS);
            output.push(']');
        }
        'b' if in_class => output.push_str("\\x08"),
        'b' => output.push_str(&format!("(?:(?<=[{WORD}])(?![{WORD}])|(?<![{WORD}])(?=[{WORD}]))")),
        'B' if !in_class => {
            output.push_str(&format!("(?:(?<=[{WORD}])(?=[{WORD}])|(?<![{WORD}])(?![{WORD}]))"));
        }
        'v' => output.push_str("\\x0B"),
        'u' => return translate_unicode_escape(characters, start, output),
        other => {
            output.push('\\');
            output.push(other);
        }
    }
    start + 2
}

/// Translates `\uXXXX` to the `\u{XXXX}` form; other `\u` uses are copied.
fn translate_unicode_escape(characters: &[char], start: usize, output: &mut String) -> usize {
    let digits = characters.get(start + 2..start + 6);
    if let Some(digits) = digits
        && digits.len() == 4
        && digits.iter().all(|character| character.is_ascii_hexdigit())
    {
        output.push_str("\\u{");
        output.extend(digits);
        output.push('}');
        return start + 6;
    }
    output.push_str("\\u");
    start + 2
}

#[cfg(test)]
mod tests {
    use super::translate_pattern;

    #[test]
    fn translates_javascript_shorthand_classes_and_anchors() {
        assert_eq!(
            translate_pattern("\\d+\\w\\s."),
            "[0-9]+[A-Za-z0-9_][ \\t\\n\\u{0B}\\u{0C}\\r\\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}][^\\n\\r\\u{2028}\\u{2029}]"
        );
        assert_eq!(
            translate_pattern("^a$"),
            "(?:^|(?<=[\\n\\r\\u{2028}\\u{2029}]))a(?=\\r|\\n|\\u{2028}|\\u{2029}|$)"
        );
        assert_eq!(
            translate_pattern("[\\d\\s]"),
            "[0-9 \\t\\n\\u{0B}\\u{0C}\\r\\u{00A0}\\u{1680}\\u{2000}-\\u{200A}\\u{2028}\\u{2029}\\u{202F}\\u{205F}\\u{3000}\\u{FEFF}]"
        );
        assert_eq!(translate_pattern("\\u0041\\x41\\v"), "\\u{0041}\\x41\\x0B");
    }
}
