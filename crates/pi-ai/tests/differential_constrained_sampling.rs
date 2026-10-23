#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::api::constrained_sampling::{
    GrammarToolInputJsonBuffer, append_grammar_tool_input_json_delta, create_grammar_tool_input_properties,
    get_grammar_tool_input, make_strict_json_schema, resolve_grammar_constrained_sampling,
    resolve_json_schema_strict_sampling,
};
use pi_ai::types::Tool;
use serde_json::{Value, json};

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

fn grammar_tool(name: &str, variants: Value) -> Value {
    json!({
        "name": name,
        "description": "Sample tool",
        "parameters": {
            "type": "object",
            "properties": {"payload": {"type": "string"}},
            "required": ["payload"]
        },
        "constrainedSampling": {"type": "grammar", "variants": variants}
    })
}

fn requests() -> Vec<Value> {
    let mut requests = vec![
        json!({"id": 0, "operation": "strict", "schema": object_parameters()}),
        json!({"id": 1, "operation": "strict", "schema": {"type": "object", "properties": {"metadata": {"type": "object", "additionalProperties": {"type": "string"}}}}}),
        json!({"id": 2, "operation": "strict", "schema": {"allOf": [{"type": "object"}]}}),
        json!({"id": 3, "operation": "strict", "schema": {"type": "object", "properties": {"value": {"anyOf": [{"type": "object", "properties": {"nested": {"type": "string"}}}, {"type": "null"}]}}}}),
        json!({"id": 4, "operation": "strict", "schema": {"type": "object", "properties": {"child": {"$ref": "https://example.com/child.json"}}, "required": ["child"]}}),
        json!({"id": 5, "operation": "strict", "schema": true}),
        json!({"id": 6, "operation": "strict", "schema": {"type": "object", "items": [{"type": "string"}]}}),
        json!({"id": 7, "operation": "strict", "schema": {"anyOf": []}}),
        json!({"id": 8, "operation": "strict", "schema": {"properties": {"a": {"type": "string"}}}}),
        json!({"id": 9, "operation": "strict", "schema": {"type": "object", "properties": {"a": {"type": "string"}}, "required": ["b"]}}),
        json!({"id": 10, "operation": "strict", "schema": {"type": "object", "required": "a"}}),
        json!({"id": 11, "operation": "strict", "schema": {}}),
        json!({"id": 12, "operation": "strict", "schema": {"type": "object", "properties": {"a": null}}}),
        json!({"id": 13, "operation": "strict", "schema": {"type": "object", "additionalProperties": false, "properties": {"a": {"type": ["string", "null"]}}, "required": []}}),
        json!({"id": 14, "operation": "strict", "schema": {"type": ["object"], "properties": {"a": {"type": "string"}}}}),
        json!({"id": 15, "operation": "strict", "schema": {"type": "object", "properties": {"a": {"const": null}, "b": {"enum": [null, "x"]}}}}),
    ];

    let valid_tool = json!({
        "name": "sample_tool",
        "description": "Sample tool",
        "parameters": object_parameters(),
        "constrainedSampling": {"type": "json_schema", "strict": "prefer"}
    });
    let required_tool = json!({
        "name": "sample_tool",
        "description": "Sample tool",
        "parameters": {"allOf": [{"type": "object"}]},
        "constrainedSampling": {"type": "json_schema", "strict": "require"}
    });
    let plain_tool = json!({"name": "sample_tool", "description": "Sample tool", "parameters": object_parameters()});
    for (supports, tool) in [
        (true, &valid_tool),
        (false, &valid_tool),
        (true, &required_tool),
        (false, &required_tool),
        (true, &plain_tool),
    ] {
        requests.push(
            json!({"id": requests.len(), "operation": "resolveJson", "tool": tool, "supportsStrictMode": supports}),
        );
    }

    let lark_tool = grammar_tool("sample_tool", json!({"openai_lark": "start: /[a-z]+/"}));
    let regex_tool = grammar_tool("sample_tool", json!({"openai_regex": "^[a-z]+$"}));
    let both_tool = grammar_tool("sample_tool", json!({"openai_lark": "lark", "openai_regex": "regex"}));
    let empty_tool = grammar_tool("sample_tool", json!({"openai_lark": "   "}));
    let bad_property_tool = json!({
        "name": "sample_tool",
        "description": "Sample tool",
        "parameters": {"type": "object", "properties": {"payload": {"type": "number"}}, "required": ["payload"]},
        "constrainedSampling": {"type": "grammar", "variants": {"openai_lark": "lark"}}
    });
    let two_required_tool = json!({
        "name": "sample_tool",
        "description": "Sample tool",
        "parameters": {"type": "object", "properties": {"a": {"type": "string"}, "b": {"type": "string"}}, "required": ["a", "b"]},
        "constrainedSampling": {"type": "grammar", "variants": {"openai_lark": "lark"}}
    });
    let no_properties_tool = json!({
        "name": "sample_tool",
        "description": "Sample tool",
        "parameters": {"type": "object", "required": ["payload"]},
        "constrainedSampling": {"type": "grammar", "variants": {"openai_lark": "lark"}}
    });
    for (supports, tool) in [
        (true, &lark_tool),
        (false, &lark_tool),
        (true, &regex_tool),
        (true, &both_tool),
        (true, &empty_tool),
        (true, &bad_property_tool),
        (true, &two_required_tool),
        (true, &no_properties_tool),
        (true, &valid_tool),
    ] {
        requests.push(json!({"id": requests.len(), "operation": "resolveGrammar", "tool": tool, "supports": supports}));
    }

    for (arguments, input_property) in [
        (json!({"payload": "abc"}), "payload"),
        (json!({"payload": 42}), "payload"),
        (json!({}), "payload"),
        (json!({"payload": "a\"b\nc"}), "payload"),
    ] {
        requests.push(
            json!({"id": requests.len(), "operation": "getInput", "toolName": "sample_tool",
            "arguments": arguments, "inputProperty": input_property}),
        );
    }

    for (input_property, steps) in [
        (
            "payload",
            json!([{"nextInput": "a\"", "close": false}, {"nextInput": "a\"\nb", "close": true}]),
        ),
        (
            "payload",
            json!([{"nextInput": "ab", "close": false}, {"nextInput": "ab", "close": false}, {"nextInput": "abc", "close": true}]),
        ),
        (
            "payload",
            json!([{"nextInput": "ab", "close": false}, {"nextInput": "a", "close": false}]),
        ),
        (
            "payload",
            json!([{"nextInput": "ab", "close": true}, {"nextInput": "ab", "close": true}, {"nextInput": "changed", "close": true}]),
        ),
        (
            "payload",
            json!([{"nextInput": "😀x", "close": false}, {"nextInput": "😀x😀", "close": true}]),
        ),
        ("payload", json!([])),
    ] {
        requests.push(
            json!({"id": requests.len(), "operation": "append", "inputProperty": input_property, "steps": steps}),
        );
    }

    requests.push(json!({"id": requests.len(), "operation": "properties",
        "tools": [grammar_tool("grammar_tool", json!({"openai_lark": "lark"})), plain_tool, regex_tool], "supports": true}));
    // Insertion order differs from lexicographic order; the returned Map keeps source order.
    requests.push(json!({"id": requests.len(), "operation": "properties",
        "tools": [grammar_tool("zeta_tool", json!({"openai_lark": "z"})), grammar_tool("alpha_tool", json!({"openai_lark": "a"}))], "supports": true}));
    requests.push(json!({"id": requests.len(), "operation": "properties",
        "tools": [lark_tool], "supports": false}));
    requests.push(json!({"id": requests.len(), "operation": "properties", "tools": null, "supports": true}));
    requests
}

fn error_value(error: pi_ai::api::constrained_sampling::ConstrainedSamplingError) -> Value {
    json!({"id": Value::Null, "ok": false, "error": {"name": "Error", "message": error.message}})
}

fn observe_append(request: &Value) -> Result<Value, pi_ai::api::constrained_sampling::ConstrainedSamplingError> {
    let mut buffer = GrammarToolInputJsonBuffer::default();
    let mut deltas = Vec::new();
    let input_property = request["inputProperty"].as_str().expect("inputProperty");
    for step in request["steps"].as_array().expect("steps") {
        let delta = append_grammar_tool_input_json_delta(
            &mut buffer,
            input_property,
            step["nextInput"].as_str().expect("nextInput"),
            step["close"].as_bool().expect("close"),
        )?;
        deltas.push(json!(delta));
    }
    Ok(json!({"deltas": deltas, "buffer": buffer}))
}

fn observe(request: &Value) -> Value {
    let id = request["id"].clone();
    let result = match request["operation"].as_str().expect("operation") {
        "strict" => make_strict_json_schema(&request["schema"])
            .map(|schema| json!({"schema": schema}))
            .map_err(error_value),
        "resolveJson" => {
            let tool: Tool = serde_json::from_value(request["tool"].clone()).expect("tool");
            let supports = request["supportsStrictMode"].as_bool().expect("supports");
            resolve_json_schema_strict_sampling(&tool, supports)
                .map(|strict| json!({"strict": strict}))
                .map_err(error_value)
        }
        "resolveGrammar" => {
            let tool: Tool = serde_json::from_value(request["tool"].clone()).expect("tool");
            let supports = request["supports"].as_bool().expect("supports");
            resolve_grammar_constrained_sampling(&tool, supports)
                .map(|grammar| json!({"grammar": grammar}))
                .map_err(error_value)
        }
        "getInput" => {
            let arguments = request["arguments"].as_object().cloned().expect("arguments");
            get_grammar_tool_input(
                request["toolName"].as_str().expect("toolName"),
                pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(arguments.clone()))
                    .as_object()
                    .unwrap(),
                request["inputProperty"].as_str().expect("inputProperty"),
            )
            .map(|input| json!({"input": input}))
            .map_err(error_value)
        }
        "append" => observe_append(request).map_err(error_value),
        "properties" => {
            let tools: Option<Vec<Tool>> = request["tools"].as_array().map(|tools| {
                tools
                    .iter()
                    .map(|tool| serde_json::from_value(tool.clone()).expect("tool"))
                    .collect()
            });
            let supports = request["supports"].as_bool().expect("supports");
            create_grammar_tool_input_properties(tools.as_deref(), supports)
                .map(|properties| json!({"properties": properties.into_iter().collect::<Vec<_>>()}))
                .map_err(error_value)
        }
        other => panic!("unknown operation {other}"),
    };
    match result {
        Ok(output) => json!({"id": id, "ok": true, "output": output}),
        Err(mut error) => {
            error["id"] = id;
            error
        }
    }
}

#[test]
fn constrained_sampling_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn(
        "constrained-sampling-oracle.ts",
        "packages/ai/src/api/constrained-sampling.ts",
    );
    for request in requests() {
        assert_eq!(observe(&request), oracle.request(&request), "request {request}");
    }
}
