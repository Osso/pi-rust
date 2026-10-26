# Codex Responses: common-pin module traceability

Verified scope: all1677locked lines of `packages/ai/src/api/openai-codex-responses.ts` and the4-line lazy wrapper at `4f9e6f8a4`. Rust: `crates/pi-ai/src/api/openai_codex_responses.rs`. [Contract](../../specs/openai-codex-responses.md). This replaces historical partial maps and stale normalization/deviation claims; historical RED/GREEN commands remain in `/tmp/pi-rust-repin-delta-ai/q15-proof-ledger.md`.

## Reference and proof boundary

The Codex oracle **exclusively** selects approved delta `45794062c21480bff5520ec808584aa3090a4016`, directly parented by the common pin, from `/home/osso/.worktrees/pi-delta-codex-ws-account`. [Commit/module hash](../delta-lock.json). Frozen bytes remain unchanged. The only delta is session+String(accountId) cache identity and corresponding per-account removal/session-wide cleanup; other logic is common-pin code. The unchanged lazy wrapper in that checkout imports this same selected module. Owned-event snapshots are installed on the selected tree's actual event-stream dependency.

Runtime proofs use injected HTTP and real loopback WebSockets, never live providers. Complete requests, events, results, debug records and relevant close frames compare. Payload-spread tests additionally compare exact raw WS frame JSON. Only message clocks are zeroed; each diagnostic timestamp must lie within its invocation's captured before/after bounds. V8 stack text is omitted under the approved stack gap; every other diagnostic field compares exactly. Host/port, platform user-agent and random WebSocket handshake key use explicit placeholders. No responseId/endTurn/scratch/JSON-error masking remains.

## Exports

| Source export | Rust | Proof |
|---|---|---|
| `OpenAICodexResponsesOptions` | same-named options plus `from_stream_options` | request/header matrix; full SDK service-tier matrix |
| `OpenAICodexWebSocketDebugStats` | same-named record | account/idle/age/busy/input matrices; exact lastWebSocketError spelling |
| `getOpenAICodexWebSocketDebugStats` | `get_openai_codex_websocket_debug_stats` | complete debug records after real requests/failures |
| `resetOpenAICodexWebSocketDebugStats` | `reset_openai_codex_websocket_debug_stats` | `close_and_reset_debug_stats_are_public`; each differential cleans only its session |
| `closeOpenAICodexWebSocketSessions` | `close_openai_codex_websocket_sessions` | all-account debug_close frames, busy-entry cleanup and idle/age close reasons |
| `stream` | `stream`, concrete provider adapter | native37-test target and differential target |
| `streamSimple` | `stream_simple -> Result<Stream, CallbackError>` | direct missing/empty-key rejection and valid request/effort differential |
| lazy `openAICodexResponsesApi` | `openai_codex_responses_api` | actual selected lazy export versus native lazy adapter, including setup-error events |

## Source branches and helper coverage

Line references below name the unmodified common pin; the account delta shifts later line numbers without otherwise changing these operations.

| Source lines / branch | Native mapping | Concrete proof |
|---|---|---|
| 88–115 options and request domain | typed options, `ServiceTier::Scale/Null`, lossless body | `codex_all_sdk_service_tiers_preserve_null_body_and_pricing`:48cases across six SDK values, two models, four returned tiers; null remains explicit in body |
| 124–167 retry classification, Number/Date headers,429-only cap | retry helpers, shared Number/Date authorities | `sse_retry_transcript_matches_source_caps_catch_order_and_callback_rejection`; native exponential/date/empty-header/cap tests |
| 169–186 sleep/timeout validation | reason-bearing signals, shared Node timer normalization | header timeout invalid-u32/identity/lifetime tests; three2147483648ms overflow deadlines |
| 235–277 key/account/body/callback/header ordering | `execute_stream` | JWT34cases; header27cases; raw session4cases; callback-before-header failures |
| 281–338 WS retry/abort/diagnostic dispatch | `execute_websocket` | connection-limit reconnect once, no SSE fallback, missing-context no retry; bounded idle/connect/close diagnostics |
| 342–453 SSE compression, retries/catches/response/body/abort | SSE attempt/timeout/event source | complete retry transcript, raw callback24cases, pending-header/body tests, abort midstream and post-completion signals |
| 459–490 direct simple setup and reasoning clamp | Result direct export; lazy setup catch | `direct_stream_simple_missing_key_is_a_synchronous_rejection`; `simple_lazy_setup_catches_missing_keys_and_both_exports_stream_valid_requests`; native xhigh/minimal clamp |
| 495–546 body/tools/reasoning | `build_request_body` and shared Responses conversion | fixed auto/null strict-tool fixtures; old hints ignored; Team call/item IDs; effort/verbosity/temperature/session requests |
| 552–595 service tier pricing | pricing/resolver helpers | full48-case matrix includes gpt-5.5 priority2.5x, flex0.5x and default-response paid-request resolution |
| 599–611 URL resolution | URL helpers/shared URL authority | full SSE request URL and real WS endpoint transcripts |
| 617–740 event/API/protocol errors, status normalization, request IDs | `map_codex_event`, native error carrier | HTTP raw21case and SSE event15case matrices; WS32error/ID/status cases; terminal status21cases |
| 752–840 streaming SSE decoding/framing/read/abort cleanup | `SseEventSource`, shared `Utf8StreamDecoder`/`js_trim` | six BOM/whitespace/data-line cases with one-byte native chunks; malformed JSON, pending body and delivered-delta abort |
| 900–950 stats/get/reset/close/register cleanup | cache/stats stores and session-resource hook | all-account lifecycle plus real debug_close frames; source-absent lastWebsocketError casing corrected |
| 1025–1044 idle/max-age lifecycle | per-entry expiry and acquire decision | ten-call/two-account staggered idle expiry; busy55min age test; no private cache mutation |
| 1047–1123 connect open/error/close/abort/deadline | `connect_websocket` | real handshake/connect deadline and configured overflow; no fallback |
| 1124–1208 acquire reusable/busy/expired/fresh; release keep/discard | lease/cache lifecycle | `websocket_concurrent_busy_call_is_one_shot_and_does_not_replace_the_cached_entry`; account and idle/age matrices |
| 1210–1253 error/close extraction | shared native-error carrier, diagnostic projection | three actual close frames before/after start: class, numeric code,1009 default reason, phase, counters exact |
| 1255–1283 binary decode/native ping | per-message shared TextDecoder and native ping interval | two real binary frames with/without BOM; `sends_native_websocket_pings_while_waiting` |
| 1285–1401 parse queue/terminal/error/close/idle/abort | `WsEventSource` | full WS scenarios; null JSON stays in protocol catch; start only after a mapped event; bounded timeouts/close diagnostics |
| 1402–1456 object-rest/JSON equality/input prefix/delta | lossless property spread and cached-input helpers | reordered body/input JSON4cases; input100type combinations; body-rest4cross-type/null cases |
| 1457–1563 start/process/save/clear/release continuation | process/save/release helpers | real successful/missing-context continuations, literal setup-throw busy retention, concurrent one-shot, per-account reuse/eviction |
| 1566–1595 HTTP friendly/raw error and request ID precedence | lossless error parser/shared ToString/ToNumber | complete HTTP request-ID/raw JSON coercion matrix, including empty/falsy/plan/reset values |
| 1598–1609 JWT atob/JSON/truthy account | lossless claim until header coercion |34actual-source JWT/account cases, Latin-1 atob semantics, empty/truthy/nonstring values and ByteString errors |
| 1612–1678 UUID and Headers construction | UUIDv4, validated header list/shared property ordering | real no-session UUID version/header match; numeric-key validation order; model/additional append/set/delete/trim/Set-Cookie projection |

## Thinking-map carrier dependency (2026-10-24)

The registry's lossless ThinkingLevelMap migration changes only `reasoning_effort_value`'s owned result to JsString and its direct JSON insertion. Frozen536-537 still uses the same nullish default; arbitrary selected units are not projected through UTF8. No map value is used as a header or ID, and transport/cache/status semantics are unchanged. Carrier/consumer acceptance remains part of the registry slice, not a reopened Codex port.

## JsValue domain bound

Parent2026-10-24 ruling: onPayload returns JsValue. Port only operations frozen performs directly on its returned body and body.input: spread/rest, nullish default, iterable spread, length/coercion, slice and JSON equality. Missing/undefined, null, string, number, boolean, plain object, array-like object and array are covered. Nested input items frozen never inspects pass opaquely to serialization; no recursive validation surface is added.

`cached_payload_input_operations_match_each_json_type_class` compares100real two-call combinations. Additional representatives distinguish astral code-point iteration from UTF16 length/slice and retain string-valued array-like length. `cached_payload_body_rest_preserves_null_throws_and_primitive_object_equality` covers four null/cross-type own-property cases. Twelve SSE/WS payload-replacement cases include array/string spread and exact raw frame bytes. Native debug lastInputItems, lastDeltaInputItems and lastPreviousResponseId retain actual assigned JsValues. Native assertions for delta1/resp_1 only changed construction syntax, not expected values.

## Discrepancies preserved literally

| Source behavior | Native outcome and proof |
|---|---|
| Terminal completed/done/incomplete with failed/cancelled status is cast into done/reason:error, without errorMessage | existing `DoneReason::Error`; old-only success guard/unknown-error synthesis removed;21terminal/status cases plus WS matrix. No source branch produces done/aborted. |
| Setup throws before the WS send/release try/finally | preserve TypeError branding, partial stats and busy cached socket until explicit cleanup; cold null.store and cached input rest/spread/slice failures compare exactly. No invented recovery. |
| previous_response_not_found is not retried | one connection/two frames, API error/no diagnostic/no extra start; retired native recovery contract replaced with literal failure |
| No SSE fallback in auto mode | transport failure diagnostic and error are preserved; real idle/connect/close proofs |

## Approved representation/runtime boundaries

| Status | Boundary | Governing policy / result |
|---|---|---|
| approved | immutable per-event partial snapshots; owned returned stats |2026-10-22 owned-event/object-identity decisions; snapshot actual selected source at push, compare every field |
| approved | opaque IDs/header names/type tags | scalar-only with explicit lone-unit rejection; human errors/text and arbitrary JSON remain lossless JsString/JsValue |
| gap | V8 stack frames and source paths |2026-10-22 stack omission decision; name/message/code/details remain exact |
| gap | Node TimeoutOverflowWarning stderr PID/stack hint | parent2026-10-24 ruling under2026-10-23 runtime-oddity policy; actual1ms overflow control flow is exact, warning is not emitted or suppressed |
| runtime domain | browser/old-runtime OS/zstd absence; Bun proxy constructor and JS-specific Blob/view/custom-global constructor capabilities | native Linux has OS/zstd and a concrete WS implementation, not JS globals/Bun/browser execution; native binary UTF8 behavior is directly proven. Source import-capability fallbacks have no native execution path. |

Direct streamSimple's former error-stream deviation is **removed**, not retained as a gap. The selected lazy adapter alone performs the required setup-error conversion. Source conversion errors no longer silently become empty continuation items; only frozen function_call_output is filtered. Unknown source-absent tool-choice/cacheRetention/recovery behavior is not retained.

## Named old-only contracts retired

| Retired native contract | Surviving frozen contract/proof |
|---|---|
| `fails_when_retry_delay_exceeds_limit` | `retry_after_limits_cap_only_rate_limit_delays` and complete retry transcript |
| `forwards_tool_choice_with_frozen_null_strict_tools` | fixed auto choice, surviving null strict fields and ignored-hint differential |
| `clamps_session_headers_and_omits_cache_affinity_for_none` | raw session headers/retained affinity; only body cache key clamps64code points |
| `websocket_one_shot_when_cache_retention_is_none` | cached connection reuse despite ignored retention hint |
| `websocket_recovers_missing_cached_continuation` | reports missing context without source-absent retry |

## Reproducible proof and logs

Runtime commands: `cargo test -j4 -p pi-ai --test openai_codex_responses` and `cargo test -j4 -p pi-ai --test differential_openai_codex_responses`. The latter includes the selected lazy module, real account/idle/age/concurrency lifetimes and all bounded input classes. Strict TS checking uses `buildSourceTsconfig` with the frozen compiler plus the exact selected delta and oracle; Biome checks the changed oracle. Cargo proofs run through the mandatory isolated runner.

Logs and exact revisions: `/tmp/pi-rust-repin-delta-ai/q15-proof-ledger.md`, latestQ15e sections. Named RED/GREEN artifacts include `q15e-codex-{simple-throw,ws-close,ws-binary,ws-null-event,input-types,service-tier,sse-framing,timer-overflow}*`, `q15e-codex-cache-{busy-green,age-1}.log`, and the account delta/lifecycle logs. Final checkpoint commands/results are appended to that ledger; historical counts are not substituted for current integration evidence.

## Retained UTF-8 boundary (2026-10-26)

Native `/proc/sys/kernel/osrelease` reading remains strict. It substitutes for frozen `os.release()` (`openai-codex-responses.ts:1636`), not a TS user-file reader; kernel ASCII output, outside authored-file decoding scope. See the [complete file-read audit](../utf8-decoding-sweep.md).
