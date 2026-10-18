use std::fmt;

use serde_json::{Map, Value};

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
pub fn repair_json(json: &str) -> String {
    let characters: Vec<char> = json.chars().collect();
    let mut repaired = String::with_capacity(json.len());
    let mut in_string = false;
    let mut index = 0;

    while index < characters.len() {
        let character = characters[index];

        if !in_string {
            repaired.push(character);
            if character == '"' {
                in_string = true;
            }
            index += 1;
            continue;
        }

        if character == '"' {
            repaired.push(character);
            in_string = false;
            index += 1;
            continue;
        }

        if character == '\\' {
            index = repair_escape(&characters, index, &mut repaired);
            continue;
        }

        if is_control_character(character) {
            repaired.push_str(&escape_control_character(character));
        } else {
            repaired.push(character);
        }
        index += 1;
    }

    repaired
}

fn repair_escape(characters: &[char], index: usize, repaired: &mut String) -> usize {
    let Some(&next_character) = characters.get(index + 1) else {
        repaired.push_str("\\\\");
        return index + 1;
    };

    if next_character == 'u' {
        let unicode_digits: String = characters.iter().skip(index + 2).take(4).collect();
        if unicode_digits.len() == 4 && unicode_digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
            repaired.push_str("\\u");
            repaired.push_str(&unicode_digits);
            return index + 6;
        }
    }

    if VALID_JSON_ESCAPES.contains(&next_character) {
        repaired.push('\\');
        repaired.push(next_character);
        return index + 2;
    }

    repaired.push_str("\\\\");
    index + 1
}

fn is_control_character(character: char) -> bool {
    (character as u32) <= 0x1f
}

fn escape_control_character(character: char) -> String {
    match character {
        '\u{8}' => "\\b".to_owned(),
        '\u{c}' => "\\f".to_owned(),
        '\n' => "\\n".to_owned(),
        '\r' => "\\r".to_owned(),
        '\t' => "\\t".to_owned(),
        other => format!("\\u{:04x}", other as u32),
    }
}

pub fn parse_json_with_repair(json: &str) -> Result<Value, JsonParseError> {
    match parse_strict_json(json) {
        Ok(value) => Ok(value),
        Err(error) => {
            let repaired_json = repair_json(json);
            if repaired_json == json {
                return Err(error);
            }
            parse_strict_json(&repaired_json)
        }
    }
}

pub fn parse_streaming_json(partial_json: Option<&str>) -> Value {
    let Some(partial_json) = partial_json else {
        return Value::Object(Map::new());
    };
    if partial_json.trim().is_empty() {
        return Value::Object(Map::new());
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

    Value::Object(Map::new())
}

fn convert_nullish_result(original: &str, value: Value) -> Value {
    if value.is_null() && !is_non_finite_literal(original) {
        Value::Object(Map::new())
    } else {
        value
    }
}

fn is_non_finite_literal(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("Inf") || trimmed.starts_with("-Inf") || trimmed.starts_with("NaN")
}

fn parse_strict_json(text: &str) -> Result<Value, JsonParseError> {
    match serde_json::from_str(text) {
        Ok(value) => Ok(value),
        Err(error) => {
            if is_non_finite_json_number(text) {
                Ok(Value::Null)
            } else {
                Err(JsonParseError {
                    message: error.to_string(),
                })
            }
        }
    }
}

fn parse_json_value(text: &str) -> Option<Value> {
    match serde_json::from_str(text) {
        Ok(value) => Some(value),
        Err(_) if is_non_finite_json_number(text) => Some(Value::Null),
        Err(_) => None,
    }
}

/// JavaScript `JSON.parse` returns non-finite numbers for overflowing literals;
/// JSON serialization of those values is `null`, which is the Rust representation.
fn is_non_finite_json_number(text: &str) -> bool {
    let trimmed = text.trim();
    let looks_numeric = trimmed
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit() || first == '-')
        && trimmed
            .chars()
            .all(|character| character.is_ascii_digit() || "+-eE.".contains(character));
    looks_numeric && trimmed.parse::<f64>().is_ok_and(|value| !value.is_finite())
}

fn parse_json_string(text: &str) -> Option<String> {
    serde_json::from_str(text).ok()
}

fn partial_parse(text: &str) -> Result<Value, PartialJsonError> {
    let characters: Vec<char> = text.trim().chars().collect();
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
    characters: &'a [char],
    index: usize,
}

impl PartialParser<'_> {
    fn current(&self) -> Option<char> {
        self.characters.get(self.index).copied()
    }

    fn skip_blank(&mut self) {
        while self
            .current()
            .is_some_and(|character| JSON_WHITESPACE.contains(character))
        {
            self.index += 1;
        }
    }

    fn parse_any(&mut self) -> Result<Value, PartialJsonError> {
        self.skip_blank();
        match self.current() {
            None => Err(PartialJsonError::Partial("unexpected end of input")),
            Some('"') => self.parse_string(),
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some(_) => self.parse_literal_or_number(),
        }
    }

    fn parse_literal_or_number(&mut self) -> Result<Value, PartialJsonError> {
        let literals = [
            ("null", Value::Null, 1),
            ("true", Value::Bool(true), 1),
            ("false", Value::Bool(false), 1),
            ("Infinity", Value::Null, 1),
            ("-Infinity", Value::Null, 2),
            ("NaN", Value::Null, 1),
        ];
        for (literal, value, minimum_remaining) in literals {
            if let Some(value) = self.take_literal(literal, value, minimum_remaining) {
                return Ok(value);
            }
        }
        self.parse_number()
    }

    fn take_literal(&mut self, literal: &str, value: Value, minimum_remaining: usize) -> Option<Value> {
        let remaining: String = self.characters[self.index..].iter().collect();
        let exact = remaining == literal;
        let partial = remaining.chars().count() >= minimum_remaining
            && remaining.chars().count() < literal.chars().count()
            && literal.starts_with(&remaining);
        if exact || partial {
            self.index = self.characters.len();
            return Some(value);
        }
        None
    }

    fn parse_string(&mut self) -> Result<Value, PartialJsonError> {
        let start = self.index;
        let mut escape = false;
        self.index += 1;

        while self.index < self.characters.len()
            && (self.characters[self.index] != '"' || (escape && self.characters[self.index - 1] == '\\'))
        {
            escape = if self.characters[self.index] == '\\' {
                !escape
            } else {
                false
            };
            self.index += 1;
        }

        if self.current() == Some('"') {
            let next_index = self.index + 1;
            let end = next_index - usize::from(escape);
            let fragment: String = self.characters[start..end].iter().collect();
            self.index = next_index;
            return parse_json_string(&fragment)
                .map(Value::String)
                .ok_or(PartialJsonError::Malformed("invalid string literal"));
        }

        self.parse_unterminated_string(start, escape)
    }

    fn parse_unterminated_string(&mut self, start: usize, escape: bool) -> Result<Value, PartialJsonError> {
        let end = self.index - usize::from(escape);
        let mut fragment: String = self.characters[start..end].iter().collect();
        fragment.push('"');
        if let Some(value) = parse_json_string(&fragment) {
            return Ok(Value::String(value));
        }

        let last_backslash = self
            .characters
            .iter()
            .rposition(|character| *character == '\\')
            .map_or(-1, |position| position as isize);
        let mut fallback = js_substring(self.characters, start as isize, last_backslash);
        fallback.push('"');
        parse_json_string(&fallback)
            .map(Value::String)
            .ok_or(PartialJsonError::Malformed("unterminated string literal"))
    }

    fn parse_object(&mut self) -> Result<Value, PartialJsonError> {
        self.index += 1;
        self.skip_blank();
        let mut object = Map::new();

        loop {
            if self.current() == Some('}') {
                self.index += 1;
                return Ok(Value::Object(object));
            }
            self.skip_blank();
            if self.index >= self.characters.len() {
                return Ok(Value::Object(object));
            }

            let Ok(Value::String(key)) = self.parse_string() else {
                return Ok(Value::Object(object));
            };
            self.skip_blank();
            self.index += 1;
            match self.parse_any() {
                Ok(value) => {
                    object.insert(key, value);
                }
                Err(_) => return Ok(Value::Object(object)),
            }
            self.skip_blank();
            if self.current() == Some(',') {
                self.index += 1;
            }
        }
    }

    fn parse_array(&mut self) -> Result<Value, PartialJsonError> {
        self.index += 1;
        let mut array = Vec::new();

        loop {
            if self.current() == Some(']') {
                self.index += 1;
                return Ok(Value::Array(array));
            }
            match self.parse_any() {
                Ok(value) => array.push(value),
                Err(_) => return Ok(Value::Array(array)),
            }
            self.skip_blank();
            if self.current() == Some(',') {
                self.index += 1;
            }
        }
    }

    fn parse_number(&mut self) -> Result<Value, PartialJsonError> {
        if self.index == 0 {
            return self.parse_whole_input_number();
        }

        let start = self.index;
        if self.current() == Some('-') {
            self.index += 1;
        }
        while let Some(character) = self.current() {
            if NUMBER_DELIMITERS.contains(character) {
                break;
            }
            self.index += 1;
        }

        let fragment: String = self.characters[start..self.index].iter().collect();
        if let Some(value) = parse_json_value(&fragment) {
            return Ok(value);
        }
        if fragment == "-" {
            return Err(PartialJsonError::Partial("not sure what '-' is"));
        }

        let last_e = self
            .characters
            .iter()
            .rposition(|character| *character == 'e')
            .map_or(-1, |position| position as isize);
        let retry = js_substring(self.characters, start as isize, last_e);
        parse_json_value(&retry).ok_or(PartialJsonError::Malformed("invalid number literal"))
    }

    fn parse_whole_input_number(&mut self) -> Result<Value, PartialJsonError> {
        let whole: String = self.characters.iter().collect();
        if whole == "-" {
            return Err(PartialJsonError::Malformed("not sure what '-' is"));
        }
        if let Some(value) = parse_json_value(&whole) {
            return Ok(value);
        }
        if let Some(last_e) = whole.rfind('e')
            && let Some(value) = parse_json_value(&whole[..last_e])
        {
            return Ok(value);
        }
        Err(PartialJsonError::Malformed("invalid number literal"))
    }
}

/// JavaScript `String.prototype.substring` clamps negative indexes to zero and
/// swaps reversed bounds.
fn js_substring(characters: &[char], start: isize, end: isize) -> String {
    let length = characters.len() as isize;
    let mut from = start.clamp(0, length);
    let mut to = end.clamp(0, length);
    if from > to {
        std::mem::swap(&mut from, &mut to);
    }
    characters[from as usize..to as usize].iter().collect()
}
