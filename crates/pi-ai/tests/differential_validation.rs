use crate::shared_oracle as oracle;

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
        pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(
            arguments.as_object().cloned().unwrap_or_else(Map::<String, Value>::new),
        ))
        .as_object()
        .unwrap()
        .clone(),
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
        if name == "plain-union-types" {
            // Exact approved discrepancy: 6d2363e6 fixes the frozen null-array throw.
            assert_eq!(expected["error"]["name"], "TypeError");
            assert_eq!(
                normalized_expected(expected),
                json!({"ok":false,"message":"Cannot read properties of null (reading 'every')"})
            );
            assert_eq!(actual, json!({"ok":true,"value":{"a":"1","b":1,"c":null}}));
        } else if actual != normalized_expected(expected) {
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
            pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(
                case["arguments"]
                    .as_object()
                    .cloned()
                    .unwrap_or_else(Map::<String, Value>::new),
            ))
            .as_object()
            .unwrap()
            .clone(),
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

#[test]
fn nullable_array_matches_approved_delta_6d2363e6() {
    let original = cases()
        .into_iter()
        .find(|case| case["name"] == "plain-union-types")
        .unwrap();
    let mut cases = vec![original.clone()];
    for value in [json!([]), json!(["valid"]), json!([{}])] {
        let mut case = original.clone();
        case["arguments"]["c"] = value;
        cases.push(case);
    }
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "validation-oracle.ts",
        "packages/ai/src/utils/validation.ts",
        &[("PI_APPROVED_DELTA", "1".into())],
    );
    let response = oracle.request(&json!({"id":1,"operation":"validate_tool_arguments","cases":cases}));
    assert_eq!(response["ok"], true, "{response}");
    let results = response["output"]["results"].as_array().unwrap();
    assert_eq!(results.len(), cases.len());
    assert_eq!(
        normalized_expected(&results[0]),
        json!({"ok":true,"value":{"a":"1","b":1,"c":null}})
    );
    for (case, expected) in cases.iter().zip(results) {
        let actual = rust_validate(&case["arguments"], &expected["schemas"][0], "plain");
        assert_eq!(actual, normalized_expected(expected), "{case}");
    }
}

#[test]
fn lossless_utf16_validation_matches_frozen_typescript() {
    use pi_ai::utils::validation::{schema_error_lines, validate_arguments};
    use pi_js_compat::js_json;
    use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};

    let strings = [
        r#""\ud800""#,
        r#""\udc00""#,
        r#""\ud800\udc00""#,
        r#""😀""#,
        r#""\ud800x""#,
        r#""\ufffd""#,
        r#""aa""#,
        r#""x\ud800""#,
        r#""\ud800\ud800""#,
    ];
    let schemas = [
        json!({"type":"string"}),
        json!({"type":"string", "minLength":2}),
        json!({"type":"string", "maxLength":1}),
        json!({"type":"string", "pattern":"^.$"}),
        json!({"type":"string", "pattern":"^..$"}),
        json!({"type":"string", "pattern":r"^[\uD800-\uDBFF]$"}),
        json!({"type":"string", "pattern":r"^(.)\1$"}),
        json!({"type":"string", "pattern":r"(?<=x)."}),
        json!({"type":"number"}),
        json!({"anyOf":[{"type":"number"},{"type":"string"}]}),
        json!({"type":"string", "format":"email"}),
        json!({"type":"string", "format":"uuid"}),
        json!({"type":"string", "format":"unknown-format"}),
    ];
    let mut fixtures = Vec::new();
    for (index, string) in strings.iter().enumerate() {
        for (schema_index, schema) in schemas.iter().enumerate() {
            let value = parse_lossless(*string).unwrap();
            fixtures.push(
                js_json!({"name":format!("utf16-{index}-{schema_index}"), "kind":"plain",
                "schema":{"type":"object","properties":{"value":schema},"required":["value"]},
                "arguments":{"value":value}}),
            );
        }
    }
    for (name, schema, arguments) in [
        (
            "unicode-pattern-property",
            r#"{"type":"object","patternProperties":{"^.$":{"type":"number"}}}"#,
            r#"{"😀":"x"}"#,
        ),
        (
            "nonunicode-additional-property",
            r#"{"type":"object","patternProperties":{"^.$":{"type":"number"}},"additionalProperties":false}"#,
            r#"{"😀":5}"#,
        ),
        (
            "numeric-property-order",
            r#"{"type":"object","properties":{"10":{"type":"number"},"2":{"type":"number"},"\ud800":{"type":"number"}}}"#,
            r#"{"10":"x","2":"y","\ud800":"z"}"#,
        ),
        (
            "surrogate-reference",
            r##"{"type":"object","properties":{"value":{"$ref":"#/$defs/\ud800"}},"$defs":{"\ud800":{"type":"number"}}}"##,
            r#"{"value":"x"}"#,
        ),
        (
            "surrogate-anchor",
            r##"{"type":"object","properties":{"value":{"$ref":"#\ud800"}},"$defs":{"a":{"$anchor":"\ud800","type":"number"}}}"##,
            r#"{"value":"x"}"#,
        ),
        (
            "surrogate-pattern",
            r#"{"type":"object","properties":{"value":{"type":"string","pattern":"^\ud800$"}}}"#,
            r#"{"value":"\ud800"}"#,
        ),
        (
            "surrogate-pattern-error",
            r#"{"type":"object","properties":{"value":{"type":"string","pattern":"^\ud800$"}}}"#,
            r#"{"value":"\udc00"}"#,
        ),
        (
            "surrogate-path",
            r#"{"type":"object","additionalProperties":{"type":"number"},"patternProperties":{"^.":{"type":"number"}}}"#,
            r#"{"\ud800":"x"}"#,
        ),
        (
            "surrogate-property",
            r#"{"type":"object","properties":{"\ud800":{"type":"number"}}}"#,
            r#"{"\ud800":"42"}"#,
        ),
        (
            "surrogate-required",
            r#"{"type":"object","required":["\ud800"]}"#,
            r#"{}"#,
        ),
        (
            "surrogate-property-name",
            r#"{"type":"object","propertyNames":{"pattern":"^[a-z]$"}}"#,
            r#"{"\ud800":1}"#,
        ),
        (
            "surrogate-const",
            r#"{"type":"object","properties":{"value":{"const":"\ud800"}}}"#,
            r#"{"value":"\ud800"}"#,
        ),
        (
            "distinct-surrogate-enum",
            r#"{"type":"object","properties":{"value":{"enum":["\ud800","\ufffd"]}}}"#,
            r#"{"value":"\udc00"}"#,
        ),
        (
            "distinct-surrogate-keys",
            r#"{"type":"object","additionalProperties":{"type":"number"}}"#,
            r#"{"\ud800":"42","\udc00":"3","�":"4"}"#,
        ),
        (
            "surrogate-unique",
            r#"{"type":"object","properties":{"value":{"type":"array","uniqueItems":true}}}"#,
            r#"{"value":[{"x":"\ud800"},{"x":"\udc00"}]}"#,
        ),
        (
            "surrogate-duplicate",
            r#"{"type":"object","properties":{"value":{"type":"array","uniqueItems":true}}}"#,
            r#"{"value":[{"x":"\ud800"},{"x":"\ud800"}]}"#,
        ),
    ] {
        fixtures.push(js_json!({"name":name,"kind":"plain","schema":parse_lossless(schema).unwrap(),"arguments":parse_lossless(arguments).unwrap()}));
    }
    fixtures.push(js_json!({"name":"nonunicode-record-conversion","kind":"typebox",
        "schema":{"t":"object","properties":{"value":{"t":"record","key":{"t":"string","options":{"pattern":"^..$"}},"value":{"t":"number"}}}},
        "arguments":{"value":{"😀":"42"}}}));
    for text in [
        r#""\ud800""#,
        r#"" 42 ""#,
        r#""TRUE""#,
        r#""123n""#,
        r#""\uFEFF42\u00a0""#,
        r#""\ud80042""#,
    ] {
        for kind in ["number", "integer", "boolean", "string", "null"] {
            fixtures.push(js_json!({"name":format!("typebox-{kind}-{text}"), "kind":"typebox",
                "schema":{"t":"object","properties":{"value":{"t":kind}}},"arguments":{"value":parse_lossless(text).unwrap()}}));
        }
    }
    for (format, valid, invalid) in [
        ("date", "2024-02-29", "2023-02-29"),
        ("time", "23:59:60Z", "25:00:00Z"),
        ("date-time", "2024-02-29T23:59:60Z", "2024-02-30T12:00:00Z"),
        ("duration", "P1Y2M3DT4H5M6S", "P"),
        ("email", "a@example.com", "a..b@example.com"),
        ("idn-email", "é@例子.测试", "x@\u{d7ff} "),
        ("hostname", "example.com", "bad_host"),
        ("idn-hostname", "éxample。com", "bad host"),
        ("ipv4", "192.168.1.1", "192.168.001.1"),
        ("ipv6", "::ffff:192.0.2.1", "1::2::3"),
        ("iri", "https://例子.测试/", ":invalid"),
        ("iri-reference", "路径/文件", "bad path"),
        ("json-pointer", "/a~1b", "/a~2b"),
        ("json-pointer-uri-fragment", "#/a%20b", "#/a%"),
        ("regex", "^(a)\\1$", "["),
        ("relative-json-pointer", "1/a", "01/a"),
        ("uri", "https://example.com/a%20b", "https://example.com/a%"),
        ("uri-reference", "../a", "a b"),
        ("uri-template", "https://example.com/{+path}", "{path:0}"),
        ("url", "https://example.com/", "http://127.0.0.1/"),
        ("uuid", "12345678-1234-1234-1234-123456789abc", "bad-uuid"),
    ] {
        for value in [
            JsValue::from(json!(valid)),
            JsValue::from(json!(invalid)),
            parse_lossless(r#""\ud800""#).unwrap(),
        ] {
            fixtures.push(js_json!({"name":format!("format-{format}-{value:?}"),"kind":"plain",
                "schema":{"type":"object","properties":{"value":{"type":"string","format":format}}},"arguments":{"value":value}}));
        }
    }
    let mut oracle = oracle::TsOracle::spawn("validation-oracle.ts", "packages/ai/src/utils/validation.ts");
    let response = oracle.request(&js_json!({"id":1,"operation":"validate_tool_arguments","cases":fixtures}));
    assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
    let results = response["output"]["results"].as_array().unwrap();
    assert_eq!(results.len(), fixtures.len());
    let mut mismatches = Vec::new();
    for (fixture, expected) in fixtures.iter().zip(results) {
        let schema = if fixture["kind"] == js_json!("plain") {
            &fixture["schema"]
        } else {
            &expected["schemas"][0]
        };
        let arguments = &fixture["arguments"];
        let before = arguments.clone();
        let result = validate_arguments(schema, arguments, fixture["kind"] == js_json!("typebox"), "echo");
        let actual = match result {
            Ok(value) => js_json!({"ok":true,"value":value}),
            Err(error) => js_json!({"ok":false,"message":error.message()}),
        };
        let expected_value = if expected["ok"] == JsValue::Bool(true) {
            js_json!({"ok":true,"value":expected["value"]})
        } else {
            js_json!({"ok":false,"message":expected["error"]["message"]})
        };
        assert_eq!(arguments, &before);
        assert_eq!(expected["argumentsAfter"], *arguments);
        if actual != expected_value {
            mismatches.push(format!("{:?}: Rust {actual:?}; TS {expected_value:?}", fixture["name"]));
        }
    }
    let lines = schema_error_lines(
        &parse_lossless(r#"{"type":"object","properties":{"\ud800":{"type":"number"}}}"#).unwrap(),
        &parse_lossless(r#"{"\ud800":"x"}"#).unwrap(),
    );
    assert_eq!(
        lines,
        vec![JsString::join(
            [
                JsString::from("  - "),
                JsString::from_utf16(vec![0xd800]),
                JsString::from(": must be number")
            ],
            ""
        )]
    );
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn overflow_numbers_match_frozen_validation() {
    use pi_ai::utils::validation::validate_arguments;
    use pi_js_compat::js_json;
    use pi_js_compat::js_json::{JsValue, parse_lossless};
    let mut fixtures = Vec::new();
    let mut wire_cases = Vec::new();
    for kind in ["plain", "typebox"] {
        for schema_type in ["number", "integer", "string", "boolean", "null"] {
            for literal in ["1e400", "-1e400"] {
                let schema = if kind == "plain" {
                    json!({"type":"object","properties":{"value":{"type":schema_type,"maximum":10}}})
                } else {
                    json!({"t":"object","properties":{"value":{"t":schema_type,"options":{"maximum":10}}}})
                };
                let arguments = format!(r#"{{"value":{literal}}}"#);
                wire_cases.push(format!(r#"{{"name":"{kind}-{schema_type}-{literal}","kind":"{kind}","schema":{schema},"arguments":{arguments}}}"#));
                fixtures.push((kind, parse_lossless(&arguments).unwrap()));
            }
        }
    }
    let request = format!(
        r#"{{"id":1,"operation":"validate_tool_arguments","cases":[{}]}}"#,
        wire_cases.join(",")
    );
    let mut oracle = oracle::TsOracle::spawn("validation-oracle.ts", "packages/ai/src/utils/validation.ts");
    let response = oracle.request_raw_json(&request);
    assert_eq!(response["ok"], JsValue::Bool(true));
    let results = response["output"]["results"].as_array().unwrap();
    assert_eq!(results.len(), fixtures.len());
    for ((kind, arguments), expected) in fixtures.iter().zip(results) {
        let actual = match validate_arguments(&expected["schemas"][0], arguments, *kind == "typebox", "echo") {
            Ok(value) => js_json!({"ok":true,"value":value}),
            Err(error) => js_json!({"ok":false,"message":error.message()}),
        };
        let normalized = if expected["ok"] == JsValue::Bool(true) {
            js_json!({"ok":true,"value":expected["value"]})
        } else {
            js_json!({"ok":false,"message":expected["error"]["message"]})
        };
        assert_eq!(actual, normalized, "{kind}: {arguments:?}");
    }
}
