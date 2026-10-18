# constrained-sampling traceability

Source: `packages/ai/src/api/constrained-sampling.ts`.
Rust: `crates/pi-ai/src/api/constrained_sampling.rs`.
Tests: `crates/pi-ai/tests/constrained_sampling.rs` (9 behavioral), `crates/pi-ai/tests/differential_constrained_sampling.rs` (47 requests vs pinned TS).

## Representation
- JSON Schema is `serde_json::Value`; `structuredClone` → `Value::clone`.
- `UnsupportedStrictJsonSchemaError` is folded into the public `ConstrainedSamplingError { message }`; TypeScript's `error.message` text is preserved verbatim.
- `GrammarConstrainedSampling.format` is `GrammarSyntax::{Lark, Regex}` (TS `"lark" | "regex"`), distinct from `types::GrammarFormat` (the variant keys).
- `createGrammarToolInputProperties` returns `BTreeMap` (TS `Map`; iteration order is not observable here beyond key lookup).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `makeStrictJsonSchema` | `make_strict_json_schema` | `derives_strict_provider_schemas_without_changing_the_input`, `rejects_schemas_that_cannot_be_safely_converted`, differential ids 0-15 |
| unsupported keys (`$ref`, `allOf`, …) | `reject_unsupported_keys` | same |
| `anyOf` validation + recursion | `make_any_of_strict` | same |
| `items` tuple rejection | `make_items_strict` | same |
| object keyword validation | `reject_unsupported_object_keywords` | same |
| optional property wrapping (`anyOf: [schema, null]`) | `make_properties_strict`, `schema_allows_null` | same |
| `required = propertyNames`, `additionalProperties = false` | `make_strict_node` | same |
| `getJsonSchemaToolParameters` | `get_json_schema_tool_parameters` | `strict_tool_parameters_only_convert_when_strict_is_true` |
| `resolveJsonSchemaStrictSampling` | `resolve_json_schema_strict_sampling` | `resolves_json_schema_strict_sampling_by_capability`, differential ids 16-20 |
| `resolveGrammarConstrainedSampling` | `resolve_grammar_constrained_sampling` | `resolves_grammar_sampling_variants`, `grammar_sampling_errors_are_prefixed_with_the_tool_name`, differential ids 21-29 |
| `inferGrammarInputProperty` | `infer_grammar_input_property` | same |
| `getGrammarToolInput` | `get_grammar_tool_input` | `reads_and_appends_grammar_tool_input_json`, differential ids 30-33 |
| `appendGrammarToolInputJsonDelta` | `append_grammar_tool_input_json_delta` | same + `grammar_input_deltas_must_be_monotonic`, differential ids 34-39 |
| `createGrammarToolInputProperties` | `create_grammar_tool_input_properties` | `creates_grammar_input_properties_for_supported_tools`, differential ids 40-42 |

## Branch notes
- `schemaAllowsNull`: `type: "null"`, `type: [... "null"]`, `const: null`, `enum` containing `null`, recursive `anyOf`.
- `isStructuredSchema` uses the `type` string/array plus presence of `properties`/`items`.
- Blank grammar variants use JS `trim()` semantics plus U+FEFF (`is_blank`).
- JSON string escaping for grammar deltas uses `serde_json::to_string` (same escapes as `JSON.stringify` for valid UTF-8 strings).

## Gaps
- `properties` accessed as a non-object (e.g. array) is rejected as "requires a properties entry" instead of JavaScript property lookup on an array. Not reachable from TypeBox-generated schemas.

## Discrepancies
- None.
