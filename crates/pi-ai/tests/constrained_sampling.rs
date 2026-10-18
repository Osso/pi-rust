use pi_ai::api::constrained_sampling::{
    GrammarSyntax, GrammarToolInputJsonBuffer, append_grammar_tool_input_json_delta,
    create_grammar_tool_input_properties, get_grammar_tool_input, get_json_schema_tool_parameters,
    make_strict_json_schema, resolve_grammar_constrained_sampling, resolve_json_schema_strict_sampling,
};
use pi_ai::types::{ConstrainedSampling, ConstrainedSamplingConfig, GrammarFormat, JsonSchemaStrictness, Tool};
use serde_json::{Value, json};

fn tool(name: &str, parameters: Value, constrained_sampling: Option<ConstrainedSampling>) -> Tool {
    Tool {
        name: name.to_owned(),
        description: "Sample tool".to_owned(),
        parameters,
        constrained_sampling,
        typebox_kind: false,
    }
}

fn json_schema(strict: JsonSchemaStrictness) -> ConstrainedSampling {
    ConstrainedSampling::Config(ConstrainedSamplingConfig::JsonSchema { strict })
}

fn grammar(variants: &[(GrammarFormat, &str)]) -> ConstrainedSampling {
    ConstrainedSampling::Config(ConstrainedSamplingConfig::Grammar {
        variants: variants
            .iter()
            .map(|(format, definition)| (*format, (*definition).to_owned()))
            .collect(),
    })
}

fn object_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {"type": "string"},
            "offset": {"type": "number"},
            "metadata": {"type": "object", "properties": {"enabled": {"type": "boolean"}}},
            "nullable": {"anyOf": [{"type": "string"}, {"type": "null"}]}
        },
        "required": ["path", "metadata"]
    })
}

#[test]
fn derives_strict_provider_schemas_without_changing_the_input() {
    let parameters = object_parameters();
    let strict = make_strict_json_schema(&parameters).expect("schema converts");

    assert_eq!(parameters.get("additionalProperties"), None);
    assert_eq!(parameters["required"], json!(["path", "metadata"]));
    assert_eq!(
        strict,
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "offset": {"anyOf": [{"type": "number"}, {"type": "null"}]},
                "metadata": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["enabled"],
                    "properties": {"enabled": {"anyOf": [{"type": "boolean"}, {"type": "null"}]}}
                },
                "nullable": {"anyOf": [{"type": "string"}, {"type": "null"}]}
            },
            "required": ["path", "offset", "metadata", "nullable"],
            "additionalProperties": false
        })
    );
}

#[test]
fn rejects_schemas_that_cannot_be_safely_converted() {
    let cases = [
        (
            json!({"type": "object", "properties": {"metadata": {"type": "object", "additionalProperties": {"type": "string"}}}}),
            "schema-valued or true additionalProperties is unsupported",
        ),
        (
            json!({"allOf": [{"type": "object"}, {"type": "object"}]}),
            "allOf schemas are unsupported",
        ),
        (
            json!({"type": "object", "properties": {"value": {"anyOf": [{"type": "object", "properties": {"nested": {"type": "string"}}}, {"type": "null"}]}}}),
            "object and array unions are unsupported",
        ),
        (
            json!({"type": "object", "properties": {"child": {"$ref": "https://example.com/child.json"}}, "required": ["child"]}),
            "$ref schemas are unsupported",
        ),
        (json!(true), "root schema must have type object"),
        (
            json!({"type": "object", "items": [{"type": "string"}]}),
            "tuple schemas are unsupported",
        ),
        (json!({"anyOf": []}), "anyOf must contain at least one schema"),
        (
            json!({"properties": {"a": {"type": "string"}}}),
            "properties require type object",
        ),
        (
            json!({"type": "object", "properties": {"a": {"type": "string"}}, "required": ["b"]}),
            "required contains an unknown property",
        ),
        (
            json!({"type": "object", "required": "a"}),
            "object required must be a string array",
        ),
        (json!({}), "root schema must have type object"),
        (
            json!({"type": "object", "properties": {"a": null}}),
            "boolean schemas are unsupported",
        ),
    ];

    for (parameters, expected) in cases {
        let error = make_strict_json_schema(&parameters).expect_err("schema must be rejected");
        assert_eq!(error.message, expected, "schema {parameters}");
    }
}

#[test]
fn resolves_json_schema_strict_sampling_by_capability() {
    let valid = tool(
        "sample_tool",
        object_parameters(),
        Some(json_schema(JsonSchemaStrictness::Prefer)),
    );
    assert_eq!(
        resolve_json_schema_strict_sampling(&valid, true).expect("resolves"),
        Some(true)
    );
    assert_eq!(
        resolve_json_schema_strict_sampling(&valid, false).expect("resolves"),
        None
    );

    let unsupported = tool(
        "sample_tool",
        json!({"allOf": [{"type": "object"}]}),
        Some(json_schema(JsonSchemaStrictness::Prefer)),
    );
    assert_eq!(
        resolve_json_schema_strict_sampling(&unsupported, true).expect("resolves"),
        None
    );

    let required = tool(
        "sample_tool",
        json!({"allOf": [{"type": "object"}]}),
        Some(json_schema(JsonSchemaStrictness::Require)),
    );
    let error = resolve_json_schema_strict_sampling(&required, true).expect_err("must reject");
    assert_eq!(
        error.message,
        "Tool \"sample_tool\" requires JSON-schema constrained sampling, but allOf schemas are unsupported."
    );

    let error = resolve_json_schema_strict_sampling(&required, false).expect_err("must reject");
    assert_eq!(
        error.message,
        "Tool \"sample_tool\" requires JSON-schema constrained sampling, but strict tools are unsupported."
    );

    let grammar_config = tool(
        "sample_tool",
        object_parameters(),
        Some(grammar(&[(GrammarFormat::OpenaiLark, "x")])),
    );
    assert_eq!(
        resolve_json_schema_strict_sampling(&grammar_config, true).expect("resolves"),
        None
    );
}

#[test]
fn strict_tool_parameters_only_convert_when_strict_is_true() {
    let tool = tool("sample_tool", object_parameters(), None);
    assert_eq!(
        get_json_schema_tool_parameters(&tool, Some(true)).expect("converts"),
        make_strict_json_schema(&tool.parameters).expect("converts")
    );
    assert_eq!(
        get_json_schema_tool_parameters(&tool, Some(false)).expect("passes through"),
        tool.parameters
    );
    assert_eq!(
        get_json_schema_tool_parameters(&tool, None).expect("passes through"),
        tool.parameters
    );
}

#[test]
fn resolves_grammar_sampling_variants() {
    let lark = tool(
        "sample_tool",
        json!({"type": "object", "properties": {"payload": {"type": "string"}}, "required": ["payload"]}),
        Some(grammar(&[(GrammarFormat::OpenaiLark, "start: /[a-z]+/")])),
    );
    let resolved = resolve_grammar_constrained_sampling(&lark, true)
        .expect("resolves")
        .expect("grammar");
    assert_eq!(resolved.format, GrammarSyntax::Lark);
    assert_eq!(resolved.definition, "start: /[a-z]+/");
    assert_eq!(resolved.input_property, "payload");

    let regex = tool(
        "sample_tool",
        json!({"type": "object", "properties": {"payload": {"type": "string"}}, "required": ["payload"]}),
        Some(grammar(&[(GrammarFormat::OpenaiRegex, "^[a-z]+$")])),
    );
    let resolved = resolve_grammar_constrained_sampling(&regex, true)
        .expect("resolves")
        .expect("grammar");
    assert_eq!(resolved.format, GrammarSyntax::Regex);

    let both = tool(
        "sample_tool",
        json!({"type": "object", "properties": {"payload": {"type": "string"}}, "required": ["payload"]}),
        Some(grammar(&[
            (GrammarFormat::OpenaiLark, "lark"),
            (GrammarFormat::OpenaiRegex, "regex"),
        ])),
    );
    assert_eq!(
        resolve_grammar_constrained_sampling(&both, true)
            .expect("resolves")
            .expect("grammar")
            .format,
        GrammarSyntax::Lark
    );

    assert_eq!(
        resolve_grammar_constrained_sampling(&lark, false).expect("resolves"),
        None
    );
    assert_eq!(
        resolve_grammar_constrained_sampling(
            &tool(
                "sample_tool",
                object_parameters(),
                Some(json_schema(JsonSchemaStrictness::Prefer))
            ),
            true
        )
        .expect("resolves"),
        None
    );
}

#[test]
fn grammar_sampling_errors_are_prefixed_with_the_tool_name() {
    let no_variants = tool(
        "sample_tool",
        object_parameters(),
        Some(grammar(&[(GrammarFormat::OpenaiLark, "   ")])),
    );
    let error = resolve_grammar_constrained_sampling(&no_variants, true).expect_err("must reject");
    assert_eq!(
        error.message,
        "Tool \"sample_tool\" cannot use grammar constrained sampling: no supported grammar variant was provided."
    );

    let bad_property = tool(
        "sample_tool",
        json!({"type": "object", "properties": {"payload": {"type": "number"}}, "required": ["payload"]}),
        Some(grammar(&[(GrammarFormat::OpenaiLark, "lark")])),
    );
    let error = resolve_grammar_constrained_sampling(&bad_property, true).expect_err("must reject");
    assert_eq!(
        error.message,
        "Tool \"sample_tool\" cannot use grammar constrained sampling: grammar constrained sampling property payload must have type string."
    );

    let two_required = tool(
        "sample_tool",
        json!({"type": "object", "properties": {"a": {"type": "string"}, "b": {"type": "string"}}, "required": ["a", "b"]}),
        Some(grammar(&[(GrammarFormat::OpenaiLark, "lark")])),
    );
    let error = resolve_grammar_constrained_sampling(&two_required, true).expect_err("must reject");
    assert_eq!(
        error.message,
        "Tool \"sample_tool\" cannot use grammar constrained sampling: grammar constrained sampling requires exactly one required string property."
    );
}

#[test]
fn reads_and_appends_grammar_tool_input_json() {
    let arguments = json!({"payload": "abc"}).as_object().cloned().expect("object");
    assert_eq!(
        get_grammar_tool_input("sample_tool", &arguments, "payload").expect("reads"),
        "abc"
    );
    let invalid = json!({"payload": 42}).as_object().cloned().expect("object");
    let error = get_grammar_tool_input("sample_tool", &invalid, "payload").expect_err("must reject");
    assert_eq!(
        error.message,
        "Grammar tool call \"sample_tool\" requires argument \"payload\" to be a string."
    );

    let mut buffer = GrammarToolInputJsonBuffer::default();
    let first = append_grammar_tool_input_json_delta(&mut buffer, "payload", "a\"", false)
        .expect("appends")
        .expect("delta");
    let second = append_grammar_tool_input_json_delta(&mut buffer, "payload", "a\"\nb", true)
        .expect("appends")
        .expect("delta");
    let parsed: Value = serde_json::from_str(&format!("{first}{second}")).expect("valid JSON");
    assert_eq!(parsed, json!({"payload": "a\"\nb"}));
    assert_eq!(
        append_grammar_tool_input_json_delta(&mut buffer, "payload", "a\"\nb", true).expect("idempotent"),
        None
    );
    let error = append_grammar_tool_input_json_delta(&mut buffer, "payload", "changed", true).expect_err("must reject");
    assert_eq!(
        error.message,
        "grammar tool input for property \"payload\" changed after it was closed"
    );
}

#[test]
fn grammar_input_deltas_must_be_monotonic() {
    let mut buffer = GrammarToolInputJsonBuffer::default();
    append_grammar_tool_input_json_delta(&mut buffer, "payload", "ab", false).expect("appends");
    let error = append_grammar_tool_input_json_delta(&mut buffer, "payload", "a", false).expect_err("must reject");
    assert_eq!(
        error.message,
        "grammar tool input for property \"payload\" changed non-monotonically"
    );
    assert_eq!(
        append_grammar_tool_input_json_delta(&mut buffer, "payload", "ab", false).expect("no change"),
        None
    );
}

#[test]
fn creates_grammar_input_properties_for_supported_tools() {
    let parameters = json!({"type": "object", "properties": {"payload": {"type": "string"}}, "required": ["payload"]});
    let tools = vec![
        tool(
            "grammar_tool",
            parameters.clone(),
            Some(grammar(&[(GrammarFormat::OpenaiLark, "lark")])),
        ),
        tool("plain_tool", parameters, None),
    ];
    let properties = create_grammar_tool_input_properties(Some(&tools), true).expect("resolves");
    assert_eq!(properties.len(), 1);
    assert_eq!(properties["grammar_tool"], "payload");
    assert!(
        create_grammar_tool_input_properties(None, true)
            .expect("resolves")
            .is_empty()
    );
}
