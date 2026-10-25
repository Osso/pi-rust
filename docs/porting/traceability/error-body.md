# utils/error-body — common pin

Frozen `packages/ai/src/utils/error-body.ts` at 4f9e6f8a4; native `utils/error_body.rs`. Complete for the current user-observable parity bar. Current proof: 27 targeted tests, including all six actual-source differentials, pass at d196a9f91 (`/tmp/ai-error-body-current.log`). The module did not need a production change in this closure.

| Export / source branch | Native mapping and proof |
|---|---|
| MAX_PROVIDER_ERROR_BODY_CHARS / NormalizedProviderError | Same 4000 cap and optional status/body/native message fields; cap and independent-format fixtures. |
| normalizeProviderError: Error versus JSON value | ProviderErrorInput::Error/Value; baseline normalization differential and eight non-Error JSON values. |
| First numeric statusCode → status → metadata → response | Optional f64 fields in the same order; precedence differential and unit controls for zero and ordinary fractions. |
| First usable body before trim: top string / parsed error object or array / response string or object | Literal predicates match source; 40 source selection cases cover empty candidates, arrays/objects, both string encodings and first-hit suppression. |
| ECMAScript trim, empty body, message substring and 4000-unit cap | Native JsString authority; trim differential plus message-contained/separate-body/cap fixtures. |
| formatProviderError: carries-body or missing fields; optional prefix/status | Same branch order and JS number rendering; 108 direct source formatting cases, including independent optional fields and empty prefix. |
| truncateErrorText | UTF-16 code-unit length/slice and exact suffix; scalar cases and actual split-surrogate/4000-unit normalization differential. |
| safeJsonStringify: JSON values | Existing lossless JS JSON authority; key enumeration, nested binary64 values and exact UTF-16 output differential. |

GAP (2026-10-25 user-observable decision and existing typed limit policy): non-JSON JS hooks/cycles/accessors, exotic SDK field shapes, limits outside the native nonnegative-integer contract and unpaired prefix units beyond its existing scalar carrier are excluded; no new representation or API was added.

Historical fixes remain source-correct: nonempty arrays qualify as objects, trimming uses ECMAScript whitespace, JSON numeric/key order is exact, and truncation preserves split surrogate units. Earlier mutation proofs killed array rejection, Rust-only trim and reversed status precedence. Historical evidence remains in `/tmp/pi-rust-repin-ai-error-body`; current results above replace the old whole-module TODO, not the source contract. [Spec](../../specs/error-body.md).
