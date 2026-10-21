use pi_ai::types::{Tool, ToolCall, TypeBoxSchema};
use pi_ai::utils::typebox_helpers::{StringEnumOptions, string_enum};
use pi_ai::utils::validation::{ValidationError, validate_tool_arguments, validate_tool_call};
use serde_json::{Map, Value, json};

fn object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn plain_tool(parameters: Value) -> Tool {
    Tool::new("echo", "Echo tool", parameters)
}

fn typebox_tool(parameters: Value) -> Tool {
    Tool::typebox("echo", "Echo tool", TypeBoxSchema::new(parameters))
}

fn tool_call(arguments: Map<String, Value>) -> ToolCall {
    ToolCall::new("tool-1", "echo", arguments)
}

/// `createToolCallWithPlainSchema` from the TypeScript test: wrap a schema as the
/// required `value` property of a plain object schema.
fn plain_value_schema(schema: Value) -> Tool {
    plain_tool(json!({
        "type": "object",
        "properties": { "value": schema },
        "required": ["value"],
    }))
}

fn value_args(value: Value) -> ToolCall {
    tool_call(object([("value", value)]))
}

fn validate(tool: &Tool, tool_call: &ToolCall) -> Value {
    validate_tool_arguments(tool, tool_call).expect("arguments must validate")
}

fn validation_message(tool: &Tool, tool_call: &ToolCall) -> String {
    validate_tool_arguments(tool, tool_call)
        .expect_err("arguments must fail validation")
        .to_string()
}

#[test]
fn typebox_number_coerces_string_arguments() {
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["count"],
        "properties": { "count": { "type": "number" } },
    }));
    let call = tool_call(object([("count", json!("42"))]));
    assert_eq!(validate(&tool, &call), json!({ "count": 42 }));
}

#[test]
fn plain_number_coerces_string_arguments() {
    let tool = plain_value_schema(json!({ "type": "number" }));
    assert_eq!(validate(&tool, &value_args(json!("42"))), json!({ "value": 42 }));
}

#[test]
fn plain_schemas_use_ajv_compatible_primitive_coercions() {
    let cases = [
        (json!({ "type": "number" }), json!("42"), json!(42)),
        (json!({ "type": "number" }), json!(true), json!(1)),
        (json!({ "type": "number" }), Value::Null, json!(0)),
        (json!({ "type": "integer" }), json!("42"), json!(42)),
        (json!({ "type": "boolean" }), json!("true"), json!(true)),
        (json!({ "type": "boolean" }), json!("false"), json!(false)),
        (json!({ "type": "boolean" }), json!(1), json!(true)),
        (json!({ "type": "boolean" }), json!(0), json!(false)),
        (json!({ "type": "string" }), Value::Null, json!("")),
        (json!({ "type": "string" }), json!(true), json!("true")),
        (json!({ "type": "null" }), json!(""), Value::Null),
        (json!({ "type": "null" }), json!(0), Value::Null),
        (json!({ "type": "null" }), json!(false), Value::Null),
        (json!({ "type": ["number", "string"] }), json!("1"), json!("1")),
        (json!({ "type": ["boolean", "number"] }), json!("1"), json!(1)),
    ];
    for (schema, input, expected) in cases {
        let tool = plain_value_schema(schema.clone());
        assert_eq!(
            validate(&tool, &value_args(input.clone())),
            json!({ "value": expected }),
            "schema {schema} input {input}"
        );
    }
}

#[test]
fn plain_schemas_reject_invalid_coercions() {
    let cases = [
        (json!({ "type": "boolean" }), json!("1")),
        (json!({ "type": "boolean" }), json!("0")),
        (json!({ "type": "null" }), json!("null")),
        (json!({ "type": "integer" }), json!("42.1")),
    ];
    for (schema, input) in cases {
        let tool = plain_value_schema(schema.clone());
        let message = validation_message(&tool, &value_args(input.clone()));
        assert!(
            message.starts_with("Validation failed"),
            "schema {schema} input {input}: {message}"
        );
    }
}

#[test]
fn plain_schemas_do_not_run_typebox_value_convert() {
    // Value.Convert truncates "42.9" for integers; plain schemas must not.
    let tool = plain_value_schema(json!({ "type": "integer" }));
    let message = validation_message(&tool, &value_args(json!("42.9")));
    assert_eq!(
        message,
        "Validation failed for tool \"echo\":\n  - value: must be integer\n\nReceived arguments:\n{\n  \"value\": \"42.9\"\n}"
    );
}

#[test]
fn optional_nulls_are_converted_without_deleting_properties() {
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["path", "metadata"],
        "properties": {
            "path": { "type": "string" },
            "offset": { "type": "number" },
            "nullable": { "anyOf": [{ "type": "string" }, { "type": "null" }] },
            "metadata": {
                "type": "object",
                "properties": { "enabled": { "type": "boolean" } }
            }
        }
    }));
    let call = tool_call(object([
        ("path", json!("file.txt")),
        ("offset", Value::Null),
        ("nullable", Value::Null),
        ("metadata", json!({ "enabled": null })),
    ]));
    assert_eq!(
        validate(&tool, &call),
        json!({ "path": "file.txt", "offset": 0, "nullable": "", "metadata": { "enabled": false } })
    );
}

#[test]
fn optional_nulls_with_nullable_references_are_preserved() {
    let tool = plain_tool(json!({
        "type": "object",
        "properties": { "value": { "$ref": "#/$defs/value" } },
        "$defs": { "value": { "anyOf": [{ "type": "number" }, { "type": "null" }] } }
    }));
    assert_eq!(validate(&tool, &value_args(Value::Null)), json!({ "value": null }));
}

#[test]
fn typebox_nullable_union_still_runs_plain_coercion_in_arm_order() {
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["value"],
        "properties": { "value": { "anyOf": [{ "type": "number" }, { "type": "null" }] } }
    }));
    assert_eq!(validate(&tool, &value_args(Value::Null)), json!({ "value": 0 }));
}

#[test]
fn plain_one_of_coerces_the_first_valid_arm_before_checking_later_arms() {
    let tool = plain_value_schema(json!({
        "oneOf": [{ "type": "number" }, { "type": "null" }]
    }));
    assert_eq!(validate(&tool, &value_args(Value::Null)), json!({ "value": 0 }));
}

#[test]
fn plain_nullable_unions_coerce_when_no_arm_matches() {
    let tool = plain_value_schema(json!({
        "anyOf": [{ "type": "number" }, { "type": "null" }]
    }));
    assert_eq!(validate(&tool, &value_args(json!("42"))), json!({ "value": 42 }));
}

#[test]
fn null_is_accepted_for_nullable_arrays_with_items() {
    let tool = plain_value_schema(json!({
        "type": ["array", "null"],
        "items": { "type": "string" }
    }));
    assert_eq!(validate(&tool, &value_args(Value::Null)), json!({ "value": null }));
}

#[test]
fn typebox_integer_truncates_numeric_strings() {
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["value"],
        "properties": { "value": { "type": "integer" } }
    }));
    assert_eq!(validate(&tool, &value_args(json!("42.9"))), json!({ "value": 42 }));
}

#[test]
fn typebox_string_coerces_null_to_the_literal_text() {
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["value"],
        "properties": { "value": { "type": "string" } }
    }));
    assert_eq!(validate(&tool, &value_args(Value::Null)), json!({ "value": "null" }));
}

#[test]
fn typebox_boolean_accepts_case_insensitive_strings() {
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["value"],
        "properties": { "value": { "type": "boolean" } }
    }));
    assert_eq!(validate(&tool, &value_args(json!("TRUE"))), json!({ "value": true }));
}

#[test]
fn typebox_null_accepts_the_literal_text() {
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["value"],
        "properties": { "value": { "type": "null" } }
    }));
    assert_eq!(validate(&tool, &value_args(json!("null"))), json!({ "value": null }));
}

#[test]
fn typebox_arrays_wrap_scalar_values() {
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["value"],
        "properties": { "value": { "type": "array", "items": { "type": "number" } } }
    }));
    assert_eq!(validate(&tool, &value_args(json!("5"))), json!({ "value": [5] }));
}

#[test]
fn typebox_literals_are_coerced_before_the_const_check() {
    // TypeBox converts 5 to "5", then the const check fails (not the type check).
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["value"],
        "properties": { "value": { "type": "string", "const": "x" } }
    }));
    let message = validation_message(&tool, &value_args(json!(5)));
    assert_eq!(
        message,
        "Validation failed for tool \"echo\":\n  - value: must be equal to constant\n\nReceived arguments:\n{\n  \"value\": 5\n}"
    );
}

#[test]
fn error_messages_list_every_failing_keyword_in_engine_order() {
    let tool = plain_tool(json!({
        "type": "object",
        "properties": { "a": { "type": "string" } },
        "required": ["a", "b"],
        "additionalProperties": false
    }));
    let call = tool_call(object([("c", json!(1)), ("d", json!(2))]));
    assert_eq!(
        validation_message(&tool, &call),
        "Validation failed for tool \"echo\":\n  \
         - a: must have required properties a, b\n  \
         - root: must not have additional properties\n\n\
         Received arguments:\n{\n  \"c\": 1,\n  \"d\": 2\n}"
    );
}

#[test]
fn required_paths_include_the_first_missing_property() {
    let tool = plain_tool(json!({
        "type": "object",
        "properties": { "meta": { "type": "object", "required": ["id"] } }
    }));
    let call = tool_call(object([("meta", json!({}))]));
    assert_eq!(
        validation_message(&tool, &call),
        "Validation failed for tool \"echo\":\n  - meta.id: must have required properties id\n\nReceived arguments:\n{\n  \"meta\": {}\n}"
    );
}

#[test]
fn errors_stop_at_eight_entries() {
    let tool = plain_tool(json!({
        "type": "object",
        "properties": {
            "a": { "type": "string" }, "b": { "type": "string" }, "c": { "type": "string" },
            "d": { "type": "string" }, "e": { "type": "string" }, "f": { "type": "string" },
            "g": { "type": "string" }, "h": { "type": "string" }, "i": { "type": "string" },
            "j": { "type": "string" }
        }
    }));
    let failing = json!({ "not": "a string" });
    let call = tool_call(object([
        ("a", failing.clone()),
        ("b", failing.clone()),
        ("c", failing.clone()),
        ("d", failing.clone()),
        ("e", failing.clone()),
        ("f", failing.clone()),
        ("g", failing.clone()),
        ("h", failing.clone()),
        ("i", failing.clone()),
        ("j", failing.clone()),
    ]));
    let message = validation_message(&tool, &call);
    assert_eq!(message.matches("  - ").count(), 8, "{message}");
}

#[test]
fn received_arguments_are_pretty_printed_with_javascript_number_formatting() {
    let tool = plain_tool(json!({
        "type": "object",
        "properties": { "a": { "type": "string" } },
        "required": ["a"]
    }));
    let call = tool_call(object([
        ("a", json!({ "x": 1 })),
        ("whole", json!(1.0)),
        ("fraction", json!(0.1)),
        ("big", json!(1e21)),
        ("negativeZero", json!(-0.0)),
        ("text", json!("quote \" slash \\ newline \n tab \t")),
        ("list", json!([1, 2.5, "x", null, true])),
        ("nested", json!({ "emptyObject": {}, "emptyArray": [] })),
    ]));
    let message = validation_message(&tool, &call);
    assert!(message.contains("\"whole\": 1,"), "{message}");
    assert!(message.contains("\"fraction\": 0.1,"), "{message}");
    assert!(message.contains("\"big\": 1e+21,"), "{message}");
    assert!(message.contains("\"negativeZero\": 0,"), "{message}");
    assert!(
        message.contains("\"text\": \"quote \\\" slash \\\\ newline \\n tab \\t\""),
        "{message}"
    );
    assert!(
        message.contains("\"list\": [\n    1,\n    2.5,\n    \"x\",\n    null,\n    true\n  ]"),
        "{message}"
    );
    assert!(
        message.contains("\"emptyObject\": {},\n    \"emptyArray\": []"),
        "{message}"
    );
}

#[test]
fn validate_tool_call_reports_unknown_tool_names() {
    let tools = vec![plain_tool(json!({ "type": "object" }))];
    let call = ToolCall::new("tool-1", "missing", object([]));
    let error = validate_tool_call(&tools, &call).expect_err("tool must be missing");
    assert_eq!(error.to_string(), "Tool \"missing\" not found");
}

#[test]
fn validate_tool_call_validates_matching_tools() {
    let tools = vec![typebox_tool(json!({
        "type": "object",
        "required": ["count"],
        "properties": { "count": { "type": "number" } }
    }))];
    let call = tool_call(object([("count", json!("7"))]));
    assert_eq!(validate_tool_call(&tools, &call).unwrap(), json!({ "count": 7 }));
}

#[test]
fn validation_error_exposes_its_message() {
    let tool = plain_tool(json!({ "type": "object", "required": ["a"] }));
    let error: ValidationError = validate_tool_arguments(&tool, &tool_call(object([]))).unwrap_err();
    assert!(error.message().starts_with("Validation failed for tool \"echo\":"));
    let dynamic: &dyn std::error::Error = &error;
    assert_eq!(dynamic.to_string(), error.message());
}

#[test]
fn string_enum_builds_a_google_compatible_string_schema() {
    let schema = string_enum(
        &["add", "subtract"],
        StringEnumOptions {
            description: Some("The operation".to_owned()),
            default: Some("add".to_owned()),
        },
    );
    assert_eq!(
        schema.value(),
        &json!({
            "type": "string",
            "enum": ["add", "subtract"],
            "description": "The operation",
            "default": "add"
        })
    );

    let parameters = json!({
        "type": "object",
        "required": ["operation"],
        "properties": { "operation": schema.value().clone() }
    });
    let tool = Tool::typebox("calc", "Calculator", TypeBoxSchema::new(parameters));
    let call = ToolCall::new("tool-1", "calc", object([("operation", json!("multiply"))]));
    let message = validate_tool_arguments(&tool, &call).unwrap_err().to_string();
    assert!(
        message.contains("operation: must be equal to one of the allowed values"),
        "{message}"
    );
}

#[test]
fn typebox_number_strings_use_the_bigint_like_fallback_rules() {
    // TypeBox `TryNumber`: `+"0123n"` and `+"9007199254740992n"` are NaN, and
    // `TryBigInt` rejects leading zeros and values above `Number.MAX_SAFE_INTEGER`,
    // so both strings stay unconverted and fail the number type.
    let tool = typebox_tool(json!({
        "type": "object",
        "required": ["count"],
        "properties": { "count": { "type": "number" } },
    }));
    for value in [json!("0123n"), json!("9007199254740992n")] {
        let call = tool_call(object([("count", value.clone())]));
        let message = validation_message(&tool, &call);
        assert!(message.starts_with("Validation failed"), "input {value}: {message}");
    }

    let call = tool_call(object([("count", json!("123n"))]));
    assert_eq!(validate(&tool, &call), json!({ "count": 123 }));
}
