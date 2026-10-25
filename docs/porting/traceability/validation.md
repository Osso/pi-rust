# utils/validation traceability

Source: complete `packages/ai/src/utils/validation.ts` (324 lines) read at
`4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Spec: [shared validation](../../specs/validation.md).

## Lossless migration proof (2026-10-21)

Rust authority: `crates/pi-ai/src/utils/validation.rs`; helper modules
`validation_regex.rs`, `validation_formats.rs`, `validation_idna.rs`, and
`validation_hash.rs` implement frozen dependent-engine semantics. `validation_regex.rs` now re-exports the [shared native UTF-16 adapter](js-regex.md); parent-library integration proof remains separate from its focused matching corpus.

On production revision `679e73d9`, sandboxed `cargo test -p pi-ai --test validation
--test differential_validation` passes 28 units and six differential tests.
`lossless_utf16_validation_matches_frozen_typescript` has 227 cases:
117 text/schema combinations; 16 raw-key/pattern/ref/enum/hash cases; 31 TypeBox
coercions (including non-Unicode record matching); 63 format cases (valid, invalid, lone surrogate for each of 21 defaults).
A separate 20-case raw-wire differential covers positive/negative overflow numbers
across plain/TypeBox number, integer, string, boolean and null schemas.
`cargo check -p pi-ai --lib` and package formatting pass without warnings.
Evidence and code hashes: `/tmp/pi-rust-repin-lossless-validation/proof-ledger.json`
and `mutants.json`. No downstream build, clippy, broad suite, or independent gate
was run in this slice; this is not whole-module parity.

| Lossless capability | Evidence | Status |
|---|---|---|
| `JsValue` arguments/results, nested UTF-16 strings and distinct keys | Complete result or exact `JsString` error comparisons; source/Rust caller snapshots | passing targeted differential |
| Raw issue paths, required names, property-name messages and received JSON | Lone high/low surrogate fixtures; direct `schema_error_lines` assertion | passing |
| JS regex backreferences/lookbehind and raw UTF-16 patterns | Repeated lone-surrogate captures, `aa`, lookbehind over `x` + lone unit; pair/lone-unit patterns | prior regex gap closed for these constructs |
| Frozen regex flags and length algorithm | Unicode `pattern`/`patternProperties`; non-Unicode conversion/additionalProperties; TypeBox grapheme min/max over pairs/lone units | passing targeted differential |
| All default formats; unknown format succeeds | 21 × valid/invalid/lone-surrogate fixtures | implemented; not exhaustive per-format conformance |
| `uniqueItems` FNV/TextEncoder replacement quirk | Distinct/equal surrogate objects | faithfully preserved source quirk |
| Schema/argument JS numeric-key order | Direct raw-schema unit + differential received JSON | passing |
| Unmatched-surrogate pointer/anchor resolution | URL-USVString temporary conversion; pointer fails, matching anchor succeeds | passing |
| Overflow binary64 arguments | Raw `±1e400` oracle requests; finite type guards, skipped numeric keywords, string coercion | passing 20-case differential |
| Nested TypeBox conversion kinds | `Tool` has one root marker; no builtin Unsafe integer node | approved one-line gap below |

The previous repair removed optional-null deletion and union prechecks; its
50-case null/order matrix still passes. Nullable-array source throw is an
**approved delta**, independently tested via the selected guarded source.
Historical proof artifacts referenced below may be absent on this host.

## Current-pin source branch matrix

| Source symbols / branches | Concrete current-pin evidence | Status |
|---|---|---|
| `validateToolCall`: found, missing | 3 lookup differential cases | passing |
| `validateToolArguments`: clone, convert, coerce, success/error | 50-case null/order matrix; oracle `argumentsAfter` and Rust input snapshots assert caller immutability on success/error | targeted proof |
| `getSchemaTypes`, `matchesJsonType`, primitive coercions | prior matrix plus 36 required/optional null cases | covered by primitive/native and raw-wire matrices; runtime extremes excluded |
| `applySchemaObjectCoercion`: properties/additional properties | prior object/additional/ref fixtures; nested optional-null fixture | targeted proof |
| `applySchemaArrayCoercion`: items and tuple; beyond tuple length | prior array/tuple fixtures | targeted proof |
| `coerceWithUnionSchema`: declared order, first rejected, no match, cloned object candidates | 12 new anyOf/oneOf cases | passing |
| `coerceWithJsonSchema`: allOf/anyOf/oneOf then type | prior composition fixtures; new union ordering fixtures | source order audited; existing composition/union matrix |
| `getValidator`: first compile, cached compile, compile throw | first compile used throughout; nullable-array exception reproduced | stateless native evaluation; cache identity is not user-observable |
| `getSubSchemaValidator`: compile success/failure | successful arm checks exercised | invalid/exotic schema compilation follows the runtime gap policy |
| `formatValidationPath`: required and root/nested paths | existing unit/path/error fixtures | targeted proof |
| `hasTypeBoxMetadata`: absent symbol | current TypeBox builders; actual conversion outcomes in matrix | frozen TypeBox uses hidden string metadata; JS Symbol mutation is outside native schema domain |
| root replacement result / invalid primitive fallback | source root-string probe returns `"1"` for argument `1` | direct core input violates frozen `ToolCall.arguments: Record<string, any>`; standing typed-core exclusion |

## Reproduced wider boundaries (no new approval)

Observed with the same frozen source and TypeBox **1.1.38** installed under that
worktree. Raw observations: `/tmp/pi-rust-repin-validation/boundary-source.jsonl`,
`boundary-request.json`, `boundary-rust.log`, `boundary-rust-probe.rs`.

- GAP (parent classification2026-10-25): synthetic nested Unsafe integer versus Integer hidden-kind distinctions are not carried; tracked tools use Unsafe only through StringEnum, whose behavior matches. No per-node metadata carrier added.
- The previous backreference mismatch (`^(a)\\1$` on `aa`) is closed by the
  lossless ECMAScript engine and covered by a unit plus the 227-case differential.
- Direct core argument `1` and schema `false` probes are outside the standing
  typed-core invalid-JavaScript-input policy, not required API-widening gaps.
  Frozen `types.ts:378` requires `ToolCall.arguments: Record<string, any>`;
  `types.ts:478–481` requires `Tool.parameters` to extend `TSchema`. Observed TS
  outcomes remain `"1"` and `TypeError: Invalid value used as weak map key`.
  This classification does not exclude external persistence decoding, valid nested
  nulls, schema kinds, regular-expression strings, or escaped UTF-16 JSON values.

## Behavioral mutation proof

On `019e13b4`, the lossless differential kills eight independent behavioral
mutants: argument replacement, path replacement, wrong regex flags, raw-unit
length, skipped TypeBox conversion, inverted format check, raw-surrogate hashing,
and rejected backreferences. A ninth mutant at `679e73d9` reinstates the wrong
non-finite type/keyword guards and is killed by the raw-wire differential. Every
mutant was restored; logs and SHA-256 hashes are in
`/tmp/pi-rust-repin-lossless-validation/mutants.json`.


On implementation `6ec5193f`, the new 50-case differential test detects both old
behaviors independently: reinstating optional-null stripping fails 16 cases;
reinstating the union valid-arm precheck fails 8. Each temporary mutant was
restored. Logs and exact source hashes are in
`/tmp/pi-rust-repin-validation/ledger.md`.

## Representation decisions

- `Tool.parameters` stays a plain `serde_json::Value` (JSON Schema). TypeBox origin is recorded on
  `Tool.typebox_kind`; `Tool::typebox` sets it, `Tool::new` leaves it false. TypeScript records the
  same fact with a hidden `~kind` string property, which a `serde_json::Value` cannot carry.
  `typebox_kind` gates only the `Value.Convert` pass. Nested custom origin follows the explicit gap above.
- `validate_arguments(&JsValue, &JsValue, bool, &str)` is the single validation
  authority. Tool entrypoints return `Result<JsValue, ValidationError>`.
  `ValidationError::message()` returns exact `&JsString`; scalar `Display` stays
  unchanged and unmatched-surrogate display uses JSON escapes.
  `schema_error_lines(&JsValue, &JsValue)` returns `Vec<JsString>`.
- Coercion is modeled with an in-place/replacement enum (`Conversion::{InPlace, Replaced}`) to mirror
  JavaScript's reference semantics: object/record conversion mutates the argument map, primitive and
  union conversion replaces the value, and `validateToolArguments` discards the root replacement.
- Validation is a hand-written JSON-Schema engine (no crate reproduces TypeBox's messages, ordering,
  instance paths, or `maxErrors`). Numbers in messages and in `JSON.stringify(…, null, 2)` use
  `pi_js_compat::number::js_number_to_string`; argument and converted numbers use binary64 in `JsValue`; shared lossless JSON
  formatting provides JavaScript number spelling.
- `Issues` carries a limit: the root context stops at TypeBox's default `maxErrors = 8`; scratch
  contexts used by `anyOf`/`oneOf`/`if`/`properties`/`additionalProperties` are unlimited and truncate
  only when merged into the root, matching `ErrorContext` vs `AccumulatedErrorContext`.

## Symbols → proof

| TypeScript | Rust | Test |
|---|---|---|
| `validateToolCall` | `validate_tool_call` | `validate_tool_call_reports_unknown_tool_names`, `validate_tool_call_validates_matching_tools`, differential `tool_call_lookup_matches_the_pinned_typescript_module` |
| `validateToolArguments` | `validate_arguments`, `validate_tool_arguments` | 28 units; existing matrices plus 227 lossless cases |
| no normalization in current source | removed old `normalize_optional_nulls` | new 50-case null/order matrix; `optional_nulls_are_converted_without_deleting_properties` |
| `Value.Convert` (`FromType` dispatch) | `convert` + `convert_*` + `try_*` | `typebox-*` differential cases, including `typebox-bigint-like-strings` |
| `CheckSchema` inside keywords | `check` (standalone sub-schema) / `check_with` (caller schema stack) | `plain-contains-ref`, `plain-not-ref`, `plain-any-of-ref`, `plain-additional-properties-ref` |
| `coerceWithJsonSchema` | `coerce_into`, `coerce_union`, `coerce_primitive_by_type` | `plain-*` differential cases, `plain_schemas_use_ajv_compatible_primitive_coercions` |
| `getValidator` cache | none (stateless engine) | n/a — no observable behavior |
| `formatValidationPath` | `Issue::path` | `required_paths_include_the_first_missing_property`, `plain-path-formatting` |
| error message + `JSON.stringify(args, null, 2)` | `JsString` assembly, shared `stringify_lossless_pretty` | `error_messages_list_every_failing_keyword_in_engine_order`, `received_arguments_are_pretty_printed_with_javascript_number_formatting`, `plain-number-formatting*`, `plain-string-escaping` |

## Engine coverage

Checks implemented in TypeBox's evaluation order (type; object keywords; array keywords; string
keywords; number keywords; ref/const/enum/if/not/allOf/anyOf/oneOf):

- `type` (string or string array; unknown type names pass, as in TypeBox), `properties`, `required`,
  `additionalProperties` (boolean and schema), `patternProperties`, `propertyNames`, `dependencies`,
  `dependentRequired`, `dependentSchemas`, `minProperties`, `maxProperties`.
- `items` (sized and unsized), `prefixItems`, `additionalItems`, `contains`, `minContains`,
  `maxContains`, `minItems`, `maxItems`, `uniqueItems`.
- `minLength`/`maxLength` using TypeBox's grapheme-cluster counting (UTF-16 units, ZWJ, regional
  indicators, variation selectors), `pattern` (ECMAScript `regress` with source flags), all default `format` checks.
- `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf` (TypeBox tolerance).
- `$ref` (JSON Pointer, `#anchor`/`$anchor`/`$dynamicAnchor`, `$id` exact match, `$defs` cyclic
  contexts), boolean schemas, `const`, `enum`, `if`/`then`/`else`, `not`, `allOf`, `anyOf`, `oneOf`.
- `maxErrors = 8`, exhaustive error accumulation inside union arms, child-error suppression for
  `additionalProperties`/`propertyNames`/`contains`, and `required` path suffixing.

## Gaps

- Dynamic registration/clearing of TypeBox format callbacks is not represented;
  the frozen default registry is implemented. Unknown default-registry keys pass.
- `$recursiveRef`, `$dynamicRef`, `unevaluatedItems`, `unevaluatedProperties`, `~refine`: not used by
  any Pi tool schema. `~refine` carries JavaScript functions, which cannot be represented in JSON.
- Invalid pattern-schema compilation exceptions are not modeled as a separate
  error phase: invalid ECMAScript patterns become non-matches. Backreferences and
  lookbehind are supported; broader RegExp conformance is not claimed.
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

## Approved nullable-array delta 6d2363e6

Native validator already returns null successfully. `differential_validation::nullable_array_matches_approved_delta_6d2363e6` explicitly selects the guarded delta and compares null, empty array, string array and invalid object members. The ordinary frozen matrix asserts exactly its TypeError (`every`) and native successful `{a:"1",b:1,c:null}` result; all other cases remain strict parity. No Rust production change.

## Whole-module common-pin closure (2026-10-25)

Audited all324source lines: both exports, clone/convert/coerce/check order, union candidate cloning and order, optional-null preservation, tuple boundaries, object/additional-property traversal, required/root/nested path messages and exact original-argument error rendering. Current source differential6+native28tests GREEN (`/tmp/ai3-validation-suite_core_{1,2}.log`). Frozen and native real search_current_session_history schema match six concrete string Integer, decimal, null, bounds and error inputs (`/tmp/ai3-validation-real-tool.log`); actual tool is loaded rather than reconstructed. No reproduced real-tool mixed-kind failure. Tracked-source audit: Unsafe only in StringEnum; Integer tool fields occur in search-current-session-history. Approved nullable-array delta remains explicitly selected; frozen discrepancy stays asserted, not normalized away. Strict unchanged-source builder fixture typecheck PASS (`/tmp/ai3-fixtures-typecheck.log`). Existing full-value error/input immutability comparisons remain unchanged.

The historical open questions above do not require new carriers for unobserved/exotic schemas. Default format checks, valid schema/argument JSON and real-tool coercion remain required; unsupported custom JS refinements, invalid/exotic compilation shapes and synthetic kind-mixing are recorded gaps, not claimed capabilities.
