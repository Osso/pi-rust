use std::fmt;

use pi_js_compat::js_json::{JsObject, JsString, JsValue};

const VALID_JSON_ESCAPES: [char; 9] = ['"', '\\', '/', 'b', 'f', 'n', 'r', 't', 'u'];
const JSON_WHITESPACE: &str = " \n\r\t";
const NUMBER_DELIMITERS: &str = ",]}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonParseError {
    message: String,
}

impl JsonParseError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for JsonParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JsonParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PartialJsonError {
    Partial(&'static str),
    Malformed(&'static str),
}

/// Repairs malformed JSON string literals by escaping raw control characters
/// and doubling backslashes before invalid escape characters.
pub fn repair_json(json: impl Into<JsString>) -> JsString {
    let json = json.into();
    let characters = json.units();
    let mut repaired = JsString::default();
    let mut in_string = false;
    let mut index = 0;

    while index < characters.len() {
        let character = characters[index];

        if !in_string {
            repaired.push_unit(character);
            if character == 0x22 {
                in_string = true;
            }
            index += 1;
            continue;
        }

        if character == 0x22 {
            repaired.push_unit(character);
            in_string = false;
            index += 1;
            continue;
        }

        if character == 0x5c {
            index = repair_escape(&characters, index, &mut repaired);
            continue;
        }

        if is_control_character(character) {
            repaired.push_str(&escape_control_character(character));
        } else {
            repaired.push_unit(character);
        }
        index += 1;
    }

    repaired
}

fn repair_escape(characters: &[u16], index: usize, repaired: &mut JsString) -> usize {
    let Some(&next_character) = characters.get(index + 1) else {
        repaired.push_str("\\\\");
        return index + 1;
    };

    if next_character == 0x75 {
        let unicode_digits = JsString::from_utf16(characters.iter().skip(index + 2).take(4).copied().collect());
        if unicode_digits.len() == 4
            && unicode_digits
                .units()
                .iter()
                .all(|unit| char::from_u32(u32::from(*unit)).is_some_and(|digit| digit.is_ascii_hexdigit()))
        {
            repaired.push_str("\\u");
            repaired.push_str(&unicode_digits);
            return index + 6;
        }
    }

    if VALID_JSON_ESCAPES.iter().any(|known| *known as u16 == next_character) {
        repaired.push_unit(0x5c);
        repaired.push_unit(next_character);
        return index + 2;
    }

    repaired.push_str("\\\\");
    index + 1
}

fn is_control_character(character: u16) -> bool {
    (character as u32) <= 0x1f
}

fn escape_control_character(character: u16) -> String {
    match character {
        0x08 => "\\b".to_owned(),
        0x0c => "\\f".to_owned(),
        0x0a => "\\n".to_owned(),
        0x0d => "\\r".to_owned(),
        0x09 => "\\t".to_owned(),
        other => format!("\\u{:04x}", other as u32),
    }
}

pub fn parse_json_with_repair(json: impl Into<JsString>) -> Result<JsValue, JsonParseError> {
    let json = json.into();
    match parse_strict_json(&json) {
        Ok(value) => Ok(value),
        Err(error) => {
            let repaired_json = repair_json(&json);
            if repaired_json == json {
                return Err(error);
            }
            parse_strict_json(&repaired_json)
        }
    }
}

pub fn parse_streaming_json(partial_json: Option<&JsString>) -> JsValue {
    let Some(partial_json) = partial_json else {
        return JsValue::Object(JsObject::new());
    };
    if partial_json.trim().is_empty() {
        return JsValue::Object(JsObject::new());
    }

    if let Ok(value) = parse_json_with_repair(partial_json) {
        return value;
    }

    if let Ok(value) = partial_parse(partial_json) {
        return convert_nullish_result(partial_json, value);
    }

    if let Ok(value) = partial_parse(&repair_json(partial_json)) {
        return convert_nullish_result(partial_json, value);
    }

    JsValue::Object(JsObject::new())
}

fn convert_nullish_result(original: &JsString, value: JsValue) -> JsValue {
    if value.is_null() && !is_non_finite_literal(original) {
        JsValue::Object(JsObject::new())
    } else {
        value
    }
}

fn is_non_finite_literal(text: &JsString) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("Inf") || trimmed.starts_with("-Inf") || trimmed.starts_with("NaN")
}

fn parse_strict_json(text: &JsString) -> Result<JsValue, JsonParseError> {
    match pi_js_compat::js_json::parse_lossless(text) {
        Ok(value) => Ok(value),
        Err(error) => {
            if is_non_finite_json_number(text) {
                Ok(JsValue::Null)
            } else {
                Err(JsonParseError {
                    message: error.to_string(),
                })
            }
        }
    }
}

fn parse_json_value(text: &JsString) -> Option<JsValue> {
    match pi_js_compat::js_json::parse_lossless(text) {
        Ok(value) => Some(value),
        Err(_) if is_non_finite_json_number(text) => Some(JsValue::Null),
        Err(_) => None,
    }
}

/// JavaScript `JSON.parse` returns non-finite numbers for overflowing literals;
/// JSON serialization of those values is `null`, which is the Rust representation.
fn is_non_finite_json_number(text: &JsString) -> bool {
    let Ok(trimmed) = text.trim().to_utf8() else {
        return false;
    };
    let looks_numeric = trimmed
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit() || first == '-')
        && trimmed
            .chars()
            .all(|character| character.is_ascii_digit() || "+-eE.".contains(character));
    looks_numeric && trimmed.parse::<f64>().is_ok_and(|value| !value.is_finite())
}

fn parse_json_string(text: &JsString) -> Option<JsString> {
    match pi_js_compat::js_json::parse_lossless(text).ok()? {
        JsValue::String(text) => Some(text),
        _ => None,
    }
}

fn partial_parse(text: &JsString) -> Result<JsValue, PartialJsonError> {
    let characters = text.trim().units().to_vec();
    if characters.is_empty() {
        return Err(PartialJsonError::Partial("input is empty"));
    }
    PartialParser {
        characters: &characters,
        index: 0,
    }
    .parse_any()
}

struct PartialParser<'a> {
    characters: &'a [u16],
    index: usize,
}

impl PartialParser<'_> {
    fn current(&self) -> Option<u16> {
        self.characters.get(self.index).copied()
    }

    fn skip_blank(&mut self) {
        while self
            .current()
            .is_some_and(|character| JSON_WHITESPACE.encode_utf16().any(|unit| unit == character))
        {
            self.index += 1;
        }
    }

    fn parse_any(&mut self) -> Result<JsValue, PartialJsonError> {
        self.skip_blank();
        match self.current() {
            None => Err(PartialJsonError::Partial("unexpected end of input")),
            Some(0x22) => self.parse_string(),
            Some(0x7b) => self.parse_object(),
            Some(0x5b) => self.parse_array(),
            Some(_) => self.parse_literal_or_number(),
        }
    }

    fn parse_literal_or_number(&mut self) -> Result<JsValue, PartialJsonError> {
        let literals = [
            ("null", JsValue::Null, 1),
            ("true", JsValue::Bool(true), 1),
            ("false", JsValue::Bool(false), 1),
            ("Infinity", JsValue::Null, 1),
            ("-Infinity", JsValue::Null, 2),
            ("NaN", JsValue::Null, 1),
        ];
        for (literal, value, minimum_remaining) in literals {
            if let Some(value) = self.take_literal(literal, value, minimum_remaining) {
                return Ok(value);
            }
        }
        self.parse_number()
    }

    fn take_literal(&mut self, literal: &str, value: JsValue, minimum_remaining: usize) -> Option<JsValue> {
        let remaining = JsString::from_utf16(self.characters[self.index..].to_vec());
        let exact = remaining == literal;
        let partial = remaining.len() >= minimum_remaining
            && remaining.len() < literal.chars().count()
            && JsString::from(literal).starts_with(&remaining);
        if exact || partial {
            self.index = self.characters.len();
            return Some(value);
        }
        None
    }

    fn parse_string(&mut self) -> Result<JsValue, PartialJsonError> {
        let start = self.index;
        let mut escape = false;
        self.index += 1;

        while self.index < self.characters.len()
            && (self.characters[self.index] != 0x22 || (escape && self.characters[self.index - 1] == 0x5c))
        {
            escape = if self.characters[self.index] == 0x5c {
                !escape
            } else {
                false
            };
            self.index += 1;
        }

        if self.current() == Some(0x22) {
            let next_index = self.index + 1;
            let end = next_index - usize::from(escape);
            let fragment = JsString::from_utf16(self.characters[start..end].to_vec());
            self.index = next_index;
            return parse_json_string(&fragment)
                .map(JsValue::String)
                .ok_or(PartialJsonError::Malformed("invalid string literal"));
        }

        self.parse_unterminated_string(start, escape)
    }

    fn parse_unterminated_string(&mut self, start: usize, escape: bool) -> Result<JsValue, PartialJsonError> {
        let end = self.index - usize::from(escape);
        let mut fragment = JsString::from_utf16(self.characters[start..end].to_vec());
        fragment.push_unit(0x22);
        if let Some(value) = parse_json_string(&fragment) {
            return Ok(JsValue::String(value));
        }

        let last_backslash = self
            .characters
            .iter()
            .rposition(|character| *character == 0x5c)
            .map_or(-1, |position| position as isize);
        let mut fallback = js_substring(self.characters, start as isize, last_backslash);
        fallback.push_unit(0x22);
        parse_json_string(&fallback)
            .map(JsValue::String)
            .ok_or(PartialJsonError::Malformed("unterminated string literal"))
    }

    fn parse_object(&mut self) -> Result<JsValue, PartialJsonError> {
        self.index += 1;
        self.skip_blank();
        let mut object = JsObject::new();

        loop {
            if self.current() == Some(0x7d) {
                self.index += 1;
                return Ok(JsValue::Object(object));
            }
            self.skip_blank();
            if self.index >= self.characters.len() {
                return Ok(JsValue::Object(object));
            }

            let Ok(JsValue::String(key)) = self.parse_string() else {
                return Ok(JsValue::Object(object));
            };
            self.skip_blank();
            self.index += 1;
            match self.parse_any() {
                Ok(value) => {
                    object.insert(key, value);
                }
                Err(_) => return Ok(JsValue::Object(object)),
            }
            self.skip_blank();
            if self.current() == Some(0x2c) {
                self.index += 1;
            }
        }
    }

    fn parse_array(&mut self) -> Result<JsValue, PartialJsonError> {
        self.index += 1;
        let mut array = Vec::new();

        loop {
            if self.current() == Some(0x5d) {
                self.index += 1;
                return Ok(JsValue::Array(array));
            }
            match self.parse_any() {
                Ok(value) => array.push(value),
                Err(_) => return Ok(JsValue::Array(array)),
            }
            self.skip_blank();
            if self.current() == Some(0x2c) {
                self.index += 1;
            }
        }
    }

    fn parse_number(&mut self) -> Result<JsValue, PartialJsonError> {
        if self.index == 0 {
            return self.parse_whole_input_number();
        }

        let start = self.index;
        if self.current() == Some(0x2d) {
            self.index += 1;
        }
        while let Some(character) = self.current() {
            if NUMBER_DELIMITERS.encode_utf16().any(|unit| unit == character) {
                break;
            }
            self.index += 1;
        }

        let fragment = JsString::from_utf16(self.characters[start..self.index].to_vec());
        if let Some(value) = parse_json_value(&fragment) {
            return Ok(value);
        }
        if fragment == "-" {
            return Err(PartialJsonError::Partial("not sure what '-' is"));
        }

        let last_e = self
            .characters
            .iter()
            .rposition(|character| *character == 0x65)
            .map_or(-1, |position| position as isize);
        let retry = js_substring(self.characters, start as isize, last_e);
        parse_json_value(&retry).ok_or(PartialJsonError::Malformed("invalid number literal"))
    }

    fn parse_whole_input_number(&mut self) -> Result<JsValue, PartialJsonError> {
        let whole = JsString::from_utf16(self.characters.to_vec());
        if whole == "-" {
            return Err(PartialJsonError::Malformed("not sure what '-' is"));
        }
        if let Some(value) = parse_json_value(&whole) {
            return Ok(value);
        }
        if let Some(last_e) = self.characters.iter().rposition(|unit| *unit == 0x65)
            && let Some(value) = parse_json_value(&JsString::from_utf16(self.characters[..last_e].to_vec()))
        {
            return Ok(value);
        }
        Err(PartialJsonError::Malformed("invalid number literal"))
    }
}

/// JavaScript `String.prototype.substring` clamps negative indexes to zero and
/// swaps reversed bounds.
fn js_substring(characters: &[u16], start: isize, end: isize) -> JsString {
    let length = characters.len() as isize;
    let mut from = start.clamp(0, length);
    let mut to = end.clamp(0, length);
    if from > to {
        std::mem::swap(&mut from, &mut to);
    }
    JsString::from_utf16(characters[from as usize..to as usize].to_vec())
}
