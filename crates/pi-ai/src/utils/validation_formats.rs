//! Frozen TypeBox 1.1.38 default format registry, preserving UTF-16 input.
use super::validation_idna::hostname;
use super::validation_regex::JsRegex;
use pi_js_compat::js_json::JsString;

pub(super) fn check(format: &JsString, text: &JsString) -> bool {
    let Ok(format) = format.to_utf8() else {
        return true;
    };
    match format.as_str() {
        "duration" => test(
            text,
            r####"^P((\d+Y(\d+M(\d+D)?)?|\d+M(\d+D)?|\d+D)(T(\d+H(\d+M(\d+S)?)?|\d+M(\d+S)?|\d+S))?|T(\d+H(\d+M(\d+S)?)?|\d+M(\d+S)?|\d+S)|\d+W)$"####,
            "",
        ),
        "email" => test(
            text,
            r####"^(?!.*\.\.)[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*@[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)*$"####,
            "i",
        ),
        "idn-email" => test(
            text,
            r####"^(?!.*\.\.)[\p{L}\p{N}!#$%&'*+/=?^_`{|}~-]+(?:\.[\p{L}\p{N}!#$%&'*+/=?^_`{|}~-]+)*@[\p{L}\p{N}](?:[\p{L}\p{N}-]{0,61}[\p{L}\p{N}])?(?:\.[\p{L}\p{N}](?:[\p{L}\p{N}-]{0,61}[\p{L}\p{N}])?)*$"####,
            "iu",
        ),
        "uuid" => test(
            text,
            r####"^(?:urn:uuid:)?[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$"####,
            "i",
        ),
        "uri-reference" => test(
            text,
            r####"^(?!.*[^\x00-\x7F])(?!.*\\)(?:(?:[a-z][a-z0-9+\-.]*:)?(?:\/\/[^\s[\]{}<>^`|]*)?|[^\s[\]{}<>^`|]*)(?:\?[^\s[\]{}<>^`|]*)?(?:#[^\s[\]{}<>^`|]*)?$"####,
            "i",
        ),
        "url" => test(
            text,
            r####"^(?:https?|ftp):\/\/(?:\S+(?::\S*)?@)?(?:(?!(?:10|127)(?:\.\d{1,3}){3})(?!(?:169\.254|192\.168)(?:\.\d{1,3}){2})(?!172\.(?:1[6-9]|2\d|3[0-1])(?:\.\d{1,3}){2})(?:[1-9]\d?|1\d\d|2[01]\d|22[0-3])(?:\.(?:1?\d{1,2}|2[0-4]\d|25[0-5])){2}(?:\.(?:[1-9]\d?|1\d\d|2[0-4]\d|25[0-4]))|(?:(?:[a-z0-9\u{00a1}-\u{ffff}]+-)*[a-z0-9\u{00a1}-\u{ffff}]+)(?:\.(?:[a-z0-9\u{00a1}-\u{ffff}]+-)*[a-z0-9\u{00a1}-\u{ffff}]+)*(?:\.(?:[a-z\u{00a1}-\u{ffff}]{2,})))(?::\d{2,5})?(?:\/[^\s]*)?$"####,
            "iu",
        ),
        "uri-template" => test(
            text,
            r####"^(?:(?:[^\x00-\x20"'<>%\\^`{|}]|%[0-9a-f]{2})|\{[+#./;?&=,!@|]?(?:[a-z0-9_]|%[0-9a-f]{2})+(?::[1-9][0-9]{0,3}|\*)?(?:,(?:[a-z0-9_]|%[0-9a-f]{2})+(?::[1-9][0-9]{0,3}|\*)?)*\})*$"####,
            "i",
        ),
        "json-pointer" => test(text, r####"^(?:\/(?:[^~/]|~0|~1)*)*$"####, ""),
        "json-pointer-uri-fragment" => test(
            text,
            r####"^#(?:\/(?:[a-z0-9_\-.!$&'()*+,;:=@]|%[0-9a-f]{2}|~0|~1)*)*$"####,
            "i",
        ),
        "relative-json-pointer" => test(text, r####"^(?:0|[1-9][0-9]*)(?:#|(?:\/(?:[^~/]|~0|~1)*)*)$"####, ""),
        "date" => date(text),
        "time" => time(text),
        "date-time" => date_time(text),
        "regex" => !text.is_empty() && JsRegex::new(text).is_ok(),
        "ipv4" => ipv4(text.units()),
        "ipv6" => ipv6(text.units()),
        "hostname" => hostname(text, false),
        "idn-hostname" => hostname(text, true),
        "uri" => uri(text.units()),
        "iri" => parse_url(text, false),
        "iri-reference" => iri_reference(text),
        _ => true,
    }
}

fn test(text: &JsString, pattern: &str, flags: &str) -> bool {
    JsRegex::with_flags(pattern, flags)
        .expect("frozen format pattern")
        .is_match(text)
}

fn ascii_digit(unit: u16) -> bool {
    (u16::from(b'0')..=u16::from(b'9')).contains(&unit)
}
fn ascii_alpha(unit: u16) -> bool {
    (u16::from(b'a')..=u16::from(b'z')).contains(&unit) || (u16::from(b'A')..=u16::from(b'Z')).contains(&unit)
}
fn ascii_hex(unit: u16) -> bool {
    ascii_digit(unit) || (65..=70).contains(&unit) || (97..=102).contains(&unit)
}
fn digits(units: &[u16]) -> u32 {
    units.iter().fold(0, |number, unit| number * 10 + u32::from(*unit - 48))
}

fn date(text: &JsString) -> bool {
    if !test(text, r"^(\d\d\d\d)-(\d\d)-(\d\d)$", "") {
        return false;
    }
    let units = text.units();
    let year = digits(&units[..4]);
    let month = digits(&units[5..7]) as usize;
    let day = digits(&units[8..10]);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        0,
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    (1..=12).contains(&month) && day >= 1 && day <= days[month]
}

fn time(text: &JsString) -> bool {
    if !test(text, r"^(\d\d):(\d\d):(\d\d(?:\.\d+)?)(?:Z|([+-])(\d\d):(\d\d))?$", "i") {
        return false;
    }
    let units = text.units();
    let hr = digits(&units[..2]) as i32;
    let min = digits(&units[3..5]) as i32;
    let suffix = units.iter().position(|unit| matches!(unit, 90 | 122 | 43 | 45));
    let Some(suffix) = suffix else {
        return false;
    };
    let sec = pi_js_compat::number::parse_number(&String::from_utf16(&units[6..suffix]).expect("ASCII seconds"));
    let (sign, tz_h, tz_m) = if matches!(units[suffix], 90 | 122) {
        (1, 0, 0)
    } else {
        (
            if units[suffix] == 45 { -1 } else { 1 },
            digits(&units[suffix + 1..suffix + 3]) as i32,
            digits(&units[suffix + 4..suffix + 6]) as i32,
        )
    };
    if tz_h > 23 || tz_m > 59 {
        return false;
    }
    if hr <= 23 && min <= 59 && sec < 60.0 {
        return true;
    }
    let utc_min = min - tz_m * sign;
    let utc_hr = hr - tz_h * sign - i32::from(utc_min < 0);
    matches!(utc_hr, 23 | -1) && matches!(utc_min, 59 | -1) && sec < 61.0
}

fn date_time(text: &JsString) -> bool {
    let parts: Vec<_> = text.units().split(|unit| matches!(unit, 84 | 116)).collect();
    parts.len() == 2 && date(&JsString::from_utf16(parts[0].to_vec())) && time(&JsString::from_utf16(parts[1].to_vec()))
}

fn ipv4(units: &[u16]) -> bool {
    let parts: Vec<_> = units.split(|unit| *unit == 46).collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.iter().all(|unit| ascii_digit(*unit))
                && !(part.len() > 1 && part[0] == 48)
                && digits(part) <= 255
        })
}

fn ipv6(units: &[u16]) -> bool {
    if units.is_empty() {
        return false;
    }
    let mut groups = 0;
    let mut compressed = units.starts_with(&[58, 58]);
    if compressed && units.len() == 2 {
        return true;
    }
    let mut index = if compressed { 2 } else { 0 };
    while index < units.len() {
        let start = index;
        while units.get(index).is_some_and(|unit| ascii_hex(*unit)) {
            index += 1;
        }
        if index == start {
            return false;
        }
        if units.get(index) == Some(&46) {
            if !ipv4(&units[start..]) {
                return false;
            }
            groups += 2;
            break;
        }
        if index - start > 4 {
            return false;
        }
        groups += 1;
        if index == units.len() {
            break;
        }
        if units[index] != 58 {
            return false;
        }
        index += 1;
        if units.get(index) == Some(&58) {
            if compressed || units.get(index + 1) == Some(&58) {
                return false;
            }
            compressed = true;
            index += 1;
        }
    }
    if compressed { groups <= 7 } else { groups == 8 }
}

// WHATWG URL first converts its string input to USVString. This is frozen
// format behavior only; caller values and validation output stay untouched.
fn parse_url(text: &JsString, relative: bool) -> bool {
    let text = pi_js_compat::js_json::encode_node_utf8(text);
    if relative {
        url::Url::options()
            .base_url(Some(&url::Url::parse("http://example.com").expect("fixed URL")))
            .parse(&text)
            .is_ok()
    } else {
        url::Url::parse(&text).is_ok()
    }
}

fn iri_reference(text: &JsString) -> bool {
    if test(text, r"[ \\\x00-\x1F\x7F]", "") || test(text, r"%(?![0-9a-fA-F]{2})", "") {
        return false;
    }
    let units = text.units();
    let colon = units.iter().position(|unit| *unit == 58);
    let valid_scheme = colon.is_some_and(|index| {
        index > 0
            && test(
                &JsString::from_utf16(units[..index].to_vec()),
                r"^[a-zA-Z][a-zA-Z0-9+\-.]*$",
                "",
            )
    });
    if !valid_scheme && colon.is_none() && test(text, r"^([a-zA-Z][a-zA-Z0-9+\-.]*)(\/\/)", "") {
        return false;
    }
    parse_url(text, true)
}

fn is_unreserved(unit: u16) -> bool {
    ascii_alpha(unit) || ascii_digit(unit) || matches!(unit, 45 | 46 | 95 | 126)
}
fn is_sub_delimiter(unit: u16) -> bool {
    matches!(unit, 33 | 36 | 38 | 39 | 40 | 41 | 42 | 43 | 44 | 59 | 61)
}
fn delimiter(unit: u16) -> bool {
    matches!(unit, 47 | 63 | 35)
}

fn valid_userinfo(units: &[u16]) -> bool {
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        if unit == 37 {
            if !percent_encoded(units, index) {
                return false;
            }
            index += 3;
            continue;
        }
        if !is_unreserved(unit) && !is_sub_delimiter(unit) && unit != 58 {
            return false;
        }
        index += 1;
    }
    true
}
fn percent_encoded(units: &[u16], index: usize) -> bool {
    units.get(index + 1).is_some_and(|unit| ascii_hex(*unit))
        && units.get(index + 2).is_some_and(|unit| ascii_hex(*unit))
}

fn authority_end(units: &[u16], mut index: usize) -> Option<usize> {
    let start = index;
    let authority = &units[start
        ..units[start..]
            .iter()
            .position(|unit| delimiter(*unit))
            .map(|offset| start + offset)
            .unwrap_or(units.len())];
    if let Some(at) = authority.iter().position(|unit| *unit == 64) {
        if !valid_userinfo(&authority[..at]) {
            return None;
        }
        index = start + at + 1;
    }
    if units.get(index) == Some(&91) {
        index += 1;
        index += units[index..].iter().position(|unit| *unit == 93)? + 1;
    } else {
        while let Some(unit) = units.get(index).copied() {
            if delimiter(unit) || unit == 58 {
                break;
            }
            if unit < 128 && !is_unreserved(unit) && !is_sub_delimiter(unit) {
                return None;
            }
            index += 1;
        }
    }
    if units.get(index) == Some(&58) {
        index += 1;
        while let Some(unit) = units.get(index).copied() {
            if delimiter(unit) {
                break;
            }
            if !ascii_digit(unit) {
                return None;
            }
            index += 1;
        }
    }
    Some(index)
}

fn uri(units: &[u16]) -> bool {
    if !units.first().is_some_and(|unit| ascii_alpha(*unit)) {
        return false;
    }
    let Some(colon) = units.iter().position(|unit| *unit == 58) else {
        return false;
    };
    if !units[1..colon]
        .iter()
        .all(|unit| ascii_alpha(*unit) || ascii_digit(*unit) || matches!(unit, 43 | 45 | 46))
    {
        return false;
    }
    let mut index = colon + 1;
    if units[index..].starts_with(&[47, 47]) {
        let Some(end) = authority_end(units, index + 2) else {
            return false;
        };
        index = end;
    }
    while index < units.len() {
        let unit = units[index];
        if unit == 37 {
            if !percent_encoded(units, index) {
                return false;
            }
            index += 3;
            continue;
        }
        if unit > 127 || !(is_unreserved(unit) || is_sub_delimiter(unit) || matches!(unit, 58 | 64 | 47 | 63 | 35)) {
            return false;
        }
        index += 1;
    }
    true
}
