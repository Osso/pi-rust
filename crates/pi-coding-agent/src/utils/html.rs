use pi_js_compat::js_json::JsString;

/// A decoded HTML entity and its source length in UTF-16 code units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedHtmlEntity {
    pub text: JsString,
    pub length: usize,
}

/// Frozen `decodeHtmlEntity`: named entities or Number.parseInt numeric prefixes.
pub fn decode_html_entity(entity: &JsString) -> Option<JsString> {
    for (name, decoded) in [("amp", "&"), ("lt", "<"), ("gt", ">"), ("quot", "\""), ("apos", "'")] {
        if entity == name {
            return Some(decoded.into());
        }
    }
    let (digits, radix) = if entity.starts_with("#x") || entity.starts_with("#X") {
        (entity.slice(2, entity.len()), 16)
    } else {
        (entity.strip_prefix("#")?, 10)
    };
    decode_code_point(parse_int(&digits, radix)?)
}

/// Mirrors the source's direct indexed decoder, including its 16-unit bound.
pub fn decode_html_entity_at(html: &JsString, index: usize) -> Option<DecodedHtmlEntity> {
    let semicolon = html.find_from(";", index.saturating_add(1))?;
    if semicolon.saturating_sub(index) > 16 {
        return None;
    }
    let entity = html.slice(index.saturating_add(1), semicolon);
    Some(DecodedHtmlEntity {
        text: decode_html_entity(&entity)?,
        length: semicolon - index + 1,
    })
}

fn decode_code_point(code_point: i64) -> Option<JsString> {
    let point = u32::try_from(code_point).ok().filter(|point| *point <= 0x10ffff)?;
    let units = if point <= 0xffff {
        vec![point as u16]
    } else {
        let offset = point - 0x10000;
        vec![0xd800 + (offset >> 10) as u16, 0xdc00 + (offset & 0x3ff) as u16]
    };
    Some(JsString::from_utf16(units))
}

fn parse_int(text: &JsString, radix: u32) -> Option<i64> {
    let trimmed = text.trim_start();
    let (negative, unsigned) = if let Some(unsigned) = trimmed.strip_prefix("-") {
        (true, unsigned)
    } else {
        (false, trimmed.strip_prefix("+").unwrap_or(trimmed))
    };
    let digits = if radix == 16 {
        unsigned
            .strip_prefix("0x")
            .or_else(|| unsigned.strip_prefix("0X"))
            .unwrap_or(unsigned)
    } else {
        unsigned
    };
    let value = parse_digits(&digits, radix)?;
    Some(if negative { -value } else { value })
}

fn parse_digits(digits: &JsString, radix: u32) -> Option<i64> {
    let mut value = 0_i64;
    let mut matched = false;
    for unit in digits.units() {
        let digit = match unit {
            48..=57 => u32::from(*unit - 48),
            65..=70 => u32::from(*unit - 65 + 10),
            97..=102 => u32::from(*unit - 97 + 10),
            _ => break,
        };
        if digit >= radix {
            break;
        }
        value = value.saturating_mul(i64::from(radix)).saturating_add(i64::from(digit));
        matched = true;
    }
    matched.then_some(value)
}
