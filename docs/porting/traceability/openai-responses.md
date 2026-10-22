# openai-responses traceability

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

Compiler checkpoint `pi-ai-slice-final.log` at `0c71c960` reached seven errors, including two owned sites fixed by `7780fac1`; remaining auth-owner errors block library acceptance. `lossless_sdk_boundaries` exercises public-provider header rejection and URL conversion; authored, not yet executed at this checkpoint.
