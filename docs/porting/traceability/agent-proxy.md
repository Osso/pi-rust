# Traceability: `packages/agent-core/src/proxy.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/agent/proxy.rs`.
Proof/exclusions: [slice-1 ledger](agent-core-api-repin.md). Status todo; no whole-module differential or object-identity parity claimed.

| Frozen export/function | Rust | Evidence |
|---|---|---|
| ProxyAssistantMessageEvent | tagged enum; toolcall_end has content_index only | reconstruction test accepts/ignores extra end metadata |
| ProxyStreamOptions | nested stream options + signal/auth/proxy URL + injectable Fetch | exact request model/context/auth/method/URL and whitelist assertions |
| ProxyMessageEventStream | AssistantMessageEventStream | done/error terminal result tests |
| buildProxyRequestOptions | build_proxy_request_options | excludes samplingParams; frozen keys forwarded explicitly |
| streamProxy | stream_proxy, run_proxy, proxy_events, read_proxy_chunk | request/auth, errors, local abort; pending body read cancellation regression |
| processProxyEvent start/text/thinking | process_proxy_event and typed per-block helpers | implementation compared; text invalid-content error tested; full thinking/text event matrix not independently differential-tested |
| processProxyEvent toolcall start/delta/end | process_toolcall_* with side-map partial JSON | parsed arguments in both terminal event and result; retain start identity, ignore extra end object |
| processProxyEvent done/error | process_done/process_error | toolUse/error/aborted fixtures; usage carried |
| non-2xx error body/status handling | proxy_error_message | error text and empty-error status default tests |
| SSE chunk/line processing | drain_lines | existing reconstruction fixtures; malformed/unterminated/chunk-boundary exhaustive matrix remains unproven |

Owned partial snapshots differ from TS shared object identity; sparse indices are rejected rather than materialized as holes. Unknown event warnings are not reproduced. Shared ToolCall requires object arguments; arbitrary JSON tool argument runtime shapes are excluded. These remain explicit gaps pending classification, not new approved deviations.
