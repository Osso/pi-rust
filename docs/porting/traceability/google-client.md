# google-client traceability

Private stand-in for the parts of `@google/genai` and `google-auth-library` that the Google adapters observe. Not a TypeScript module in `packages/ai`; it exists because the Rust port cannot depend on the Node SDKs.

Source of truth (read-only): `node_modules/@google/genai` (`ApiClient`, `Models.generateContentStream`, generated `*ToMldev`/`*ToVertex` transformers, SSE `processStreamResponse`, `throwErrorIfNotOK`, `NodeAuth`) and `node_modules/google-auth-library` (`GoogleAuth`, `JWT`, `UserRefreshClient`, `gtoken`).
Rust: `crates/pi-ai/src/api/google_client.rs`.
Tests: `crates/pi-ai/tests/differential_google_api.rs` (66 scenarios), `crates/pi-ai/tests/google_api.rs`.

## Reason-bearing SDK fetch boundary

The pinned SDK's includeExtraHttpOptionsToRequestInit creates a child controller for a supplied caller and retains an abort listener that deliberately discards custom reasons. `google_request_signal` mirrors this before async header resolution; retained child signals still receive default AbortError after completion. Send/body catches consume CallbackError::thrown_value via the explicit Error.message-or-JSON projection, preserving native UTF-16 and raw values without inferring brands from name/message fields.

Actual frozen Generative provider27-case matrix in `differential_provider_reasons`: `google-mistral-reasons-red-2.log` → `google-mistral-reasons-expanded.log` GREEN. Covers absent caller, preabort/no fetch, live/default/custom reasons, retained caller signal, typed/renamed Errors and raw send/body rejection. Asserts final stopReason/errorMessage and retained signal reason. Vertex's shared implementation compiles but new reason matrix does not independently establish Vertex credential-fetch timing or auth failure semantics.

## Q4 SDK identity boundary

Frozen `@google/genai/dist/node/index.mjs:13327–13332,21331` sends `google-genai-sdk/1.52.0 gl-node/${process.version}` in User-Agent and x-goog-api-client. Native now retains the stable SDK and language labels, using `unknown` for unavailable host Node version. The wrapper masks only the gl-node version component at those two captured request-header paths, and only when neither model nor caller configured that header. A concrete normalizer regression preserves altered SDK versions, language tags and explicit headers. No oracle-side header waiver exists. Both actual-source Google targets advanced from missing language-label RED to source-absent rawStopReason RED (`google-runtime-identity-{red-2,green-1}.log`). Removed that source-absent output field and its derived error string; frozen Google emits `An unknown error occurred` for mapped error reasons. Both targets then advanced to missing maxOutputTokens65535 cap (`google-stop-fields-green-1.log`); subsequent [shared request repairs](google-shared.md) restore the66-scenario whole-provider matrix. `google-retry-wrapper-green-1.log`: both source matrices plus supplied-ID/runtime-header regressions GREEN. Seven/eight behavioral tests passed; the remaining old retry assertion was replaced as described below. The whole Google module bodies and current remaining SDK gaps are now recorded in [shared](google-shared.md), [Studio](google-generative-ai.md) and [Vertex](google-vertex.md); this remains distinct from workspace acceptance.

## Frozen caller retry boundary

Frozen Generative90/Vertex105 await SDK generateContentStream directly; their providers do not wrap the call in retryGoogleRequest or consume maxRetries. Exact source scenarios Generative35/Vertex119 sent one failed request while native sent two. Removed the native provider wrapper; request error/event/result compares now pass. The old `vertex_retries_reuse_the_cached_access_token` test asserted the superseded wrapper, so it now asserts one authenticated500 API request and the exact source error without provider retries. GoogleRetryOptions/retry_google_request and their old-only tests are now deleted after the [whole-source absence audit](google-shared.md#q4-old-only-retry-api-removal). This preserves the observed frozen caller contract rather than silently adding retry behavior.

## Behavior → proof
| SDK behavior | Rust | Test |
|---|---|---|
| `getRequestUrlInternal` (trailing slash, optional version segment) | `GoogleClient::request_url` | differential URLs, all scenarios |
| `constructUrl` + `shouldPrependVertexProjectPath` | `request_url` parsed URL/project/location prefix and source alt=sse setter | differential ids100-108 plus full projects/... resource and query-bearing base URL regressions |
| `tModel` (Gemini `models/`, Vertex `publishers/...`) | `transform_model_name` | differential ids 100-107; model ids are plain in every scenario |
| `generateContentParametersTo{Mldev,Vertex}` + config/part/tool transformers | `request_body`, `transform_*` | differential request bodies |
| `getDefaultHeaders` + `patchHttpOptions` + `Headers.append` semantics | `HeaderList`, `GoogleClient::request_headers` | differential request headers |
| `NodeAuth.addKeyHeader` / `addGoogleAuthHeaders` | `request_headers` auth branch | differential headers |
| `processStreamResponse` SSE split (`\n\n`, `\r\r`, `\r\n\r\n`), `data:` prefix, per-chunk error JSON, incomplete tail | `GoogleStream::next_chunk`, `take_sse_event`, `check_chunk_error` | differential ids 39-43 |
| `TextDecoder('utf-8', {stream:true})` | `Utf8Decoder` | differential id 43 (multibyte split) |
| `throwErrorIfNotOK` (JSON vs text body, `ApiError.status`) | `read_http_error` | differential ids 33, 34 |
| `JSON.stringify` wire formatting (integral numbers) | `js_json` | differential request bodies (e.g. `maxOutputTokens: 100`) |
| `GoogleToken` JWT bearer exchange | `token_request_body`, `sign_service_account_jwt` | differential ids 103, 105-108, 120; `service_account_tokens_are_signed_rs256_with_the_google_claims` |
| `OAuth2Client.refreshTokenNoCache` | `token_request_body` authorized-user branch | differential id 104; `authorized_user_tokens_use_the_refresh_token_grant` |
| token caching / expiry | `GoogleClient::fetch_access_token` | `vertex_retries_reuse_the_cached_access_token` |
| `GoogleAuth` ADC file resolution and deferred I/O | captured keyFilename; request-time `load_credentials`, `well_known_adc_path`, `parse_credentials` | differential ids103-108; onPayload rejection wins over missing credential file; isolated HOME discovery loads authorized-user ADC and proves token + API requests |

## Gaps
- GCE metadata credentials, external/impersonated credential types, and universe-domain token URLs.
- gaxios retry config, proxy support, timeouts, and multipart/upload paths (not used by the adapters).
- SDK global process-env endpoint overrides and HOME-based ADC discovery have an isolated native/source proof; malformed credential/OS fault messages are not generalized from successful token-grant proofs.

## Discrepancies
- Query-bearing base URLs retain the pinned SDK's literal concatenation: the appended model path becomes part of the existing query before URLSearchParams writes alt=sse. The native port preserves that source output, even when it is unsuitable for the intended server route.

## Direct Simple acquisition boundary (2026-10-25)

`differential_simple_acquisition` invokes actual frozen modules and native ProviderStreams for missing/empty API keys across six APIs. All twelve cases were genuine RED: native returned a stream instead of rejecting acquisition. Direct stream_simple now returns Result and adapters propagate it; lazy acquisition continues through its existing catch. Mechanical consumer changes preserve valid-stream assertions. The remaining module audit is separate from this bounded proof.

## Native callback catch audit (2026-10-25)

`differential_api_catches` compares full actual-source/native error events and final messages for eight APIs × four rejection values: unmatched-UTF16 string, raw Error-lookalike object, and genuine Errors with status/body or statusCode/body. All callbacks reject before HTTP (source records zero requests). Genuine REDs were empty native messages for raw rejections and discarded branded status/body fields. Anthropic keeps its literal Error.message/JSON catch; Mistral probes only statusCode with its own prefix/body truncation; Google/OpenAI/Azure/images adapt the original branded fields to the existing shared normalizer. No utility algorithm or dependency changed. Logs: `/tmp/ai-api-catches-{red,red-2,images-red,green}.log`.

## Typed-native payload signal boundary

| Status | Boundary |
|---|---|
| gap | Frozen Studio/Vertex put the runtime AbortSignal object in params.config.abortSignal before onPayload. Native OnPayload is JSON-only; cancellation goes through request.signal. Parent classified this under the existing typed-native callback/owned-value approval on2026-10-25. No new JSON carrier or runtime-signal projection is added. |

## Current whole-module renewal

The shared SDK path now has separate Studio and Vertex module counters; an isolated native process is compared against four sequential actual-source calls and must produce suffixes1,1,2,2. Missing/empty/duplicate IDs generate; missing function name retains the literal undefined prefix. ADC keyFilename is captured before onPayload but credential file I/O occurs only in request header authentication, matching the source ordering. Default/header/body/SSE behavior remains covered by the original66 actual-source scenarios. New direct helper/conversion, replay/thinking/config/URL/auth/counter cases and34 native shared/API behaviors are recorded in the [lane ledger](../ai-api-repin-proof.md). The two old signed-empty retention tests were replaced with source-proven drop semantics and a complete following-tool-call assertion, not weakened protocol assertions.
