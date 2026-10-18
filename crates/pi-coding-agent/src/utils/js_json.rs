//! JavaScript `JSON.parse` / `JSON.stringify` semantics for files shared with the TypeScript runtime.
//!
//! Persistent JSON files (auth, trust, settings, model stores) must be byte-identical between the
//! TypeScript and Rust runtimes, and their read errors embed V8's `JSON.parse` messages. This module
//! reproduces both: V8 error messages with UTF-16 positions, JavaScript number formatting, and the
//! JavaScript object key order (array-index keys first, ascending; then insertion order).

use std::fmt;

use pi_js_compat::number::js_number_to_string;
use serde_json::{Map, Number, Value};

const CONTEXT_CHARACTERS: usize = 10;
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
const SPECIAL_SOURCES: [&str; 4] = ["[object Object]", "undefined", "Infinity", "NaN"];

/// `SyntaxError` thrown by `JSON.parse`; `Display` is the V8 message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonParseError {
    message: String,
}

impl fmt::Display for JsonParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for JsonParseError {}

/// `JSON.parse(text)`.
pub fn parse(text: &str) -> Result<Value, JsonParseError> {
    let mut parser = Parser {
        source: text,
        chars: text.chars().collect(),
        index: 0,
    };
    parser.skip_whitespace();
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.peek().is_some() {
        return Err(parser.error_at("Unexpected non-whitespace character after JSON"));
    }
    Ok(value)
}

/// `JSON.stringify(value)`.
pub fn stringify(value: &Value) -> String {
    let mut output = String::new();
    write_value(&mut output, value, None, 0);
    output
}

/// `JSON.stringify(value, null, 2)`.
pub fn stringify_pretty(value: &Value) -> String {
    let mut output = String::new();
    write_value(&mut output, value, Some(2), 0);
    output
}

/// `Object.entries(object)`: array-index keys ascending, then string keys in insertion order.
pub fn object_entries(object: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut index_entries: Vec<(u32, (&String, &Value))> = Vec::new();
    let mut string_entries = Vec::new();
    for entry in object {
        match array_index(entry.0) {
            Some(index) => index_entries.push((index, entry)),
            None => string_entries.push(entry),
        }
    }
    index_entries.sort_by_key(|(index, _)| *index);
    index_entries
        .into_iter()
        .map(|(_, entry)| entry)
        .chain(string_entries)
        .collect()
}

/// Canonical array index: `0..=2^32-2` without leading zeros.
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key.starts_with('0')) || !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    key.parse::<u32>().ok().filter(|index| *index != u32::MAX)
}

/// A JavaScript number as a JSON value; non-finite numbers become `null`, as `JSON.stringify` writes them.
pub fn number_value(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER {
        return Value::from(value as i64);
    }
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

struct Parser<'a> {
    source: &'a str,
    chars: Vec<char>,
    index: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.index += 1;
        }
    }

    fn parse_value(&mut self) -> Result<Value, JsonParseError> {
        match self.peek() {
            Some('"') => self.parse_string().map(Value::String),
            Some('-' | '0'..='9') => self.parse_number(),
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('t') => self.parse_literal("true", Value::Bool(true)),
            Some('f') => self.parse_literal("false", Value::Bool(false)),
            Some('n') => self.parse_literal("null", Value::Null),
            _ => Err(self.unexpected_token()),
        }
    }

    fn parse_literal(&mut self, literal: &str, value: Value) -> Result<Value, JsonParseError> {
        for expected in literal.chars() {
            if self.peek() != Some(expected) {
                return Err(self.unexpected_token());
            }
            self.index += 1;
        }
        Ok(value)
    }

    fn parse_object(&mut self) -> Result<Value, JsonParseError> {
        self.index += 1;
        self.skip_whitespace();
        let mut object = Map::new();
        match self.peek() {
            Some('}') => {
                self.index += 1;
                return Ok(Value::Object(object));
            }
            Some('"') => {}
            _ => return Err(self.error_at("Expected property name or '}' in JSON")),
        }
        loop {
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(':', "Expected ':' after property name in JSON")?;
            self.skip_whitespace();
            let value = self.parse_value()?;
            object.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.index += 1;
                    self.skip_whitespace();
                    if self.peek() != Some('"') {
                        return Err(self.error_at("Expected double-quoted property name in JSON"));
                    }
                }
                Some('}') => {
                    self.index += 1;
                    return Ok(Value::Object(object));
                }
                _ => return Err(self.error_at("Expected ',' or '}' after property value in JSON")),
            }
        }
    }

    fn parse_array(&mut self) -> Result<Value, JsonParseError> {
        self.index += 1;
        self.skip_whitespace();
        let mut items = Vec::new();
        if self.peek() == Some(']') {
            self.index += 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => {
                    self.index += 1;
                    self.skip_whitespace();
                }
                Some(']') => {
                    self.index += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.error_at("Expected ',' or ']' after array element in JSON")),
            }
        }
    }

    fn expect(&mut self, expected: char, message: &str) -> Result<(), JsonParseError> {
        if self.peek() != Some(expected) {
            return Err(self.error_at(message));
        }
        self.index += 1;
        Ok(())
    }

    fn parse_number(&mut self) -> Result<Value, JsonParseError> {
        let start = self.index;
        if self.peek() == Some('-') {
            self.index += 1;
            if !self.peek().is_some_and(|character| character.is_ascii_digit()) {
                return Err(self.error_at("No number after minus sign in JSON"));
            }
        }
        if self.peek() == Some('0') {
            self.index += 1;
            if self.peek().is_some_and(|character| character.is_ascii_digit()) {
                return Err(self.unexpected_token());
            }
        } else {
            self.skip_digits();
        }
        if self.peek() == Some('.') {
            self.index += 1;
            self.require_digits("Unterminated fractional number in JSON")?;
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.index += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.index += 1;
            }
            self.require_digits("Exponent part is missing a number in JSON")?;
        }
        let text: String = self.chars[start..self.index].iter().collect();
        let value: f64 = text.parse().expect("validated JSON number text must parse");
        Ok(number_value(value))
    }

    fn require_digits(&mut self, message: &str) -> Result<(), JsonParseError> {
        if !self.peek().is_some_and(|character| character.is_ascii_digit()) {
            return Err(self.error_at(message));
        }
        self.skip_digits();
        Ok(())
    }

    fn skip_digits(&mut self) {
        while self.peek().is_some_and(|character| character.is_ascii_digit()) {
            self.index += 1;
        }
    }

    fn parse_string(&mut self) -> Result<String, JsonParseError> {
        self.index += 1;
        let mut units: Vec<u16> = Vec::new();
        loop {
            let Some(character) = self.peek() else {
                return Err(self.error_at("Unterminated string in JSON"));
            };
            match character {
                '"' => {
                    self.index += 1;
                    return Ok(String::from_utf16_lossy(&units));
                }
                '\\' => {
                    self.index += 1;
                    self.parse_escape(&mut units)?;
                }
                '\u{0}'..='\u{1f}' => return Err(self.error_at("Bad control character in string literal in JSON")),
                _ => {
                    let mut buffer = [0; 2];
                    units.extend_from_slice(character.encode_utf16(&mut buffer));
                    self.index += 1;
                }
            }
        }
    }

    fn parse_escape(&mut self, units: &mut Vec<u16>) -> Result<(), JsonParseError> {
        let unit = match self.peek() {
            None => return Err(self.unexpected_token()),
            Some('"') => '"' as u16,
            Some('\\') => '\\' as u16,
            Some('/') => '/' as u16,
            Some('b') => 0x08,
            Some('f') => 0x0c,
            Some('n') => '\n' as u16,
            Some('r') => '\r' as u16,
            Some('t') => '\t' as u16,
            Some('u') => return self.parse_unicode_escape(units),
            Some(_) => return Err(self.error_at("Bad escaped character in JSON")),
        };
        units.push(unit);
        self.index += 1;
        Ok(())
    }

    fn parse_unicode_escape(&mut self, units: &mut Vec<u16>) -> Result<(), JsonParseError> {
        self.index += 1;
        let mut unit: u16 = 0;
        for _ in 0..4 {
            let Some(digit) = self.peek().and_then(|character| character.to_digit(16)) else {
                return Err(self.error_at("Bad Unicode escape in JSON"));
            };
            unit = unit * 16 + digit as u16;
            self.index += 1;
        }
        units.push(unit);
        Ok(())
    }

    /// V8 `ReportUnexpectedToken` for a token without a dedicated message; the offending
    /// character's token class picks the template.
    fn unexpected_token(&self) -> JsonParseError {
        let message = match self.peek() {
            None => "Unexpected end of JSON input".to_owned(),
            Some('"') => return self.error_at("Unexpected string in JSON"),
            Some('-' | '0'..='9') => return self.error_at("Unexpected number in JSON"),
            Some(_) if SPECIAL_SOURCES.contains(&self.source) => format!("\"{}\" is not valid JSON", self.source),
            Some(token) => self.unexpected_token_with_context(token),
        };
        JsonParseError { message }
    }

    fn unexpected_token_with_context(&self, token: char) -> String {
        let units: Vec<u16> = self.source.encode_utf16().collect();
        // V8 reports one UTF-16 unit; a non-BMP character therefore shows as U+FFFD in UTF-8.
        let token = if token.len_utf16() == 2 { '\u{fffd}' } else { token };
        let length = units.len();
        let position = self.utf16_position();
        if length <= CONTEXT_CHARACTERS * 2 {
            return format!("Unexpected token '{token}', \"{}\" is not valid JSON", self.source);
        }
        let excerpt = |start: usize, end: usize| String::from_utf16_lossy(&units[start..end.min(length)]);
        if position < CONTEXT_CHARACTERS {
            let context = excerpt(0, position + CONTEXT_CHARACTERS);
            format!("Unexpected token '{token}', \"{context}\"... is not valid JSON")
        } else if position < length - CONTEXT_CHARACTERS {
            let context = excerpt(position - CONTEXT_CHARACTERS, position + CONTEXT_CHARACTERS);
            format!("Unexpected token '{token}', ...\"{context}\"... is not valid JSON")
        } else {
            let context = excerpt(position - CONTEXT_CHARACTERS, length);
            format!("Unexpected token '{token}', ...\"{context}\" is not valid JSON")
        }
    }

    /// A V8 message carrying `at position N (line L column C)`; positions count UTF-16 units.
    fn error_at(&self, message: &str) -> JsonParseError {
        let (line, column) = self.line_and_column();
        JsonParseError {
            message: format!(
                "{message} at position {} (line {line} column {column})",
                self.utf16_position()
            ),
        }
    }

    fn utf16_position(&self) -> usize {
        self.chars[..self.index]
            .iter()
            .map(|character| character.len_utf16())
            .sum()
    }

    /// V8 `CalculateFileLocation`: `\r\n`, `\r`, and `\n` each end a line.
    fn line_and_column(&self) -> (usize, usize) {
        let units: Vec<u16> = self.chars[..self.index]
            .iter()
            .flat_map(|character| {
                let mut buffer = [0; 2];
                character.encode_utf16(&mut buffer).to_vec()
            })
            .collect();
        let (carriage_return, line_feed) = ('\r' as u16, '\n' as u16);
        let mut line = 1;
        let mut line_start = 0;
        let mut cursor = 0;
        while cursor < units.len() {
            if units[cursor] == carriage_return && units.get(cursor + 1) == Some(&line_feed) {
                cursor += 1;
            }
            if units[cursor] == carriage_return || units[cursor] == line_feed {
                line += 1;
                line_start = cursor + 1;
            }
            cursor += 1;
        }
        (line, 1 + units.len() - line_start)
    }
}

fn write_value(output: &mut String, value: &Value, indent: Option<usize>, depth: usize) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(flag) => output.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => output.push_str(&js_number_to_string(number.as_f64().unwrap_or(f64::NAN))),
        Value::String(text) => write_string(output, text),
        Value::Array(items) => {
            let items: Vec<&Value> = items.iter().collect();
            write_container(output, ('[', ']'), &items, indent, depth, |output, item, depth| {
                write_value(output, item, indent, depth);
            });
        }
        Value::Object(object) => {
            let entries = object_entries(object);
            let separator = if indent.is_some() { ": " } else { ":" };
            write_container(
                output,
                ('{', '}'),
                &entries,
                indent,
                depth,
                |output, (key, value), depth| {
                    write_string(output, key);
                    output.push_str(separator);
                    write_value(output, value, indent, depth);
                },
            );
        }
    }
}

fn write_container<T: Copy>(
    output: &mut String,
    (open, close): (char, char),
    items: &[T],
    indent: Option<usize>,
    depth: usize,
    write_item: impl Fn(&mut String, T, usize),
) {
    output.push(open);
    if items.is_empty() {
        output.push(close);
        return;
    }
    for (position, item) in items.iter().enumerate() {
        if position > 0 {
            output.push(',');
        }
        push_line_break(output, indent, depth + 1);
        write_item(output, *item, depth + 1);
    }
    push_line_break(output, indent, depth);
    output.push(close);
}

fn push_line_break(output: &mut String, indent: Option<usize>, depth: usize) {
    if let Some(width) = indent {
        output.push('\n');
        output.push_str(&" ".repeat(width * depth));
    }
}

fn write_string(output: &mut String, text: &str) {
    output.push('"');
    for character in text.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{8}' => output.push_str("\\b"),
            '\u{c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{0}'..='\u{1f}' => output.push_str(&format!("\\u{:04x}", character as u32)),
            _ => output.push(character),
        }
    }
    output.push('"');
}
