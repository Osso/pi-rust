# utils/validation traceability

Sources: `packages/ai/src/utils/validation.ts` (350 lines) at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/utils/validation.rs`.

Differential oracle: `oracle/validation-oracle.ts` (builds TypeBox schemas with the pinned
`Type.*` builders or passes plain JSON, calls the real `validateToolArguments`/`validateToolCall`).
Tests: `crates/pi-ai/tests/validation.rs` (26 behavioral cases), `crates/pi-ai/tests/differential_validation.rs`
(68 schema/argument cases + 3 tool-lookup cases), both byte-for-byte on returned values and `Error.message`.

## Representation decisions

- `Tool.parameters` stays a plain `serde_json::Value` (JSON Schema). TypeBox origin is recorded on
  `Tool.typebox_kind`; `Tool::typebox` sets it, `Tool::new` leaves it false. TypeScript records the
  same fact with a hidden `~kind` string property, which a `serde_json::Value` cannot carry.
  `typebox_kind` gates only the `Value.Convert` pass, because that is the only place TypeBox
  dispatches on the marker.
- `validate_tool_arguments` / `validate_tool_call` return `Result<Value, ValidationError>`.
  `ValidationError` implements `Display`/`Error` with the exact TypeScript `Error.message`.
- Coercion is modeled with an in-place/replacement enum (`Conversion::{InPlace, Replaced}`) to mirror
  JavaScript's reference semantics: object/record conversion mutates the argument map, primitive and
  union conversion replaces the value, and `validateToolArguments` discards the root replacement.
- Validation is a hand-written JSON-Schema engine (no crate reproduces TypeBox's messages, ordering,
  instance paths, or `maxErrors`). Numbers in messages and in `JSON.stringify(…, null, 2)` use
  `pi_js_compat::number::js_number_to_string`; converted JSON numbers are canonicalized to integers when
  integral so `Value` equality and serialization match JavaScript.
- `Issues` carries a limit: the root context stops at TypeBox's default `maxErrors = 8`; scratch
  contexts used by `anyOf`/`oneOf`/`if`/`properties`/`additionalProperties` are unlimited and truncate
  only when merged into the root, matching `ErrorContext` vs `AccumulatedErrorContext`.

## Symbols → proof

| TypeScript | Rust | Test |
|---|---|---|
| `validateToolCall` | `validate_tool_call` | `validate_tool_call_reports_unknown_tool_names`, `validate_tool_call_validates_matching_tools`, differential `tool_call_lookup_matches_the_pinned_typescript_module` |
| `validateToolArguments` | `validate_tool_arguments` | 26 behavioral tests + 68 differential cases |
| `normalizeOptionalNulls` | `normalize_optional_nulls` | `null_is_omitted_for_optional_non_nullable_typebox_properties`, `optional_nulls_with_nullable_references_are_preserved`, `nulls_matching_a_typebox_nullable_union_arm_are_preserved`, `typebox-array-of-objects-optional-nulls` |
| `Value.Convert` (`FromType` dispatch) | `convert` + `convert_*` + `try_*` | `typebox-*` differential cases, including `typebox-bigint-like-strings` |
| `CheckSchema` inside keywords | `check` (standalone sub-schema) / `check_with` (caller schema stack) | `plain-contains-ref`, `plain-not-ref`, `plain-any-of-ref`, `plain-additional-properties-ref` |
| `coerceWithJsonSchema` | `coerce_into`, `coerce_union`, `coerce_primitive_by_type` | `plain-*` differential cases, `plain_schemas_use_ajv_compatible_primitive_coercions` |
| `getValidator` cache | none (stateless engine) | n/a — no observable behavior |
| `formatValidationPath` | `Issue::path` | `required_paths_include_the_first_missing_property`, `plain-path-formatting` |
| error message + `JSON.stringify(args, null, 2)` | `json_stringify_pretty`, `write_json_string` | `error_messages_list_every_failing_keyword_in_engine_order`, `received_arguments_are_pretty_printed_with_javascript_number_formatting`, `plain-number-formatting*`, `plain-string-escaping` |

## Engine coverage

Checks implemented in TypeBox's evaluation order (type; object keywords; array keywords; string
keywords; number keywords; ref/const/enum/if/not/allOf/anyOf/oneOf):

- `type` (string or string array; unknown type names pass, as in TypeBox), `properties`, `required`,
  `additionalProperties` (boolean and schema), `patternProperties`, `propertyNames`, `dependencies`,
  `dependentRequired`, `dependentSchemas`, `minProperties`, `maxProperties`.
- `items` (sized and unsized), `prefixItems`, `additionalItems`, `contains`, `minContains`,
  `maxContains`, `minItems`, `maxItems`, `uniqueItems`.
- `minLength`/`maxLength` using TypeBox's grapheme-cluster counting (UTF-16 units, ZWJ, regional
  indicators, variation selectors), `pattern` (Rust `regex`), `format` (gap).
- `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` (TypeBox tolerance).
- `$ref` (JSON Pointer, `#anchor`/`$anchor`/`$dynamicAnchor`, `$id` exact match, `$defs` cyclic
  contexts), boolean schemas, `const`, `enum`, `if`/`then`/`else`, `not`, `allOf`, `anyOf`, `oneOf`.
- `maxErrors = 8`, exhaustive error accumulation inside union arms, child-error suppression for
  `additionalProperties`/`propertyNames`/`contains`, and `required` path suffixing.

## Gaps

- `format`: not enforced (unknown formats pass, matching TypeBox's registry fallback). Pi tool schemas
  in `packages/*/src` never use `format`; registered formats (`email`, `uuid`, `date-time`, …) would
  diverge. Reason: no in-repo consumer; revisit with MCP schema parity.
- `$recursiveRef`, `$dynamicRef`, `unevaluatedItems`, `unevaluatedProperties`, `~refine`: not used by
  any Pi tool schema. `~refine` carries JavaScript functions, which cannot be represented in JSON.
- JavaScript-only regular-expression constructs (lookbehind, backreferences) in `pattern` /
  `patternProperties`: Rust `regex` rejects them; a rejected pattern is treated as non-matching.
  Pi tool schemas only use `^.*$`-style patterns.
- Relative `$id` URI resolution beyond exact `$id`/`$ref` string equality (TypeBox uses WHATWG URL
  pathname comparison). Pi schemas use `$id: "Node"` with `$ref: "Node"` and JSON-Pointer fragments.
- `Value.Convert` resolves union-arm refs against its conversion `$defs` context in TypeScript; the
  Rust conversion resolves refs against the schema tree. Pi schemas do not combine `Type.Union` with
  `Type.Ref` inside `Type.Cyclic`.
- Non-object `ToolCall.arguments`: the Rust type is `JsonObject`, per the typed-core decision; the
  TypeScript runtime accepts any value (e.g. `arguments: 1`). Differential cases keep arguments objects.
- Nested `Type.Unsafe` nodes are converted as TypeBox nodes because origin is recorded per tool, not
  per node. `StringEnum` is the only in-repo `Type.Unsafe` use and its observable behavior matches;
  an `Unsafe({type:"integer"})` node with `"42.9"` would truncate in Rust where TypeScript fails.

## Discrepancies

- `TYPEBOX_KIND` is dead code at the pinned TypeBox version (1.3.7). `Type.Object(...)` has no symbol
  properties (`Object.getOwnPropertySymbols` is empty); the marker is a hidden `~kind` string. The
  `!Object.getOwnPropertySymbols(...).includes(TYPEBOX_KIND)` condition is therefore always true and
  `coerceWithJsonSchema` always runs. Rust preserves that: coercion always runs, and `typebox_kind`
  only enables `Value.Convert`. A TypeBox `Literal("x")` receiving `5` demonstrates the difference
  (coercion turns it into `"5"`, so the error is `const`, not `type`).
- A root boolean schema (`parameters: false`) throws `TypeError: Invalid value used as weak map key`
  in TypeScript (`validatorCache` is a `WeakMap`). Rust validates it normally. Pi tools always use
  object schemas; the case is excluded from the differential matrix.
- `Value.Convert`'s `FromUnion` selects a coerced candidate with `Check(context, type, candidate)`
  where `type` is the union schema (variable shadowing), not the arm. Rust reproduces the observable
  selection (first candidate that satisfies the union).
- `dependencies` uses `Array.prototype.every`, which short-circuits: only the first missing
  dependency produces an error. Rust reproduces this; `dependentRequired` uses `EveryAll` and reports
  every missing dependency.
- `minContains` and `maxContains` failures are reported with keyword `contains` and the fixed message
  `must contain at least 1 valid item` (their params are not localized). Rust reproduces both.
- `additionalProperties` validates each unmatched value in a throwaway context, so only the summary
  error is emitted; Rust discards the per-key errors the same way.
- `Value.Convert`'s `FromArray` uses `TryArray`, which always succeeds and wraps non-arrays. Rust
  wraps non-array values before converting elements.
