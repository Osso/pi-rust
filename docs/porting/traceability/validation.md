# utils/validation traceability

Source: complete `packages/ai/src/utils/validation.ts` (324 lines) read at
`4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Spec: [shared validation](../../specs/validation.md).

## Re-pin status — bounded repair, not whole-module parity

- Repairs through `f86144ba` are integrated by canonical merge `8d9bb557`.
  Independent proof at `8d9bb557` passed 26 units, the 50-case differential,
  9 model-tool tests and 5 viewed-target tests. Validation clippy/fmt passed;
  model clippy stopped on six control-library diagnostics. Exact scope and
  remaining gaps: `/tmp/pi-rust-repin-validation-model-verify/REPORT.md`.
- Removed the old pre-conversion optional-null deletion pass; source has no such pass.
- Removed plain union coercion's already-valid-arm precheck. Source coerces and checks
  cloned candidates in declaration order (`coerceWithUnionSchema`, lines 193–204).
- New differential test `optional_nulls_and_union_order_match_the_frozen_source`
  covers 50 cases: optional/required primitive, enum, nullable, array and object nulls;
  anyOf/oneOf ordering and rejected-candidate isolation; exact set_agent_model-shaped
  arguments. RED observed before production changes (24 new-case mismatches).
- The prior 68-case differential matrix is not current-pin whole-module proof:
  `plain-union-types` now exposes a TS nullable-array compiler exception. Classification
  is pending with the user; 118 schema cases currently record 117 matches and this
  one known failure. No waiver, runtime-input exclusion, or source-bug choice is made here.
- Historical claims below describe the previous implementation, not fresh proof of
  every engine branch. Wider gaps remain unresolved, even if older text says unused.
Rust: `crates/pi-ai/src/utils/validation.rs`.

Differential oracle: `oracle/validation-oracle.ts` (builds TypeBox schemas with the pinned
`Type.*` builders or passes plain JSON, calls the real `validateToolArguments`/`validateToolCall`).
Tests: `crates/pi-ai/tests/validation.rs` (26 behavioral cases), `crates/pi-ai/tests/differential_validation.rs`
(68 prior schema/argument cases, 50 new null/order cases, 3 lookup cases).
Differentials compare complete values or exact error messages; the nullable-array case
is still a known failing comparison.

## Current-pin source branch matrix

| Source symbols / branches | Concrete current-pin evidence | Status |
|---|---|---|
| `validateToolCall`: found, missing | 3 lookup differential cases | passing |
| `validateToolArguments`: clone, convert, coerce, success/error | 50-case null/order matrix; oracle `argumentsAfter` and Rust input snapshots assert caller immutability on success/error | targeted proof |
| `getSchemaTypes`, `matchesJsonType`, primitive coercions | prior matrix plus 36 required/optional null cases | partial: not every string/number boundary |
| `applySchemaObjectCoercion`: properties/additional properties | prior object/additional/ref fixtures; nested optional-null fixture | targeted proof |
| `applySchemaArrayCoercion`: items and tuple; beyond tuple length | prior array/tuple fixtures | targeted proof |
| `coerceWithUnionSchema`: declared order, first rejected, no match, cloned object candidates | 12 new anyOf/oneOf cases | passing |
| `coerceWithJsonSchema`: allOf/anyOf/oneOf then type | prior composition fixtures; new union ordering fixtures | partial: combined-keyword ordering not independently proven |
| `getValidator`: first compile, cached compile, compile throw | first compile used throughout; nullable-array exception reproduced | cache reuse not independently exercised |
| `getSubSchemaValidator`: compile success/failure | successful arm checks exercised | exception branch not independently exercised |
| `formatValidationPath`: required and root/nested paths | existing unit/path/error fixtures | targeted proof |
| `hasTypeBoxMetadata`: absent symbol | current TypeBox builders; actual conversion outcomes in matrix | symbol-present branch not represented by Rust API |
| root replacement result / invalid primitive fallback | source root-string probe returns `"1"` for argument `1` | direct core input violates frozen `ToolCall.arguments: Record<string, any>`; standing typed-core exclusion |

## Reproduced wider boundaries (no new approval)

Observed with the same frozen source and TypeBox **1.1.38** installed under that
worktree. Raw observations: `/tmp/pi-rust-repin-validation/boundary-source.jsonl`,
`boundary-request.json`, `boundary-rust.log`, `boundary-rust-probe.rs`.

- Nested `Type.Unsafe({type:"integer"})` under `Type.Object`, input
  `{value:"42.9"}`: TS integer error; Rust `{value:42}`. Replacing the Unsafe node
  with `Type.Integer()` yields the **same serialized schema**, but TS `{value:42}`.
  Source `Value.Convert` dispatches node-by-node through
  `node_modules/typebox/build/value/convert/from_type.mjs:24–47`;
  source validation invokes it at `validation.ts:294`. Rust `types.rs:744,773`
  stores a JSON schema and only one root `typebox_kind` boolean. A per-node
  conversion-kind tree associated with `TypeBoxSchema` is the narrow proposed API
  change; ownership/authorization is required before implementation. The literal
  hidden-kind evidence and candidate API are recorded in
  `/tmp/pi-rust-repin-validation/API-PACKET.md`; that proposal is not an approved API.
- Pattern `^(a)\\1$`, input `{value:"aa"}`: TS accepts; Rust rejects the pattern.
  Source validation delegates to `Compile` (`validation.ts:246–254`). Regex engine
  work is outside this bounded repair; no unsupported-pattern waiver is assumed.
- Direct core argument `1` and schema `false` probes are outside the standing
  typed-core invalid-JavaScript-input policy, not required API-widening gaps.
  Frozen `types.ts:378` requires `ToolCall.arguments: Record<string, any>`;
  `types.ts:478–481` requires `Tool.parameters` to extend `TSchema`. Observed TS
  outcomes remain `"1"` and `TypeError: Invalid value used as weak map key`.
  This classification does not exclude external persistence decoding, valid nested
  nulls, schema kinds, regular-expression strings, or escaped UTF-16 JSON values.

## Behavioral mutation proof

On implementation `6ec5193f`, the new 50-case differential test detects both old
behaviors independently: reinstating optional-null stripping fails 16 cases;
reinstating the union valid-arm precheck fails 8. Each temporary mutant was
restored. Logs and exact source hashes are in
`/tmp/pi-rust-repin-validation/ledger.md`.

## Representation decisions

- `Tool.parameters` stays a plain `serde_json::Value` (JSON Schema). TypeBox origin is recorded on
  `Tool.typebox_kind`; `Tool::typebox` sets it, `Tool::new` leaves it false. TypeScript records the
  same fact with a hidden `~kind` string property, which a `serde_json::Value` cannot carry.
  `typebox_kind` gates only the `Value.Convert` pass. Nested origin is not represented;
  that is an unresolved parity boundary, not a new exclusion.
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
| no normalization in current source | removed old `normalize_optional_nulls` | new 50-case null/order matrix; `optional_nulls_are_converted_without_deleting_properties` |
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

## Current-pin discrepancy awaiting classification

`plain-union-types` uses `{type:["array","null"],items:{type:"string"}}`
for property `c`, receiving `null`. The actual frozen module throws
`Cannot read properties of null (reading 'every')`; Rust currently accepts it.
The existing differential test remains red rather than silently dropping this case.

## Historical discrepancies (re-validation required)

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
