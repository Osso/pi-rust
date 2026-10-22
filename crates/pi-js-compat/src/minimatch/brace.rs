//! Brace expansion (`brace-expansion` 5.x as bundled with minimatch 10.2.5).
use super::units::*;
use crate::js_json::JsString;

const EXPANSION_MAX: usize = 100_000;
const EXPANSION_MAX_LENGTH: usize = 4_000_000;

const ESC_SLASH: &str = "\u{0}\u{1}SLASH\u{1}\u{0}";
const ESC_OPEN: &str = "\u{0}\u{1}OPEN\u{1}\u{0}";
const ESC_CLOSE: &str = "\u{0}\u{1}CLOSE\u{1}\u{0}";
const ESC_COMMA: &str = "\u{0}\u{1}COMMA\u{1}\u{0}";
const ESC_PERIOD: &str = "\u{0}\u{1}PERIOD\u{1}\u{0}";

struct Balanced<'a> {
    pre: &'a [u16],
    body: &'a [u16],
    post: &'a [u16],
}

fn balanced(value: &[u16]) -> Option<Balanced<'_>> {
    let start = value.iter().position(|character| *character == OPEN_BRACE)?;
    let mut depth = 0usize;
    for (index, character) in value.iter().enumerate().skip(start) {
        match *character {
            OPEN_BRACE => depth += 1,
            CLOSE_BRACE => {
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

fn escape_braces(value: &[u16]) -> Vec<u16> {
    let mut output = Vec::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        let character = value[index];
        if character == BACKSLASH
            && let Some(next) = value.get(index + 1).copied()
        {
            let sentinel = match next {
                BACKSLASH => Some(ESC_SLASH),
                OPEN_BRACE => Some(ESC_OPEN),
                CLOSE_BRACE => Some(ESC_CLOSE),
                COMMA => Some(ESC_COMMA),
                DOT => Some(ESC_PERIOD),
                _ => None,
            };
            if let Some(sentinel) = sentinel {
                output.extend(sentinel.encode_utf16());
                index += 2;
                continue;
            }
        }
        output.push(character);
        index += 1;
    }
    output
}

fn unescape_braces(value: JsString) -> JsString {
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

fn split_comma(value: &[u16]) -> Vec<Vec<u16>> {
    value
        .split(|character| *character == COMMA)
        .map(<[u16]>::to_vec)
        .collect()
}

fn parse_comma_parts(value: &[u16]) -> Vec<Vec<u16>> {
    if value.is_empty() {
        return vec![Vec::new()];
    }
    let Some(matched) = balanced(value) else {
        return split_comma(value);
    };
    let mut parts = split_comma(matched.pre);
    let last = parts.len() - 1;
    parts[last].push(OPEN_BRACE);
    parts[last].extend_from_slice(matched.body);
    parts[last].push(CLOSE_BRACE);
    let mut post_parts = parse_comma_parts(matched.post);
    if !matched.post.is_empty() {
        let first = post_parts.remove(0);
        parts[last].extend_from_slice(&first);
        parts.extend(post_parts);
    }
    parts
}

fn is_padded(value: &[u16]) -> bool {
    let digits: &[u16] = match value.first() {
        Some(&DASH) => &value[1..],
        _ => value,
    };
    digits.len() >= 2 && digits[0] == ZERO && (ZERO..=NINE).contains(&digits[1])
}

fn numeric(value: &[u16]) -> f64 {
    let negative = value.first() == Some(&DASH);
    let digits = if negative { &value[1..] } else { value };
    if !digits.is_empty() && digits.iter().all(|unit| (ZERO..=NINE).contains(unit)) {
        let number = digits
            .iter()
            .fold(0.0, |number, unit| number * 10.0 + f64::from(unit - ZERO));
        return if negative { -number } else { number };
    }
    f64::from(value.first().copied().unwrap_or(0))
}

fn split_sequence(body: &[u16]) -> Vec<Vec<u16>> {
    let mut parts = Vec::new();
    let mut current = Vec::new();
    let mut index = 0;
    while index < body.len() {
        if body[index] == DOT && body.get(index + 1) == Some(&DOT) {
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

fn format_sequence_number(value: f64) -> JsString {
    if value.fract() == 0.0 && value.abs() < 1e21 {
        JsString::from(format!("{}", value as i128))
    } else {
        JsString::from(format!("{value}"))
    }
}

fn expand_sequence(body: &[u16], is_alpha: bool, max: usize, max_length: usize) -> Vec<JsString> {
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
            let character = code as u16;
            if character == BACKSLASH {
                JsString::default()
            } else {
                JsString::from(character)
            }
        } else {
            format_sequence_number(value)
        };
        if !is_alpha && padded {
            let need = width.saturating_sub(text.units().iter().copied().count());
            if need > 0 {
                let zeros = "0".repeat(need);
                if value < 0.0 {
                    text = JsString::join(
                        [
                            JsString::from("-"),
                            JsString::from(zeros),
                            JsString::from(&text.slice(1, text.len())),
                        ],
                        "",
                    );
                } else {
                    text = JsString::join([JsString::from(zeros), JsString::from(text)], "");
                }
            }
        }
        if length + text.units().iter().copied().count() > max_length {
            break;
        }
        length += text.units().iter().copied().count();
        values.push(text);
        value += increment;
    }
    values
}

fn combine(
    accumulated: &[JsString],
    pre: &JsString,
    values: &[JsString],
    max: usize,
    max_length: usize,
    drop_empties: bool,
) -> Vec<JsString> {
    let mut output = Vec::new();
    let mut length = 0usize;
    for prefix in accumulated {
        for value in values {
            if output.len() >= max {
                return output;
            }
            let expansion = JsString::join([JsString::from(prefix), JsString::from(pre), JsString::from(value)], "");
            if drop_empties && expansion.is_empty() {
                continue;
            }
            let expansion_length = expansion.units().iter().copied().count();
            if length + expansion_length > max_length {
                return output;
            }
            length += expansion_length;
            output.push(expansion);
        }
    }
    output
}

fn contains_unescaped_brace_set(value: &[u16]) -> bool {
    for (index, character) in value.iter().enumerate() {
        if *character != OPEN_BRACE {
            continue;
        }
        let mut inner = index + 1;
        while inner < value.len() && value[inner] != OPEN_BRACE {
            if value[inner] == CLOSE_BRACE {
                return true;
            }
            inner += 1;
        }
    }
    false
}

pub(crate) fn brace_expand(pattern: &JsString, max: usize) -> Vec<JsString> {
    let characters: Vec<u16> = pattern.units().iter().copied().collect();
    if !contains_unescaped_brace_set(&characters) {
        return vec![pattern.to_owned()];
    }
    let mut characters = characters;
    if characters.starts_with(&[OPEN_BRACE, CLOSE_BRACE]) {
        let mut prefixed = vec![BACKSLASH, OPEN_BRACE, BACKSLASH, CLOSE_BRACE];
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
    Single(JsString),
    Values(Vec<JsString>),
}

enum GroupAction {
    Continue(Vec<u16>),
    Return(Vec<JsString>),
}

struct GroupInput<'a> {
    accumulated: &'a [JsString],
    pre: &'a JsString,
    body: &'a [u16],
    post: &'a [u16],
    max: usize,
    max_length: usize,
    drop_empties: bool,
}

fn expand_escaped(mut remaining: Vec<u16>, max: usize, max_length: usize, is_top: bool) -> Vec<JsString> {
    let mut accumulated = vec![JsString::default()];
    let mut drop_empties = false;
    let mut first_group = true;
    loop {
        let Some(matched) = balanced(&remaining) else {
            let rest: JsString = remaining.iter().collect();
            return combine(
                &accumulated,
                &rest,
                &[JsString::default()],
                max,
                max_length,
                drop_empties,
            );
        };
        let pre: JsString = matched.pre.iter().collect();
        let body: Vec<u16> = matched.body.to_vec();
        let post: Vec<u16> = matched.post.to_vec();
        let input = group_input(&accumulated, &pre, &body, &post, max, max_length, drop_empties);
        if pre.ends_with(DOLLAR) {
            accumulated = expand_dollar_group(&input);
            first_group = false;
            if post.is_empty() {
                break;
            }
            remaining = post;
            continue;
        }
        let body_text: JsString = body.iter().collect();
        let is_alpha_sequence = is_alpha_sequence(&body_text);
        let is_sequence = is_numeric_sequence(&body_text) || is_alpha_sequence;
        let is_options = body.contains(&COMMA);
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
    accumulated: &'a [JsString],
    pre: &'a JsString,
    body: &'a [u16],
    post: &'a [u16],
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

fn expand_dollar_group(input: &GroupInput<'_>) -> Vec<JsString> {
    let literal = JsString::join(
        [
            JsString::from(input.pre),
            JsString::from("{"),
            JsString::from(input.body.iter().collect::<JsString>()),
            JsString::from("}"),
        ],
        "",
    );
    combine(
        input.accumulated,
        &literal,
        &[JsString::default()],
        input.max,
        input.max_length,
        input.drop_empties && input.post.is_empty(),
    )
}

fn expand_plain_group(input: &GroupInput<'_>) -> GroupAction {
    if post_has_trailing_comma_group(input.post) {
        let mut combined = input.pre.units().iter().copied().collect::<Vec<_>>();
        combined.push(OPEN_BRACE);
        combined.extend_from_slice(input.body);
        combined.extend(ESC_CLOSE.encode_utf16());
        combined.extend_from_slice(input.post);
        return GroupAction::Continue(combined);
    }
    let literal = JsString::join(
        [
            JsString::from(input.pre),
            JsString::from("{"),
            JsString::from(input.body.iter().collect::<JsString>()),
            JsString::from("}"),
            JsString::from(input.post.iter().collect::<JsString>()),
        ],
        "",
    );
    GroupAction::Return(combine(
        input.accumulated,
        &literal,
        &[JsString::default()],
        input.max,
        input.max_length,
        input.drop_empties,
    ))
}

fn expand_group_values(input: &GroupInput<'_>, is_sequence: bool, is_alpha_sequence: bool) -> Vec<JsString> {
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
            &JsString::join([JsString::from(input.pre), JsString::from(value)], ""),
            &[JsString::default()],
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

fn expand_options(body: &[u16], max: usize, max_length: usize, drops_empties: bool) -> OptionValues {
    let mut alternatives = parse_comma_parts(body);
    if alternatives.len() == 1 {
        let expanded: Vec<JsString> = expand_escaped(alternatives[0].clone(), max, max_length, false)
            .into_iter()
            .map(|value| JsString::join([JsString::from("{"), JsString::from(value), JsString::from("}")], ""))
            .collect();
        if expanded.len() == 1 {
            return OptionValues::Single(expanded[0].clone());
        }
        alternatives = expanded
            .into_iter()
            .map(|value| value.units().iter().copied().collect())
            .collect();
    }
    let mut values = Vec::new();
    let mut values_length = 0usize;
    'outer: for alternative in alternatives {
        for expanded in expand_escaped(alternative, max, max_length, false) {
            if drops_empties && expanded.is_empty() {
                continue;
            }
            if values.len() >= max || values_length + expanded.units().iter().copied().count() > max_length {
                break 'outer;
            }
            values_length += expanded.units().iter().copied().count();
            values.push(expanded);
        }
    }
    OptionValues::Values(values)
}

fn post_has_trailing_comma_group(post: &[u16]) -> bool {
    for (index, character) in post.iter().enumerate() {
        if *character == COMMA
            && post.get(index + 1).copied() != Some(COMMA)
            && post[index + 1..].contains(&CLOSE_BRACE)
        {
            return true;
        }
    }
    false
}

fn is_numeric_sequence(body: &JsString) -> bool {
    let parts: Vec<JsString> = split_sequence(body.units())
        .into_iter()
        .map(JsString::from_utf16)
        .collect();
    match parts.as_slice() {
        [first, second] => is_signed_digits(first) && is_signed_digits(second),
        [first, second, third] => is_signed_digits(first) && is_signed_digits(second) && is_signed_digits(third),
        _ => false,
    }
}

fn is_signed_digits(value: &JsString) -> bool {
    let digits = value.strip_prefix(DASH).unwrap_or_else(|| value.clone());
    !digits.is_empty() && digits.units().iter().copied().all(|byte| (ZERO..=NINE).contains(&byte))
}

fn is_alpha_sequence(body: &JsString) -> bool {
    let parts: Vec<JsString> = split_sequence(body.units())
        .into_iter()
        .map(JsString::from_utf16)
        .collect();
    let is_alpha = |value: &JsString| {
        let mut characters = value.units().iter().copied();
        matches!(characters.next(), Some(character) if ((UPPER_A..=UPPER_Z).contains(&character) || (LOWER_A..=LOWER_Z).contains(&character)))
            && characters.next().is_none()
    };
    match parts.as_slice() {
        [first, second] => is_alpha(first) && is_alpha(second),
        [first, second, third] => is_alpha(first) && is_alpha(second) && is_signed_digits(third),
        _ => false,
    }
}
