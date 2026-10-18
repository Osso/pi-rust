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

/// JavaScript `String.prototype.replace(searchValue, replaceValue)` for string
/// arguments: replaces the first occurrence and expands `$$`, `$&`, `` $` ``
/// and `$'` in the replacement. Capture references stay literal because a
/// string search value has no capture groups.
pub fn replace(input: &str, search: &str, replacement: &str) -> String {
    let Some(index) = input.find(search) else {
        return input.to_owned();
    };
    let end = index + search.len();
    let mut output = String::with_capacity(input.len());
    output.push_str(&input[..index]);
    output.push_str(&expand_replacement(replacement, search, &input[..index], &input[end..]));
    output.push_str(&input[end..]);
    output
}

fn expand_replacement(replacement: &str, matched: &str, prefix: &str, suffix: &str) -> String {
    let mut output = String::with_capacity(replacement.len());
    let mut characters = replacement.chars();
    while let Some(character) = characters.next() {
        if character != '$' {
            output.push(character);
            continue;
        }
        match characters.clone().next() {
            Some('$') => {
                characters.next();
                output.push('$');
            }
            Some('&') => {
                characters.next();
                output.push_str(matched);
            }
            Some('`') => {
                characters.next();
                output.push_str(prefix);
            }
            Some('\'') => {
                characters.next();
                output.push_str(suffix);
            }
            _ => output.push('$'),
        }
    }
    output
}
