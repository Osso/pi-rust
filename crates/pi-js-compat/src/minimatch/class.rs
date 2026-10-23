//! POSIX character class parsing (`brace-expressions.ts`).
use super::units::*;
use crate::js_json::JsString;

use super::escape::{brace_escape, regexp_escape};

const POSIX_CLASSES: &[(&str, &str, bool, bool)] = &[
    ("[:alnum:]", "\\p{L}\\p{Nl}\\p{Nd}", true, false),
    ("[:alpha:]", "\\p{L}\\p{Nl}", true, false),
    ("[:ascii:]", "\\x00-\\x7f", false, false),
    ("[:blank:]", "\\p{Zs}\\t", true, false),
    ("[:cntrl:]", "\\p{Cc}", true, false),
    ("[:digit:]", "\\p{Nd}", true, false),
    ("[:graph:]", "\\p{Z}\\p{C}", true, true),
    ("[:lower:]", "\\p{Ll}", true, false),
    ("[:print:]", "\\p{C}", true, true),
    ("[:punct:]", "\\p{P}", true, false),
    ("[:space:]", "\\p{Z}\\t\\r\\n\\v\\f", true, false),
    ("[:upper:]", "\\p{Lu}", true, false),
    ("[:word:]", "\\p{L}\\p{Nl}\\p{Nd}\\p{Pc}", true, false),
    ("[:xdigit:]", "A-Fa-f0-9", false, false),
];

pub(crate) struct ParsedClass {
    pub source: JsString,
    pub needs_uflag: bool,
    pub consumed: usize,
    pub magic: bool,
}

pub(crate) fn parse_class(glob: &[u16], position: usize) -> ParsedClass {
    debug_assert_eq!(glob.get(position), Some(&OPEN_BRACKET));
    let mut ranges: Vec<JsString> = Vec::new();
    let mut negs: Vec<JsString> = Vec::new();
    let mut index = position + 1;
    let mut saw_start = false;
    let mut uflag = false;
    let mut escaping = false;
    let mut negate = false;
    let mut end_position = position;
    let mut range_start: Option<u16> = None;

    while index < glob.len() {
        let character = glob[index];
        if (character == BANG || character == CARET) && index == position + 1 {
            negate = true;
            index += 1;
            continue;
        }
        if character == CLOSE_BRACKET && saw_start && !escaping {
            end_position = index + 1;
            break;
        }
        saw_start = true;
        if character == BACKSLASH && !escaping {
            escaping = true;
            index += 1;
            continue;
        }
        if character == OPEN_BRACKET && !escaping {
            let mut matched = false;
            for (name, replacement, needs_uflag, negated) in POSIX_CLASSES {
                let name_chars: Vec<u16> = name.encode_utf16().collect();
                if glob[index..].starts_with(&name_chars) {
                    if range_start.is_some() {
                        return ParsedClass {
                            source: JsString::from("$."),
                            needs_uflag: false,
                            consumed: glob.len() - position,
                            magic: true,
                        };
                    }
                    index += name_chars.len();
                    if *negated {
                        negs.push(JsString::from(*replacement));
                    } else {
                        ranges.push(JsString::from(*replacement));
                    }
                    uflag = uflag || *needs_uflag;
                    matched = true;
                    break;
                }
            }
            if matched {
                continue;
            }
        }
        escaping = false;
        if let Some(start) = range_start {
            if character > start {
                ranges.push(JsString::join(
                    [brace_escape(start), JsString::from("-"), brace_escape(character)],
                    "",
                ));
            } else if character == start {
                ranges.push(brace_escape(character));
            }
            range_start = None;
            index += 1;
            continue;
        }
        if glob.get(index + 1) == Some(&DASH) && glob.get(index + 2) == Some(&CLOSE_BRACKET) {
            ranges.push(JsString::join([brace_escape(character), JsString::from("-")], ""));
            index += 2;
            continue;
        }
        if glob.get(index + 1) == Some(&DASH) {
            range_start = Some(character);
            index += 2;
            continue;
        }
        ranges.push(brace_escape(character));
        index += 1;
    }

    if end_position < index {
        return ParsedClass {
            source: JsString::default(),
            needs_uflag: false,
            consumed: 0,
            magic: false,
        };
    }
    if ranges.is_empty() && negs.is_empty() {
        return ParsedClass {
            source: JsString::from("$."),
            needs_uflag: false,
            consumed: glob.len() - position,
            magic: true,
        };
    }
    if negs.is_empty() && ranges.len() == 1 && !negate {
        let range: Vec<u16> = ranges[0].units().to_vec();
        let single = match range.as_slice() {
            [single] => Some(*single),
            [BACKSLASH, single] => Some(*single),
            _ => None,
        };
        if let Some(single) = single {
            return ParsedClass {
                source: regexp_escape(&JsString::from_utf16(vec![single])),
                needs_uflag: false,
                consumed: end_position - position,
                magic: false,
            };
        }
    }

    let joined_ranges = JsString::join(ranges.iter(), "");
    let joined_negs = JsString::join(negs.iter(), "");
    let sranges = JsString::join(
        [
            JsString::from("["),
            JsString::from(if negate { "^" } else { "" }),
            joined_ranges,
            JsString::from("]"),
        ],
        "",
    );
    let snegs = JsString::join(
        [
            JsString::from("["),
            JsString::from(if negate { "" } else { "^" }),
            joined_negs,
            JsString::from("]"),
        ],
        "",
    );
    let combined = if !ranges.is_empty() && !negs.is_empty() {
        JsString::join(
            [
                JsString::from("("),
                sranges,
                JsString::from("|"),
                snegs,
                JsString::from(")"),
            ],
            "",
        )
    } else if !ranges.is_empty() {
        sranges
    } else {
        snegs
    };
    ParsedClass {
        source: combined,
        needs_uflag: uflag,
        consumed: end_position - position,
        magic: true,
    }
}
