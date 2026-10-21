# Shared JSON UTF-16 strings

`crates/pi-js-compat/src/js_json.rs` supplies additive JSON parsing/stringification retaining JavaScript UTF-16 string and property-key identity. Legacy parse callers retain their explicit replacement semantics; [Q18 message fields](lossless-content.md) use the lossless authority. Implementation/proof map: [js_json traceability](../porting/traceability/js-json.md).

## What it must do

- [x] Retain lone high/low surrogate units separately from U+FFFD in values and keys, including nested objects/arrays.
- [x] Treat raw Unicode and equivalent escaped surrogate pairs as equal strings/keys; overwrite actual duplicate keys without moving their first insertion slot.
- [x] Enumerate canonical indices `0..=4294967294` numerically before other keys in insertion order; reject leading-zero, signed, decimal, and `4294967295` keys as indices.
- [x] Stringify lone surrogates as lowercase `\uXXXX`, valid pairs as raw Unicode, and controls with JavaScript escapes; support compact and two-space pretty output.
- [x] Supply UTF-16 construction/access, code-unit equality/hash/order, valid Rust-string construction, checked UTF-8 conversion, and explicitly lossy UTF-8 conversion.
- [x] Preserve existing null/bool/number/array/object support and V8 diagnostic text/UTF-16 positions.
- [x] Retain JavaScript binary64 numbers, including negative zero and overflow infinities, until stringify or explicit serde conversion maps non-finite values to null.
- [x] Format numeric JSON/coercion using [shared shortest-decimal semantics](js-number-format.md), distinct from generic serde numeric storage.
- [x] Provide JSON-value Boolean and fallible String coercions without losing UTF-16 units; objects shadowing `toString` throw rather than silently stringify.
- [x] Keep legacy serde parse behavior, including replacement before duplicate-key collision handling. Explicit later lossy conversion need not produce the same collisions as legacy parsing.

## JS-visible serde boundaries

Use the shared `stringify`/`stringify_pretty` writer whenever serde-backed arbitrary JSON is embedded in text, persisted, measured for a storage/context cap, or used as a deduplication key. Typed records first convert through `serde_json::to_value`, retaining their declared field order and serialization errors. Caller property insertion order for typed session messages is an [approved semantic-JSON exception](compatibility.md#out-of-scope), not a byte-order guarantee. Plain-object enumeration uses `object_entries`, including shared-budget traversal in [remote compaction](../porting/traceability/openai-remote-compact.md#numeric-key-budget-traversal); JS Map registries retain insertion order.

### Decision: egress, not ingress (2026-10-20)

Ingress canonicalization is insufficient: `serde_json::Number` cannot represent the complete JS decimal spelling contract (`2.0`, `-0`, exponent thresholds, and binary64 rounding above 2^53). Even a recursively reordered ingress value would diverge after Rust construction/insertion or when serde writes a float. Replacing all ingress values with `JsValue` would instead change typed API/storage contracts and lone-surrogate collision behavior. Reuse the existing writer at the observable boundary, without altering stored values or parsing errors.

Evidence: the serialization audit's `{"2":2.0,"1":1.0,"x":3.0}` changes tool-details caps, approval/tree text, Pyrun text, remote-compaction byte/character counts and argument strings, embedded HTML and persisted session entries. `{"2":"second","1":"first"}` changes plain-object traversal. Existing `js_json_lossless` and `differential_js_json_lossless` prove UTF-16 identity and binary64 behavior; session codec numeric validation remains independently covered by `differential_session_codec_numbers`. Neither parser nor codec representation is changed by this migration.

The audit has six uncertain groups (despite the brief's count of five): scanning's nested custom data, search-history context assistant arguments, detached bridge params/results, remote dedup numeric leaves, SDK arbitrary settings, and numeric provider IDs. All are reachable and confirmed. Exact cases, RED/GREEN evidence and remaining pattern classifications: [boundary proof](../porting/js-value-semantics-proof.md).

## How it works

- [Shared parser/writer and test mapping](../porting/traceability/js-json.md)

## Implementation inventory

- `crates/pi-js-compat/src/js_json.rs` — shared parser/writer, UTF-16 values, explicit serde conversions, and legacy entrypoints.

## Tests asserting this spec

- `crates/pi-js-compat/tests/js_json_lossless.rs` — string identity/order/hash, conversion boundaries, legacy collision regression; includes the existing six focused serde tests.
- `crates/pi-js-compat/tests/differential_js_json_lossless.rs` — real Node output/key-unit comparison without normalization; includes the existing scalar/error mutation corpus.
- `oracle/js-json-lossless-oracle.ts` — Node built-ins with verbatim stringification/error output and `Object.keys` units.
- `crates/pi-js-compat/tests/image_generator_coercion.rs` and `oracle/image-generator-coercion-oracle.ts` — real Node truthiness/String conversion, including raw overflow and own-method shadowing.

## Known gaps (current cycle)

- [ ] Independently verify integrated model-data consumption (`f2ec514`), including the original-UTF-16 timestamp adapter at `3e6cb09`. Integration is complete; validator parity and Batch A acceptance remain pending.

## Out of scope

- Migrating every serde caller globally. This slice covers the audited visible boundaries; legacy replacement behavior remains intentional compatibility at that API boundary, not a waiver for raw-file validator inputs.
- Arbitrary-precision numbers: `JsValue::Number(f64)` deliberately uses JavaScript binary64, not arbitrary-precision JSON arithmetic.
- Error excerpts containing split surrogate units retain existing UTF-8 replacement behavior; the legacy mutation oracle normalizes only such diagnostic strings. The new bounded lossless oracle does not normalize outputs.

## Shared serde (Q18, 2026-10-21)

`JsString` and `JsValue` implement lossless JSON serde using raw JSON fragments and the shared parser/writer. JSON text/byte deserializers preserve lone-surrogate escapes and object keys; non-JSON formats and serde buffered untagged enums are unsupported for arbitrary UTF-16. Existing scalar-only `serde_json::Value` inputs convert exactly, without replacement; lossless unions select from the raw AST before decoding their typed records. Optional arbitrary properties use `deserialize_present` with `default`, retaining missing versus explicit null. No conversion through `Value` occurs.

`pi_js_compat::js_json!` constructs the lossless AST directly; expression leaves serialize through shared lossless serde, never `serde_json::Value`. Object/array/literal syntax follows the serde_json macro.
