# Traceability: `packages/agent-core/src/proxy.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/agent/proxy.rs`.
Proof/exclusions: [slice-1 ledger](agent-core-api-repin.md). Status todo; whole-module differential expansion in progress.

| Frozen export/function | Rust | Evidence |
|---|---|---|
| ProxyAssistantMessageEvent | tagged enum; toolcall_end has content_index only | reconstruction test accepts/ignores extra end metadata |
| ProxyStreamOptions | nested stream options + signal/auth/proxy URL + injectable Fetch | exact request model/context/auth/method/URL and whitelist assertions |
| ProxyMessageEventStream | AssistantMessageEventStream | done/error terminal result tests |
| buildProxyRequestOptions | build_proxy_request_options | excludes samplingParams; frozen keys forwarded explicitly |
| streamProxy | stream_proxy, run_proxy, proxy_events, read_proxy_chunk | request/auth, errors, local abort; pending body read cancellation regression |
| processProxyEvent start/text/thinking | process_proxy_event and typed per-block helpers | 16-case frozen direct-module event matrix compares every serialized push/result/request, including text/thinking signatures, all tool stages, invalid delta/end types, error/aborted terminals and four byte-chunk sizes |
| processProxyEvent toolcall start/delta/end | process_toolcall_* with native ToolCall.partial_json | parsed arguments in both terminal event and result; retain start identity, ignore extra end object |
| processProxyEvent done/error | process_done/process_error | toolUse/error/aborted fixtures; usage carried |
| non-2xx error body/status handling | proxy_error_message | error text and empty-error status default tests; `agent_verifier_regressions::proxy_body` covers failing body reads |
| SSE chunk/line processing | drain_lines | frozen differential covers one-byte split UTF-8, initial TextDecoder BOM handling, ECMAScript trimming, ignored trailing unterminated line and exact Node malformed-JSON diagnostics |

- **Approved snapshot deviation (AGENTS.md, 2026-10-22):** immutable per-event partial snapshots replace JS shared-reference mutation.
- **Approved sparse gap (2026-10-22):** frozen proxy.ts:246,280,313 assigns sparse array slots; native non-contiguous indices reject with `Proxy protocol error: non-contiguous contentIndex N (next index: L)`. Native stream test covers all nine text/thinking/tool start/delta/end tags beyond the next slot; no pi-ai content representation change. Approved malformed/out-of-order gap also covers negative, fractional, NaN/Infinity, and integer indices outside JS's array-index range; four named native decoder tests assert explicit protocol errors, rather than scalar-deserializer errors. RED/GREEN `proxy-index-domain-{red,green}.log`.
- Frozen proxy.ts:363 console.warn now emits exact `Unhandled proxy event type: TYPE` plus newline to stderr. Direct-module oracle verifies text; native child process verifies stderr bytes and no stdout warning. RED/GREEN: `proxy-warning-gap-{red,green}.log`; ten proxy tests GREEN.
- Shared ToolCall requires object arguments; arbitrary JSON tool argument runtime shapes remain an explicit typed-input exclusion.
- RED `proxy-full-matrix-red{,2,3}.log` found three independent errors: omitted partialJson on emitted tool starts/deltas, initial TextDecoder BOM not stripped, and serde-vs-Node JSON diagnostics. Fixed at native authority/decoder/parser boundaries. GREEN `proxy-full-matrix-green.log`: 16 differential cases. Unterminated streams with no terminal event and native auth/URL boundaries still await dedicated proof.
- **DISCREPANCY, preserved:** source proxy.ts:321 uses `partialJson += delta`; a delta after toolcall_end coerces the deleted property to `"undefined"` before concatenation. Native carrier preserves that coercion instead of inventing an empty-string fallback.
