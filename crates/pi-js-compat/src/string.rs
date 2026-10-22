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

/// `URIError: URI malformed`, thrown by `encodeURIComponent` for an unpaired surrogate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UriMalformedError;

impl std::fmt::Display for UriMalformedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("URIError: URI malformed")
    }
}

impl std::error::Error for UriMalformedError {}

/// JavaScript `encodeURIComponent`.
pub fn encode_uri_component(value: &crate::js_json::JsString) -> Result<String, UriMalformedError> {
    let mut encoded = String::new();
    for character in char::decode_utf16(value.units().iter().copied()) {
        let character = character.map_err(|_| UriMalformedError)?;
        let mut buffer = [0; 4];
        for byte in character.encode_utf8(&mut buffer).bytes() {
            match byte {
                b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'_'
                | b'.'
                | b'!'
                | b'~'
                | b'*'
                | b'\''
                | b'('
                | b')' => encoded.push(char::from(byte)),
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
    }
    Ok(encoded)
}

/// JavaScript `String.prototype.length` for valid UTF-8 input.
pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// Slice by UTF-16 code units, then perform the UTF-8 conversion used by Node
/// SQLite string bindings. Unpaired surrogates become U+FFFD rather than being
/// dropped. JS's intermediate string still differs (approved name-helper gap).
pub fn utf16_slice_to_utf8(value: &str, start: usize, end: usize) -> String {
    let units: Vec<u16> = value
        .encode_utf16()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect();
    String::from_utf16_lossy(&units)
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

/// Global literal-pattern replacement with ECMAScript replacement expansion.
/// The search must be nonempty (no regex captures or empty UTF-16 matches).
pub fn replace_global(input: &str, search: &str, replacement: &str) -> String {
    assert!(!search.is_empty(), "global replacement requires a nonempty search");
    let mut output = String::with_capacity(input.len());
    let mut previous_end = 0;
    for (index, matched) in input.match_indices(search) {
        let end = index + matched.len();
        output.push_str(&input[previous_end..index]);
        output.push_str(&expand_replacement(
            replacement,
            matched,
            &input[..index],
            &input[end..],
        ));
        previous_end = end;
    }
    output.push_str(&input[previous_end..]);
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
