/// A decoded HTML entity and the number of source bytes it consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedHtmlEntity {
    pub text: String,
    pub length: usize,
}

/// Decode the entity name that follows `&` and precedes `;`.
///
/// Supports the five named entities and numeric character references. Returns
/// `None` for unknown names, malformed numbers, and code points outside the
/// Unicode scalar range. Lone surrogate references are not representable in a
/// Rust `String` and therefore also return `None`.
pub fn decode_html_entity(entity: &str) -> Option<String> {
    match entity {
        "amp" => return Some("&".to_owned()),
        "lt" => return Some("<".to_owned()),
        "gt" => return Some(">".to_owned()),
        "quot" => return Some("\"".to_owned()),
        "apos" => return Some("'".to_owned()),
        _ => {}
    }

    if let Some(digits) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
        return decode_code_point(parse_int(digits, 16)?);
    }

    if let Some(digits) = entity.strip_prefix('#') {
        return decode_code_point(parse_int(digits, 10)?);
    }

    None
}

/// Decode an entity starting at `html[index]` when it is a `&...;` reference.
pub fn decode_html_entity_at(html: &str, index: usize) -> Option<DecodedHtmlEntity> {
    let remainder = html.get(index..)?;
    if !remainder.starts_with('&') {
        return None;
    }

    let semicolon_offset = remainder.get(1..)?.find(';')?;
    if semicolon_offset + 1 > 16 {
        return None;
    }

    let entity = remainder.get(1..=semicolon_offset)?;
    let text = decode_html_entity(entity)?;
    Some(DecodedHtmlEntity {
        text,
        length: semicolon_offset + 2,
    })
}

fn decode_code_point(code_point: i64) -> Option<String> {
    let scalar = u32::try_from(code_point).ok()?;
    char::from_u32(scalar).map(|value| value.to_string())
}

/// Port of `Number.parseInt(text, radix)` for an integer or `NaN` result.
fn parse_int(text: &str, radix: u32) -> Option<i64> {
    let trimmed = text.trim_start_matches(|value: char| value.is_whitespace());
    let (negative, unsigned) = match trimmed.as_bytes().first() {
        Some(b'-') => (true, &trimmed[1..]),
        Some(b'+') => (false, &trimmed[1..]),
        _ => (false, trimmed),
    };

    let digits = strip_radix_prefix(unsigned, radix);
    let mut value: i64 = 0;
    let mut matched = false;
    for character in digits.chars() {
        let Some(digit) = character.to_digit(radix) else {
            break;
        };
        value = value.saturating_mul(i64::from(radix)).saturating_add(i64::from(digit));
        matched = true;
    }

    if !matched {
        return None;
    }
    Some(if negative { -value } else { value })
}

fn strip_radix_prefix(text: &str, radix: u32) -> &str {
    if radix != 16 {
        return text;
    }
    text.strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text)
}
