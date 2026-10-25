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

## Native boundary and current proof

Rust retains `ProviderErrorInput::{Error, Value}`, `ErrorBody::{Text, Json}`, `Option<f64>` statuses and the existing typed nonnegative-integer truncation limits. Text and JSON use the current lossless `JsString`/`JsValue` authorities; split-surrogate truncation stays exact.

The 2026-10-25 user-observable parity decision replaces the previous requirement to add representations for exotic JavaScript objects/hooks and pathological inputs. The single recorded gap is in the [module trace](../porting/traceability/error-body.md); no new carrier or fallback is required. This does not waive field selection or text fidelity for realistic SDK errors and JSON bodies.

Current 27 targeted tests, including six actual-source differentials, cover every exported helper and realistic source branch. [Traceability and evidence](../porting/traceability/error-body.md).
