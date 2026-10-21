//! Frozen TypeBox IDNA guards (intentionally not a general-purpose IDNA implementation).
use super::validation_regex::JsRegex;
use pi_js_compat::js_json::JsString;

pub(super) fn hostname(text: &JsString, international: bool) -> bool {
    if text.is_empty() || (international && text.contains(" ")) {
        return false;
    }
    let text = if international {
        canonicalize(text)
    } else {
        text.clone()
    };
    if text.len() > 253 {
        return false;
    }
    text.split('.').all(|label| label_valid(&label, international))
}

// JS normalize preserves unmatched surrogates. Normalize only valid Unicode runs.
fn canonicalize(text: &JsString) -> JsString {
    let normalizer = icu_normalizer::ComposingNormalizer::new_nfc();
    let mut result = JsString::default();
    let mut run = String::new();
    for point in char::decode_utf16(text.units().iter().copied()) {
        match point {
            Ok(character) => run.push(if matches!(character, '\u{3002}' | '\u{ff0e}' | '\u{ff61}') {
                '.'
            } else {
                character
            }),
            Err(error) => {
                result.push_str(normalizer.normalize(&run).as_ref());
                run.clear();
                result.push_str(JsString::from_utf16(vec![error.unpaired_surrogate()]));
            }
        }
    }
    result.push_str(normalizer.normalize(&run).as_ref());
    result
}

fn label_valid(label: &JsString, international: bool) -> bool {
    if label.is_empty() || label.len() > 63 {
        return false;
    }
    let units = label.units();
    let puny = units.len() >= 4
        && units[..4]
            .iter()
            .zip([120, 110, 45, 45])
            .all(|(unit, expected)| (*unit | 32) == expected);
    if puny {
        return decode_punycode(&units[4..]).is_some_and(|decoded| unicode_label(&decoded));
    }
    if international {
        return unicode_label(label);
    }
    if units.first() == Some(&45) || units.last() == Some(&45) || units.get(2..4) == Some(&[45, 45]) {
        return false;
    }
    units.iter().all(|unit| matches!(unit,65..=90|97..=122|48..=57|45))
}

fn property(point: u32, pattern: &str) -> bool {
    let text = if let Some(character) = char::from_u32(point) {
        JsString::from(character)
    } else {
        JsString::from_utf16(vec![point as u16])
    };
    JsRegex::unicode(pattern)
        .expect("Unicode property pattern")
        .is_match(text)
}

fn unicode_label(label: &JsString) -> bool {
    let points: Vec<_> = char::decode_utf16(label.units().iter().copied())
        .map(|point| {
            point
                .map(u32::from)
                .unwrap_or_else(|error| u32::from(error.unpaired_surrogate()))
        })
        .collect();
    if points.is_empty()
        || points.first() == Some(&45)
        || points.last() == Some(&45)
        || points.get(2..4) == Some(&[45, 45])
    {
        return false;
    }
    if property(points[0], r"[\p{Mn}\p{Mc}\p{Me}]") {
        return false;
    }
    let mut japanese = false;
    let mut arabic = false;
    let mut extended_arabic = false;
    for (index, point) in points.iter().copied().enumerate() {
        if matches!(point, 0x0640 | 0x07fa | 0x302e | 0x302f | 0x3031..=0x3035 | 0x303b) {
            return false;
        }
        japanese |= property(point, r"[\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Han}]");
        arabic |= (0x0660..=0x0669).contains(&point);
        extended_arabic |= (0x06f0..=0x06f9).contains(&point);
        let previous = index.checked_sub(1).and_then(|previous| points.get(previous)).copied();
        let next = points.get(index + 1).copied();
        let valid_context = match point {
            0x00b7 => previous == Some(0x006c) && next == Some(0x006c),
            0x0375 => next.is_some_and(|point| property(point, r"\p{Script=Greek}")),
            0x05f3 | 0x05f4 => previous.is_some_and(|point| property(point, r"\p{Script=Hebrew}")),
            0x200d => previous.is_some_and(is_virama),
            _ => true,
        };
        if !valid_context {
            return false;
        }
    }
    (!points.contains(&0x30fb) || japanese) && !(arabic && extended_arabic)
}

fn is_virama(point: u32) -> bool {
    matches!(
        point,
        0x094d
            | 0x09cd
            | 0x0a4d
            | 0x0acd
            | 0x0b4d
            | 0x0bcd
            | 0x0c4d
            | 0x0ccd
            | 0x0d3b
            | 0x0d3c
            | 0x0d4d
            | 0x0dca
            | 0x1b44
            | 0x1baa
            | 0x1bab
            | 0xa9c0
            | 0x11046
            | 0x1107f
            | 0x110b9
            | 0x11133
            | 0x11134
            | 0x111c0
            | 0x11235
            | 0x1134d
            | 0x11442
            | 0x114c2
            | 0x115bf
            | 0x1163f
            | 0x116b6
            | 0x11c3f
            | 0x11d44
            | 0x11d45
    )
}

fn adapt_bias(mut delta: u64, points: u64, first: bool) -> u64 {
    delta = if first { delta / 700 } else { delta >> 1 };
    delta += delta / points;
    let mut bias = 0;
    while delta > 455 {
        delta /= 35;
        bias += 36;
    }
    bias + 36 * delta / (delta + 38)
}

fn decode_digit(unit: u16) -> Option<u64> {
    match unit {
        97..=122 => Some(u64::from(unit - 97)),
        65..=90 => Some(u64::from(unit - 65)),
        48..=57 => Some(u64::from(unit - 48 + 26)),
        _ => None,
    }
}

fn decode_punycode(units: &[u16]) -> Option<JsString> {
    let delimiter = units.iter().rposition(|unit| *unit == 45);
    let mut output: Vec<u32> = match delimiter {
        Some(index) if index > 0 => units[..index].iter().map(|unit| u32::from(*unit)).collect(),
        _ => Vec::new(),
    };
    if output.iter().any(|point| *point >= 128) {
        return None;
    }
    let mut position = delimiter.map(|index| index + 1).unwrap_or(0);
    let (mut n, mut index, mut bias) = (128u64, 0u64, 72u64);
    while position < units.len() {
        let old = index;
        let (mut weight, mut k) = (1u64, 36u64);
        loop {
            let digit = decode_digit(*units.get(position)?)?;
            position += 1;
            index = index.checked_add(digit.checked_mul(weight)?)?;
            let threshold = if k <= bias {
                1
            } else if k >= bias + 26 {
                26
            } else {
                k - bias
            };
            if digit < threshold {
                break;
            }
            weight = weight.checked_mul(36 - threshold)?;
            k += 36;
        }
        let count = output.len() as u64 + 1;
        bias = adapt_bias(index - old, count, old == 0);
        n = n.checked_add(index / count)?;
        if n > 0x10ffff {
            return None;
        }
        index %= count;
        output.insert(index as usize, n as u32);
        index += 1;
    }
    let mut result = JsString::default();
    for point in output {
        if let Some(character) = char::from_u32(point) {
            result.push_str(character);
        } else {
            result.push_str(JsString::from_utf16(vec![point as u16]));
        }
    }
    Some(result)
}
