# openai-codex-responses traceability

## Reason-bearing SSE header deadline

`fetch_with_header_timeout` now uses shared `AbortSignal::timeout` and literal `combine_abort_signals` with explicit cleanup at header completion. Zero/single signals retain frozen absence/identity; successful body completion no longer fabricates a default abort. Actual frozen-module tests reproduce invalid >u32 timeout rejection before fetch (RED→GREEN), four caller/deadline combinations with native post-completion caller reasons, and default TimeoutError DOMException branding/code at a pending header boundary. Proof: `codex-timeout-reasons-{red,green-2,expanded-2}.log` under `/tmp/pi-rust-repin-delta-ai`; five bounded SSE suites GREEN. Request rejection value/brand transport remains partial pending CallbackError carrier work.

## Q4 partial re-port

Removed producer-side old grammar/deferred validation and shared option threading; actual-source old-tool fixture RED (zero native requests vs one source request)→GREEN. Removed source-absent AssistantMessage.endTurn capture/production from SSE and WebSocket event adapters, after exact source SSE event/result RED (`codex-end-turn-red.log`). Native source end_turn response fields are ignored, matching frozen; the coding-agent end-turn tool is unchanged. All6 recorded differential suites pass at75e1336b, including real local-WebSocket cached continuation/timeouts and new old-tool metadata regression. Native37-test target exposed four stale assertions for removed rawStopReason, old cache-write accounting and old strict-tool rewriting; expectations now follow the verified frozen contracts. Partial-event responseId is still normalized by the pre-existing oracle wrapper; no whole-module parity claim. Historical counts below are not current acceptance evidence.

Q9 UUID ownership correction: frozen `openai-codex-responses.ts:1613` generates `crypto.randomUUID()` when no session ID is supplied. Rust now uses the v4 OS-entropy primitive, not agent-core UUIDv7. Real local-WebSocket test `websocket_without_a_session_uses_a_crypto_random_uuid_v4_request_id` reproduced version 7 RED, now version 4 GREEN; verifies matching session/request headers. Logs: `/tmp/pi-rust-repin-delta-agent-core/codex-uuid-{red,green}.log`. [UUID ownership](uuid.md).

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Q18 ingress representation and current bounded proof: [lossless provider ingress](lossless-provider-ingress.md). Earlier suite counts below are historical, not new-pin rerun evidence.

## Q15d whole-module re-port (active, 2026-10-24)

Native37-test baseline passes; current actual-source differential8/10 passes, with only the two WS timeout diagnostic comparisons pending clock-bound/V8-stack classification already approved. Source request-body REDs independently reproduce missing Team tool-call ID preservation (frozen64–70) and source-absent native CodexToolChoice override (frozen options88–94; fixed literal auto in request builder). The typed override field/enum is removed; `forwards_tool_choice_with_frozen_null_strict_tools` is retired as an old-only contract. Its surviving null-strict-tool assertions remain in `frozen_auto_tool_choice_and_null_strict_tools_ignore_the_old_hint`; new actual-source body tests cover unknown/required/none ignored hints and Team followup call/item IDs. No shared option/type or provider-normalization edits. WS timeout proofs now capture each invocation's before/after clocks and assert diagnostic timestamps lie within those bounds; every other message/event/diagnostic field compares exactly, with only the approved V8 error.stack omitted. No timestamp blanket-stripping or other field waiver. [Current contract](../../specs/openai-codex-responses.md).

Frozen164–167/352–435 retry transcript RED reproduces the old delay-limit throw and early HTTP/onResponse exits. Native now caps only429 header delays, preserves other statuses' requested delay, and keeps fetch/onResponse/body/parser/HTTP retry errors inside the same attempt catch. `OnRetry` now returns `Result<(), CallbackError>` (parent-approved shared signature); callback rejections follow the same catch/outer boundaries, with mechanical successful consumers in pi-ai/coding-agent and only the approved agent-core test closure. `fails_when_retry_delay_exceeds_limit` is retired as source-absent; `retry_after_limits_cap_only_rate_limit_delays` keeps the surviving delay contract. Header-timeout RED also proves that frozen awaits a fetch ignoring abort; the native independent race is removed, while the old native pending-header fixture now honors its supplied signal like the actual-source fixture. No timeout assertion is dropped.

HTTP error ingress matrix produced21actual-source RED differences: missing header/body request IDs, raw message/coercion defaults, falsy error values, reset coercion and human plan-name lone units. Native HTTP parser now retains JsValue until the exact frozen coercion/new-Error boundary, uses shared ToNumber and UTF16 lowercase/trim, and intentionally catches internal JSON/coercion failures while preserving an already-extracted request ID. No opaque-scalar projection is applied to human plan/error text. Literal error-code matching uses the shared ASCII-only fold matching JS non-Unicode /i.

SSE error-event matrix independently reproduced14differences over15cases: HTTP/body request-ID precedence, empty message/code truthiness, response.failed arbitrary JSON message conversion, human lone units and JSON null's first type access. Native map now retains the source error/default/catch behavior and threads only the actual HTTP request ID into the event mapper; payloads are not mutated to carry that context.

Actual-source two-call cached WebSocket transcript RED proves `previous_response_not_found` is not retried by this pin. The native special-code predicate/retry flag/branch are deleted. The old `websocket_recovers_missing_cached_continuation` recovery contract is retired; `websocket_reports_missing_cached_continuation_without_an_old_only_retry` asserts the source API error, no transport diagnostic, no start event, one connection/two frames and exact debug counters. The retained continuation-success transcript is unchanged; fixture cleanup targets only its own session.

Frozen1409–1414 cache equality uses JSON.stringify, not structural object equality. Independent real two-call WebSocket REDs reproduce both a reordered replacement body and a reordered input-item object's properties incorrectly retaining a native delta. Native comparisons now use the shared lossless JSON stringifier. The four-case cached transcript compares complete request frames, stream events and results, retaining normal reuse and missing-context rejection.

JWT/account ingress17-case actual-source matrix reproduces11differences before fixes; the expanded34-case matrix additionally checks onPayload-before-header-failure ordering. Frozen extractAccountId parses atob's Latin-1 string, retains any truthy JSON account value, and leaves ToString/ByteString/header normalization failures outside the extraction catch. Native now retains lossless JsValue until after onPayload, uses shared coercion/ByteString validation, and reproduces account-header trim and NUL/CR/LF rejection. Proof compares complete requests, events and final messages; broader model/additional-header construction remains in the whole-module audit.

Historical counts/normalizations below are not current whole-module evidence; the current oracle snapshots at push and compares partial fields without the former responseId/endTurn/scratch masking.

Source: `packages/ai/src/api/openai-codex-responses.ts` and `openai-codex-responses.lazy.ts`.
Rust: `crates/pi-ai/src/api/openai_codex_responses.rs`.
Tests: `crates/pi-ai/tests/openai_codex_responses.rs` (31 behavioral), `crates/pi-ai/tests/differential_openai_codex_responses.rs` (5 differential suites) against the pinned TS module. The WebSocket suites use the shared scripted server in `crates/pi-ai/tests/support/ws_server.rs` for both TS and Rust.

## Exports

| TS | Rust | Proof |
|---|---|---|
| `OpenAICodexResponsesOptions` | `OpenAICodexResponsesOptions` + `from_stream_options` | header/body tests; differential request scenarios |
| `OpenAICodexWebSocketDebugStats` | `OpenAICodexWebSocketDebugStats` | WebSocket cache/account tests |
| `getOpenAICodexWebSocketDebugStats` | `get_openai_codex_websocket_debug_stats` | cache tests; `close_and_reset_debug_stats_are_public` |
| `resetOpenAICodexWebSocketDebugStats` | `reset_openai_codex_websocket_debug_stats` | cache tests |
| `closeOpenAICodexWebSocketSessions` | `close_openai_codex_websocket_sessions` | cache tests |
| `stream` | `stream`, `OpenAICodexResponsesApi::stream` | SSE + WebSocket suites; differential |
| `streamSimple` | `stream_simple`, `OpenAICodexResponsesApi::stream_simple` | `stream_simple_preserves_xhigh_and_clamps_minimal` |
| `openAICodexResponsesApi()` | `openai_codex_responses_api` | exercised through `ProviderStreams` construction |
| `resolveCodexUrl` / `resolveCodexWebSocketUrl` (private) | `resolve_codex_url` / `resolve_codex_websocket_url` | differential request URLs; WS server URLs |

## Branches and helpers

| TS branch | Rust | Proof |
|---|---|---|
| `extractAccountId` JWT claim | `extract_account_id` | all authenticated tests |
| `buildSSEHeaders` | `build_sse_headers` | `streams_sse_text_and_codex_headers`, session tests |
| `buildWebSocketHeaders` | `build_websocket_headers` | WS cache/account tests; differential snapshot |
| `buildBaseCodexHeaders` (originator, user-agent, auth, account) | `build_base_codex_headers` | header tests |
| `normalizeTimeoutMs` | `normalize_timeout_ms` | timeout suites |
| `compressRequestBodyZstd` | `compress_request_body_zstd` | `zstd_compresses_sse_request_bodies` |
| `isTerminalRateLimitError` / `isRetryableError` | same-named helpers | retry/backoff and delay-limit tests |
| `getRetryAfterDelayMs` (ms/seconds/`Date.parse`) | `get_retry_after_delay_ms` | retry tests; `honors_iso_8601_retry_after_dates`; differential retry scenario |
| `validateRetryDelayMs` | `validate_retry_delay_ms` | `fails_when_retry_delay_exceeds_limit` |
| SSE fetch retry loop and `onRetry` events | `fetch_sse_with_retries`, `notify_retry` | retry/backoff suites; differential |
| header timeout (`AbortSignal.timeout`) | `fetch_with_header_timeout` | header-timeout unit + differential |
| `onPayload` / `onResponse` | `execute_stream`, `fetch_sse_with_retries` | exercised in request paths |
| `parseErrorResponse` friendly usage-limit message | `parse_error_response` | HTTP-error differential |
| SSE parser (`parseSSE`, `[DONE]`, data-line join) | `SseEventSource` | text/reasoning/tool/usage suites |
| SSE body idle timeout | `SseEventSource::read_chunk` | body-timeout unit + differential |
| SSE abort mid-stream | `SseEventSource` + signal select | abort unit + differential |
| `mapCodexEvents` (`error`, `response.failed`, `response.done` normalization, `end_turn`) | `map_codex_event`, `CodexMappedSource`, `WsEventSource::handle_text` | `response_done_is_normalized_to_completed`; differential |
| `normalizeCodexStatus` | `normalize_codex_response` | differential scenarios |
| `assertSuccessfulOutput` | `assert_successful_output` | error/stop suites |
| `processResponsesStream` options (service tier, pricing) | `codex_stream_options`, `apply_service_tier_pricing` | service-tier pricing test |
| `resolveCodexServiceTier` | `resolve_codex_service_tier` | service-tier pricing test |
| `getServiceTierCostMultiplier` | `get_service_tier_cost_multiplier` | service-tier pricing test |
| `processWebSocketStream` | `process_websocket_stream` | WS suites; differential |
| `connectWebSocket` (open/error/close/abort/timeout) | `connect_websocket` | connect-timeout suite; differential |
| `acquireWebSocket` / release / session cache | `acquire_websocket`, `release_websocket`, `take_cached_connection` | cache, account, one-shot suites |
| idle cache expiry (5 min) and connection age limit (55 min) | `schedule_session_websocket_expiry`, `take_cached_connection` | cache lifetime code; age path not clock-tested |
| `buildCachedWebSocketRequestBody` / `getCachedWebSocketInputDelta` | same-named helpers | cached-continuation and missing-continuation suites; differential |
| WebSocket continuation state (`lastRequestBody`, `lastResponseId`, `lastResponseItems`) | `CachedWebSocketContinuationState` | cached-continuation suite |
| `parseWebSocket` (queue, completion, close errors, idle timeout, abort) | `WsEventSource` | WS suites; differential idle |
| native WebSocket ping interval | `WsEventSource::wait` ping branch | `sends_native_websocket_pings_while_waiting` |
| WebSocket connection-limit retry | `execute_websocket` | connection-limit suite |
| previous-response-not-found retry | `execute_websocket` | missing-continuation suite |
| transport failure diagnostic + debug stats | `execute_websocket`, `record_websocket_failure` | connect-timeout and idle suites |
| `requestBodyWithoutInput` / `requestBodiesMatchExceptInput` | same-named helpers | cached-continuation suite |
| `convertResponsesMessages` response items for continuation | `convert_response_items` | cached-continuation suite |
| session resource cleanup registration | `register_websocket_cleanup` | cleanup tests |
| `openai-codex-responses.lazy.ts` | `openai_codex_responses_api` | compiled/used by `ProviderStreams` tests |

## Differential normalization

- Timestamps are zeroed.
- `user-agent`, WebSocket `host`, and `sec-websocket-key` are replaced with placeholders (platform/port/random).
- `Invalid Codex SSE/WebSocket JSON: ...` messages collapse to `<invalid json>` because JS `JSON.parse` and `serde_json` word the syntax error differently.
- Non-`done` event partials drop `responseId` and `endTurn`, and streaming scratch fields (`partialJson`, `customInput`) are removed. The TS stream yields live references to the mutable output object, so a later `response.created`/terminal event retroactively appears on earlier partial snapshots; Rust event partials are clones at push time and keep scratch state in typed slots. This is a representation difference, not an observed-message difference.

## Discrepancies and intentional deviations

- **No SSE fallback in `auto` mode.** The task brief mentioned "WS connect timeout → SSE fallback", but the pinned TS module never falls back: a WebSocket transport failure is recorded as `provider_transport_failure` and thrown. The Rust port preserves that behavior and the TS test `surfaces websocket connect timeout without falling back to SSE`.
- `streamSimple` returns an error stream when the API key is missing; TS throws synchronously. `ProviderStreams` cannot throw, and `lazyStream` already maps setup failures to an error event.
- WebSocket availability: TS returns `"WebSocket transport is not available in this runtime"` when `globalThis.WebSocket` is absent. `tokio-tungstenite` is always available in Rust, so that branch has no runtime equivalent. The Bun HTTP-proxy WebSocket constructor branch is likewise not ported (no Bun runtime).
- zstd compression: TS falls back to an uncompressed body when `node:zlib`/zstd is unavailable (browser/Vite). Rust always compresses.
- `User-Agent`: Linux reads `/proc/sys/kernel/osrelease`; other platforms use `std::env::consts::OS` in place of `node:os.release()`. Arch maps `x86_64`→`x64`, `aarch64`→`arm64`, `x86`→`ia32`.
- `close_openai_codex_websocket_sessions` returns synchronously like TS and sends the close frame (`1000`, `"debug_close"`) on a spawned task; called outside a Tokio runtime it can only drop the sockets (`closing_sessions_sends_debug_close_frame_to_cached_sockets`). An empty session id closes every session, matching the source's falsy check.
- Session cleanup registration happens on first WebSocket use, not at module load; the observable cleanup set is the same.
- `retry-after-ms`/`retry-after` numbers use `pi_js_compat::number::parse_number` (`Number(string)`), so `retry-after-ms: ""` retries after 0 ms as in TS (`empty_retry_after_ms_header_retries_immediately`). Dates use `pi_js_compat::date::date_parse` (`Date.parse`): HTTP dates and ECMAScript ISO 8601 are supported, and zone-less values use the process-local zone like V8; V8 legacy formats and HTTP dates before 1970 are not (`honors_iso_8601_retry_after_dates`).
- JWT payloads are decoded with `pi_js_compat::base64::atob` (`atob` semantics: padding optional, standard alphabet); URL-safe payloads fail in both implementations (`extracts_account_id_from_unpadded_jwt_payload`).
- Event `partial` messages are clones (see normalization above).
- `HttpResponse::status_text` supplies the TS `response.statusText` fallback for empty error bodies.

## Q18 provider request integration (2026-10-22)

Codex token splitting and shared atob operate on native units. Request endpoint normalization and session/request ID headers remain native until SSE HttpRequest or WebSocket connect; URL/ByteString failures become Codex transport errors. Decoded account IDs retain the explicitly approved scalar metadata restriction.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.
