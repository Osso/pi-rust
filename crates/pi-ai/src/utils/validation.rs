//! Port of `packages/ai/src/utils/validation.ts`.
//!
//! Frozen source `4f9e6f8a4` clones arguments, runs `Value.Convert`, applies
//! plain-schema coercion, then validates. Explicit optional nulls participate
//! in conversion and validation rather than being deleted.
//!
//! The frozen worktree's TypeBox 1.1.38 builders use hidden `~kind` strings,
//! not `Symbol.for("TypeBox.Kind")`, so plain-schema coercion also runs after
//! their conversion. `Tool::typebox_kind` gates the conversion pass; it cannot
//! represent distinct nested node kinds. Remaining parity boundaries are
//! recorded in `docs/porting/traceability/validation.md`.

use std::fmt;

use super::validation_regex::JsRegex as Regex;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value, stringify_lossless_pretty};
use serde_json::Value as Schema;

use crate::types::{Tool, ToolCall};
use pi_js_compat::number::js_number_to_string;

/// TypeBox's default `Settings.maxErrors`.
const MAX_ERRORS: usize = 8;

/// Failure of [`validate_tool_arguments`] or [`validate_tool_call`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    message: JsString,
}

impl ValidationError {
    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.message.to_utf8() {
            Ok(text) => formatter.write_str(&text),
            Err(_) => formatter.write_str(&pi_js_compat::js_json::stringify_lossless(&Value::String(
                self.message.clone(),
            ))),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Finds a tool by name and validates the tool call arguments against its schema.
pub fn validate_tool_call(tools: &[Tool], tool_call: &ToolCall) -> Result<Value, ValidationError> {
    let tool = tools
        .iter()
        .find(|tool| tool.name == tool_call.name)
        .ok_or_else(|| ValidationError {
            message: format!("Tool \"{}\" not found", tool_call.name).into(),
        })?;
    validate_tool_arguments(tool, tool_call)
}

/// Validates tool call arguments against the tool's schema.
pub fn validate_tool_arguments(tool: &Tool, tool_call: &ToolCall) -> Result<Value, ValidationError> {
    validate_arguments(
        &Value::from(&tool.parameters),
        &Value::from(Schema::Object(tool_call.arguments.clone())),
        tool.typebox_kind,
        &tool_call.name,
    )
}

/// Lossless authority. Clones arguments without crossing a Unicode-scalar boundary.
pub fn validate_arguments(
    schema: &Value,
    arguments: &Value,
    typebox_kind: bool,
    tool_name: &str,
) -> Result<Value, ValidationError> {
    let mut args = arguments.clone();
    if typebox_kind {
        let _ = convert(schema, &mut args, &JsObject::new());
    }
    let coerced = coerce(schema, args.clone());
    if coerced != args {
        if args.is_object() && coerced.is_object() {
            args = coerced;
        } else {
            return Ok(if check(schema, &coerced) { coerced } else { args });
        }
    }
    let issues = collect_issues(schema, &args);
    if issues.is_empty() {
        return Ok(args);
    }
    let errors = JsString::join(issues.items.iter().map(Issue::line), "\n");
    let message = JsString::join(
        [
            JsString::from(format!("Validation failed for tool \"{tool_name}\":\n")),
            errors,
            JsString::from("\n\nReceived arguments:\n"),
            JsString::from(stringify_lossless_pretty(arguments)),
        ],
        "",
    );
    Err(ValidationError { message })
}

// ---------------------------------------------------------------------------
// Schema accessors
// ---------------------------------------------------------------------------

fn string_field(schema: &Value, key: &str) -> Option<String> {
    schema.get(key).and_then(schema_text)
}

fn object_field<'a>(schema: &'a Value, key: &str) -> Option<&'a JsObject> {
    schema.get(key).and_then(Value::as_object)
}

fn array_field<'a>(schema: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    schema.get(key).and_then(Value::as_array)
}

fn object_string<'a>(object: &'a JsObject, key: &str) -> Option<&'a JsString> {
    object.get(key).and_then(Value::as_string)
}

fn object_array<'a>(object: &'a JsObject, key: &str) -> Option<&'a Vec<Value>> {
    object.get(key).and_then(Value::as_array)
}

fn object_number(object: &JsObject, key: &str) -> Option<f64> {
    object.get(key).and_then(Value::as_f64)
}

fn is_schema(value: &Value) -> bool {
    value.is_object() || value.as_bool().is_some()
}

// ---------------------------------------------------------------------------
// JavaScript value semantics
// ---------------------------------------------------------------------------

/// JavaScript `===` for JSON values, with `JSON.stringify`-compatible numbers.
fn js_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left == right,
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
        (Value::Number(left), Value::Number(right)) => left == right,
        _ => false,
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

fn convert_in_place(value: &mut Value, schema: &Value, context: &JsObject) {
    let conversion = convert(schema, value, context);
    apply_conversion(value, conversion);
}

fn convert_literal_in_place(value: &mut Value, constant: &Value) {
    let conversion = convert_literal(constant, value);
    apply_conversion(value, conversion);
}

fn convert(schema: &Value, value: &mut Value, context: &JsObject) -> Conversion {
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

fn convert_reference(schema: &Value, object: &JsObject, value: &mut Value, context: &JsObject) -> Option<Conversion> {
    if object.contains_key("$defs")
        && let Some(reference) = string_field(schema, "$ref")
    {
        let mut merged = context.clone();
        if let Some(definitions) = object_field(schema, "$defs") {
            for (key, definition) in definitions {
                merged.insert(key.clone(), definition.clone());
            }
        }
        return Some(match merged.get(&reference) {
            Some(target) => convert(target, value, &merged),
            None => Conversion::InPlace,
        });
    }
    if let Some(reference) = string_field(schema, "$ref") {
        return Some(match context.get(&reference) {
            Some(target) => convert(target, value, context),
            None => Conversion::InPlace,
        });
    }
    None
}

fn convert_typed(schema: &Value, object: &JsObject, value: &mut Value, context: &JsObject) -> Conversion {
    match string_field(schema, "type").as_deref() {
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

fn convert_union(members: &[Value], union_schema: &Value, value: &mut Value, context: &JsObject) -> Conversion {
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

fn convert_array(items: Option<&Value>, value: &mut Value, context: &JsObject) -> Conversion {
    match items {
        Some(Value::Array(item_schemas)) => convert_tuple(item_schemas, value, context),
        Some(item_schema) if item_schema.is_object() => convert_array_items(item_schema, value, context),
        _ => Conversion::InPlace,
    }
}

fn convert_tuple(item_schemas: &[Value], value: &mut Value, context: &JsObject) -> Conversion {
    if let Value::Array(values) = value {
        for (index, item) in values.iter_mut().enumerate() {
            if let Some(item_schema) = item_schemas.get(index) {
                convert_in_place(item, item_schema, context);
            }
        }
    }
    Conversion::InPlace
}

fn convert_array_items(item_schema: &Value, value: &mut Value, context: &JsObject) -> Conversion {
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

fn convert_object(schema: &JsObject, value: &mut Value, context: &JsObject) -> Conversion {
    let Some(map) = value.as_object_mut() else {
        return Conversion::InPlace;
    };
    convert_object_properties(schema, map, context);
    convert_object_additional(schema, map, context);
    Conversion::InPlace
}

fn convert_object_properties(schema: &JsObject, map: &mut JsObject, context: &JsObject) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    for (key, property_schema) in properties {
        if let Some(property) = map.get_mut(key) {
            convert_in_place(property, property_schema, context);
        }
    }
}

fn convert_object_additional(schema: &JsObject, map: &mut JsObject, context: &JsObject) {
    let Some(additional) = schema.get("additionalProperties").filter(|value| value.is_object()) else {
        return;
    };
    let patterns: Vec<JsString> = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            properties
                .keys()
                .map(|key| JsString::join([JsString::from("^"), key.clone(), JsString::from("$")], ""))
                .collect()
        })
        .unwrap_or_default();
    let keys: Vec<JsString> = map.keys().cloned().collect();
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

fn convert_record(schema: &JsObject, value: &mut Value, context: &JsObject) -> Conversion {
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

fn convert_record_patterns(patterns: &JsObject, map: &mut JsObject, context: &JsObject) {
    for (pattern, property_schema) in patterns {
        let regex = Regex::new(pattern).ok();
        let keys: Vec<JsString> = map.keys().cloned().collect();
        for key in keys {
            if regex.as_ref().is_some_and(|regex| regex.is_match(&key))
                && let Some(property) = map.get_mut(&key)
            {
                convert_in_place(property, property_schema, context);
            }
        }
    }
}

fn convert_record_additional(patterns: &JsObject, schema: &JsObject, map: &mut JsObject, context: &JsObject) {
    let Some(additional) = schema.get("additionalProperties").filter(|value| value.is_object()) else {
        return;
    };
    let keys: Vec<JsString> = map.keys().cloned().collect();
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

fn number_from_string(text: &JsString) -> Option<Value> {
    let text = text.to_utf8().ok()?;
    if let Some(parsed) = Some(pi_js_compat::number::parse_number(&text)).filter(|parsed| parsed.is_finite()) {
        return Some(Value::Number(parsed));
    }
    let lowercase = text.to_lowercase();
    if lowercase == "false" {
        return Some(json!(0));
    }
    if lowercase == "true" {
        return Some(json!(1));
    }
    let digits = text.strip_suffix('n')?;
    let unsigned = digits.strip_prefix('-').unwrap_or(digits);
    if !is_canonical_integer(unsigned) {
        return None;
    }
    let value: i64 = digits.parse().ok()?;
    // TypeBox converts bigint-like strings only within `Number.MAX_SAFE_INTEGER`.
    (value.unsigned_abs() <= 9_007_199_254_740_991).then(|| Value::Number(value as f64))
}

/// TypeBox `TryBigInt` integer pattern: `0` or a non-zero digit followed by digits.
fn is_canonical_integer(digits: &str) -> bool {
    digits == "0"
        || (matches!(digits.as_bytes().first(), Some(b'1'..=b'9')) && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

fn try_integer(value: &Value) -> Option<Value> {
    try_number(value).map(|number| {
        let parsed = number.as_f64().unwrap_or(0.0);
        Value::Number(parsed.trunc())
    })
}

fn try_boolean(value: &Value) -> Option<Value> {
    match value {
        Value::Bool(boolean) => Some(json!(*boolean)),
        Value::Number(number) => match *number {
            0.0 => Some(json!(false)),
            1.0 => Some(json!(true)),
            _ => None,
        },
        Value::Null => Some(json!(false)),
        Value::String(text) => {
            let lowercase = text.to_utf8().ok()?.to_lowercase();
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
        Value::Bool(boolean) => Some(Value::String(boolean.to_string().into())),
        Value::Number(number) => Some(Value::String(js_number_to_string(*number).into())),
        Value::Null => Some(json!("null")),
        _ => None,
    }
}

fn try_null(value: &Value) -> Option<Value> {
    match value {
        Value::Null => Some(Value::Null),
        Value::Bool(false) => Some(Value::Null),
        Value::Number(number) if *number == 0.0 => Some(Value::Null),
        Value::String(text) => {
            let lowercase = text.to_utf8().ok()?.to_lowercase();
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
    coerce_composition_keywords(schema, value);
    coerce_typed(object, value);
    Conversion::InPlace
}

fn coerce_composition_keywords(schema: &Value, value: &mut Value) {
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
}

fn coerce_typed(schema: &JsObject, value: &mut Value) {
    let types = schema_types(schema);
    let matches_union_member = types.len() > 1 && types.iter().any(|schema_type| matches_json_type(value, schema_type));
    if !types.is_empty() && !matches_union_member {
        coerce_primitive(&types, value);
    }
    if types.iter().any(|schema_type| schema_type == "object") && value.is_object() {
        apply_object_coercion(value, schema);
    }
    if types.iter().any(|schema_type| schema_type == "array") && value.is_array() {
        apply_array_coercion(value, schema);
    }
}

fn coerce_primitive(types: &[String], value: &mut Value) {
    for schema_type in types {
        if let Some(candidate) = coerce_primitive_by_type(value, schema_type) {
            *value = candidate;
            break;
        }
    }
}

fn coerce_union(schemas: &[Value], value: &mut Value) -> Conversion {
    for schema in schemas {
        let mut candidate = value.clone();
        coerce_in_place(&mut candidate, schema);
        if check(schema, &candidate) {
            return Conversion::Replaced(candidate);
        }
    }
    Conversion::InPlace
}

fn schema_types(object: &JsObject) -> Vec<String> {
    match object.get("type") {
        Some(Value::String(schema_type)) => schema_type.to_utf8().ok().into_iter().collect(),
        Some(Value::Array(types)) => types.iter().filter_map(schema_text).collect(),
        _ => Vec::new(),
    }
}

fn matches_json_type(value: &Value, schema_type: &str) -> bool {
    match schema_type {
        "number" => value.as_f64().is_some(),
        "integer" => value
            .as_f64()
            .is_some_and(|number| number.is_finite() && number.fract() == 0.0),
        "boolean" => value.as_bool().is_some(),
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
            Value::Number(number) if *number == 1.0 => Some(json!(true)),
            Value::Number(number) if *number == 0.0 => Some(json!(false)),
            _ => None,
        },
        "string" => match value {
            Value::Null => Some(json!("")),
            Value::Number(number) => Some(Value::String(js_number_to_string(*number).into())),
            Value::Bool(boolean) => Some(Value::String(boolean.to_string().into())),
            _ => None,
        },
        "null" => match value {
            Value::String(text) if text.is_empty() => Some(Value::Null),
            Value::Number(number) if *number == 0.0 => Some(Value::Null),
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
            let parsed = Some(parse_number(text)).filter(|parsed| parsed.is_finite())?;
            if integer && parsed.fract() != 0.0 {
                return None;
            }
            Some(Value::Number(parsed))
        }
        Value::Bool(boolean) => Some(json!(if *boolean { 1 } else { 0 })),
        _ => None,
    }
}

fn apply_object_coercion(value: &mut Value, schema: &JsObject) {
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
    let defined: Vec<&JsString> = properties
        .map(|properties| properties.keys().collect())
        .unwrap_or_default();
    let keys: Vec<JsString> = map.keys().cloned().collect();
    for key in keys {
        if defined.iter().any(|defined| **defined == key) {
            continue;
        }
        if let Some(property) = map.get_mut(&key) {
            coerce_in_place(property, additional);
        }
    }
}

fn apply_array_coercion(value: &mut Value, schema: &JsObject) {
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
    instance_path: JsString,
    message: JsString,
    required_first: Option<JsString>,
}

impl Issue {
    fn new(instance_path: &JsString, message: impl Into<JsString>) -> Self {
        Self {
            instance_path: instance_path.clone(),
            message: message.into(),
            required_first: None,
        }
    }

    fn path(&self) -> JsString {
        let units = self.instance_path.units();
        let units = units.strip_prefix(&[u16::from(b'/')]).unwrap_or(units);
        let base = JsString::from_utf16(
            units
                .iter()
                .map(|unit| {
                    if *unit == u16::from(b'/') {
                        u16::from(b'.')
                    } else {
                        *unit
                    }
                })
                .collect(),
        );
        if let Some(required) = &self.required_first {
            return if base.is_empty() {
                required.clone()
            } else {
                JsString::join([base, JsString::from("."), required.clone()], "")
            };
        }
        if base.is_empty() { "root".into() } else { base }
    }
    fn line(&self) -> JsString {
        JsString::join(
            [
                JsString::from("  - "),
                self.path(),
                JsString::from(": "),
                self.message.clone(),
            ],
            "",
        )
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

    fn message(&mut self, instance_path: &JsString, message: impl Into<JsString>) {
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
    collect_schema(schema, schema, schema, &JsString::default(), value, &mut issues);
    issues
}

/// TypeBox-style validation lines (`  - path: message`) for a schema compiled by `Compile(...).Errors`.
pub fn schema_error_lines(schema: &Value, value: &Value) -> Vec<JsString> {
    collect_issues(schema, value).items.iter().map(Issue::line).collect()
}

fn check(schema: &Value, value: &Value) -> bool {
    collect_issues(schema, value).is_empty()
}

/// Validity check that keeps the caller's schema stack for `$ref` resolution.
fn check_with(root: &Value, base: &Value, schema: &Value, value: &Value) -> bool {
    scratch_issues(root, base, schema, &JsString::default(), value).is_empty()
}

fn scratch_issues(root: &Value, base: &Value, schema: &Value, instance_path: &JsString, value: &Value) -> Issues {
    let mut issues = Issues::scratch();
    collect_schema(root, base, schema, instance_path, value, &mut issues);
    issues
}

fn child_path(instance_path: &JsString, key: impl Into<JsString>) -> JsString {
    JsString::join([instance_path.clone(), JsString::from("/"), key.into()], "")
}

fn collect_schema(
    root: &Value,
    base: &Value,
    schema: &Value,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let base = if schema.get("$id").is_some() { schema } else { base };
    if schema.as_bool() == Some(false) {
        issues.message(instance_path, "schema is false".to_owned());
        return;
    }
    let Some(object) = schema.as_object() else {
        return;
    };
    collect_type(object, instance_path, value, issues);
    collect_value_keywords(root, base, object, instance_path, value, issues);
    collect_composition_keywords(root, base, schema, object, instance_path, value, issues);
}

fn collect_value_keywords(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    if value.is_object() {
        collect_object_keywords(root, base, schema, instance_path, value, issues);
    }
    if value.is_array() {
        collect_array_keywords(root, base, schema, instance_path, value, issues);
    }
    if let Some(text) = value.as_string() {
        collect_string_keywords(schema, instance_path, text, issues);
    }
    if let Some(number) = value.as_f64() {
        collect_number_keywords(schema, instance_path, number, issues);
    }
}

fn collect_composition_keywords(
    root: &Value,
    base: &Value,
    schema: &Value,
    object: &JsObject,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    collect_ref(root, base, object, instance_path, value, issues);
    collect_const_and_enum(object, instance_path, value, issues);
    collect_if_keyword(root, base, object, instance_path, value, issues);
    collect_not_keyword(root, base, object, instance_path, value, issues);
    collect_all_of(root, base, schema, instance_path, value, issues);
    collect_any_of_keyword(root, base, schema, instance_path, value, issues);
    collect_one_of_keyword(root, base, schema, instance_path, value, issues);
}

fn collect_ref(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(reference) = schema.get("$ref").and_then(schema_text) else {
        return;
    };
    match resolve_ref(root, base, &reference) {
        Some(target) => collect_schema(root, base, target, instance_path, value, issues),
        None => issues.message(instance_path, "schema is false".to_owned()),
    }
}

fn collect_const_and_enum(schema: &JsObject, instance_path: &JsString, value: &Value, issues: &mut Issues) {
    if let Some(constant) = schema.get("const")
        && !js_equal(constant, value)
    {
        issues.message(instance_path, "must be equal to constant".to_owned());
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array)
        && !values.iter().any(|option| js_equal(option, value))
    {
        issues.message(instance_path, "must be equal to one of the allowed values".to_owned());
    }
}

fn collect_if_keyword(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(if_schema) = schema.get("if").filter(|schema| is_schema(schema)) else {
        return;
    };
    collect_if(root, base, schema, if_schema, instance_path, value, issues);
}

fn collect_not_keyword(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(not_schema) = schema.get("not").filter(|schema| is_schema(schema)) else {
        return;
    };
    if check_with(root, base, not_schema, value) {
        issues.message(instance_path, "must not be valid".to_owned());
    }
}

fn collect_all_of(
    root: &Value,
    base: &Value,
    schema: &Value,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(members) = array_field(schema, "allOf") else {
        return;
    };
    if !members.iter().all(is_schema) {
        return;
    }
    for member in members {
        let failed = scratch_issues(root, base, member, instance_path, value);
        issues.merge(failed);
    }
}

fn collect_any_of_keyword(
    root: &Value,
    base: &Value,
    schema: &Value,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(members) = array_field(schema, "anyOf") else {
        return;
    };
    if !members.iter().all(is_schema) {
        return;
    }
    collect_any_of(root, base, members, instance_path, value, issues);
}

fn collect_one_of_keyword(
    root: &Value,
    base: &Value,
    schema: &Value,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(members) = array_field(schema, "oneOf") else {
        return;
    };
    if !members.iter().all(is_schema) {
        return;
    }
    collect_one_of(root, base, members, instance_path, value, issues);
}

fn collect_type(schema: &JsObject, instance_path: &JsString, value: &Value, issues: &mut Issues) {
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
        Value::String(schema_type) => schema_type.to_utf8().is_ok_and(|name| check_type_name(&name, value)),
        Value::Array(types) => types
            .iter()
            .filter_map(schema_text)
            .any(|schema_type| check_type_name(&schema_type, value)),
        _ => true,
    }
}

fn check_type_name(schema_type: &str, value: &Value) -> bool {
    match schema_type {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "boolean" => value.as_bool().is_some(),
        "integer" => value
            .as_f64()
            .is_some_and(|number| number.is_finite() && number.fract() == 0.0),
        "number" => value.as_f64().is_some(),
        "null" => value.is_null(),
        "string" => value.is_string(),
        _ => true,
    }
}

fn type_message(type_spec: &Value) -> JsString {
    match type_spec {
        Value::String(schema_type) => JsString::join([JsString::from("must be "), schema_type.clone()], ""),
        Value::Array(types) => {
            let names = types.iter().filter_map(Value::as_string);
            JsString::join([JsString::from("must be either "), JsString::join(names, " or ")], "")
        }
        _ => JsString::default(),
    }
}

fn collect_object_keywords(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(map) = value.as_object() else {
        return;
    };
    collect_required(schema, instance_path, map, issues);
    if let Some(additional) = schema.get("additionalProperties").filter(|value| is_schema(value)) {
        collect_additional_properties(root, base, schema, additional, instance_path, map, issues);
    }
    collect_dependencies_keyword(root, base, schema, instance_path, value, map, issues);
    collect_dependent_required_keyword(schema, instance_path, map, issues);
    collect_dependent_schemas(root, base, schema, instance_path, value, map, issues);
    collect_pattern_properties(root, base, schema, instance_path, map, issues);
    collect_properties(root, base, schema, instance_path, map, issues);
    collect_property_names(root, base, schema, instance_path, map, issues);
    collect_property_counts(schema, instance_path, map, issues);
}

fn collect_required(schema: &JsObject, instance_path: &JsString, map: &JsObject, issues: &mut Issues) {
    let Some(required) = object_array(schema, "required") else {
        return;
    };
    if !required.iter().all(Value::is_string) {
        return;
    }
    let missing: Vec<JsString> = required
        .iter()
        .filter_map(Value::as_string)
        .filter(|key| !map.contains_key(*key))
        .cloned()
        .collect();
    if missing.is_empty() {
        return;
    }
    let mut issue = Issue::new(
        instance_path,
        JsString::join(
            [
                JsString::from("must have required properties "),
                JsString::join(&missing, ", "),
            ],
            "",
        ),
    );
    issue.required_first = missing.first().cloned();
    issues.add(issue);
}

fn collect_dependencies_keyword(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    value: &Value,
    map: &JsObject,
    issues: &mut Issues,
) {
    let Some(dependencies) = schema.get("dependencies").and_then(Value::as_object) else {
        return;
    };
    if map.is_empty() || !dependencies.values().all(is_dependency) {
        return;
    }
    collect_dependencies(root, base, dependencies, instance_path, value, issues);
}

fn is_dependency(dependency: &Value) -> bool {
    is_schema(dependency)
        || dependency
            .as_array()
            .is_some_and(|names| names.iter().all(Value::is_string))
}

fn collect_dependent_required_keyword(
    schema: &JsObject,
    instance_path: &JsString,
    map: &JsObject,
    issues: &mut Issues,
) {
    let Some(dependencies) = schema.get("dependentRequired").and_then(Value::as_object) else {
        return;
    };
    let all_name_arrays = dependencies.values().all(|dependency| {
        dependency
            .as_array()
            .is_some_and(|names| names.iter().all(Value::is_string))
    });
    if map.is_empty() || !all_name_arrays {
        return;
    }
    collect_dependent_required(dependencies, instance_path, map, issues);
}

fn collect_dependent_schemas(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    value: &Value,
    map: &JsObject,
    issues: &mut Issues,
) {
    let Some(dependencies) = schema.get("dependentSchemas").and_then(Value::as_object) else {
        return;
    };
    if map.is_empty() || !dependencies.values().all(is_schema) {
        return;
    }
    for (key, dependent) in dependencies {
        if map.contains_key(key) {
            collect_schema(root, base, dependent, instance_path, value, issues);
        }
    }
}

fn collect_pattern_properties(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    map: &JsObject,
    issues: &mut Issues,
) {
    let Some(patterns) = schema.get("patternProperties").and_then(Value::as_object) else {
        return;
    };
    if !patterns.values().all(is_schema) {
        return;
    }
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

fn collect_properties(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    map: &JsObject,
    issues: &mut Issues,
) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    if !properties.values().all(is_schema) {
        return;
    }
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

fn collect_property_names(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    map: &JsObject,
    issues: &mut Issues,
) {
    let Some(property_names) = schema.get("propertyNames").filter(|value| is_schema(value)) else {
        return;
    };
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
            JsString::join(
                [
                    JsString::from("property names "),
                    JsString::join(&invalid, ", "),
                    JsString::from(" are invalid"),
                ],
                "",
            ),
        );
    }
}

fn collect_property_counts(schema: &JsObject, instance_path: &JsString, map: &JsObject, issues: &mut Issues) {
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
    schema: &JsObject,
    additional: &Value,
    instance_path: &JsString,
    map: &JsObject,
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
fn additional_properties_regex(schema: &JsObject) -> Option<Regex> {
    let mut patterns: Vec<JsString> = Vec::new();
    if let Some(patterns_object) = schema.get("patternProperties").and_then(Value::as_object) {
        patterns.extend(patterns_object.keys().cloned());
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        patterns.extend(
            properties
                .keys()
                .map(|key| JsString::join([JsString::from("^"), escape_js_regex(key), JsString::from("$")], "")),
        );
    }
    if patterns.is_empty() {
        return None;
    }
    Regex::new(JsString::join(
        [JsString::from("("), JsString::join(patterns, "|"), JsString::from(")")],
        "",
    ))
    .ok()
}

/// JavaScript `key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')`.
fn escape_js_regex(key: &JsString) -> JsString {
    const SPECIAL: &[u16] = &[46, 42, 43, 63, 94, 36, 123, 125, 40, 41, 124, 91, 93, 92];
    let mut escaped = Vec::new();
    for unit in key.units() {
        if SPECIAL.contains(unit) {
            escaped.push(u16::from(b'\\'));
        }
        escaped.push(*unit);
    }
    JsString::from_utf16(escaped)
}

fn collect_dependencies(
    root: &Value,
    base: &Value,
    dependencies: &JsObject,
    instance_path: &JsString,
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
            let names: Vec<&JsString> = names.iter().filter_map(Value::as_string).collect();
            // `Array.prototype.every` stops at the first missing dependency.
            if names.iter().any(|name| !map.contains_key(*name)) {
                issues.message(
                    instance_path,
                    JsString::join(
                        [
                            JsString::from("must have properties "),
                            JsString::join(names.iter().copied(), ", "),
                            JsString::from(" when property "),
                            key.clone(),
                            JsString::from(" is present"),
                        ],
                        "",
                    ),
                );
            }
        } else if is_schema(dependency) {
            collect_schema(root, base, dependency, instance_path, value, issues);
        }
    }
}

fn collect_dependent_required(dependencies: &JsObject, instance_path: &JsString, map: &JsObject, issues: &mut Issues) {
    for (key, dependency) in dependencies {
        if !map.contains_key(key) {
            continue;
        }
        let Some(names) = dependency.as_array() else {
            continue;
        };
        let names: Vec<&JsString> = names.iter().filter_map(Value::as_string).collect();
        for name in &names {
            if !map.contains_key(*name) {
                issues.message(
                    instance_path,
                    JsString::join(
                        [
                            JsString::from("must have properties "),
                            JsString::join(names.iter().copied(), ", "),
                            JsString::from(" when property "),
                            key.clone(),
                            JsString::from(" is present"),
                        ],
                        "",
                    ),
                );
            }
        }
    }
}

fn collect_array_keywords(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    value: &Value,
    issues: &mut Issues,
) {
    let Some(values) = value.as_array() else {
        return;
    };
    let items = schema.get("items");
    collect_additional_items(root, base, schema, items, instance_path, values, issues);
    collect_contains(root, base, schema, instance_path, values, issues);
    collect_items(root, base, schema, items, instance_path, values, issues);
    collect_contains_counts(root, base, schema, instance_path, values, issues);
    collect_item_counts(schema, instance_path, values, issues);
    collect_prefix_items(root, base, schema, instance_path, values, issues);
    collect_unique_items(schema, instance_path, values, issues);
}

fn collect_additional_items(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    items: Option<&Value>,
    instance_path: &JsString,
    values: &[Value],
    issues: &mut Issues,
) {
    let Some(Value::Array(item_schemas)) = items else {
        return;
    };
    let Some(additional) = schema.get("additionalItems").filter(|value| is_schema(value)) else {
        return;
    };
    for (index, item) in values.iter().enumerate().skip(item_schemas.len()) {
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

fn collect_contains(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    values: &[Value],
    issues: &mut Issues,
) {
    let Some(contains) = schema.get("contains").filter(|value| is_schema(value)) else {
        return;
    };
    if object_number(schema, "minContains") == Some(0.0) {
        return;
    }
    let matched = values.iter().any(|item| check_with(root, base, contains, item));
    if values.is_empty() || !matched {
        issues.message(instance_path, "must contain at least 1 valid item".to_owned());
    }
}

fn collect_items(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    items: Option<&Value>,
    instance_path: &JsString,
    values: &[Value],
    issues: &mut Issues,
) {
    match items {
        Some(Value::Array(item_schemas)) => {
            collect_sized_items(root, base, item_schemas, instance_path, values, issues);
        }
        Some(item_schema) if is_schema(item_schema) => {
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
        _ => {}
    }
}

fn collect_sized_items(
    root: &Value,
    base: &Value,
    item_schemas: &[Value],
    instance_path: &JsString,
    values: &[Value],
    issues: &mut Issues,
) {
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
}

fn collect_contains_counts(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    values: &[Value],
    issues: &mut Issues,
) {
    let Some(contains) = schema.get("contains").filter(|value| is_schema(value)) else {
        return;
    };
    let maximum = object_number(schema, "maxContains");
    let minimum = object_number(schema, "minContains");
    if maximum.is_none() && minimum.is_none() {
        return;
    }
    let count = values
        .iter()
        .filter(|item| check_with(root, base, contains, item))
        .count();
    if maximum.is_some_and(|maximum| (count as f64) > maximum) {
        // TypeBox reports both bounds with the `contains` keyword and message.
        issues.message(instance_path, "must contain at least 1 valid item".to_owned());
    }
    if minimum.is_some_and(|minimum| (count as f64) < minimum) {
        issues.message(instance_path, "must contain at least 1 valid item".to_owned());
    }
}

fn collect_item_counts(schema: &JsObject, instance_path: &JsString, values: &[Value], issues: &mut Issues) {
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
}

fn collect_prefix_items(
    root: &Value,
    base: &Value,
    schema: &JsObject,
    instance_path: &JsString,
    values: &[Value],
    issues: &mut Issues,
) {
    let Some(prefix_items) = object_array(schema, "prefixItems") else {
        return;
    };
    if !prefix_items.iter().all(is_schema) || values.is_empty() {
        return;
    }
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

fn collect_unique_items(schema: &JsObject, instance_path: &JsString, values: &[Value], issues: &mut Issues) {
    if schema.get("uniqueItems").and_then(Value::as_bool) != Some(true) {
        return;
    }
    let has_duplicates = values
        .iter()
        .enumerate()
        .any(|(index, item)| values.iter().skip(index + 1).any(|other| deep_equal(item, other)));
    if has_duplicates {
        issues.message(instance_path, "must not have duplicate items".to_owned());
    }
}

fn collect_string_keywords(schema: &JsObject, instance_path: &JsString, text: &JsString, issues: &mut Issues) {
    let units = text.units();
    if let Some(maximum) = object_number(schema, "maxLength")
        && !is_max_length_fast(units, maximum)
    {
        issues.message(
            instance_path,
            format!("must not have more than {} characters", js_number_to_string(maximum)),
        );
    }
    if let Some(minimum) = object_number(schema, "minLength")
        && !is_min_length_fast(units, minimum)
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
            issues.message(
                instance_path,
                JsString::join(
                    [
                        JsString::from("must match pattern \""),
                        pattern.clone(),
                        JsString::from("\""),
                    ],
                    "",
                ),
            );
        }
    }
}

fn collect_number_keywords(schema: &JsObject, instance_path: &JsString, number: f64, issues: &mut Issues) {
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
    schema: &JsObject,
    if_schema: &Value,
    instance_path: &JsString,
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
    instance_path: &JsString,
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
    instance_path: &JsString,
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
    if string_field(schema, "$id").as_deref() == Some(reference) {
        return Some(schema);
    }
    match schema {
        Value::Object(map) => map.values().find_map(|value| find_id(value, reference)),
        Value::Array(items) => items.iter().find_map(|item| find_id(item, reference)),
        _ => None,
    }
}

fn find_anchor<'a>(schema: &'a Value, anchor: &str) -> Option<&'a Value> {
    if string_field(schema, "$anchor").as_deref() == Some(anchor)
        || string_field(schema, "$dynamicAnchor").as_deref() == Some(anchor)
    {
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

fn schema_text(value: &Value) -> Option<String> {
    value.as_string()?.to_utf8().ok()
}
fn parse_number(text: &JsString) -> f64 {
    text.to_utf8()
        .map(|text| pi_js_compat::number::parse_number(&text))
        .unwrap_or(f64::NAN)
}
