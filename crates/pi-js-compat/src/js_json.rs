//! JavaScript `JSON.parse` / `JSON.stringify` semantics for files shared with the TypeScript runtime.
//!
//! Persistent JSON files (auth, trust, settings, model stores) must be byte-identical between the
//! TypeScript and Rust runtimes, and their read errors embed V8's `JSON.parse` messages. This module
//! reproduces both: V8 error messages with UTF-16 positions, JavaScript number formatting, and the
//! JavaScript object key order (array-index keys first, ascending; then insertion order).

use serde::Deserialize;
use std::fmt;

use crate::number::{js_number_to_string, js_number_value};
use indexmap::IndexMap;
use serde_json::{Map, Value};

/// A JavaScript string: identity and ordering are UTF-16 code-unit based.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct JsString(Vec<u16>);

impl JsString {
    pub fn from_utf16(units: Vec<u16>) -> Self {
        Self(units)
    }

    pub fn units(&self) -> &[u16] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// JavaScript string length, measured in UTF-16 code units.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn push_str(&mut self, text: impl Into<Self>) {
        self.0.extend(text.into().0);
    }

    pub fn contains(&self, text: impl Into<Self>) -> bool {
        let text = text.into();
        text.is_empty() || self.0.windows(text.len()).any(|window| window == text.units())
    }

    pub fn starts_with(&self, text: impl Into<Self>) -> bool {
        self.0.starts_with(text.into().units())
    }
    pub fn ends_with(&self, text: impl Into<Self>) -> bool {
        self.0.ends_with(text.into().units())
    }

    pub fn encode_utf16(&self) -> impl Iterator<Item = u16> + '_ {
        self.0.iter().copied()
    }

    pub fn split(&self, separator: char) -> impl Iterator<Item = Self> + '_ {
        let separator = JsString::from(separator);
        assert_eq!(separator.len(), 1, "split requires one UTF-16 code unit");
        let unit = separator.0[0];
        self.0.split(move |item| *item == unit).map(|part| Self(part.to_vec()))
    }

    pub fn trim(&self) -> Self {
        self.clone().trim_js_whitespace()
    }

    fn trim_js_whitespace(self) -> Self {
        let start = self
            .0
            .iter()
            .position(|unit| !crate::string::is_js_whitespace_unit(*unit));
        let end = self
            .0
            .iter()
            .rposition(|unit| !crate::string::is_js_whitespace_unit(*unit));
        match (start, end) {
            (Some(start), Some(end)) => Self(self.0[start..=end].to_vec()),
            _ => Self::default(),
        }
    }

    pub fn join(texts: impl IntoIterator<Item = impl Into<Self>>, separator: &str) -> Self {
        let mut result = Self::default();
        for (index, text) in texts.into_iter().enumerate() {
            if index != 0 {
                result.push_str(separator);
            }
            result.push_str(text);
        }
        result
    }

    /// Checked UTF-8 boundary; lone surrogates are rejected.
    pub fn to_utf8(&self) -> Result<String, std::string::FromUtf16Error> {
        String::from_utf16(&self.0)
    }

    /// Explicit Node UTF-8 encoding boundary, replacing lone surrogates with U+FFFD.
    pub fn to_utf8_lossy(&self) -> String {
        encode_node_utf8(self)
    }
}

/// Lossless JSON serde uses serde_json's raw-fragment protocol. Deserialize from JSON
/// text/bytes, not from `serde_json::Value` or serde's buffered untagged-enum content.
/// Other serde formats are unsupported: their string model cannot represent lone surrogates.
impl serde::Serialize for JsString {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        JsValue::String(self.clone()).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for JsString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match JsValue::deserialize(deserializer)? {
            JsValue::String(text) => Ok(text),
            _ => Err(serde::de::Error::custom("expected a JSON string")),
        }
    }
}

impl std::hash::Hash for JsString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self.to_utf8() {
            Ok(text) => text.hash(state),
            Err(_) => self.0.hash(state),
        }
    }
}

// Exact UTF-8 key lookup in a UTF-16 object, without allocating a temporary key.
impl indexmap::Equivalent<JsString> for str {
    fn equivalent(&self, key: &JsString) -> bool {
        key == self
    }
}
impl indexmap::Equivalent<JsString> for String {
    fn equivalent(&self, key: &JsString) -> bool {
        key == self
    }
}

impl PartialEq<str> for JsString {
    fn eq(&self, other: &str) -> bool {
        self.0.iter().copied().eq(other.encode_utf16())
    }
}
impl PartialEq<&str> for JsString {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}
impl PartialEq<String> for JsString {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

impl From<&JsString> for JsString {
    fn from(text: &JsString) -> Self {
        text.clone()
    }
}

impl From<String> for JsString {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
}

impl From<&String> for JsString {
    fn from(text: &String) -> Self {
        Self::from(text.as_str())
    }
}

impl From<char> for JsString {
    fn from(character: char) -> Self {
        Self::from(character.to_string())
    }
}

impl From<&str> for JsString {
    fn from(text: &str) -> Self {
        Self(text.encode_utf16().collect())
    }
}

/// Node encodes a JS string to a UTF-8 stream, path, argument or environment
/// value by replacing unmatched surrogate units with U+FFFD. Use only at that
/// external boundary; JSON serialization must use the lossless writer instead.
pub fn encode_node_utf8(text: &JsString) -> String {
    String::from_utf16_lossy(text.units())
}

/// Insertion-ordered storage. Use `object_entries_lossless` for JavaScript enumeration order.
pub type JsObject = IndexMap<JsString, JsValue>;

/// JSON values retaining UTF-16 strings/keys and JavaScript binary64 numbers.
/// Non-finite numbers and negative zero survive parsing; JSON serialization maps non-finite to null.
#[derive(Debug, Clone, PartialEq)]
pub enum JsValue {
    Null,
    Bool(bool),
    Number(f64),
    String(JsString),
    Array(Vec<JsValue>),
    Object(JsObject),
}

impl Default for JsValue {
    fn default() -> Self {
        Self::Null
    }
}

impl serde::Serialize for JsValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let raw =
            serde_json::value::RawValue::from_string(stringify_lossless(self)).map_err(serde::ser::Error::custom)?;
        raw.serialize(serializer)
    }
}

/// Requires a serde_json text/bytes deserializer; see [`JsString`]'s serde contract.
impl<'de> serde::Deserialize<'de> for JsValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> serde::de::Visitor<'de> for JsonVisitor {
            type Value = JsValue;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a raw JSON value")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<JsValue, M::Error> {
                let key = map.next_key::<String>()?;
                if key.as_deref() != Some("$serde_json::private::RawValue") {
                    return Err(serde::de::Error::custom("expected raw JSON fragment"));
                }
                let raw = map.next_value::<String>()?;
                parse_lossless(&raw).map_err(serde::de::Error::custom)
            }
            // serde_json::Value has already admitted only Unicode scalar strings.
            // This is an exact conversion of that input, not replacement of raw UTF-16.
            fn visit_newtype_struct<D: serde::Deserializer<'de>>(self, d: D) -> Result<JsValue, D::Error> {
                Value::deserialize(d).map(JsValue::from)
            }
        }
        deserializer.deserialize_newtype_struct("$serde_json::private::RawValue", JsonVisitor)
    }
}

/// Serde adapter for object-only fields. Serializing keys individually cannot
/// carry lone surrogates through serde's map-key interface, so emit the whole
/// object through the shared lossless value serializer.
pub fn serialize_object<S: serde::Serializer>(object: &JsObject, serializer: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&JsValue::Object(object.clone()), serializer)
}

pub fn deserialize_object<'de, D: serde::Deserializer<'de>>(d: D) -> Result<JsObject, D::Error> {
    match JsValue::deserialize(d)? {
        JsValue::Object(object) => Ok(object),
        _ => Err(serde::de::Error::custom("expected a JSON object")),
    }
}

/// Deserialize an optional JSON property without conflating explicit null with omission.
/// Use with `#[serde(default, deserialize_with = "deserialize_present")]`.
pub fn deserialize_present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<JsValue>, D::Error> {
    serde::Deserialize::deserialize(deserializer).map(Some)
}

impl JsValue {
    pub fn object(entries: impl IntoIterator<Item = (impl Into<JsString>, JsValue)>) -> Self {
        Self::Object(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Self> {
        self.as_object_mut()?.get_mut(&JsString::from(key))
    }

    pub fn as_object_mut(&mut self) -> Option<&mut JsObject> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Self>> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    pub fn get(&self, key: &str) -> Option<&Self> {
        self.as_object()?.get(&JsString::from(key))
    }

    pub fn as_object(&self) -> Option<&JsObject> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Self>> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&JsString> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Checked adapter for consumers whose contract requires Unicode scalar strings.
    /// Lossless public credential storage must not use this adapter.
    pub fn to_serde(&self) -> Result<Value, serde_json::Error> {
        serde_json::from_str(&stringify_lossless(self))
    }

    /// Explicit lossy conversion. Key collisions occur in insertion order at this boundary.
    /// This is not legacy `parse`, which replaces strings before duplicate-key handling.
    pub fn into_serde_lossy(self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(flag) => Value::Bool(flag),
            Self::Number(number) => js_number_value(number),
            Self::String(text) => Value::String(text.to_utf8_lossy()),
            Self::Array(items) => Value::Array(items.into_iter().map(Self::into_serde_lossy).collect()),
            Self::Object(object) => {
                let mut result = Map::new();
                for (key, value) in object {
                    result.insert(key.to_utf8_lossy(), value.into_serde_lossy());
                }
                Value::Object(result)
            }
        }
    }
}

/// Convert a serializable record directly to the lossless authority, without an
/// intermediate serde_json::Value (which rejects lone-surrogate string escapes).
pub fn to_value<T: serde::Serialize + ?Sized>(value: &T) -> Result<JsValue, serde_json::Error> {
    let json = serde_json::to_string(value)?;
    parse_lossless(&json).map_err(<serde_json::Error as serde::de::Error>::custom)
}

/// JavaScript Boolean conversion at a lossless JSON boundary.
pub fn is_truthy(value: &JsValue) -> bool {
    match value {
        JsValue::Null => false,
        JsValue::Bool(flag) => *flag,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(text) => !text.units().is_empty(),
        JsValue::Array(_) | JsValue::Object(_) => true,
    }
}

/// JavaScript String conversion at a lossless JSON boundary.
/// JSON objects cannot supply callable own methods; a shadowed `toString` therefore throws.
pub fn to_js_string(value: &JsValue) -> Result<JsString, &'static str> {
    match value {
        JsValue::Null => Ok(JsString::from("null")),
        JsValue::Bool(flag) => Ok(JsString::from(if *flag { "true" } else { "false" })),
        JsValue::Number(number) => Ok(JsString::from(js_number_to_string(*number).as_str())),
        JsValue::String(text) => Ok(text.clone()),
        JsValue::Array(items) => {
            let mut units = Vec::new();
            for (index, item) in items.iter().enumerate() {
                if index != 0 {
                    units.push(u16::from(b','));
                }
                if !matches!(item, JsValue::Null) {
                    units.extend_from_slice(to_js_string(item)?.units());
                }
            }
            Ok(JsString::from_utf16(units))
        }
        JsValue::Object(object) => {
            if object.contains_key(&JsString::from("toString")) {
                Err("Cannot convert object to primitive value")
            } else {
                Ok(JsString::from("[object Object]"))
            }
        }
    }
}

impl From<&JsValue> for JsValue {
    fn from(value: &JsValue) -> Self {
        value.clone()
    }
}

impl From<Value> for JsValue {
    fn from(value: Value) -> Self {
        Self::from(&value)
    }
}

impl From<&Value> for JsValue {
    fn from(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(flag) => Self::Bool(*flag),
            Value::Number(number) => Self::Number(number.as_f64().expect("serde JSON number fits binary64")),
            Value::String(text) => Self::String(JsString::from(text.as_str())),
            Value::Array(items) => Self::Array(items.iter().map(Self::from).collect()),
            Value::Object(object) => Self::Object(
                object
                    .iter()
                    .map(|(key, value)| (JsString::from(key.as_str()), Self::from(value)))
                    .collect(),
            ),
        }
    }
}

const CONTEXT_CHARACTERS: usize = 10;
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
    parse_with_string_boundary(text, false).map(JsValue::into_serde_lossy)
}

/// `JSON.parse(text)` retaining every UTF-16 string and key code unit.
pub fn parse_lossless(text: &str) -> Result<JsValue, JsonParseError> {
    parse_with_string_boundary(text, true)
}

fn parse_with_string_boundary(text: &str, lossless: bool) -> Result<JsValue, JsonParseError> {
    let mut parser = Parser {
        source: text,
        chars: text.chars().collect(),
        index: 0,
        lossless,
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
    write_value(&mut output, &JsValue::from(value), None, 0);
    output
}

/// `JSON.stringify(value, null, 2)`.
pub fn stringify_pretty(value: &Value) -> String {
    let mut output = String::new();
    write_value(&mut output, &JsValue::from(value), Some(2), 0);
    output
}

/// `JSON.stringify(value)` retaining lone surrogate escapes.
pub fn stringify_lossless(value: &JsValue) -> String {
    let mut output = String::new();
    write_value(&mut output, value, None, 0);
    output
}

/// `JSON.stringify(value, null, 2)` retaining lone surrogate escapes.
pub fn stringify_lossless_pretty(value: &JsValue) -> String {
    let mut output = String::new();
    write_value(&mut output, value, Some(2), 0);
    output
}

/// `Object.entries(object)` for UTF-16 keys: array indices first, then insertion order.
pub fn object_entries_lossless(object: &JsObject) -> Vec<(&JsString, &JsValue)> {
    let mut entries: Vec<_> = object.iter().collect();
    // Stable sort preserves insertion order among non-index keys.
    entries.sort_by_key(|(key, _)| array_index_units(key.units().iter().copied()).unwrap_or(u32::MAX));
    entries
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
    array_index_units(key.encode_utf16())
}

fn array_index_units(mut units: impl Iterator<Item = u16>) -> Option<u32> {
    let first = units.next()?;
    if first == u16::from(b'0') {
        return units.next().is_none().then_some(0);
    }
    let digit = |unit: u16| {
        unit.checked_sub(u16::from(b'0'))
            .filter(|digit| *digit <= 9)
            .map(u32::from)
    };
    let mut index = digit(first)?;
    for unit in units {
        index = index.checked_mul(10)?.checked_add(digit(unit)?)?;
    }
    (index != u32::MAX).then_some(index)
}

struct Parser<'a> {
    source: &'a str,
    chars: Vec<char>,
    index: usize,
    lossless: bool,
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

    fn parse_value(&mut self) -> Result<JsValue, JsonParseError> {
        match self.peek() {
            Some('"') => self.parse_string().map(JsValue::String),
            Some('-' | '0'..='9') => self.parse_number(),
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('t') => self.parse_literal("true", JsValue::Bool(true)),
            Some('f') => self.parse_literal("false", JsValue::Bool(false)),
            Some('n') => self.parse_literal("null", JsValue::Null),
            _ => Err(self.unexpected_token()),
        }
    }

    fn parse_literal(&mut self, literal: &str, value: JsValue) -> Result<JsValue, JsonParseError> {
        for expected in literal.chars() {
            if self.peek() != Some(expected) {
                return Err(self.unexpected_token());
            }
            self.index += 1;
        }
        Ok(value)
    }

    fn parse_object(&mut self) -> Result<JsValue, JsonParseError> {
        self.index += 1;
        self.skip_whitespace();
        let mut object = JsObject::new();
        match self.peek() {
            Some('}') => {
                self.index += 1;
                return Ok(JsValue::Object(object));
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
                    return Ok(JsValue::Object(object));
                }
                _ => return Err(self.error_at("Expected ',' or '}' after property value in JSON")),
            }
        }
    }

    fn parse_array(&mut self) -> Result<JsValue, JsonParseError> {
        self.index += 1;
        self.skip_whitespace();
        let mut items = Vec::new();
        if self.peek() == Some(']') {
            self.index += 1;
            return Ok(JsValue::Array(items));
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
                    return Ok(JsValue::Array(items));
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

    fn parse_number(&mut self) -> Result<JsValue, JsonParseError> {
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
        Ok(JsValue::Number(value))
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

    fn parse_string(&mut self) -> Result<JsString, JsonParseError> {
        self.index += 1;
        let mut units: Vec<u16> = Vec::new();
        loop {
            let Some(character) = self.peek() else {
                return Err(self.error_at("Unterminated string in JSON"));
            };
            match character {
                '"' => {
                    self.index += 1;
                    return Ok(if self.lossless {
                        JsString::from_utf16(units)
                    } else {
                        JsString::from(String::from_utf16_lossy(&units).as_str())
                    });
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

fn write_value(output: &mut String, value: &JsValue, indent: Option<usize>, depth: usize) {
    match value {
        JsValue::Null => output.push_str("null"),
        JsValue::Bool(flag) => output.push_str(if *flag { "true" } else { "false" }),
        JsValue::Number(number) => {
            if number.is_finite() {
                output.push_str(&js_number_to_string(*number));
            } else {
                output.push_str("null");
            }
        }
        JsValue::String(text) => write_string(output, text),
        JsValue::Array(items) => {
            let items: Vec<&JsValue> = items.iter().collect();
            write_container(output, ('[', ']'), &items, indent, depth, |output, item, depth| {
                write_value(output, item, indent, depth);
            });
        }
        JsValue::Object(object) => {
            let entries = object_entries_lossless(object);
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

fn write_string(output: &mut String, text: &JsString) {
    output.push('"');
    for character in char::decode_utf16(text.units().iter().copied()) {
        let character = match character {
            Ok(character) => character,
            Err(error) => {
                output.push_str(&format!("\\u{:04x}", error.unpaired_surrogate()));
                continue;
            }
        };
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
