# Provider error-body normalization

Source: `packages/ai/src/utils/error-body.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

## Contract

- Normalize non-Error throws using `safeJsonStringify`; no status/body and `messageCarriesBody = false`.
- For Error inputs select the first numeric status: `statusCode`, `status`, `$metadata.httpStatusCode`, `$response.statusCode`. Zero and nonfinite numbers are numeric hits.
- Select the first body candidate before trimming: string `body`, nonempty object/array `error`, string or nonempty object/array `$response.body`. Empty arrays/objects and other JSON primitives do not qualify as objects. Top-level non-string `body` does not qualify. A selected empty/whitespace string suppresses later candidates.
- Trim ECMAScript whitespace; absent/empty body remains absent. Cap at 4000 UTF-16 units. A missing body or message containing the selected, trimmed, truncated body sets `messageCarriesBody`.
- Format message when carries-body is true or status/body is absent; add `prefix (status): ` only when both prefix and status exist. Otherwise format `status: body` or `prefix (status): body`. Empty prefix is present. Use JavaScript number formatting.
- Stringify JSON with JavaScript property order and binary64 number semantics, including nested values.
- `truncateErrorText` uses UTF-16 length and `slice(0, maxChars)`, appending `... [truncated N chars]` only when length exceeds the limit. `safeJsonStringify` returns `String(value)` when JSON.stringify returns undefined or throws.

## Integrated bounded status

Representable fixes in `5b3b7962` are integrated by `b85d0179`. Whole-module status remains TODO. [Traceability](../porting/traceability/error-body.md) links revision-scoped development evidence and the completed bounded independent verification; this docs audit makes no fresh gate claim.

## Existing API boundary — module remains TODO

Rust retains `ProviderErrorInput::{Error, Value}`, `ErrorBody::{Text, Json}`, `Option<f64>` statuses and `usize` limits. Q18 uses native `JsValue`/`JsString` for arbitrary JSON and text.

**Q18 closed:** native `JsString` retains the exact split surrogate for `truncateErrorText("a🙈b", 2)` and normalization at 4000 units for `"a".repeat(3999) + "🙈b"`. Differential proof compares numeric UTF-16 units; no replacement-character projection is used.

Source `unknown` inputs legitimately include undefined, symbols, functions, bigint, cyclic objects, toJSON behavior and runtime Error field objects/getters. `JsValue` supplies JSON values and all UTF-16 strings, but not arbitrary JavaScript runtime objects or their behavior. They are not invalid source input types, so the standing typed-input policy alone does not waive these source branches. Their shared public representation remains OPEN; this slice neither invents SDK class exclusions nor broadens the API. JSON-backed field shapes are tested by literal source predicates, not SDK brand/class assumptions.

`maxChars` is a source number: negatives/fractions/NaN/infinities follow the literal comparison, slice coercion and subtraction. Existing `usize` API follows the standing August 12 typed nonnegative-integer truncation-limit policy ([historical compatibility policy](compatibility.md)); that policy does not approve lone-surrogate loss. Nonnumeric runtime status candidates cannot be stored in the existing numeric options; source skips them by `typeof === "number"`. Such fields on an otherwise valid Error are valid source inputs, not automatically excluded invalid arguments. Native optional numeric fields prove numeric hit order, not wrong-type runtime object parity.

[Traceability and evidence](../porting/traceability/error-body.md).
