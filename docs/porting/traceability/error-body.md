# utils/error-body traceability — re-pin TODO

Frozen source: `packages/ai/src/utils/error-body.ts` (127 lines), `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Source checkout read-only. Base native revision: `5989c9a20bc6cea40f9adf70559a8e9132d7962f`.

[Contract and OPEN boundaries](../../specs/error-body.md). This module is **not done**: the old scalar-safe truncation waiver is not renewed; shared public UTF-16/value representation requires user decision.

| Export / source branches | Native mapping | Behavioral proof |
|---|---|---|
| `MAX_PROVIDER_ERROR_BODY_CHARS = 4000` | same constant | `truncates_the_body_at_the_cap`; differential 4000/4001/4050 boundaries |
| `NormalizedProviderError` fields | same fields, optional numeric status / String body/message | unit formatting cross-product; normalized differential field comparisons |
| `normalizeProviderError`: non-Error branch | `ProviderErrorInput::Value` | 8 JSON value fixtures, including nested binary64 numbers and indexed object keys; remaining unknown values OPEN below |
| Error branch; first numeric statusCode → status → metadata → response → absent | `ProviderError` optional f64 fields, `extract_status` | `first_numeric_status_wins_including_zero_and_nonfinite`; baseline 17 fixtures plus `body_and_status_selection_match_source` 4 precedence fixtures; invalid source runtime field types not represented |
| `pickBodyText`: top-level string body; nonempty error object; response string/object; none | `pick_body_text`; both enum string encodings select raw text, nonempty JSON arrays and objects qualify | `accepts_nonempty_array_response_bodies`, `body_selection_preserves_first_hit_even_when_empty`, `json_string_bodies_are_raw_text_in_both_sdk_fields`; selection differential 40 body fixtures crosses all JSON kinds and empty candidates |
| `isNonEmptyObject`: object + non-null + Object.keys length > 0 | nonempty JSON object OR array | corrected source direction: TS accepts `["stream","internals"]`; old native rejected it. Empty arrays/objects rejected; primitives ignored |
| `extractBody`: undefined; JS trim; empty; cap | existing `pi_js_compat::string::js_trim`; truncation helper | `trims_only_ecmascript_whitespace`; 11 source trim cases include BOM, NEL, NBSP, Unicode spaces/separators and non-whitespace; empty first candidate never falls through |
| messageCarriesBody: no body / substring yes / substring no | same | baseline message-contained/no-body/separate-body tests; original cap fixture |
| `formatProviderError`: carries OR absent status OR absent body; optional prefix/status pair; separate body with/without prefix | `format_provider_error`, existing JS number formatting | unit independent optional fields/empty-prefix/nonfinite cross-product; 108 direct-source formatting fixtures include absent/empty body and prefix, flags independent of body, zero/fraction/NaN/±Infinity statuses; all normalized differential cases compare prefixed and bare formatting |
| `truncateErrorText`: <= limit; > limit; UTF-16 length and suffix | current public String helper | 9 scalar-representable source differential cases plus unit UTF-16 counting. **OPEN** split-boundary diagnostic compares source code units for explicit limits and actual `normalizeProviderError` at its 4000-unit body cap; proves mismatch, not parity |
| `safeJsonStringify`: JSON.stringify success | existing `pi_js_compat::js_json::stringify` | `stringify_uses_javascript_key_order_and_binary64_numbers`, 8 non-Error source values. Keys reordered numerically then insertion order; nested numbers round to binary64 and render JS notation |
| `safeJsonStringify`: serialized undefined or exception → String(value) | not expressible with JSON Value | OPEN shared public representation; not newly waived by typed-invalid-input policy |

## Evidence and discrepancy classification

Baseline targeted actual-source differential, before edits: source body `["stream","internals"]`, native null, status 400. This is a Rust bug, not a TS bug or source class-instance rule. New RED actual-source tests separately reproduce array precedence, BOM/NEL trim and JSON ordering/number mismatches; unit RED also proves JSON-encoded string bodies are ignored incorrectly. Fixes stay in the owned native module using existing compatibility helpers; no dependencies, provider/type/global API changes or fallback paths.

The old unit assertion `truncate_error_text("a🙈b", 2) == "a... [truncated 2 chars]"` was removed as false parity evidence. Native implementation remains untouched at this boundary pending authorization. UTF-16 source response uses numeric code units so JSON transport cannot erase/reject lone surrogates.

Source runtime unknown values, lone-surrogate input/output strings, toJSON/cycles and SDK field runtime behavior remain bounded public API questions, not invented class/SDK exclusions. The frozen module contains no brand checks except `instanceof Error`; JSON field selection follows the literal predicates. See spec for standing typed-limit policy applicability and numeric source limit behavior. No new exclusion or waiver is approved in this slice.

Development evidence, exact revisions and mutation results are recorded in `/tmp/pi-rust-repin-ai-error-body/{REPORT.md,ledger.md}`; no broad discovery/test run, check/clippy/readability/final gate, delegation, push, deploy or live-state action is part of this slice.
