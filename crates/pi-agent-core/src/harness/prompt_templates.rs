use super::types::PromptTemplate;

/// Parses ASCII-space/tab-delimited arguments with source-compatible quote handling.
///
/// Quotes are removed; backslashes and other whitespace remain literal.
/// Unmatched quotes are accepted, and empty arguments are omitted.
pub fn parse_command_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for character in input.chars() {
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            } else {
                current.push(character);
            }
        } else if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character == ' ' || character == '\t' {
            push_nonempty_token(&mut args, &mut current);
        } else {
            current.push(character);
        }
    }

    push_nonempty_token(&mut args, &mut current);
    args
}

/// Substitutes positional, slice, `$ARGUMENTS`, and `$@` placeholders in source order.
///
/// Aggregate replacements preserve JavaScript replacement-string semantics.
pub fn substitute_args(content: &str, args: &[String]) -> String {
    let positional = replace_positional(content, args);
    let slices = replace_slices(&positional, args);
    let all_args = args.join(" ");
    let arguments = replace_with_js_replacement_semantics(&slices, "$ARGUMENTS", &all_args);
    replace_with_js_replacement_semantics(&arguments, "$@", &all_args)
}

/// Formats `template.content` with positional arguments.
///
/// Pass `&[]` to represent the TypeScript default empty argument list.
pub fn format_prompt_template_invocation(template: &PromptTemplate, args: &[String]) -> String {
    substitute_args(&template.content, args)
}

fn push_nonempty_token(args: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        args.push(std::mem::take(current));
    }
}

fn replace_positional(content: &str, args: &[String]) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut copy_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'$' {
            let digit_start = index + 1;
            if digit_start < bytes.len() && is_ascii_digit(bytes[digit_start]) {
                let match_end = scan_ascii_digits(bytes, digit_start);
                result.push_str(&content[copy_start..index]);
                append_positional_value(&mut result, &bytes[digit_start..match_end], args);
                copy_start = match_end;
                index = match_end;
                continue;
            }
        }
        index += 1;
    }

    result.push_str(&content[copy_start..]);
    result
}

fn append_positional_value(result: &mut String, digits: &[u8], args: &[String]) {
    let Some(position) = parse_decimal_at_most(digits, args.len()) else {
        return;
    };
    if position > 0 {
        result.push_str(&args[position - 1]);
    }
}

fn replace_slices(content: &str, args: &[String]) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut copy_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        if let Some(slice_match) = parse_slice_match(bytes, index) {
            result.push_str(&content[copy_start..index]);
            append_slice_value(&mut result, &slice_match, args);
            copy_start = slice_match.end;
            index = slice_match.end;
        } else {
            index += 1;
        }
    }

    result.push_str(&content[copy_start..]);
    result
}

struct SliceMatch<'a> {
    end: usize,
    start_digits: &'a [u8],
    length_digits: Option<&'a [u8]>,
}

fn parse_slice_match(bytes: &[u8], start: usize) -> Option<SliceMatch<'_>> {
    let prefix_end = start.checked_add(4)?;
    if bytes.get(start..prefix_end) != Some(b"${@:") {
        return None;
    }

    let start_digits_begin = prefix_end;
    let start_digits_end = scan_ascii_digits(bytes, start_digits_begin);
    if start_digits_begin == start_digits_end {
        return None;
    }

    let mut cursor = start_digits_end;
    let length_digits = if bytes.get(cursor) == Some(&b':') {
        let length_digits_begin = cursor + 1;
        let length_digits_end = scan_ascii_digits(bytes, length_digits_begin);
        if length_digits_begin == length_digits_end {
            return None;
        }
        cursor = length_digits_end;
        Some(&bytes[length_digits_begin..length_digits_end])
    } else {
        None
    };

    if bytes.get(cursor) != Some(&b'}') {
        return None;
    }

    Some(SliceMatch {
        end: cursor + 1,
        start_digits: &bytes[start_digits_begin..start_digits_end],
        length_digits,
    })
}

fn append_slice_value(result: &mut String, slice_match: &SliceMatch<'_>, args: &[String]) {
    let start_number = parse_decimal_at_most(slice_match.start_digits, args.len());
    let start = start_number.map_or(args.len(), |number| number.saturating_sub(1));
    let end = match slice_match.length_digits {
        None => args.len(),
        Some(length_digits) => {
            let length = parse_decimal_at_most(length_digits, args.len()).unwrap_or(args.len());
            start.saturating_add(length).min(args.len())
        }
    };

    append_joined_args(result, &args[start..end]);
}

fn append_joined_args(result: &mut String, args: &[String]) {
    for (index, argument) in args.iter().enumerate() {
        if index > 0 {
            result.push(' ');
        }
        result.push_str(argument);
    }
}

fn replace_with_js_replacement_semantics(content: &str, needle: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut search_start = 0;

    while let Some(relative_match_start) = content[search_start..].find(needle) {
        let match_start = search_start + relative_match_start;
        let match_end = match_start + needle.len();
        result.push_str(&content[search_start..match_start]);
        append_replacement(&mut result, content, match_start, match_end, replacement);
        search_start = match_end;
    }

    result.push_str(&content[search_start..]);
    result
}

fn append_replacement(output: &mut String, content: &str, match_start: usize, match_end: usize, replacement: &str) {
    let bytes = replacement.as_bytes();
    let mut literal_start = 0;
    let mut index = 0;

    while index < bytes.len().saturating_sub(1) {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }

        let token = bytes[index + 1];
        let token_end = index + 2;
        let value = match token {
            b'$' => Some("$"),
            b'&' => Some(&content[match_start..match_end]),
            b'`' => Some(&content[..match_start]),
            b'\'' => Some(&content[match_end..]),
            _ => None,
        };
        let Some(value) = value else {
            index += 1;
            continue;
        };

        output.push_str(&replacement[literal_start..index]);
        output.push_str(value);
        literal_start = token_end;
        index = token_end;
    }

    output.push_str(&replacement[literal_start..]);
}

fn scan_ascii_digits(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && is_ascii_digit(bytes[end]) {
        end += 1;
    }
    end
}

fn is_ascii_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

fn parse_decimal_at_most(digits: &[u8], maximum: usize) -> Option<usize> {
    let mut value = 0;
    for &digit in digits {
        let digit = usize::from(digit - b'0');
        if value > maximum / 10 || (value == maximum / 10 && digit > maximum % 10) {
            return None;
        }
        value = value * 10 + digit;
    }
    Some(value)
}
