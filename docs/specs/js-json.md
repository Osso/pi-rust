# Shared JSON UTF-16 strings

`crates/pi-js-compat/src/js_json.rs` supplies additive JSON parsing/stringification retaining JavaScript UTF-16 string and property-key identity. Existing serde-facing callers remain unchanged. Implementation/proof map: [js_json traceability](../porting/traceability/js-json.md).

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

- [ ] Parent must integrate the lossless entrypoints into the model-data validator and independently verify the integrated revision. This prerequisite does not complete validator parity.

## Out of scope

- Migrating existing serde callers; legacy replacement behavior remains intentional compatibility at that API boundary, not a waiver for raw-file validator inputs.
- Arbitrary-precision numbers: `JsValue::Number(f64)` deliberately uses JavaScript binary64, not arbitrary-precision JSON arithmetic.
- Error excerpts containing split surrogate units retain existing UTF-8 replacement behavior; the legacy mutation oracle normalizes only such diagnostic strings. The new bounded lossless oracle does not normalize outputs.
