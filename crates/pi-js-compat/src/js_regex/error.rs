//! Single authority for native RegExp diagnostics. Reason wording may differ
//! from V8 (AGENTS.md, approved 2026-10-22); pattern/flags retain exact units.

use crate::js_json::{JsString, encode_node_utf8};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsRegexError {
    message: JsString,
}

impl JsRegexError {
    pub fn message(&self) -> &JsString {
        &self.message
    }

    pub(super) fn from_parser(pattern: &JsString, flags: &str, diagnostic: &str) -> Self {
        let reason = parser_reason(pattern, flags, diagnostic);
        Self {
            message: JsString::from("Invalid regular expression: /") + pattern + "/" + flags + ": " + reason,
        }
    }

    fn invalid_flags(flags: &str) -> Self {
        Self {
            message: JsString::from("Invalid flags supplied to RegExp constructor '") + flags + "'",
        }
    }
}

impl std::fmt::Display for JsRegexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}
impl std::error::Error for JsRegexError {}

pub(super) fn validate_flags(flags: &str) -> Result<(), JsRegexError> {
    let mut seen = 0u16;
    for flag in flags.chars() {
        let Some(index) = "dgimsuvy".find(flag) else {
            return Err(JsRegexError::invalid_flags(flags));
        };
        let bit = 1 << index;
        if seen & bit != 0 {
            return Err(JsRegexError::invalid_flags(flags));
        }
        seen |= bit;
    }
    if flags.contains('u') && flags.contains('v') {
        return Err(JsRegexError::invalid_flags(flags));
    }
    Ok(())
}

/// Exact mappings retained from bwrap and independently exercised by the pinned
/// diagnostics corpus. Context-sensitive categories outside that proven subset
/// keep their engine wording under the approved reason-text deviation.
fn parser_reason<'a>(pattern: &JsString, flags: &str, diagnostic: &'a str) -> &'a str {
    let unicode = flags.contains('u') || flags.contains('v');
    match diagnostic {
        "Unbalanced bracket" => "Unterminated character class",
        "Unbalanced parenthesis"
            if inspect_regex_structure(pattern) == Some(StructuralRegexError::UnmatchedClosingGroup) =>
        {
            "Unmatched ')'"
        }
        "Unbalanced parenthesis" => "Unterminated group",
        "Invalid atom character" | "Invalid braced quantifier" if !unicode => "Nothing to repeat",
        "Quantifier not allowed here"
            if inspect_regex_structure(pattern) == Some(StructuralRegexError::QuantifiedAnchor) =>
        {
            "Nothing to repeat"
        }
        "Quantifier not allowed here" => "Invalid quantifier",
        "Invalid quantifier" if !unicode => "numbers out of order in {} quantifier",
        "Range values reversed, start char code is greater than end char code." => {
            "Range out of order in character class"
        }
        "Incomplete escape" | "Unterminated escape" => "\\ at end of pattern",
        "Invalid group modifier" if !has_modifier_group(pattern) => "Invalid group",
        "Invalid token at named capture group identifier" if pattern.contains("(?<") => "Invalid capture group name",
        "Duplicate capture group name" => "Duplicate capture group name",
        "Invalid named backreference syntax" => "Invalid named reference",
        diagnostic if diagnostic.starts_with("Backreference to invalid named capture group: ") => {
            "Invalid named capture referenced"
        }
        diagnostic => diagnostic,
    }
}

fn has_modifier_group(pattern: &JsString) -> bool {
    let prefix = [u16::from(b'('), u16::from(b'?')];
    let modifiers = [u16::from(b'i'), u16::from(b'm'), u16::from(b's'), u16::from(b'-')];
    pattern
        .units()
        .windows(3)
        .any(|units| units.starts_with(&prefix) && modifiers.contains(&units[2]))
}

#[derive(PartialEq, Eq)]
enum StructuralRegexError {
    UnmatchedClosingGroup,
    QuantifiedAnchor,
    QuantifiedLookbehind,
}

#[derive(Default)]
struct RegexStructure {
    lookbehind_groups: Vec<bool>,
    in_class: bool,
}

impl RegexStructure {
    fn inspect_token(&mut self, units: &[u16], index: usize) -> Option<StructuralRegexError> {
        let unit = units[index];
        if [u16::from(b'^'), u16::from(b'$')].contains(&unit) && starts_quantifier(units, index + 1) {
            return Some(StructuralRegexError::QuantifiedAnchor);
        }
        if unit == u16::from(b'(') {
            self.lookbehind_groups.push(is_lookbehind_open(units, index));
        }
        if unit != u16::from(b')') {
            return None;
        }
        match self.lookbehind_groups.pop() {
            None => Some(StructuralRegexError::UnmatchedClosingGroup),
            Some(true) if starts_quantifier(units, index + 1) => Some(StructuralRegexError::QuantifiedLookbehind),
            Some(_) => None,
        }
    }
}

fn is_lookbehind_open(units: &[u16], index: usize) -> bool {
    let prefix = [u16::from(b'?'), u16::from(b'<')];
    let marker = units.get(index + 3);
    units.get(index + 1..index + 3) == Some(prefix.as_slice())
        && (marker == Some(&u16::from(b'=')) || marker == Some(&u16::from(b'!')))
}

fn inspect_regex_structure(pattern: &JsString) -> Option<StructuralRegexError> {
    let units = pattern.units();
    let mut state = RegexStructure::default();
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        if unit == u16::from(b'\\') {
            let boundary = units
                .get(index + 1)
                .is_some_and(|next| [u16::from(b'b'), u16::from(b'B')].contains(next));
            if !state.in_class && boundary && starts_quantifier(units, index + 2) {
                return Some(StructuralRegexError::QuantifiedAnchor);
            }
            index += 2;
            continue;
        }
        if unit == u16::from(b'[') && !state.in_class {
            state.in_class = true;
        } else if unit == u16::from(b']') {
            state.in_class = false;
        } else if !state.in_class
            && let Some(error) = state.inspect_token(units, index)
        {
            return Some(error);
        }
        index += 1;
    }
    None
}

fn starts_quantifier(units: &[u16], index: usize) -> bool {
    let Some(unit) = units.get(index) else {
        return false;
    };
    if [u16::from(b'*'), u16::from(b'+'), u16::from(b'?')].contains(unit) {
        return true;
    }
    if *unit != u16::from(b'{') {
        return false;
    }
    let after_minimum = skip_decimal_digits(units, index + 1);
    if after_minimum == index + 1 {
        return false;
    }
    let after_range = if units.get(after_minimum) == Some(&u16::from(b',')) {
        skip_decimal_digits(units, after_minimum + 1)
    } else {
        after_minimum
    };
    units.get(after_range) == Some(&u16::from(b'}'))
}
fn skip_decimal_digits(units: &[u16], start: usize) -> usize {
    let mut position = start;
    let digits = u16::from(b'0')..=u16::from(b'9');
    while units.get(position).is_some_and(|unit| digits.contains(unit)) {
        position += 1;
    }
    position
}
