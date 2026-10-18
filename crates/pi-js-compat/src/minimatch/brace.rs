//! Brace expansion (`brace-expansion` 5.x as bundled with minimatch 10.2.5).

const EXPANSION_MAX: usize = 100_000;
const EXPANSION_MAX_LENGTH: usize = 4_000_000;

const ESC_SLASH: &str = "\u{0}\u{1}SLASH\u{1}\u{0}";
const ESC_OPEN: &str = "\u{0}\u{1}OPEN\u{1}\u{0}";
const ESC_CLOSE: &str = "\u{0}\u{1}CLOSE\u{1}\u{0}";
const ESC_COMMA: &str = "\u{0}\u{1}COMMA\u{1}\u{0}";
const ESC_PERIOD: &str = "\u{0}\u{1}PERIOD\u{1}\u{0}";

struct Balanced<'a> {
    pre: &'a [char],
    body: &'a [char],
    post: &'a [char],
}

fn balanced(value: &[char]) -> Option<Balanced<'_>> {
    let start = value.iter().position(|character| *character == '{')?;
    let mut depth = 0usize;
    for (index, character) in value.iter().enumerate().skip(start) {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(Balanced {
                        pre: &value[..start],
                        body: &value[start + 1..index],
                        post: &value[index + 1..],
                    });
                }
            }
            _ => {}
        }
    }
    None
}

fn escape_braces(value: &[char]) -> Vec<char> {
    let mut output = Vec::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        let character = value[index];
        if character == '\\'
            && let Some(next) = value.get(index + 1).copied()
        {
            let sentinel = match next {
                '\\' => Some(ESC_SLASH),
                '{' => Some(ESC_OPEN),
                '}' => Some(ESC_CLOSE),
                ',' => Some(ESC_COMMA),
                '.' => Some(ESC_PERIOD),
                _ => None,
            };
            if let Some(sentinel) = sentinel {
                output.extend(sentinel.chars());
                index += 2;
                continue;
            }
        }
        output.push(character);
        index += 1;
    }
    output
}

fn unescape_braces(value: String) -> String {
    let mut output = value;
    for (sentinel, replacement) in [
        (ESC_SLASH, "\\"),
        (ESC_OPEN, "{"),
        (ESC_CLOSE, "}"),
        (ESC_COMMA, ","),
        (ESC_PERIOD, "."),
    ] {
        output = output.replace(sentinel, replacement);
    }
    output
}

fn split_comma(value: &[char]) -> Vec<Vec<char>> {
    value
        .split(|character| *character == ',')
        .map(<[char]>::to_vec)
        .collect()
}

fn parse_comma_parts(value: &[char]) -> Vec<Vec<char>> {
    if value.is_empty() {
        return vec![Vec::new()];
    }
    let Some(matched) = balanced(value) else {
        return split_comma(value);
    };
    let mut parts = split_comma(matched.pre);
    let last = parts.len() - 1;
    parts[last].push('{');
    parts[last].extend_from_slice(matched.body);
    parts[last].push('}');
    let mut post_parts = parse_comma_parts(matched.post);
    if !matched.post.is_empty() {
        let first = post_parts.remove(0);
        parts[last].extend_from_slice(&first);
        parts.extend(post_parts);
    }
    parts
}

fn is_padded(value: &[char]) -> bool {
    let digits: &[char] = match value.first() {
        Some('-') => &value[1..],
        _ => value,
    };
    digits.len() >= 2 && digits[0] == '0' && digits[1].is_ascii_digit()
}

fn numeric(value: &[char]) -> f64 {
    let text: String = value.iter().collect();
    match text.parse::<f64>() {
        Ok(number) => number.trunc(),
        Err(_) => f64::from(value.first().copied().unwrap_or('\0') as u32),
    }
}

fn split_sequence(body: &[char]) -> Vec<Vec<char>> {
    let mut parts = Vec::new();
    let mut current = Vec::new();
    let mut index = 0;
    while index < body.len() {
        if body[index] == '.' && body.get(index + 1) == Some(&'.') {
            parts.push(current);
            current = Vec::new();
            index += 2;
        } else {
            current.push(body[index]);
            index += 1;
        }
    }
    parts.push(current);
    parts
}

fn format_sequence_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e21 {
        format!("{}", value as i128)
    } else {
        format!("{value}")
    }
}

fn expand_sequence(body: &[char], is_alpha: bool, max: usize, max_length: usize) -> Vec<String> {
    let parts = split_sequence(body);
    let (Some(first), Some(second)) = (parts.first(), parts.get(1)) else {
        return Vec::new();
    };
    let x = numeric(first);
    let y = numeric(second);
    let width = first.len().max(second.len());
    let mut increment = match parts.get(2) {
        Some(third) => numeric(third).abs().max(1.0),
        None => 1.0,
    };
    let reverse = y < x;
    if reverse {
        increment = -increment;
    }
    let padded = parts.iter().any(|part| is_padded(part));
    let mut values = Vec::new();
    let mut length = 0usize;
    let mut value = x;
    while (if reverse { value >= y } else { value <= y }) && values.len() < max {
        let mut text = if is_alpha {
            let code = value as i64 as u32 % 65_536;
            let character = char::from_u32(code).unwrap_or('\u{fffd}');
            if character == '\\' {
                String::new()
            } else {
                character.to_string()
            }
        } else {
            format_sequence_number(value)
        };
        if !is_alpha && padded {
            let need = width.saturating_sub(text.chars().count());
            if need > 0 {
                let zeros = "0".repeat(need);
                if value < 0.0 {
                    text = format!("-{zeros}{}", &text[1..]);
                } else {
                    text = format!("{zeros}{text}");
                }
            }
        }
        if length + text.chars().count() > max_length {
            break;
        }
        length += text.chars().count();
        values.push(text);
        value += increment;
    }
    values
}

fn combine(
    accumulated: &[String],
    pre: &str,
    values: &[String],
    max: usize,
    max_length: usize,
    drop_empties: bool,
) -> Vec<String> {
    let mut output = Vec::new();
    let mut length = 0usize;
    for prefix in accumulated {
        for value in values {
            if output.len() >= max {
                return output;
            }
            let expansion = format!("{prefix}{pre}{value}");
            if drop_empties && expansion.is_empty() {
                continue;
            }
            let expansion_length = expansion.chars().count();
            if length + expansion_length > max_length {
                return output;
            }
            length += expansion_length;
            output.push(expansion);
        }
    }
    output
}

fn contains_unescaped_brace_set(value: &[char]) -> bool {
    for (index, character) in value.iter().enumerate() {
        if *character != '{' {
            continue;
        }
        let mut inner = index + 1;
        while inner < value.len() && value[inner] != '{' {
            if value[inner] == '}' {
                return true;
            }
            inner += 1;
        }
    }
    false
}

pub(crate) fn brace_expand(pattern: &str, max: usize) -> Vec<String> {
    let characters: Vec<char> = pattern.chars().collect();
    if !contains_unescaped_brace_set(&characters) {
        return vec![pattern.to_owned()];
    }
    let mut characters = characters;
    if characters.starts_with(&['{', '}']) {
        let mut prefixed = vec!['\\', '{', '\\', '}'];
        prefixed.extend_from_slice(&characters[2..]);
        characters = prefixed;
    }
    expand_escaped(escape_braces(&characters), max, EXPANSION_MAX_LENGTH, true)
        .into_iter()
        .map(unescape_braces)
        .collect()
}

pub(crate) fn default_max() -> usize {
    EXPANSION_MAX
}

enum OptionValues {
    Single(String),
    Values(Vec<String>),
}

enum GroupAction {
    Continue(Vec<char>),
    Return(Vec<String>),
}

struct GroupInput<'a> {
    accumulated: &'a [String],
    pre: &'a str,
    body: &'a [char],
    post: &'a [char],
    max: usize,
    max_length: usize,
    drop_empties: bool,
}

fn expand_escaped(mut remaining: Vec<char>, max: usize, max_length: usize, is_top: bool) -> Vec<String> {
    let mut accumulated = vec![String::new()];
    let mut drop_empties = false;
    let mut first_group = true;
    loop {
        let Some(matched) = balanced(&remaining) else {
            let rest: String = remaining.iter().collect();
            return combine(&accumulated, &rest, &[String::new()], max, max_length, drop_empties);
        };
        let pre: String = matched.pre.iter().collect();
        let body: Vec<char> = matched.body.to_vec();
        let post: Vec<char> = matched.post.to_vec();
        let input = group_input(&accumulated, &pre, &body, &post, max, max_length, drop_empties);
        if pre.ends_with('$') {
            accumulated = expand_dollar_group(&input);
            first_group = false;
            if post.is_empty() {
                break;
            }
            remaining = post;
            continue;
        }
        let body_text: String = body.iter().collect();
        let is_alpha_sequence = is_alpha_sequence(&body_text);
        let is_sequence = is_numeric_sequence(&body_text) || is_alpha_sequence;
        let is_options = body.contains(&',');
        if !is_sequence && !is_options {
            match expand_plain_group(&input) {
                GroupAction::Continue(next) => {
                    remaining = next;
                    continue;
                }
                GroupAction::Return(values) => return values,
            }
        }
        if first_group {
            drop_empties = is_top && !is_sequence;
            first_group = false;
        }
        let input = group_input(&accumulated, &pre, &body, &post, max, max_length, drop_empties);
        accumulated = expand_group_values(&input, is_sequence, is_alpha_sequence);
        if post.is_empty() {
            break;
        }
        remaining = post;
    }
    accumulated
}

fn group_input<'a>(
    accumulated: &'a [String],
    pre: &'a str,
    body: &'a [char],
    post: &'a [char],
    max: usize,
    max_length: usize,
    drop_empties: bool,
) -> GroupInput<'a> {
    GroupInput {
        accumulated,
        pre,
        body,
        post,
        max,
        max_length,
        drop_empties,
    }
}

fn expand_dollar_group(input: &GroupInput<'_>) -> Vec<String> {
    let literal = format!("{}{{{}}}", input.pre, input.body.iter().collect::<String>());
    combine(
        input.accumulated,
        &literal,
        &[String::new()],
        input.max,
        input.max_length,
        input.drop_empties && input.post.is_empty(),
    )
}

fn expand_plain_group(input: &GroupInput<'_>) -> GroupAction {
    if post_has_trailing_comma_group(input.post) {
        let mut combined = input.pre.chars().collect::<Vec<_>>();
        combined.push('{');
        combined.extend_from_slice(input.body);
        combined.extend(ESC_CLOSE.chars());
        combined.extend_from_slice(input.post);
        return GroupAction::Continue(combined);
    }
    let literal = format!(
        "{}{{{}}}{}",
        input.pre,
        input.body.iter().collect::<String>(),
        input.post.iter().collect::<String>()
    );
    GroupAction::Return(combine(
        input.accumulated,
        &literal,
        &[String::new()],
        input.max,
        input.max_length,
        input.drop_empties,
    ))
}

fn expand_group_values(input: &GroupInput<'_>, is_sequence: bool, is_alpha_sequence: bool) -> Vec<String> {
    if is_sequence {
        let values = expand_sequence(input.body, is_alpha_sequence, input.max, input.max_length);
        return combine(
            input.accumulated,
            input.pre,
            &values,
            input.max,
            input.max_length,
            input.drop_empties && input.post.is_empty(),
        );
    }
    let drops_empties = input.drop_empties
        && input.post.is_empty()
        && input.pre.is_empty()
        && input.accumulated.iter().all(|value| value.is_empty());
    match expand_options(input.body, input.max, input.max_length, drops_empties) {
        OptionValues::Single(value) => combine(
            input.accumulated,
            &format!("{}{value}", input.pre),
            &[String::new()],
            input.max,
            input.max_length,
            input.drop_empties && input.post.is_empty(),
        ),
        OptionValues::Values(values) => combine(
            input.accumulated,
            input.pre,
            &values,
            input.max,
            input.max_length,
            input.drop_empties && input.post.is_empty(),
        ),
    }
}

fn expand_options(body: &[char], max: usize, max_length: usize, drops_empties: bool) -> OptionValues {
    let mut alternatives = parse_comma_parts(body);
    if alternatives.len() == 1 {
        let expanded: Vec<String> = expand_escaped(alternatives[0].clone(), max, max_length, false)
            .into_iter()
            .map(|value| format!("{{{value}}}"))
            .collect();
        if expanded.len() == 1 {
            return OptionValues::Single(expanded[0].clone());
        }
        alternatives = expanded.into_iter().map(|value| value.chars().collect()).collect();
    }
    let mut values = Vec::new();
    let mut values_length = 0usize;
    'outer: for alternative in alternatives {
        for expanded in expand_escaped(alternative, max, max_length, false) {
            if drops_empties && expanded.is_empty() {
                continue;
            }
            if values.len() >= max || values_length + expanded.chars().count() > max_length {
                break 'outer;
            }
            values_length += expanded.chars().count();
            values.push(expanded);
        }
    }
    OptionValues::Values(values)
}

fn post_has_trailing_comma_group(post: &[char]) -> bool {
    for (index, character) in post.iter().enumerate() {
        if *character == ',' && post.get(index + 1).copied() != Some(',') && post[index + 1..].contains(&'}') {
            return true;
        }
    }
    false
}

fn is_numeric_sequence(body: &str) -> bool {
    let parts: Vec<&str> = body.split("..").collect();
    match parts.as_slice() {
        [first, second] => is_signed_digits(first) && is_signed_digits(second),
        [first, second, third] => is_signed_digits(first) && is_signed_digits(second) && is_signed_digits(third),
        _ => false,
    }
}

fn is_signed_digits(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_alpha_sequence(body: &str) -> bool {
    let parts: Vec<&str> = body.split("..").collect();
    let is_alpha = |value: &str| {
        let mut characters = value.chars();
        matches!(characters.next(), Some(character) if character.is_ascii_alphabetic()) && characters.next().is_none()
    };
    match parts.as_slice() {
        [first, second] => is_alpha(first) && is_alpha(second),
        [first, second, third] => is_alpha(first) && is_alpha(second) && is_signed_digits(third),
        _ => false,
    }
}
