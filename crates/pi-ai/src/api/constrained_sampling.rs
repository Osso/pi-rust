use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::types::{ConstrainedSampling, ConstrainedSamplingConfig, GrammarFormat, JsonSchemaStrictness, Tool};

/// Error thrown by constrained-sampling helpers; `message` matches the TypeScript `Error.message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstrainedSamplingError {
    pub message: String,
}

impl fmt::Display for ConstrainedSamplingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConstrainedSamplingError {}

fn error(message: impl Into<String>) -> ConstrainedSamplingError {
    ConstrainedSamplingError {
        message: message.into(),
    }
}

const UNSUPPORTED_STRICT_SCHEMA_KEYS: [&str; 16] = [
    "$ref",
    "$defs",
    "definitions",
    "allOf",
    "oneOf",
    "patternProperties",
    "dependentSchemas",
    "dependencies",
    "unevaluatedProperties",
    "propertyNames",
    "contains",
    "prefixItems",
    "not",
    "if",
    "then",
    "else",
];

/// Convert a tool schema to the strict subset expected by provider constrained sampling.
pub fn make_strict_json_schema(schema: &Value) -> Result<Value, ConstrainedSamplingError> {
    let mut cloned = schema.clone();
    if !cloned.is_object() {
        return Err(error("root schema must have type object"));
    }
    make_strict_node(&mut cloned)?;
    if cloned.get("type") != Some(&Value::String("object".to_owned())) {
        return Err(error("root schema must have type object"));
    }
    Ok(cloned)
}

pub fn get_json_schema_tool_parameters(tool: &Tool, strict: Option<bool>) -> Result<Value, ConstrainedSamplingError> {
    if strict == Some(true) {
        make_strict_json_schema(&tool.parameters)
    } else {
        Ok(tool.parameters.clone())
    }
}

/// Grammar format expected by the provider (`"lark" | "regex"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrammarSyntax {
    Lark,
    Regex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrammarConstrainedSampling {
    pub format: GrammarSyntax,
    pub definition: String,
    pub input_property: String,
}

/// Append-only grammar tool input JSON buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrammarToolInputJsonBuffer {
    pub input: String,
    pub started: bool,
    pub closed: bool,
}

pub fn get_grammar_tool_input(
    tool_name: &str,
    arguments: &Map<String, Value>,
    input_property: &str,
) -> Result<String, ConstrainedSamplingError> {
    arguments
        .get(input_property)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            error(format!(
                "Grammar tool call \"{tool_name}\" requires argument \"{input_property}\" to be a string."
            ))
        })
}

pub fn append_grammar_tool_input_json_delta(
    buffer: &mut GrammarToolInputJsonBuffer,
    input_property: &str,
    next_input: &str,
    close: bool,
) -> Result<Option<String>, ConstrainedSamplingError> {
    if buffer.closed {
        if close && next_input == buffer.input {
            return Ok(None);
        }
        return Err(error(format!(
            "grammar tool input for property \"{input_property}\" changed after it was closed"
        )));
    }
    let Some(input_delta) = next_input.strip_prefix(buffer.input.as_str()) else {
        return Err(error(format!(
            "grammar tool input for property \"{input_property}\" changed non-monotonically"
        )));
    };
    if !close && input_delta.is_empty() {
        return Ok(None);
    }

    let mut delta = String::new();
    if !buffer.started {
        delta.push('{');
        delta.push_str(&json_string(input_property));
        delta.push_str(":\"");
        buffer.started = true;
    }
    delta.push_str(&json_string_inner(input_delta));
    buffer.input = next_input.to_owned();
    if close {
        delta.push_str("\"}");
        buffer.closed = true;
    }
    Ok(Some(delta))
}

pub fn resolve_json_schema_strict_sampling(
    tool: &Tool,
    supports_strict_mode: bool,
) -> Result<Option<bool>, ConstrainedSamplingError> {
    let strictness = match &tool.constrained_sampling {
        Some(ConstrainedSampling::Config(ConstrainedSamplingConfig::JsonSchema { strict })) => *strict,
        _ => return Ok(None),
    };

    if supports_strict_mode {
        return match make_strict_json_schema(&tool.parameters) {
            Ok(_) => Ok(Some(true)),
            Err(schema_error) if strictness == JsonSchemaStrictness::Require => Err(error(format!(
                "Tool \"{}\" requires JSON-schema constrained sampling, but {}.",
                tool.name, schema_error.message
            ))),
            Err(_) => Ok(None),
        };
    }
    if strictness == JsonSchemaStrictness::Require {
        return Err(error(format!(
            "Tool \"{}\" requires JSON-schema constrained sampling, but strict tools are unsupported.",
            tool.name
        )));
    }
    Ok(None)
}

pub fn resolve_grammar_constrained_sampling(
    tool: &Tool,
    supports_openai_grammar_tools: bool,
) -> Result<Option<GrammarConstrainedSampling>, ConstrainedSamplingError> {
    let variants = match &tool.constrained_sampling {
        Some(ConstrainedSampling::Config(ConstrainedSamplingConfig::Grammar { variants })) => variants,
        _ => return Ok(None),
    };
    if !supports_openai_grammar_tools {
        return Ok(None);
    }

    let lark_definition = variants
        .get(&GrammarFormat::OpenaiLark)
        .filter(|definition| !is_blank(definition));
    let regex_definition = variants
        .get(&GrammarFormat::OpenaiRegex)
        .filter(|definition| !is_blank(definition));
    let (format, definition) = match (lark_definition, regex_definition) {
        (Some(definition), _) => (GrammarSyntax::Lark, definition),
        (None, Some(definition)) => (GrammarSyntax::Regex, definition),
        (None, None) => {
            return Err(error(format!(
                "Tool \"{}\" cannot use grammar constrained sampling: no supported grammar variant was provided.",
                tool.name
            )));
        }
    };

    let input_property = infer_grammar_input_property(tool).map_err(|inference_error| {
        error(format!(
            "Tool \"{}\" cannot use grammar constrained sampling: {}.",
            tool.name, inference_error.message
        ))
    })?;
    Ok(Some(GrammarConstrainedSampling {
        format,
        definition: definition.clone(),
        input_property,
    }))
}

pub fn create_grammar_tool_input_properties(
    tools: Option<&[Tool]>,
    supports_openai_grammar_tools: bool,
) -> Result<BTreeMap<String, String>, ConstrainedSamplingError> {
    let mut properties = BTreeMap::new();
    for tool in tools.into_iter().flatten() {
        if let Some(grammar) = resolve_grammar_constrained_sampling(tool, supports_openai_grammar_tools)? {
            properties.insert(tool.name.clone(), grammar.input_property);
        }
    }
    Ok(properties)
}

fn make_strict_node(schema: &mut Value) -> Result<(), ConstrainedSamplingError> {
    let Value::Object(object) = schema else {
        return Err(error("boolean schemas are unsupported"));
    };
    reject_unsupported_keys(object)?;
    make_any_of_strict(object)?;
    make_items_strict(object)?;

    if object.contains_key("properties") && !is_object_schema(object) {
        return Err(error("properties require type object"));
    }
    if !is_object_schema(object) {
        return Ok(());
    }

    reject_unsupported_object_keywords(object)?;
    let property_names = property_names(object);
    let required = required_names(object);
    if required.iter().any(|key| !property_names.contains(key)) {
        return Err(error("required contains an unknown property"));
    }
    make_properties_strict(object, &property_names, &required)?;
    object.insert(
        "required".to_owned(),
        Value::Array(property_names.into_iter().map(Value::String).collect()),
    );
    object.insert("additionalProperties".to_owned(), Value::Bool(false));
    Ok(())
}

fn reject_unsupported_keys(object: &Map<String, Value>) -> Result<(), ConstrainedSamplingError> {
    for key in UNSUPPORTED_STRICT_SCHEMA_KEYS {
        if object.contains_key(key) {
            return Err(error(format!("{key} schemas are unsupported")));
        }
    }
    Ok(())
}

fn make_any_of_strict(object: &mut Map<String, Value>) -> Result<(), ConstrainedSamplingError> {
    let Some(any_of) = object.get_mut("anyOf") else {
        return Ok(());
    };
    let Value::Array(variants) = any_of else {
        return Err(error("anyOf must contain at least one schema"));
    };
    if variants.is_empty() {
        return Err(error("anyOf must contain at least one schema"));
    }
    for variant in variants.iter_mut() {
        if is_structured_schema(variant) {
            return Err(error("object and array unions are unsupported"));
        }
        make_strict_node(variant)?;
    }
    Ok(())
}

fn make_items_strict(object: &mut Map<String, Value>) -> Result<(), ConstrainedSamplingError> {
    let Some(items) = object.get_mut("items") else {
        return Ok(());
    };
    if items.is_array() {
        return Err(error("tuple schemas are unsupported"));
    }
    make_strict_node(items)
}

fn reject_unsupported_object_keywords(object: &Map<String, Value>) -> Result<(), ConstrainedSamplingError> {
    if let Some(additional_properties) = object.get("additionalProperties")
        && additional_properties != &Value::Bool(false)
    {
        return Err(error("schema-valued or true additionalProperties is unsupported"));
    }
    if object
        .get("properties")
        .is_some_and(|properties| !properties.is_object())
    {
        return Err(error("object properties must be a schema map"));
    }
    if object
        .get("required")
        .is_some_and(|required| !is_string_array(required))
    {
        return Err(error("object required must be a string array"));
    }
    Ok(())
}

fn property_names(object: &Map<String, Value>) -> Vec<String> {
    object
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default()
}

fn required_names(object: &Map<String, Value>) -> Vec<String> {
    object
        .get("required")
        .and_then(Value::as_array)
        .map(|required| required.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default()
}

fn make_properties_strict(
    object: &mut Map<String, Value>,
    property_names: &[String],
    required: &[String],
) -> Result<(), ConstrainedSamplingError> {
    let Some(Value::Object(properties)) = object.get_mut("properties") else {
        return Ok(());
    };
    for name in property_names {
        let mut child = properties.get(name).cloned().expect("property key must exist");
        make_strict_node(&mut child)?;
        if !required.contains(name) && !schema_allows_null(&child) {
            child = json!({"anyOf": [child, {"type": "null"}]});
        }
        properties.insert(name.clone(), child);
    }
    Ok(())
}

fn is_object_schema(object: &Map<String, Value>) -> bool {
    object.get("type").and_then(Value::as_str) == Some("object")
}

fn is_structured_schema(schema: &Value) -> bool {
    let Value::Object(object) = schema else {
        return false;
    };
    let types: Vec<&str> = match object.get("type") {
        Some(Value::String(schema_type)) => vec![schema_type.as_str()],
        Some(Value::Array(schema_types)) => schema_types.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    types.contains(&"object")
        || types.contains(&"array")
        || object.contains_key("properties")
        || object.contains_key("items")
}

fn schema_allows_null(schema: &Value) -> bool {
    let Value::Object(object) = schema else {
        return false;
    };
    if object.get("type") == Some(&Value::String("null".to_owned())) {
        return true;
    }
    if let Some(Value::Array(types)) = object.get("type")
        && types.contains(&Value::String("null".to_owned()))
    {
        return true;
    }
    if object.get("const") == Some(&Value::Null) {
        return true;
    }
    if let Some(Value::Array(values)) = object.get("enum")
        && values.contains(&Value::Null)
    {
        return true;
    }
    if let Some(Value::Array(variants)) = object.get("anyOf")
        && variants.iter().any(schema_allows_null)
    {
        return true;
    }
    false
}

fn is_string_array(value: &Value) -> bool {
    matches!(value, Value::Array(items) if items.iter().all(Value::is_string))
}

fn infer_grammar_input_property(tool: &Tool) -> Result<String, ConstrainedSamplingError> {
    let Some(schema) = tool.parameters.as_object() else {
        return Err(error(
            "grammar constrained sampling requires an object parameter schema",
        ));
    };
    if schema.get("type") != Some(&Value::String("object".to_owned())) {
        return Err(error(
            "grammar constrained sampling requires an object parameter schema",
        ));
    }
    let Some(Value::Array(required)) = schema.get("required") else {
        return Err(error(
            "grammar constrained sampling requires exactly one required string property",
        ));
    };
    if required.len() != 1 {
        return Err(error(
            "grammar constrained sampling requires exactly one required string property",
        ));
    }
    let Some(input_property) = required[0].as_str() else {
        return Err(error(
            "grammar constrained sampling requires exactly one required string property",
        ));
    };
    let Some(Value::Object(properties)) = schema.get("properties") else {
        return Err(error(format!(
            "grammar constrained sampling requires a properties entry for {input_property}"
        )));
    };
    let Some(property) = properties.get(input_property) else {
        return Err(error(format!(
            "grammar constrained sampling requires a properties entry for {input_property}"
        )));
    };
    if property.as_object().and_then(|property| property.get("type")) != Some(&Value::String("string".to_owned())) {
        return Err(error(format!(
            "grammar constrained sampling property {input_property} must have type string"
        )));
    }
    Ok(input_property.to_owned())
}

fn is_blank(value: &str) -> bool {
    value
        .trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
        .is_empty()
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("strings always serialize")
}

fn json_string_inner(value: &str) -> String {
    let quoted = json_string(value);
    quoted[1..quoted.len() - 1].to_owned()
}
