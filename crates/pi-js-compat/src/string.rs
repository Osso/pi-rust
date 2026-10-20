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

/// JavaScript `encodeURIComponent` for valid UTF-8 input.
pub fn encode_uri_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                encoded.push(char::from(byte))
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// JavaScript `String.prototype.length` for valid UTF-8 input.
pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// JavaScript `String.prototype.slice(start, end)` on UTF-16 code units.
/// Indices always land on a `char` boundary; a split surrogate pair is not
/// representable in Rust, so the boundary is rounded down.
pub fn slice_utf16(value: &str, start: usize, end: usize) -> &str {
    let start = byte_at_utf16(value, start);
    let end = byte_at_utf16(value, end);
    &value[start..end.max(start)]
}

fn byte_at_utf16(value: &str, index: usize) -> usize {
    let mut units = 0;
    for (byte, character) in value.char_indices() {
        if units >= index || units + character.len_utf16() > index {
            return byte;
        }
        units += character.len_utf16();
    }
    value.len()
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
