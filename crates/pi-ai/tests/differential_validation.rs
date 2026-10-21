#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::types::{Tool, ToolCall, TypeBoxSchema};
use pi_ai::utils::validation::{validate_tool_arguments, validate_tool_call};
use serde_json::{Map, Value, json};

fn typebox(schema: Value) -> Value {
    json!({ "kind": "typebox", "schema": schema })
}

fn plain(schema: Value) -> Value {
    json!({ "kind": "plain", "schema": schema })
}

fn case(name: &str, schema: Value, arguments: Value) -> Value {
    json!({
        "name": name,
        "kind": schema["kind"],
        "schema": schema["schema"],
        "arguments": arguments,
    })
}

fn cases() -> Vec<Value> {
    let object_with = |properties: Value, required: Value| json!({ "t": "object", "properties": properties, "options": { "required": required } });
    vec![
        // -------------------------------------------------------------
        // TypeBox-built schemas: `Value.Convert`
        // -------------------------------------------------------------
        case(
            "typebox-valid-object",
            typebox(json!({
                "t": "object",
                "properties": {
                    "text": { "t": "string" },
                    "count": { "t": "integer" },
                    "ratio": { "t": "number" },
                    "flag": { "t": "boolean" },
                    "nothing": { "t": "null" },
                    "items": { "t": "array", "items": { "t": "number" } },
                    "nested": object_with(json!({ "enabled": { "t": "boolean" } }), json!(["enabled"]))
                },
                "options": { "required": ["text", "count", "ratio", "flag", "nothing", "items", "nested"] }
            })),
            json!({
                "text": "hello",
                "count": 3,
                "ratio": 1.5,
                "flag": true,
                "nothing": null,
                "items": [1, 2],
                "nested": { "enabled": false }
            }),
        ),
        case(
            "typebox-primitive-coercions",
            typebox(object_with(
                json!({
                    "text": { "t": "string" },
                    "count": { "t": "integer" },
                    "ratio": { "t": "number" },
                    "flag": { "t": "boolean" },
                    "nothing": { "t": "null" }
                }),
                json!(["text", "count", "ratio", "flag", "nothing"]),
            )),
            json!({
                "text": 5,
                "count": "42.9",
                "ratio": "0x10",
                "flag": "TRUE",
                "nothing": "null"
            }),
        ),
        case(
            "typebox-bigint-like-strings",
            typebox(object_with(
                json!({
                    "canonical": { "t": "number" },
                    "leadingZero": { "t": "number" },
                    "unsafe": { "t": "number" }
                }),
                json!(["canonical", "leadingZero", "unsafe"]),
            )),
            json!({ "canonical": "123n", "leadingZero": "0123n", "unsafe": "9007199254740992n" }),
        ),
        case(
            "typebox-string-null",
            typebox(object_with(json!({ "value": { "t": "string" } }), json!(["value"]))),
            json!({ "value": null }),
        ),
        case(
            "typebox-array-wraps-scalar",
            typebox(object_with(
                json!({ "value": { "t": "array", "items": { "t": "number" } } }),
                json!(["value"]),
            )),
            json!({ "value": "5" }),
        ),
        case(
            "typebox-tuple-conversion",
            typebox(object_with(
                json!({ "value": { "t": "tuple", "items": [{ "t": "number" }, { "t": "string" }] } }),
                json!(["value"]),
            )),
            json!({ "value": ["1", 2, true] }),
        ),
        case(
            "typebox-record-conversion",
            typebox(object_with(
                json!({ "value": { "t": "record", "key": { "t": "string" }, "value": { "t": "number" } } }),
                json!(["value"]),
            )),
            json!({ "value": { "x": "1", "y": true, "z": "nope" } }),
        ),
        case(
            "typebox-enum-string-number",
            typebox(object_with(
                json!({ "value": { "t": "enum", "values": [1, 2] } }),
                json!(["value"]),
            )),
            json!({ "value": "1" }),
        ),
        case(
            "typebox-enum-string-invalid",
            typebox(object_with(
                json!({ "value": { "t": "enum", "values": ["a", "b"] } }),
                json!(["value"]),
            )),
            json!({ "value": 5 }),
        ),
        case(
            "typebox-literal-number",
            typebox(object_with(
                json!({ "value": { "t": "literal", "valueLiteral": 42 } }),
                json!(["value"]),
            )),
            json!({ "value": "42" }),
        ),
        case(
            "typebox-literal-string-mismatch",
            typebox(object_with(
                json!({ "value": { "t": "literal", "valueLiteral": "x" } }),
                json!(["value"]),
            )),
            json!({ "value": 5 }),
        ),
        case(
            "typebox-nullable-union-null",
            typebox(object_with(
                json!({ "value": { "t": "union", "anyOf": [{ "t": "number" }, { "t": "null" }] } }),
                json!(["value"]),
            )),
            json!({ "value": null }),
        ),
        case(
            "typebox-nullable-union-coerces",
            typebox(object_with(
                json!({ "value": { "t": "union", "anyOf": [{ "t": "number" }, { "t": "null" }] } }),
                json!(["value"]),
            )),
            json!({ "value": "42" }),
        ),
        case(
            "typebox-union-bool-to-number",
            typebox(object_with(
                json!({ "value": { "t": "union", "anyOf": [{ "t": "number" }, { "t": "string" }] } }),
                json!(["value"]),
            )),
            json!({ "value": true }),
        ),
        case(
            "typebox-intersect",
            typebox(json!({
                "t": "intersect",
                "allOf": [
                    { "t": "object", "properties": { "a": { "t": "string" } }, "options": { "required": ["a"] } },
                    { "t": "object", "properties": { "b": { "t": "number" } }, "options": { "required": ["b"] } }
                ]
            })),
            json!({ "a": 1, "b": "2" }),
        ),
        case(
            "typebox-intersect-same-key",
            typebox(json!({
                "t": "intersect",
                "allOf": [
                    { "t": "object", "properties": { "a": { "t": "string" } }, "options": { "required": ["a"] } },
                    { "t": "object", "properties": { "a": { "t": "string", "options": { "minLength": 2 } } }, "options": { "required": ["a"] } }
                ]
            })),
            json!({ "a": 1 }),
        ),
        case(
            "typebox-array-of-objects-optional-nulls",
            typebox(object_with(
                json!({
                    "values": {
                        "t": "array",
                        "items": {
                            "t": "object",
                            "properties": {
                                "name": { "t": "string" },
                                "extra": { "t": "optional", "schema": { "t": "number" } }
                            },
                            "options": { "required": ["name"] }
                        }
                    }
                }),
                json!(["values"]),
            )),
            json!({ "values": [{ "name": "a", "extra": null }, { "name": "b", "extra": "3" }] }),
        ),
        case(
            "typebox-cyclic-ref",
            typebox(json!({
                "t": "cyclic",
                "defs": {
                    "Node": {
                        "t": "object",
                        "properties": {
                            "v": { "t": "number" },
                            "next": { "t": "optional", "schema": { "t": "ref", "ref": "Node" } }
                        },
                        "options": { "required": ["v"] }
                    }
                },
                "ref": "Node"
            })),
            json!({ "v": "1", "next": { "v": "2" } }),
        ),
        case(
            "typebox-unresolved-ref",
            typebox(object_with(
                json!({ "value": { "t": "ref", "ref": "Missing" } }),
                json!(["value"]),
            )),
            json!({ "value": "1" }),
        ),
        case(
            "typebox-additional-properties-conversion",
            typebox(json!({
                "t": "object",
                "properties": { "known": { "t": "string" } },
                "options": { "required": ["known"], "additionalProperties": { "type": "number" } }
            })),
            json!({ "known": 1, "extra": "2" }),
        ),
        case(
            "typebox-unsafe-string-enum",
            typebox(object_with(
                json!({ "value": { "t": "unsafe", "schema": { "type": "string", "enum": ["a", "b"] } } }),
                json!(["value"]),
            )),
            json!({ "value": 5 }),
        ),
        case(
            "typebox-unknown-any",
            typebox(object_with(
                json!({ "a": { "t": "unknown" }, "b": { "t": "any" } }),
                json!(["a", "b"]),
            )),
            json!({ "a": { "deep": [1, null] }, "b": "text" }),
        ),
        case(
            "typebox-constraints-errors",
            typebox(object_with(
                json!({
                    "count": { "t": "integer", "options": { "minimum": 1, "maximum": 5 } },
                    "text": { "t": "string", "options": { "minLength": 2, "maxLength": 3 } }
                }),
                json!(["count", "text"]),
            )),
            json!({ "count": 0, "text": "" }),
        ),
        case(
            "typebox-missing-required-and-extra",
            typebox(json!({
                "t": "object",
                "properties": { "a": { "t": "string" } },
                "options": { "required": ["a", "b"], "additionalProperties": false }
            })),
            json!({ "c": 1, "d": 2 }),
        ),
        case(
            "typebox-optional-nulls",
            typebox(json!({
                "t": "object",
                "properties": {
                    "path": { "t": "string" },
                    "offset": { "t": "optional", "schema": { "t": "number" } },
                    "nullable": { "t": "optional", "schema": { "t": "union", "anyOf": [{ "t": "string" }, { "t": "null" }] } },
                    "metadata": {
                        "t": "object",
                        "properties": { "enabled": { "t": "optional", "schema": { "t": "boolean" } } }
                    }
                },
                "options": { "required": ["path", "metadata"] }
            })),
            json!({ "path": "file.txt", "offset": null, "nullable": null, "metadata": { "enabled": null } }),
        ),
        // -------------------------------------------------------------
        // Plain JSON schemas: AJV-compatible coercion + full validation
        // -------------------------------------------------------------
        case(
            "plain-primitive-coercions",
            plain(json!({
                "type": "object",
                "properties": {
                    "number": { "type": "number" },
                    "integer": { "type": "integer" },
                    "boolean": { "type": "boolean" },
                    "string": { "type": "string" },
                    "nothing": { "type": "null" }
                },
                "required": ["number", "integer", "boolean", "string", "nothing"]
            })),
            json!({ "number": "0x10", "integer": "42", "boolean": "false", "string": true, "nothing": "" }),
        ),
        case(
            "plain-union-types",
            plain(json!({
                "type": "object",
                "properties": {
                    "a": { "type": ["number", "string"] },
                    "b": { "type": ["boolean", "number"] },
                    "c": { "type": ["array", "null"], "items": { "type": "string" } }
                },
                "required": ["a", "b", "c"]
            })),
            json!({ "a": "1", "b": "1", "c": null }),
        ),
        case(
            "plain-nullable-union-coercion",
            plain(json!({
                "type": "object",
                "properties": { "value": { "anyOf": [{ "type": "number" }, { "type": "null" }] } },
                "required": ["value"]
            })),
            json!({ "value": "42" }),
        ),
        case(
            "plain-nullable-union-null",
            plain(json!({
                "type": "object",
                "properties": { "value": { "anyOf": [{ "type": "number" }, { "type": "null" }] } },
                "required": ["value"]
            })),
            json!({ "value": null }),
        ),
        case(
            "plain-one-of-null",
            plain(json!({
                "type": "object",
                "properties": { "value": { "oneOf": [{ "type": "number" }, { "type": "null" }] } },
                "required": ["value"]
            })),
            json!({ "value": null }),
        ),
        case(
            "plain-one-of-two-passes",
            plain(json!({
                "type": "object",
                "properties": { "value": { "oneOf": [{ "type": "number" }, { "type": "integer" }] } },
                "required": ["value"]
            })),
            json!({ "value": 1 }),
        ),
        case(
            "plain-any-of-errors",
            plain(json!({
                "type": "object",
                "properties": { "value": { "anyOf": [{ "type": "number" }, { "type": "string" }] } },
                "required": ["value"]
            })),
            json!({ "value": true }),
        ),
        case(
            "plain-all-of-errors",
            plain(json!({
                "allOf": [
                    { "type": "object", "required": ["a"], "properties": { "a": { "type": "string" } } },
                    { "type": "object", "required": ["b"], "properties": { "b": { "type": "number" } } }
                ]
            })),
            json!({}),
        ),
        case(
            "plain-not",
            plain(json!({
                "type": "object",
                "properties": { "value": { "not": { "type": "number" } } },
                "required": ["value"]
            })),
            json!({ "value": 5 }),
        ),
        case(
            "plain-if-then",
            plain(json!({
                "type": "object",
                "properties": { "kind": { "type": "string" }, "size": { "type": "number" } },
                "required": ["kind", "size"],
                "if": { "properties": { "kind": { "const": "big" } } },
                "then": { "properties": { "size": { "minimum": 100 } } }
            })),
            json!({ "kind": "big", "size": 5 }),
        ),
        case(
            "plain-if-else",
            plain(json!({
                "type": "object",
                "properties": { "kind": { "type": "string" }, "size": { "type": "number" } },
                "required": ["kind", "size"],
                "if": { "properties": { "kind": { "const": "big" } } },
                "else": { "properties": { "size": { "maximum": 10 } } }
            })),
            json!({ "kind": "small", "size": 50 }),
        ),
        case(
            "plain-ref-nullable",
            plain(json!({
                "type": "object",
                "properties": { "value": { "$ref": "#/$defs/value" } },
                "$defs": { "value": { "anyOf": [{ "type": "number" }, { "type": "null" }] } }
            })),
            json!({ "value": null }),
        ),
        case(
            "plain-ref-object",
            plain(json!({
                "type": "object",
                "properties": { "value": { "$ref": "#/$defs/value" } },
                "$defs": {
                    "value": {
                        "type": "object",
                        "properties": { "count": { "type": "number" } },
                        "required": ["count"]
                    }
                }
            })),
            json!({ "value": { "count": "3" } }),
        ),
        case(
            "plain-unresolved-ref",
            plain(json!({ "type": "object", "properties": { "value": { "$ref": "#/$defs/missing" } } })),
            json!({ "value": 1 }),
        ),
        case(
            "plain-const-and-enum",
            plain(json!({
                "type": "object",
                "properties": {
                    "constant": { "const": 3 },
                    "choice": { "enum": ["a", "b", 5] }
                },
                "required": ["constant", "choice"]
            })),
            json!({ "constant": 4, "choice": "c" }),
        ),
        case(
            "plain-string-constraints",
            plain(json!({
                "type": "object",
                "properties": { "text": { "type": "string", "minLength": 2, "maxLength": 3, "pattern": "^x" } },
                "required": ["text"]
            })),
            json!({ "text": "" }),
        ),
        case(
            "plain-number-constraints",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "number",
                        "exclusiveMaximum": 10,
                        "exclusiveMinimum": 0,
                        "maximum": 5,
                        "minimum": 1,
                        "multipleOf": 2
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": 0.5 }),
        ),
        case(
            "plain-array-constraints",
            plain(json!({
                "type": "object",
                "properties": {
                    "values": {
                        "type": "array",
                        "items": { "type": "number" },
                        "minItems": 2,
                        "maxItems": 3,
                        "uniqueItems": true
                    }
                },
                "required": ["values"]
            })),
            json!({ "values": [1, "x", 1] }),
        ),
        case(
            "plain-tuple-items",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "array",
                        "items": [{ "type": "number" }, { "type": "string" }],
                        "additionalItems": { "type": "boolean" }
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": ["x", 2, "extra"] }),
        ),
        case(
            "plain-prefix-items",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "array",
                        "prefixItems": [{ "type": "number" }],
                        "items": { "type": "string" }
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": ["x", 2, true] }),
        ),
        case(
            "plain-contains",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "array",
                        "contains": { "type": "number" },
                        "minContains": 2,
                        "maxContains": 2
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": [1, "x", true] }),
        ),
        case(
            "plain-object-constraints",
            plain(json!({
                "type": "object",
                "properties": { "value": { "type": "object", "minProperties": 2, "maxProperties": 2 } },
                "required": ["value"]
            })),
            json!({ "value": { "a": 1 } }),
        ),
        case(
            "plain-property-names",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": { "type": "object", "propertyNames": { "pattern": "^[a-z]+$" } }
                },
                "required": ["value"]
            })),
            json!({ "value": { "good": 1, "Bad": 2, "also bad": 3 } }),
        ),
        case(
            "plain-pattern-properties",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "object",
                        "patternProperties": { "^n": { "type": "number" } },
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": { "num": "1", "text": 2 } }),
        ),
        case(
            "plain-dependencies",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "object",
                        "dependencies": { "a": ["b", "c"] },
                        "dependentRequired": { "x": ["y"] },
                        "dependentSchemas": { "m": { "required": ["n"] } }
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": { "a": 1, "x": 1, "m": 1 } }),
        ),
        case(
            "plain-additional-properties-schema",
            plain(json!({
                "type": "object",
                "properties": { "known": { "type": "string" } },
                "required": ["known"],
                "additionalProperties": { "type": "number" }
            })),
            json!({ "known": "x", "extra": "nope" }),
        ),
        case(
            "plain-boolean-schema-false",
            plain(json!({ "type": "object", "properties": { "value": false }, "required": ["value"] })),
            json!({ "value": 1 }),
        ),
        case(
            "plain-error-limit",
            plain(json!({
                "type": "object",
                "properties": {
                    "a": { "type": "string" }, "b": { "type": "string" }, "c": { "type": "string" },
                    "d": { "type": "string" }, "e": { "type": "string" }, "f": { "type": "string" },
                    "g": { "type": "string" }, "h": { "type": "string" }, "i": { "type": "string" },
                    "j": { "type": "string" }
                }
            })),
            json!({
                "a": {}, "b": {}, "c": {}, "d": {}, "e": {},
                "f": {}, "g": {}, "h": {}, "i": {}, "j": {}
            }),
        ),
        case(
            "plain-number-formatting",
            plain(json!({
                "type": "object",
                "properties": { "value": { "type": "string" } },
                "required": ["value"]
            })),
            json!({
                "value": { "nested": true },
                "whole": 1.0,
                "fraction": 0.1,
                "big": 1e21,
                "small": 1e-7,
                "negativeZero": -0.0,
                "huge": 123456789012345680000.0
            }),
        ),
        case(
            "plain-string-escaping",
            plain(json!({
                "type": "object",
                "properties": { "value": { "type": "string" } },
                "required": ["value"]
            })),
            json!({
                "value": { "nested": true },
                "text": "quote \" backslash \\ newline \n tab \t control \u{0001} unicode ✓ emoji 😀",
                "key with \"quote\"": "v",
                "key.with/slash": "v"
            }),
        ),
        case(
            "plain-path-formatting",
            plain(json!({
                "type": "object",
                "properties": {
                    "outer": {
                        "type": "object",
                        "properties": { "inner": { "type": "number" } },
                        "required": ["inner"]
                    }
                },
                "required": ["outer"]
            })),
            json!({ "outer": { "inner": "nope" } }),
        ),
        case(
            "plain-dependent-required-multiple",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "object",
                        "dependentRequired": { "x": ["y", "z"] }
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": { "x": 1 } }),
        ),
        case(
            "plain-any-of-error-limit",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "anyOf": [
                            { "type": "object", "properties": { "a": { "type": "string" }, "b": { "type": "string" }, "c": { "type": "string" }, "d": { "type": "string" }, "e": { "type": "string" } } },
                            { "type": "object", "properties": { "f": { "type": "string" }, "g": { "type": "string" }, "h": { "type": "string" }, "i": { "type": "string" }, "j": { "type": "string" } } }
                        ]
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": { "a": 1, "b": 1, "c": 1, "d": 1, "e": 1, "f": 1, "g": 1, "h": 1, "i": 1, "j": 1 } }),
        ),
        case(
            "plain-regex-special-property-keys",
            plain(json!({
                "type": "object",
                "properties": { "a.b": { "type": "string" }, "c+d": { "type": "string" } },
                "required": ["a.b", "c+d"],
                "additionalProperties": { "type": "number" }
            })),
            json!({ "a.b": 1, "c+d": "x", "aXb": "not additional", "extra": "nope" }),
        ),
        case(
            "plain-number-formatting-small",
            plain(json!({
                "type": "object",
                "properties": { "value": { "type": "string" } },
                "required": ["value"]
            })),
            json!({ "value": 1, "small": 1e-7, "tiny": 1e-21, "precise": 0.30000000000000004 }),
        ),
        case(
            "plain-grapheme-family-emoji",
            plain(json!({
                "type": "object",
                "properties": { "value": { "type": "string", "minLength": 2 } },
                "required": ["value"]
            })),
            json!({ "value": "👨‍👩‍👧‍👦" }),
        ),
        case(
            "plain-contains-ref",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": { "type": "array", "contains": { "$ref": "#/$defs/num" } }
                },
                "$defs": { "num": { "type": "number" } },
                "required": ["value"]
            })),
            json!({ "value": ["x", "y"] }),
        ),
        case(
            "plain-not-ref",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": { "not": { "$ref": "#/$defs/num" } }
                },
                "$defs": { "num": { "type": "number" } },
                "required": ["value"]
            })),
            json!({ "value": 5 }),
        ),
        case(
            "plain-any-of-ref",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "anyOf": [
                            { "$ref": "#/$defs/num" },
                            { "$ref": "#/$defs/text" }
                        ]
                    }
                },
                "$defs": {
                    "num": { "type": "number" },
                    "text": { "type": "string" }
                },
                "required": ["value"]
            })),
            json!({ "value": true }),
        ),
        case(
            "plain-additional-properties-ref",
            plain(json!({
                "type": "object",
                "properties": { "known": { "type": "string" } },
                "additionalProperties": { "$ref": "#/$defs/num" },
                "$defs": { "num": { "type": "number" } },
                "required": ["known"]
            })),
            json!({ "known": "x", "extra": "nope" }),
        ),
        case(
            "plain-dependencies-schema",
            plain(json!({
                "type": "object",
                "properties": {
                    "value": {
                        "type": "object",
                        "dependencies": { "a": { "required": ["b"], "properties": { "b": { "type": "number" } } } }
                    }
                },
                "required": ["value"]
            })),
            json!({ "value": { "a": 1 } }),
        ),
        case(
            "plain-unknown-format-passes",
            plain(json!({
                "type": "object",
                "properties": { "value": { "type": "string", "format": "custom-format" } },
                "required": ["value"]
            })),
            json!({ "value": "anything" }),
        ),
        case(
            "plain-unicode-length",
            plain(json!({
                "type": "object",
                "properties": { "value": { "type": "string", "minLength": 2, "maxLength": 2 } },
                "required": ["value"]
            })),
            json!({ "value": "a\u{0301}b" }),
        ),
    ]
}

fn rust_validate(arguments: &Value, schema: &Value, kind: &str) -> Value {
    let parameters = schema.clone();
    let tool = if kind == "typebox" {
        Tool::typebox("echo", "Echo tool", TypeBoxSchema::new(parameters))
    } else {
        Tool::new("echo", "Echo tool", parameters)
    };
    let tool_call = ToolCall::new(
        "tool-1",
        "echo",
        arguments.as_object().cloned().unwrap_or_else(Map::<String, Value>::new),
    );
    let original = tool_call.arguments.clone();
    let result = match validate_tool_arguments(&tool, &tool_call) {
        Ok(value) => json!({ "ok": true, "value": value }),
        Err(error) => json!({ "ok": false, "message": error.to_string() }),
    };
    assert_eq!(
        tool_call.arguments, original,
        "validation must not mutate caller arguments"
    );
    result
}

fn normalized_expected(result: &Value) -> Value {
    if result["ok"].as_bool() == Some(true) {
        json!({ "ok": true, "value": result["value"].clone() })
    } else {
        json!({ "ok": false, "message": result["error"]["message"].clone() })
    }
}

#[test]
fn validation_matches_the_pinned_typescript_module() {
    let cases = cases();
    let request = json!({ "id": 1, "operation": "validate_tool_arguments", "cases": cases });
    let mut oracle = oracle::TsOracle::spawn("validation-oracle.ts", "packages/ai/src/utils/validation.ts");
    let response = oracle.request(&request);
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), cases.len());

    let mut mismatches = Vec::new();
    for (case, expected) in cases.iter().zip(results) {
        let name = case["name"].as_str().expect("case name");
        let kind = case["kind"].as_str().expect("case kind");
        let schema = &expected["schemas"][0];
        let actual = rust_validate(&case["arguments"], schema, kind);
        if actual != normalized_expected(expected) {
            mismatches.push(format!("{name}: Rust {actual}; TS {}", normalized_expected(expected)));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn tool_call_lookup_matches_the_pinned_typescript_module() {
    let schema = json!({
        "t": "object",
        "properties": { "count": { "t": "number" } },
        "options": { "required": ["count"] }
    });
    let arguments = json!({ "count": "7" });
    let cases = json!([
        {
            "name": "found",
            "toolName": "echo",
            "tools": [{ "name": "echo", "kind": "typebox", "schema": schema }],
            "arguments": arguments
        },
        {
            "name": "missing",
            "toolName": "missing",
            "tools": [{ "name": "echo", "kind": "typebox", "schema": schema }],
            "arguments": arguments
        },
        {
            "name": "plain",
            "toolName": "plain",
            "tools": [{ "name": "plain", "kind": "plain", "schema": { "type": "object" } }],
            "arguments": {}
        }
    ]);
    let mut oracle = oracle::TsOracle::spawn("validation-oracle.ts", "packages/ai/src/utils/validation.ts");
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "validate_tool_call",
        "cases": cases
    }));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let results = response["output"]["results"].as_array().expect("results array");

    for (case, expected) in cases.as_array().expect("cases").iter().zip(results) {
        let tools: Vec<Tool> = case["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let parameters = expected["schemas"][index].clone();
                if spec["kind"] == json!("typebox") {
                    Tool::typebox(
                        spec["name"].as_str().expect("name"),
                        "oracle tool",
                        TypeBoxSchema::new(parameters),
                    )
                } else {
                    Tool::new(spec["name"].as_str().expect("name"), "oracle tool", parameters)
                }
            })
            .collect();
        let tool_call = ToolCall::new(
            "tool-1",
            case["toolName"].as_str().expect("toolName"),
            case["arguments"]
                .as_object()
                .cloned()
                .unwrap_or_else(Map::<String, Value>::new),
        );
        let actual = match validate_tool_call(&tools, &tool_call) {
            Ok(value) => json!({ "ok": true, "value": value }),
            Err(error) => json!({ "ok": false, "message": error.to_string() }),
        };
        assert_eq!(actual, normalized_expected(expected), "case {}", case["name"]);
    }
}

#[test]
fn optional_nulls_and_union_order_match_the_frozen_source() {
    let mut fixtures = Vec::new();
    for (name, plain_schema, typebox_schema) in [
        ("string", json!({"type":"string"}), json!({"t":"string"})),
        ("number", json!({"type":"number"}), json!({"t":"number"})),
        ("integer", json!({"type":"integer"}), json!({"t":"integer"})),
        ("boolean", json!({"type":"boolean"}), json!({"t":"boolean"})),
        ("null", json!({"type":"null"}), json!({"t":"null"})),
        (
            "enum",
            json!({"type":"string","enum":["high","low"]}),
            json!({"t":"unsafe","schema":{"type":"string","enum":["high","low"]}}),
        ),
        (
            "nullable",
            json!({"anyOf":[{"type":"number"},{"type":"null"}]}),
            json!({"t":"union","anyOf":[{"t":"number"},{"t":"null"}]}),
        ),
        (
            "array",
            json!({"type":"array","items":{"type":"number"}}),
            json!({"t":"array","items":{"t":"number"}}),
        ),
        (
            "object",
            json!({"type":"object"}),
            json!({"t":"object","properties":{}}),
        ),
    ] {
        for required in [false, true] {
            let required_keys = if required { json!(["value"]) } else { json!([]) };
            fixtures.push(case(
                &format!("plain-{name}-required-{required}"),
                plain(json!({
                    "type":"object", "properties":{"value":plain_schema}, "required":required_keys
                })),
                json!({"value":null}),
            ));
            let property = if required {
                typebox_schema.clone()
            } else {
                json!({"t":"optional","schema":typebox_schema})
            };
            fixtures.push(case(
                &format!("typebox-{name}-required-{required}"),
                typebox(json!({
                    "t":"object", "properties":{"value":property}
                })),
                json!({"value":null}),
            ));
        }
    }
    for keyword in ["anyOf", "oneOf"] {
        for (name, arms, input) in [
            (
                "coerce-before-existing-match",
                json!([{"type":"number"},{"type":"string"}]),
                json!("42"),
            ),
            ("null-first", json!([{"type":"null"},{"type":"number"}]), Value::Null),
            ("number-first", json!([{"type":"number"},{"type":"null"}]), Value::Null),
            (
                "reject-first-candidate",
                json!([{"type":"number","minimum":5},{"type":"string"}]),
                json!("2"),
            ),
            (
                "no-valid-candidate",
                json!([{"type":"integer"},{"type":"boolean"}]),
                json!({}),
            ),
            (
                "object-candidate-clone",
                json!([
                    {"type":"object","properties":{"value":{"type":"number"}},"required":["missing"]},
                    {"type":"object","properties":{"value":{"type":"string"}}}
                ]),
                json!({"value":null}),
            ),
        ] {
            let mut union = Map::new();
            union.insert(keyword.into(), arms);
            fixtures.push(case(
                &format!("{keyword}-{name}"),
                plain(json!({
                    "type":"object", "properties":{"value":union}, "required":["value"]
                })),
                json!({"value":input}),
            ));
        }
    }
    fixtures.push(case("set-agent-model-explicit-nulls", typebox(json!({
        "t":"object", "properties":{
            "agentId":{"t":"string"},
            "provider":{"t":"optional","schema":{"t":"string"}},
            "modelId":{"t":"optional","schema":{"t":"string"}},
            "thinkingLevel":{"t":"optional","schema":{"t":"unsafe","schema":{"type":"string","enum":["off","minimal","low","medium","high","xhigh"]}}}
        }
    })), json!({"agentId":1,"provider":null,"modelId":null})));
    let mut rejected = fixtures.last().unwrap().clone();
    rejected["name"] = json!("set-agent-model-null-thinking-level");
    rejected["arguments"]["thinkingLevel"] = Value::Null;
    fixtures.push(rejected);

    let mut oracle = oracle::TsOracle::spawn("validation-oracle.ts", "packages/ai/src/utils/validation.ts");
    let response = oracle.request(&json!({"id":1,"operation":"validate_tool_arguments","cases":fixtures}));
    assert_eq!(response["ok"], json!(true), "{response}");
    let results = response["output"]["results"].as_array().unwrap();
    assert_eq!(results.len(), fixtures.len());
    let mut mismatches = Vec::new();
    for (fixture, expected) in fixtures.iter().zip(results) {
        assert_eq!(
            expected["argumentsAfter"], fixture["arguments"],
            "source caller arguments: {}",
            fixture["name"]
        );
        let actual = rust_validate(
            &fixture["arguments"],
            &expected["schemas"][0],
            fixture["kind"].as_str().unwrap(),
        );
        if actual != normalized_expected(expected) {
            mismatches.push(format!(
                "{}: Rust {actual}; TS {}",
                fixture["name"],
                normalized_expected(expected)
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
