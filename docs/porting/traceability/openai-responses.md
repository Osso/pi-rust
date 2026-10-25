# openai-responses traceability

## Q4 common-pin status

Partial. Corrected global-fetch oracle now drives frozen SDK requests. SDK header6.26.0 restored; removed native output floor and explicit-cache field. Shared usage/tool/incomplete-event fixes advance the exact differential to scenario10 affinity-header drift. Restored frozen sendSessionIdHeader (default true), x-client-request-id and options-header precedence; removed xAI include override and incremented retry headers. Current20 recorded scenarios pass at dbcc11de (`responses-retry-green.log`). Shared-module differentials still expose old grammar/custom-tool conversion and namespace drift, so this is not whole-module parity. [Exact audit/proof](../ai-scripted-provider-oracles.md). Historical recorded-fetch parity claims below are superseded. No assertion narrowed; `request.fetch` is an approved Rust-only seam; unset must match frozen default fetch (`AGENTS.md`).

Source: `packages/ai/src/api/openai-responses.ts` and `openai-responses.lazy.ts`.
Rust: `crates/pi-ai/src/api/openai_responses.rs`.
Tests: module unit tests, `crates/pi-ai/tests/differential_openai_responses.rs` (20 recorded-fetch scenarios).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `OPENAI_TOOL_CALL_PROVIDERS` | constant | conversion scenarios |
| `OPENAI_RESPONSES_MIN_OUTPUT_TOKENS` | constant | scenario 0 (maxTokens 5 -> 16) |
| `hasHeader`, `getClientApiKey` | same-named helpers | header/auth scenarios |
| `detectSessionAffinityFormat` | `detect_session_affinity_format` | 0, 10; unit test |
| `resolveCacheRetention` | `resolve_cache_retention` | 8, 9 |
| `getCompat` | `get_compat` | all request scenarios; unit test |
| `getPromptCacheRetention` | `get_prompt_cache_retention` | 8 |
| `formatOpenAIResponsesError` | `format_sdk_error`/`format_request_error` with `"OpenAI API error"` | 12-14 |
| `OpenAIResponsesOptions` | `OpenAIResponsesOptions` | 2, 3, 7 |
| `stream` | `stream` / `OpenAIResponsesApi` | all |
| `streamSimple` | `stream_simple` | 3 |
| `createClient` (headers, copilot, affinity) | `create_client` | 0, 1, 8-10 |
| `buildParams` | `build_params` | all request comparisons |
| developer/system role | `convert_responses_messages` | 0 vs 1 |
| reasoning effort/summary/off | `build_params` | 2, 3 |
| xAI include | `build_params` | 11 |
| tool strict/grammar conversion | shared converter | 4, 5 |
| service-tier pricing | `apply_service_tier_pricing` | 7; unit test |
| lazy wrapper | `open_ai_responses_api` | module compiles; `LazyApi` behavior covered by `api_lazy` tests |

## Differential normalization
- Timestamps (`Date.now()` in TypeScript, `now_millis()` in Rust) become `0`.
- Environment identity headers (`user-agent`, `x-stainless-*`) become `<env>`.
- JSON syntax-error text for malformed streams is the V8 `SyntaxError` message from `pi_js_compat::js_json` and is compared byte-for-byte (the malformed-stream scenario has no normalization).
- Event `partial` is snapshotted at `push` time by the oracle.

## Gaps
- `streamSimple` throws synchronously in TypeScript when the API key is missing; the Rust `ProviderStreams` contract cannot throw, so it returns an immediate error stream (typed-API deviation).
- `onPayload`/`onResponse` callbacks are exercised by Rust behavior but not serialized through the JSON oracle.

## Discrepancies
- None. The TypeScript source accepts arbitrary `service_tier` strings; Rust stores them as strings unchanged.

## Q18 provider request integration (2026-10-22)

Client key selection, default/model/header merges and affinity session IDs stay native. SDK request construction owns URL/header conversion; no affinity-value UTF-8 projection remains.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.

## Direct Simple acquisition boundary (2026-10-25)

`differential_simple_acquisition` invokes actual frozen modules and native ProviderStreams for missing/empty API keys across six APIs. All twelve cases were genuine RED: native returned a stream instead of rejecting acquisition. Direct stream_simple now returns Result and adapters propagate it; lazy acquisition continues through its existing catch. Mechanical consumer changes preserve valid-stream assertions. The remaining module audit is separate from this bounded proof.
