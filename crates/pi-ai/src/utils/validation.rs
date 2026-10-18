//! Port of `packages/ai/src/utils/validation.ts`.
//!
//! The TypeScript module normalizes optional `null`s, runs TypeBox's
//! `Value.Convert`, applies the AJV-compatible coercion used for plain JSON
//! schemas, and finally validates with TypeBox's compiled validator. This
//! module reproduces those four steps and TypeBox's English validation
//! messages byte-for-byte.
//!
//! TypeBox version: 1.3.7 (pinned). That release marks schema nodes with a
//! hidden `~kind` string property and no longer defines
//! `Symbol.for("TypeBox.Kind")`, so `validation.ts`'s `TYPEBOX_KIND` check is
//! always false and the plain-schema coercion pass always runs. The Rust port
//! preserves that behavior; `Tool::typebox_kind` only gates `Value.Convert`,
//! which is the real role of the hidden marker.

use std::fmt;

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::types::{Tool, ToolCall};
use crate::utils::js_number::js_number_to_string;

/// TypeBox's default `Settings.maxErrors`.
const MAX_ERRORS: usize = 8;

/// Failure of [`validate_tool_arguments`] or [`validate_tool_call`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    message: String,
}

impl ValidationError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Finds a tool by name and validates the tool call arguments against its schema.
pub fn validate_tool_call(tools: &[Tool], tool_call: &ToolCall) -> Result<Value, ValidationError> {
    let tool = tools
        .iter()
        .find(|tool| tool.name == tool_call.name)
        .ok_or_else(|| ValidationError {
            message: format!("Tool \"{}\" not found", tool_call.name),
        })?;
    validate_tool_arguments(tool, tool_call)
}

/// Validates tool call arguments against the tool's schema.
pub fn validate_tool_arguments(tool: &Tool, tool_call: &ToolCall) -> Result<Value, ValidationError> {
    let mut args = Value::Object(tool_call.arguments.clone());
    normalize_optional_nulls(&mut args, &tool.parameters);
    if tool.typebox_kind {
        let _ = convert(&tool.parameters, &mut args, &Map::new());
    }

    // `validation.ts` always runs this pass for the pinned TypeBox release.
    let coerced = coerce(&tool.parameters, args.clone());
    if coerced != args {
        if args.is_object() && coerced.is_object() {
            args = coerced;
        } else {
            return Ok(if check(&tool.parameters, &coerced) {
                coerced
            } else {
                args
            });
        }
    }

    let issues = collect_issues(&tool.parameters, &args);
    if issues.is_empty() {
        return Ok(args);
    }

    let errors = issues
        .items
        .iter()
        .map(|issue| format!("  - {}: {}", issue.path(), issue.message))
        .collect::<Vec<_>>()
        .join("\n");
    let errors = if errors.is_empty() {
        "Unknown validation error".to_owned()
    } else {
        errors
    };
    let received = json_stringify_pretty(&Value::Object(tool_call.arguments.clone()));
    Err(ValidationError {
        message: format!(
            "Validation failed for tool \"{}\":\n{}\n\nReceived arguments:\n{}",
            tool_call.name, errors, received
        ),
    })
}

// ---------------------------------------------------------------------------
// Schema accessors
// ---------------------------------------------------------------------------

fn string_field<'a>(schema: &'a Value, key: &str) -> Option<&'a str> {
    schema.get(key).and_then(Value::as_str)
}

fn object_field<'a>(schema: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    schema.get(key).and_then(Value::as_object)
}

fn array_field<'a>(schema: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    schema.get(key).and_then(Value::as_array)
}

fn object_string<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn object_array<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Vec<Value>> {
    object.get(key).and_then(Value::as_array)
}

fn object_number(object: &Map<String, Value>, key: &str) -> Option<f64> {
    object.get(key).and_then(Value::as_f64)
}

fn is_schema(value: &Value) -> bool {
    value.is_object() || value.is_boolean()
}

// ---------------------------------------------------------------------------
// JavaScript value semantics
// ---------------------------------------------------------------------------

/// JavaScript `Number::toString` for a JSON number.
fn js_number(value: f64) -> Value {
    if value.is_finite() && value.fract() == 0.0 && value.abs() <= i64::MAX as f64 {
        return Value::from(value as i64);
    }
    Value::from(value)
}

/// JavaScript `Number(string)` (used by the plain-schema coercion pass).
fn js_number_parse(input: &str) -> Option<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    if let Some(digits) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        return parse_radix(digits, 16);
    }
    if let Some(digits) = trimmed.strip_prefix("0b").or_else(|| trimmed.strip_prefix("0B")) {
        return parse_radix(digits, 2);
    }
    if let Some(digits) = trimmed.strip_prefix("0o").or_else(|| trimmed.strip_prefix("0O")) {
        return parse_radix(digits, 8);
    }
    match trimmed {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    if unsigned
        .chars()
        .any(|character| character.is_ascii_alphabetic() && character != 'e' && character != 'E')
    {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

fn parse_radix(digits: &str, radix: u32) -> Option<f64> {
    if digits.is_empty() {
        return None;
    }
    u128::from_str_radix(digits, radix).ok().map(|value| value as f64)
}

/// JavaScript `===` for JSON values, with `JSON.stringify`-compatible numbers.
fn js_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::Array(_) | Value::Object(_), _) | (_, Value::Array(_) | Value::Object(_)) => deep_equal(left, right),
        _ => left == right,
    }
}

/// TypeBox `Guard.IsDeepEqual`.
fn deep_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(left, right)| js_equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, left)| right.get(key).is_some_and(|right| js_equal(left, right)))
        }
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Optional null normalization
// ---------------------------------------------------------------------------

fn normalize_optional_nulls(value: &mut Value, schema: &Value) {
    if let Value::Array(items) = value {
        normalize_array_nulls(items, schema);
        return;
    }
    normalize_object_nulls(value, schema);
}

fn normalize_array_nulls(items: &mut [Value], schema: &Value) {
    match schema.get("items") {
        Some(Value::Array(item_schemas)) => {
            for (index, item) in items.iter_mut().enumerate() {
                if let Some(item_schema) = item_schemas.get(index) {
                    normalize_optional_nulls(item, item_schema);
                }
            }
        }
        Some(item_schema) if item_schema.is_object() => {
            for item in items.iter_mut() {
                normalize_optional_nulls(item, item_schema);
            }
        }
        _ => {}
    }
}

fn normalize_object_nulls(value: &mut Value, schema: &Value) {
    if !value.is_object() {
        return;
    }
    let Some(properties) = object_field(schema, "properties") else {
        return;
    };
    let required: Vec<&str> = array_field(schema, "required")
        .map(|keys| keys.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let keys: Vec<String> = properties.keys().cloned().collect();
    let Some(map) = value.as_object_mut() else {
        return;
    };
    for key in keys {
        let Some(property_schema) = properties.get(&key) else {
            continue;
        };
        if !map.contains_key(&key) {
            continue;
        }
        let should_delete = map.get(&key).is_some_and(Value::is_null)
            && !required.contains(&key.as_str())
            && !matches!(property_schema.get("$ref"), Some(Value::String(_)))
            && !check(property_schema, &Value::Null);
        if should_delete {
            map.remove(&key);
        } else if let Some(property) = map.get_mut(&key) {
            normalize_optional_nulls(property, property_schema);
        }
    }
}

// ---------------------------------------------------------------------------
// TypeBox `Value.Convert`
// ---------------------------------------------------------------------------

enum Conversion {
    InPlace,
    Replaced(Value),
}

fn apply_conversion(value: &mut Value, conversion: Conversion) {
    if let Conversion::Replaced(next) = conversion {
        *value = next;
    }
}

fn convert_in_place(value: &mut Value, schema: &Value, context: &Map<String, Value>) {
    let conversion = convert(schema, value, context);
    apply_conversion(value, conversion);
}

fn convert_literal_in_place(value: &mut Value, constant: &Value) {
    let conversion = convert_literal(constant, value);
    apply_conversion(value, conversion);
}

fn convert(schema: &Value, value: &mut Value, context: &Map<String, Value>) -> Conversion {
    let Some(object) = schema.as_object() else {
        return Conversion::InPlace;
    };
    if let Some(conversion) = convert_reference(schema, object, value, context) {
        return conversion;
    }
    if let Some(members) = array_field(schema, "enum") {
        return convert_enum(members, value);
    }
    if let Some(members) = array_field(schema, "allOf") {
        for member in members {
            convert_in_place(value, member, context);
        }
        return Conversion::InPlace;
    }
    if let Some(constant) = schema.get("const") {
        return convert_literal(constant, value);
    }
    if let Some(members) = array_field(schema, "anyOf") {
        return convert_union(members, schema, value, context);
    }
    convert_typed(schema, object, value, context)
}

fn convert_reference(
    schema: &Value,
    object: &Map<String, Value>,
    value: &mut Value,
    context: &Map<String, Value>,
) -> Option<Conversion> {
    if object.contains_key("$defs")
        && let Some(reference) = string_field(schema, "$ref")
    {
        let mut merged = context.clone();
        if let Some(definitions) = object_field(schema, "$defs") {
            for (key, definition) in definitions {
                merged.insert(key.clone(), definition.clone());
            }
        }
        return Some(match merged.get(reference) {
            Some(target) => convert(target, value, &merged),
            None => Conversion::InPlace,
        });
    }
    if let Some(reference) = string_field(schema, "$ref") {
        return Some(match context.get(reference) {
            Some(target) => convert(target, value, context),
            None => Conversion::InPlace,
        });
    }
    None
}

fn convert_typed(
    schema: &Value,
    object: &Map<String, Value>,
    value: &mut Value,
    context: &Map<String, Value>,
) -> Conversion {
    match string_field(schema, "type") {
        Some("array") => convert_array(schema.get("items"), value, context),
        Some("object") => {
            if object.contains_key("patternProperties") {
                convert_record(object, value, context)
            } else {
                convert_object(object, value, context)
            }
        }
        Some("integer") => replace_if_changed(value, try_integer(value)),
        Some("number") => replace_if_changed(value, try_number(value)),
        Some("boolean") => replace_if_changed(value, try_boolean(value)),
        Some("null") => replace_if_changed(value, try_null(value)),
        Some("string") => replace_if_changed(value, try_string(value)),
        _ => Conversion::InPlace,
    }
}

fn replace_if_changed(value: &Value, next: Option<Value>) -> Conversion {
    match next {
        Some(next) if next != *value => Conversion::Replaced(next),
        _ => Conversion::InPlace,
    }
}

fn convert_literal(constant: &Value, value: &mut Value) -> Conversion {
    if js_equal(constant, value) {
        return Conversion::InPlace;
    }
    let converted = match constant {
        Value::String(_) => try_string(value),
        Value::Bool(_) => try_boolean(value),
        Value::Number(_) => try_number(value),
        _ => None,
    };
    match converted {
        Some(converted) if js_equal(constant, &converted) => Conversion::Replaced(converted),
        _ => Conversion::InPlace,
    }
}

fn convert_enum(members: &[Value], value: &mut Value) -> Conversion {
    if members.iter().any(|member| js_equal(member, value)) {
        return Conversion::InPlace;
    }
    for member in members {
        let mut candidate = value.clone();
        convert_literal_in_place(&mut candidate, member);
        if members.iter().any(|member| js_equal(member, &candidate)) {
            return Conversion::Replaced(candidate);
        }
    }
    Conversion::InPlace
}

fn convert_union(
    members: &[Value],
    union_schema: &Value,
    value: &mut Value,
    context: &Map<String, Value>,
) -> Conversion {
    if members.iter().any(|member| check(member, value)) {
        return Conversion::InPlace;
    }
    for member in members {
        let mut candidate = value.clone();
        convert_in_place(&mut candidate, member, context);
        if check(union_schema, &candidate) {
            return Conversion::Replaced(candidate);
        }
    }
    Conversion::InPlace
}

fn convert_array(items: Option<&Value>, value: &mut Value, context: &Map<String, Value>) -> Conversion {
    match items {
        Some(Value::Array(item_schemas)) => convert_tuple(item_schemas, value, context),
        Some(item_schema) if item_schema.is_object() => convert_array_items(item_schema, value, context),
        _ => Conversion::InPlace,
    }
}

fn convert_tuple(item_schemas: &[Value], value: &mut Value, context: &Map<String, Value>) -> Conversion {
    if let Value::Array(values) = value {
        for (index, item) in values.iter_mut().enumerate() {
            if let Some(item_schema) = item_schemas.get(index) {
                convert_in_place(item, item_schema, context);
            }
        }
    }
    Conversion::InPlace
}

fn convert_array_items(item_schema: &Value, value: &mut Value, context: &Map<String, Value>) -> Conversion {
    if let Value::Array(values) = value {
        for item in values.iter_mut() {
            convert_in_place(item, item_schema, context);
        }
        return Conversion::InPlace;
    }
    // TypeBox's `TryArray` wraps non-arrays before mapping.
    let mut wrapped = value.clone();
    convert_in_place(&mut wrapped, item_schema, context);
    Conversion::Replaced(Value::Array(vec![wrapped]))
}

fn convert_object(schema: &Map<String, Value>, value: &mut Value, context: &Map<String, Value>) -> Conversion {
    let Some(map) = value.as_object_mut() else {
        return Conversion::InPlace;
    };
    convert_object_properties(schema, map, context);
    convert_object_additional(schema, map, context);
    Conversion::InPlace
}

fn convert_object_properties(schema: &Map<String, Value>, map: &mut Map<String, Value>, context: &Map<String, Value>) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    for (key, property_schema) in properties {
        if let Some(property) = map.get_mut(key) {
            convert_in_place(property, property_schema, context);
        }
    }
}

fn convert_object_additional(schema: &Map<String, Value>, map: &mut Map<String, Value>, context: &Map<String, Value>) {
    let Some(additional) = schema.get("additionalProperties").filter(|value| value.is_object()) else {
        return;
    };
    let patterns: Vec<String> = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().map(|key| format!("^{key}$")).collect())
        .unwrap_or_default();
    let keys: Vec<String> = map.keys().cloned().collect();
    for pattern in patterns {
        let regex = Regex::new(&pattern).ok();
        for key in &keys {
            if regex.as_ref().is_some_and(|regex| regex.is_match(key)) {
                continue;
            }
            if let Some(property) = map.get_mut(key) {
                convert_in_place(property, additional, context);
            }
        }
    }
}

fn convert_record(schema: &Map<String, Value>, value: &mut Value, context: &Map<String, Value>) -> Conversion {
    let Some(map) = value.as_object_mut() else {
        return Conversion::InPlace;
    };
    let Some(patterns) = schema.get("patternProperties").and_then(Value::as_object) else {
        return Conversion::InPlace;
    };
    convert_record_patterns(patterns, map, context);
    convert_record_additional(patterns, schema, map, context);
    Conversion::InPlace
}

fn convert_record_patterns(patterns: &Map<String, Value>, map: &mut Map<String, Value>, context: &Map<String, Value>) {
    for (pattern, property_schema) in patterns {
        let regex = Regex::new(pattern).ok();
        let keys: Vec<String> = map.keys().cloned().collect();
        for key in keys {
            if regex.as_ref().is_some_and(|regex| regex.is_match(&key))
                && let Some(property) = map.get_mut(&key)
            {
                convert_in_place(property, property_schema, context);
            }
        }
    }
}

fn convert_record_additional(
    patterns: &Map<String, Value>,
    schema: &Map<String, Value>,
    map: &mut Map<String, Value>,
    context: &Map<String, Value>,
) {
    let Some(additional) = schema.get("additionalProperties").filter(|value| value.is_object()) else {
        return;
    };
    let keys: Vec<String> = map.keys().cloned().collect();
    for pattern in patterns.keys() {
        let regex = Regex::new(pattern).ok();
        for key in &keys {
            if regex.as_ref().is_some_and(|regex| regex.is_match(key)) {
                continue;
            }
            if let Some(property) = map.get_mut(key) {
                convert_in_place(property, additional, context);
            }
        }
    }
}

fn try_number(value: &Value) -> Option<Value> {
    match value {
        Value::Number(number) => Some(Value::Number(number.clone())),
        Value::Bool(boolean) => Some(json!(if *boolean { 1 } else { 0 })),
        Value::Null => Some(json!(0)),
        Value::String(text) => number_from_string(text),
        _ => None,
    }
}

fn number_from_string(text: &str) -> Option<Value> {
    if let Some(parsed) = js_number_parse(text).filter(|parsed| parsed.is_finite()) {
        return Some(js_number(parsed));
    }
    let lowercase = text.to_lowercase();
    if lowercase == "false" {
        return Some(json!(0));
    }
    if lowercase == "true" {
        return Some(json!(1));
    }
    let digits = text.strip_suffix('n')?;
    if digits.is_empty() {
        return None;
    }
    digits.parse::<i64>().ok().map(Value::from)
}

fn try_integer(value: &Value) -> Option<Value> {
    try_number(value).map(|number| {
        let parsed = number.as_f64().unwrap_or(0.0);
        js_number(parsed.trunc())
    })
}

fn try_boolean(value: &Value) -> Option<Value> {
    match value {
        Value::Bool(boolean) => Some(json!(*boolean)),
        Value::Number(number) => match number.as_f64() {
            Some(0.0) => Some(json!(false)),
            Some(1.0) => Some(json!(true)),
            _ => None,
        },
        Value::Null => Some(json!(false)),
        Value::String(text) => {
            let lowercase = text.to_lowercase();
            if lowercase == "false" || text == "0" {
                Some(json!(false))
            } else if lowercase == "true" || text == "1" {
                Some(json!(true))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn try_string(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(text.clone())),
        Value::Bool(boolean) => Some(Value::String(boolean.to_string())),
        Value::Number(number) => Some(Value::String(js_number_to_string(number.as_f64().unwrap_or(f64::NAN)))),
        Value::Null => Some(json!("null")),
        _ => None,
    }
}

fn try_null(value: &Value) -> Option<Value> {
    match value {
        Value::Null => Some(Value::Null),
        Value::Bool(false) => Some(Value::Null),
        Value::Number(number) if number.as_f64() == Some(0.0) => Some(Value::Null),
        Value::String(text) => {
            let lowercase = text.to_lowercase();
            if lowercase == "undefined" || lowercase == "null" || text.is_empty() || text == "0" {
                Some(Value::Null)
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Plain JSON schema coercion (`coerceWithJsonSchema`)
// ---------------------------------------------------------------------------

fn coerce_in_place(value: &mut Value, schema: &Value) {
    let conversion = coerce_into(schema, value);
    apply_conversion(value, conversion);
}

fn coerce(schema: &Value, mut value: Value) -> Value {
    coerce_in_place(&mut value, schema);
    value
}

fn coerce_into(schema: &Value, value: &mut Value) -> Conversion {
    let Some(object) = schema.as_object() else {
        return Conversion::InPlace;
    };
    if let Some(all_of) = array_field(schema, "allOf") {
        for nested in all_of {
            coerce_in_place(value, nested);
        }
    }
    if let Some(any_of) = array_field(schema, "anyOf") {
        let conversion = coerce_union(any_of, value);
        apply_conversion(value, conversion);
    }
    if let Some(one_of) = array_field(schema, "oneOf") {
        let conversion = coerce_union(one_of, value);
        apply_conversion(value, conversion);
    }

    let types = schema_types(object);
    let matches_union_member = types.len() > 1 && types.iter().any(|schema_type| matches_json_type(value, schema_type));
    if !types.is_empty() && !matches_union_member {
        for schema_type in &types {
            if let Some(candidate) = coerce_primitive_by_type(value, schema_type) {
                *value = candidate;
                break;
            }
        }
    }

    if types.iter().any(|schema_type| schema_type == "object") && value.is_object() {
        apply_object_coercion(value, object);
    }
    if types.iter().any(|schema_type| schema_type == "array") && value.is_array() {
        apply_array_coercion(value, object);
    }
    Conversion::InPlace
}

fn coerce_union(schemas: &[Value], value: &mut Value) -> Conversion {
    for schema in schemas {
        if check(schema, value) {
            return Conversion::InPlace;
        }
    }
    for schema in schemas {
        let mut candidate = value.clone();
        coerce_in_place(&mut candidate, schema);
        if check(schema, &candidate) {
            return Conversion::Replaced(candidate);
        }
    }
    Conversion::InPlace
}

fn schema_types(object: &Map<String, Value>) -> Vec<String> {
    match object.get("type") {
        Some(Value::String(schema_type)) => vec![schema_type.clone()],
        Some(Value::Array(types)) => types.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

fn matches_json_type(value: &Value, schema_type: &str) -> bool {
    match schema_type {
        "number" => value.is_number(),
        "integer" => value
            .as_f64()
            .is_some_and(|number| number.is_finite() && number.fract() == 0.0),
        "boolean" => value.is_boolean(),
        "string" => value.is_string(),
        "null" => value.is_null(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    }
}

fn coerce_primitive_by_type(value: &Value, schema_type: &str) -> Option<Value> {
    match schema_type {
        "number" => coerce_number(value, false),
        "integer" => coerce_number(value, true),
        "boolean" => match value {
            Value::Null => Some(json!(false)),
            Value::String(text) if text == "true" => Some(json!(true)),
            Value::String(text) if text == "false" => Some(json!(false)),
            Value::Number(number) if number.as_f64() == Some(1.0) => Some(json!(true)),
            Value::Number(number) if number.as_f64() == Some(0.0) => Some(json!(false)),
            _ => None,
        },
        "string" => match value {
            Value::Null => Some(json!("")),
            Value::Number(number) => Some(Value::String(js_number_to_string(number.as_f64().unwrap_or(f64::NAN)))),
            Value::Bool(boolean) => Some(Value::String(boolean.to_string())),
            _ => None,
        },
        "null" => match value {
            Value::String(text) if text.is_empty() => Some(Value::Null),
            Value::Number(number) if number.as_f64() == Some(0.0) => Some(Value::Null),
            Value::Bool(false) => Some(Value::Null),
            _ => None,
        },
        _ => None,
    }
}

fn coerce_number(value: &Value, integer: bool) -> Option<Value> {
    match value {
        Value::Null => Some(json!(0)),
        Value::String(text) if !text.trim().is_empty() => {
            let parsed = js_number_parse(text).filter(|parsed| parsed.is_finite())?;
            if integer && parsed.fract() != 0.0 {
                return None;
            }
            Some(js_number(parsed))
        }
        Value::Bool(boolean) => Some(json!(if *boolean { 1 } else { 0 })),
        _ => None,
    }
}

fn apply_object_coercion(value: &mut Value, schema: &Map<String, Value>) {
    let Some(map) = value.as_object_mut() else {
        return;
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    if let Some(properties) = properties {
        for (key, property_schema) in properties {
            if let Some(property) = map.get_mut(key) {
                coerce_in_place(property, property_schema);
            }
        }
    }
    let Some(additional) = schema.get("additionalProperties").filter(|value| value.is_object()) else {
        return;
    };
    let defined: Vec<&String> = properties
        .map(|properties| properties.keys().collect())
        .unwrap_or_default();
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        if defined.iter().any(|defined| **defined == key) {
            continue;
        }
        if let Some(property) = map.get_mut(&key) {
            coerce_in_place(property, additional);
        }
    }
}

fn apply_array_coercion(value: &mut Value, schema: &Map<String, Value>) {
    let Some(values) = value.as_array_mut() else {
        return;
    };
    match schema.get("items") {
        Some(Value::Array(item_schemas)) => {
            for (index, item) in values.iter_mut().enumerate() {
                if let Some(item_schema) = item_schemas.get(index) {
                    coerce_in_place(item, item_schema);
                }
            }
        }
        Some(item_schema) if item_schema.is_object() => {
            for item in values.iter_mut() {
                coerce_in_place(item, item_schema);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// JSON Schema validation (`Compile(...).Errors`)
// ---------------------------------------------------------------------------

struct Issue {
    instance_path: String,
    message: String,
    required_first: Option<String>,
}

impl Issue {
    fn new(instance_path: &str, message: String) -> Self {
        Self {
            instance_path: instance_path.to_owned(),
            message,
            required_first: None,
        }
    }

    fn path(&self) -> String {
        let base = self
            .instance_path
            .strip_prefix('/')
            .unwrap_or(&self.instance_path)
            .replace('/', ".");
        if let Some(required) = &self.required_first {
            return if base.is_empty() {
                required.clone()
            } else {
                format!("{base}.{required}")
            };
        }
        if base.is_empty() { "root".to_owned() } else { base }
    }
}

struct Issues {
    items: Vec<Issue>,
    limit: usize,
}

impl Issues {
    fn root() -> Self {
        Self {
            items: Vec::new(),
            limit: MAX_ERRORS,
        }
    }

    fn scratch() -> Self {
        Self {
            items: Vec::new(),
            limit: usize::MAX,
        }
    }

    fn add(&mut self, issue: Issue) {
        if self.items.len() < self.limit {
            self.items.push(issue);
        }
    }

    fn message(&mut self, instance_path: &str, message: String) {
        self.add(Issue::new(instance_path, message));
    }

    fn merge(&mut self, other: Issues) {
        for issue in other.items {
            self.add(issue);
        }
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

fn collect_issues(schema: &Value, value: &Value) -> Issues {
    let mut issues = Issues::root();
    collect_schema(schema, schema, schema, "", value, &mut issues);
    issues
}

fn check(schema: &Value, value: &Value) -> bool {
    collect_issues(schema, value).is_empty()
}

fn scratch_issues(root: &Value, base: &Value, schema: &Value, instance_path: &str, value: &Value) -> Issues {
    let mut issues = Issues::scratch();
    collect_schema(root, base, schema, instance_path, value, &mut issues);
    issues
}

fn child_path(instance_path: &str, key: &str) -> String {
    format!("{instance_path}/{key}")
}

fn collect_schema(root: &Value, base: &Value, schema: &Value, instance_path: &str, value: &Value, issues: &mut Issues) {
    let base = if schema.get("$id").is_some() { schema } else { base };
    if schema.as_bool() == Some(false) {
        issues.message(instance_path, "schema is false".to_owned());
        return;
    }
    let Some(object) = schema.as_object() else {
        return;
    };

    collect_type(object, instance_path, value, issues);
    if value.is_object() {
        collect_object_keywords(root, base, object, instance_path, value, issues);
    }
    if value.is_array() {
        collect_array_keywords(root, base, object, instance_path, value, issues);
    }
    if let Some(text) = value.as_str() {
        collect_string_keywords(object, instance_path, text, issues);
    }
    if let Some(number) = value.as_f64() {
        collect_number_keywords(object, instance_path, number, issues);
    }

    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        match resolve_ref(root, base, reference) {
            Some(target) => collect_schema(root, base, target, instance_path, value, issues),
            None => issues.message(instance_path, "schema is false".to_owned()),
        }
    }
    if let Some(constant) = object.get("const")
        && !js_equal(constant, value)
    {
        issues.message(instance_path, "must be equal to constant".to_owned());
    }
    if let Some(values) = array_field(schema, "enum")
        && !values.iter().any(|option| js_equal(option, value))
    {
        issues.message(instance_path, "must be equal to one of the allowed values".to_owned());
    }
    if let Some(if_schema) = object.get("if").filter(|schema| is_schema(schema)) {
        collect_if(root, base, object, if_schema, instance_path, value, issues);
    }
    if let Some(not_schema) = object.get("not").filter(|schema| is_schema(schema))
        && check(not_schema, value)
    {
        issues.message(instance_path, "must not be valid".to_owned());
    }
    if let Some(members) = array_field(schema, "allOf")
        && members.iter().all(is_schema)
    {
        for member in members {
            let failed = scratch_issues(root, base, member, instance_path, value);
            issues.merge(failed);
        }
    }
    if let Some(members) = array_field(schema, "anyOf")
        && members.iter().all(is_schema)
    {
        collect_any_of(root, base, members, instance_path, value, issues);
    }
    if let Some(members) = array_field(schema, "oneOf")
        && members.iter().all(is_schema)
    {
        collect_one_of(root, base, members, instance_path, value, issues);
    }
}

fn collect_type(schema: &Map<String, Value>, instance_path: &str, value: &Value, issues: &mut Issues) {
    let Some(type_spec) = schema.get("type") else {
        return;
    };
    let valid = match type_spec {
        Value::String(_) => true,
        Value::Array(types) => types.iter().all(Value::is_string),
        _ => false,
    };
    if valid && !check_type(type_spec, value) {
        issues.message(instance_path, type_message(type_spec));
    }
}

fn check_type(type_spec: &Value, value: &Value) -> bool {
    match type_spec {
        Value::String(schema_type) => check_type_name(schema_type, value),
        Value::Array(types) => types
            .iter()
            .filter_map(Value::as_str)
            .any(|schema_type| check_type_name(schema_type, value)),
        _ => true,
    }
}

fn check_type_name(schema_type: &str, value: &Value) -> bool {
    match schema_type {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "boolean" => value.is_boolean(),
        "integer" => value
            .as_f64()
            .is_some_and(|number| number.is_finite() && number.fract() == 0.0),
        "number" => value.is_number(),
        "null" => value.is_null(),
        "string" => value.is_string(),
        _ => true,
    }
}

fn type_message(type_spec: &Value) -> String {
    match type_spec {
        Value::String(schema_type) => format!("must be {schema_type}"),
        Value::Array(types) => {
            let names: Vec<&str> = types.iter().filter_map(Value::as_str).collect();
            format!("must be either {}", names.join(" or "))
        }
        _ => String::new(),
    }
}

fn collect_object_keywords(
    root: &Value,
    base: &Value,
    schema: &Map<String, Value>,
    instance_path: &str,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(map) = value.as_object() else {
        return;
    };
    if let Some(required) = object_array(schema, "required")
        && required.iter().all(Value::is_string)
    {
        let missing: Vec<String> = required
            .iter()
            .filter_map(Value::as_str)
            .filter(|key| !map.contains_key(*key))
            .map(str::to_owned)
            .collect();
        if !missing.is_empty() {
            let mut issue = Issue::new(
                instance_path,
                format!("must have required properties {}", missing.join(", ")),
            );
            issue.required_first = missing.first().cloned();
            issues.add(issue);
        }
    }
    if let Some(additional) = schema.get("additionalProperties").filter(|value| is_schema(value)) {
        collect_additional_properties(root, base, schema, additional, instance_path, map, issues);
    }
    if let Some(dependencies) = schema.get("dependencies").and_then(Value::as_object)
        && !map.is_empty()
        && dependencies.values().all(|dependency| {
            is_schema(dependency)
                || dependency
                    .as_array()
                    .is_some_and(|names| names.iter().all(Value::is_string))
        })
    {
        collect_dependencies(root, base, dependencies, instance_path, value, issues);
    }
    if let Some(dependencies) = schema.get("dependentRequired").and_then(Value::as_object)
        && !map.is_empty()
        && dependencies.values().all(|dependency| {
            dependency
                .as_array()
                .is_some_and(|names| names.iter().all(Value::is_string))
        })
    {
        collect_dependent_required(dependencies, instance_path, map, issues);
    }
    if let Some(dependencies) = schema.get("dependentSchemas").and_then(Value::as_object)
        && !map.is_empty()
        && dependencies.values().all(is_schema)
    {
        for (key, dependent) in dependencies {
            if map.contains_key(key) {
                collect_schema(root, base, dependent, instance_path, value, issues);
            }
        }
    }
    if let Some(patterns) = schema.get("patternProperties").and_then(Value::as_object)
        && patterns.values().all(is_schema)
    {
        for (pattern, property_schema) in patterns {
            let regex = Regex::new(pattern).ok();
            for (key, property) in map {
                if regex.as_ref().is_some_and(|regex| regex.is_match(key)) {
                    collect_schema(
                        root,
                        base,
                        property_schema,
                        &child_path(instance_path, key),
                        property,
                        issues,
                    );
                }
            }
        }
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object)
        && properties.values().all(is_schema)
    {
        for (key, property_schema) in properties {
            if let Some(property) = map.get(key) {
                collect_schema(
                    root,
                    base,
                    property_schema,
                    &child_path(instance_path, key),
                    property,
                    issues,
                );
            }
        }
    }
    if let Some(property_names) = schema.get("propertyNames").filter(|value| is_schema(value)) {
        let mut invalid = Vec::new();
        for key in map.keys() {
            let key_value = Value::String(key.clone());
            if !scratch_issues(root, base, property_names, instance_path, &key_value).is_empty() {
                invalid.push(key.clone());
            }
        }
        if !invalid.is_empty() {
            issues.message(
                instance_path,
                format!("property names {} are invalid", invalid.join(", ")),
            );
        }
    }
    if let Some(minimum) = object_number(schema, "minProperties")
        && (map.len() as f64) < minimum
    {
        issues.message(
            instance_path,
            format!("must not have fewer than {} properties", js_number_to_string(minimum)),
        );
    }
    if let Some(maximum) = object_number(schema, "maxProperties")
        && (map.len() as f64) > maximum
    {
        issues.message(
            instance_path,
            format!("must not have more than {} properties", js_number_to_string(maximum)),
        );
    }
}

fn collect_additional_properties(
    root: &Value,
    base: &Value,
    schema: &Map<String, Value>,
    additional: &Value,
    instance_path: &str,
    map: &Map<String, Value>,
    issues: &mut Issues,
) {
    let regex = additional_properties_regex(schema);
    let mut additional_keys = Vec::new();
    for (key, property) in map {
        if regex.as_ref().is_some_and(|regex| regex.is_match(key)) {
            continue;
        }
        let property_issues = scratch_issues(root, base, additional, &child_path(instance_path, key), property);
        if !property_issues.is_empty() {
            additional_keys.push(key.clone());
        }
    }
    if !additional_keys.is_empty() {
        issues.message(instance_path, "must not have additional properties".to_owned());
    }
}

/// TypeBox `GetPropertiesPattern`: `patternProperties` keys plus anchored
/// `properties` keys. A schema without either never matches.
fn additional_properties_regex(schema: &Map<String, Value>) -> Option<Regex> {
    let mut patterns: Vec<String> = Vec::new();
    if let Some(patterns_object) = schema.get("patternProperties").and_then(Value::as_object) {
        patterns.extend(patterns_object.keys().cloned());
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        patterns.extend(properties.keys().map(|key| format!("^{}$", escape_js_regex(key))));
    }
    if patterns.is_empty() {
        return None;
    }
    Regex::new(&format!("({})", patterns.join("|"))).ok()
}

/// JavaScript `key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')`.
fn escape_js_regex(key: &str) -> String {
    const SPECIAL: &[char] = &['.', '*', '+', '?', '^', '$', '{', '}', '(', ')', '|', '[', ']', '\\'];
    let mut escaped = String::with_capacity(key.len());
    for character in key.chars() {
        if SPECIAL.contains(&character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn collect_dependencies(
    root: &Value,
    base: &Value,
    dependencies: &Map<String, Value>,
    instance_path: &str,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(map) = value.as_object() else {
        return;
    };
    for (key, dependency) in dependencies {
        if !map.contains_key(key) {
            continue;
        }
        if let Some(names) = dependency.as_array() {
            let names: Vec<&str> = names.iter().filter_map(Value::as_str).collect();
            // `Array.prototype.every` stops at the first missing dependency.
            if names.iter().any(|name| !map.contains_key(*name)) {
                issues.message(
                    instance_path,
                    format!(
                        "must have properties {} when property {} is present",
                        names.join(", "),
                        key
                    ),
                );
            }
        } else if is_schema(dependency) {
            collect_schema(root, base, dependency, instance_path, value, issues);
        }
    }
}

fn collect_dependent_required(
    dependencies: &Map<String, Value>,
    instance_path: &str,
    map: &Map<String, Value>,
    issues: &mut Issues,
) {
    for (key, dependency) in dependencies {
        if !map.contains_key(key) {
            continue;
        }
        let Some(names) = dependency.as_array() else {
            continue;
        };
        let names: Vec<&str> = names.iter().filter_map(Value::as_str).collect();
        for name in &names {
            if !map.contains_key(*name) {
                issues.message(
                    instance_path,
                    format!(
                        "must have properties {} when property {} is present",
                        names.join(", "),
                        key
                    ),
                );
            }
        }
    }
}

fn collect_array_keywords(
    root: &Value,
    base: &Value,
    schema: &Map<String, Value>,
    instance_path: &str,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(values) = value.as_array() else {
        return;
    };
    let items = schema.get("items");
    if let (Some(Value::Array(item_schemas)), Some(additional)) =
        (items, schema.get("additionalItems").filter(|value| is_schema(value)))
    {
        for (index, item) in values.iter().enumerate() {
            if index < item_schemas.len() {
                continue;
            }
            collect_schema(
                root,
                base,
                additional,
                &child_path(instance_path, &index.to_string()),
                item,
                issues,
            );
        }
    }
    if let Some(contains) = schema.get("contains").filter(|value| is_schema(value))
        && object_number(schema, "minContains") != Some(0.0)
    {
        let matched = values.iter().any(|item| check(contains, item));
        if values.is_empty() || !matched {
            issues.message(instance_path, "must contain at least 1 valid item".to_owned());
        }
    }
    if let Some(Value::Array(item_schemas)) = items {
        for (index, item) in values.iter().enumerate() {
            let Some(item_schema) = item_schemas.get(index) else {
                break;
            };
            collect_schema(
                root,
                base,
                item_schema,
                &child_path(instance_path, &index.to_string()),
                item,
                issues,
            );
        }
    } else if let Some(item_schema) = items.filter(|value| is_schema(value)) {
        let offset = object_array(schema, "prefixItems")
            .map(|prefix| prefix.len())
            .unwrap_or(0);
        for (index, item) in values.iter().enumerate().skip(offset) {
            collect_schema(
                root,
                base,
                item_schema,
                &child_path(instance_path, &index.to_string()),
                item,
                issues,
            );
        }
    }
    if let Some(contains) = schema.get("contains").filter(|value| is_schema(value)) {
        if let Some(maximum) = object_number(schema, "maxContains") {
            let count = values.iter().filter(|item| check(contains, item)).count();
            if (count as f64) > maximum {
                issues.message(instance_path, "must contain at least 1 valid item".to_owned());
            }
        }
        if let Some(minimum) = object_number(schema, "minContains") {
            let count = values.iter().filter(|item| check(contains, item)).count();
            if (count as f64) < minimum {
                issues.message(instance_path, "must contain at least 1 valid item".to_owned());
            }
        }
    }
    if let Some(maximum) = object_number(schema, "maxItems")
        && (values.len() as f64) > maximum
    {
        issues.message(
            instance_path,
            format!("must not have more than {} items", js_number_to_string(maximum)),
        );
    }
    if let Some(minimum) = object_number(schema, "minItems")
        && (values.len() as f64) < minimum
    {
        issues.message(
            instance_path,
            format!("must not have fewer than {} items", js_number_to_string(minimum)),
        );
    }
    if let Some(prefix_items) = object_array(schema, "prefixItems")
        && prefix_items.iter().all(is_schema)
        && !values.is_empty()
    {
        for (index, item_schema) in prefix_items.iter().enumerate() {
            let Some(item) = values.get(index) else {
                break;
            };
            collect_schema(
                root,
                base,
                item_schema,
                &child_path(instance_path, &index.to_string()),
                item,
                issues,
            );
        }
    }
    if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
        let has_duplicates = values
            .iter()
            .enumerate()
            .any(|(index, item)| values.iter().skip(index + 1).any(|other| deep_equal(item, other)));
        if has_duplicates {
            issues.message(instance_path, "must not have duplicate items".to_owned());
        }
    }
}

fn collect_string_keywords(schema: &Map<String, Value>, instance_path: &str, text: &str, issues: &mut Issues) {
    let units: Vec<u16> = text.encode_utf16().collect();
    if let Some(maximum) = object_number(schema, "maxLength")
        && !is_max_length_fast(&units, maximum)
    {
        issues.message(
            instance_path,
            format!("must not have more than {} characters", js_number_to_string(maximum)),
        );
    }
    if let Some(minimum) = object_number(schema, "minLength")
        && !is_min_length_fast(&units, minimum)
    {
        issues.message(
            instance_path,
            format!("must not have fewer than {} characters", js_number_to_string(minimum)),
        );
    }
    // `format` is not enforced: Pi tool schemas never use it. See the
    // traceability doc for the recorded gap.
    if let Some(pattern) = object_string(schema, "pattern") {
        let matched = Regex::new(pattern).is_ok_and(|regex| regex.is_match(text));
        if !matched {
            issues.message(instance_path, format!("must match pattern \"{pattern}\""));
        }
    }
}

fn collect_number_keywords(schema: &Map<String, Value>, instance_path: &str, number: f64, issues: &mut Issues) {
    if let Some(limit) = object_number(schema, "exclusiveMaximum")
        && number >= limit
    {
        issues.message(instance_path, format!("must be < {}", js_number_to_string(limit)));
    }
    if let Some(limit) = object_number(schema, "exclusiveMinimum")
        && number <= limit
    {
        issues.message(instance_path, format!("must be > {}", js_number_to_string(limit)));
    }
    if let Some(limit) = object_number(schema, "maximum")
        && number > limit
    {
        issues.message(instance_path, format!("must be <= {}", js_number_to_string(limit)));
    }
    if let Some(limit) = object_number(schema, "minimum")
        && number < limit
    {
        issues.message(instance_path, format!("must be >= {}", js_number_to_string(limit)));
    }
    if let Some(divisor) = object_number(schema, "multipleOf")
        && !is_multiple_of(number, divisor)
    {
        issues.message(
            instance_path,
            format!("must be multiple of {}", js_number_to_string(divisor)),
        );
    }
}

fn is_multiple_of(dividend: f64, divisor: f64) -> bool {
    let tolerance = 1e-10;
    if dividend.fract() == 0.0 && (1.0 / divisor) % 1.0 == 0.0 {
        return true;
    }
    let remainder = dividend % divisor;
    remainder
        .abs()
        .min((remainder - divisor).abs())
        .min((remainder + divisor).abs())
        < tolerance
}

fn collect_if(
    root: &Value,
    base: &Value,
    schema: &Map<String, Value>,
    if_schema: &Value,
    instance_path: &str,
    value: &Value,
    issues: &mut Issues,
) {
    let condition = scratch_issues(root, base, if_schema, instance_path, value);
    if condition.is_empty() {
        let Some(then_schema) = schema.get("then").filter(|value| is_schema(value)) else {
            return;
        };
        let then_issues = scratch_issues(root, base, then_schema, instance_path, value);
        if !then_issues.is_empty() {
            issues.message(instance_path, "must match \"then\" schema".to_owned());
        }
        return;
    }
    if let Some(else_schema) = schema.get("else").filter(|value| is_schema(value)) {
        let else_issues = scratch_issues(root, base, else_schema, instance_path, value);
        if !else_issues.is_empty() {
            issues.merge(else_issues);
            issues.message(instance_path, "must match \"else\" schema".to_owned());
        }
    }
}

fn collect_any_of(
    root: &Value,
    base: &Value,
    members: &[Value],
    instance_path: &str,
    value: &Value,
    issues: &mut Issues,
) {
    let mut failures = Vec::new();
    let mut passed = false;
    for member in members {
        let member_issues = scratch_issues(root, base, member, instance_path, value);
        if member_issues.is_empty() {
            passed = true;
        } else {
            failures.push(member_issues);
        }
    }
    if passed {
        return;
    }
    for failure in failures {
        issues.merge(failure);
    }
    issues.message(instance_path, "must match a schema in anyOf".to_owned());
}

fn collect_one_of(
    root: &Value,
    base: &Value,
    members: &[Value],
    instance_path: &str,
    value: &Value,
    issues: &mut Issues,
) {
    let mut failures = Vec::new();
    let mut passed = 0;
    for member in members {
        let member_issues = scratch_issues(root, base, member, instance_path, value);
        if member_issues.is_empty() {
            passed += 1;
        } else {
            failures.push(member_issues);
        }
    }
    if passed == 1 {
        return;
    }
    if passed == 0 {
        for failure in failures {
            issues.merge(failure);
        }
    }
    issues.message(instance_path, "must match exactly one schema in oneOf".to_owned());
}

// ---------------------------------------------------------------------------
// Reference resolution (TypeBox `Stack.Ref` / `Resolve.Ref`)
// ---------------------------------------------------------------------------

fn resolve_ref<'a>(root: &'a Value, base: &'a Value, reference: &str) -> Option<&'a Value> {
    if reference == "#" {
        return Some(base);
    }
    if let Some(fragment) = reference.strip_prefix('#') {
        if fragment.starts_with('/') {
            return pointer_get(base, fragment);
        }
        return find_anchor(base, fragment);
    }
    find_id(root, reference)
}

fn find_id<'a>(schema: &'a Value, reference: &str) -> Option<&'a Value> {
    if string_field(schema, "$id") == Some(reference) {
        return Some(schema);
    }
    match schema {
        Value::Object(map) => map.values().find_map(|value| find_id(value, reference)),
        Value::Array(items) => items.iter().find_map(|item| find_id(item, reference)),
        _ => None,
    }
}

fn find_anchor<'a>(schema: &'a Value, anchor: &str) -> Option<&'a Value> {
    if string_field(schema, "$anchor") == Some(anchor) || string_field(schema, "$dynamicAnchor") == Some(anchor) {
        return Some(schema);
    }
    match schema {
        Value::Object(map) => map.values().find_map(|value| find_anchor(value, anchor)),
        Value::Array(items) => items.iter().find_map(|item| find_anchor(item, anchor)),
        _ => None,
    }
}

fn pointer_get<'a>(schema: &'a Value, pointer: &str) -> Option<&'a Value> {
    let decoded = percent_decode(pointer);
    let mut current = schema;
    for token in decoded.trim_start_matches('/').split('/') {
        let token = token.replace("~1", "/").replace("~0", "~");
        current = match current {
            Value::Object(map) => map.get(&token)?,
            Value::Array(items) => items.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(value) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                decoded.push(value);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

// ---------------------------------------------------------------------------
// Grapheme counting (TypeBox `guard/string`)
// ---------------------------------------------------------------------------

fn is_between(value: u32, minimum: u32, maximum: u32) -> bool {
    value >= minimum && value <= maximum
}

fn is_zero_width_joiner(value: u32) -> bool {
    value == 0x200D
}

fn is_high_surrogate(value: u32) -> bool {
    is_between(value, 0xD800, 0xDBFF)
}

fn is_regional_indicator(value: u32) -> bool {
    is_between(value, 0x1F1E6, 0x1F1FF)
}

fn is_variation_selector(value: u32) -> bool {
    is_between(value, 0xFE00, 0xFE0F)
}

fn is_combining_mark(value: u32) -> bool {
    is_between(value, 0x0300, 0x036F)
        || is_between(value, 0x1AB0, 0x1AFF)
        || is_between(value, 0x1DC0, 0x1DFF)
        || is_between(value, 0xFE20, 0xFE2F)
}

fn code_point_length(value: u32) -> usize {
    if value > 0xFFFF { 2 } else { 1 }
}

fn code_point_at(units: &[u16], index: usize) -> u32 {
    let first = u32::from(units[index]);
    if is_high_surrogate(first)
        && let Some(second) = units.get(index + 1).map(|unit| u32::from(*unit))
        && is_between(second, 0xDC00, 0xDFFF)
    {
        return 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
    }
    first
}

fn consume_modifiers(units: &[u16], mut index: usize) -> usize {
    while index < units.len() {
        let point = code_point_at(units, index);
        if is_combining_mark(point) || is_variation_selector(point) {
            index += code_point_length(point);
        } else {
            break;
        }
    }
    index
}

fn next_grapheme_cluster_index(units: &[u16], cluster_start: usize) -> usize {
    let start_point = code_point_at(units, cluster_start);
    let mut cluster_end = cluster_start + code_point_length(start_point);
    cluster_end = consume_modifiers(units, cluster_end);
    while cluster_end + 1 < units.len() && units[cluster_end] == 0x200D {
        let next_point = code_point_at(units, cluster_end + 1);
        cluster_end += 1 + code_point_length(next_point);
        cluster_end = consume_modifiers(units, cluster_end);
    }
    if is_regional_indicator(start_point)
        && cluster_end < units.len()
        && is_regional_indicator(code_point_at(units, cluster_end))
    {
        cluster_end += code_point_length(code_point_at(units, cluster_end));
    }
    cluster_end
}

fn is_grapheme_code_point(unit: u16) -> bool {
    let unit = u32::from(unit);
    is_high_surrogate(unit) || is_combining_mark(unit) || is_variation_selector(unit) || is_zero_width_joiner(unit)
}

fn is_min_length(units: &[u16], minimum: f64) -> bool {
    if minimum == 0.0 {
        return true;
    }
    let mut count = 0;
    let mut index = 0;
    while index < units.len() {
        index = next_grapheme_cluster_index(units, index);
        count += 1;
        if count as f64 >= minimum {
            return true;
        }
    }
    false
}

fn is_max_length(units: &[u16], maximum: f64) -> bool {
    let mut count = 0;
    let mut index = 0;
    while index < units.len() {
        index = next_grapheme_cluster_index(units, index);
        count += 1;
        if count as f64 > maximum {
            return false;
        }
    }
    true
}

fn is_min_length_fast(units: &[u16], minimum: f64) -> bool {
    if minimum == 0.0 {
        return true;
    }
    let mut index = 0;
    while index < units.len() {
        if is_grapheme_code_point(units[index]) {
            return is_min_length(units, minimum);
        }
        index += 1;
        if index as f64 >= minimum {
            return true;
        }
    }
    false
}

fn is_max_length_fast(units: &[u16], maximum: f64) -> bool {
    let mut index = 0;
    while index < units.len() {
        if is_grapheme_code_point(units[index]) {
            return is_max_length(units, maximum);
        }
        index += 1;
        if index as f64 > maximum {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// `JSON.stringify(value, null, 2)`
// ---------------------------------------------------------------------------

fn json_stringify_pretty(value: &Value) -> String {
    let mut output = String::new();
    write_json(&mut output, value, 0);
    output
}

fn write_json(output: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(boolean) => output.push_str(if *boolean { "true" } else { "false" }),
        Value::Number(number) => output.push_str(&js_number_to_string(number.as_f64().unwrap_or(f64::NAN))),
        Value::String(text) => write_json_string(output, text),
        Value::Array(items) => {
            if items.is_empty() {
                output.push_str("[]");
                return;
            }
            output.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push_str(",\n");
                }
                push_indent(output, indent + 1);
                write_json(output, item, indent + 1);
            }
            output.push('\n');
            push_indent(output, indent);
            output.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                output.push_str("{}");
                return;
            }
            output.push_str("{\n");
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    output.push_str(",\n");
                }
                push_indent(output, indent + 1);
                write_json_string(output, key);
                output.push_str(": ");
                write_json(output, item, indent + 1);
            }
            output.push('\n');
            push_indent(output, indent);
            output.push('}');
        }
    }
}

fn push_indent(output: &mut String, indent: usize) {
    for _ in 0..indent {
        output.push_str("  ");
    }
}

fn write_json_string(output: &mut String, text: &str) {
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
            character if (character as u32) < 0x20 => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
}
