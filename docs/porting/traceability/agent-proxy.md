# Traceability: `packages/agent/src/proxy.ts`

Rust: `crates/pi-agent-core/src/agent/proxy.rs`. Tests: `crates/pi-agent-core/tests/proxy.rs`.

| TypeScript export | Rust item | Tests |
|---|---|---|
| `ProxyAssistantMessageEvent` | `agent::ProxyAssistantMessageEvent` | `proxy.rs` (serde via fixtures) |
| `ProxyStreamOptions` | `agent::ProxyStreamOptions` | `preserves_tool_call_metadata_received_only_on_toolcall_end` |
| `streamProxy` | `agent::stream_proxy` | all four proxy tests |

## Branches

| TS branch | Rust handling | Tests |
|---|---|---|
| POST `${proxyUrl}/api/stream` with bearer auth and serialized model/context/options | `HttpRequest` built identically; option keys inserted in TS order | `preserves_tool_call_metadata_received_only_on_toolcall_end` |
| `!response.ok` with JSON `error` | `proxy_error_message` prefers `Proxy error: {error}` | `reports_proxy_error_body_message` |
| `!response.ok` without parseable error | `Proxy error: {status} {statusText}` (requires `HttpResponse::status_text`, added to `pi-ai`) | gap: not separately asserted |
| abort handler / `signal.aborted` checks | `CancellationToken` passed to `Fetch`, re-checked after each chunk; reason `aborted` | `reports_aborted_when_the_local_signal_is_cancelled` |
| text/thinking start/delta/end | typed slot mutation and partial snapshot events | `preserves_tool_call_metadata_received_only_on_toolcall_end` (tool path); delta/end branches exercised by oracle fixture |
| `toolcall_start`/`delta`/`end`, `partialJson` parsing | partial JSON kept in a side map because `ToolCall` has no `partialJson` field; arguments parsed with `parse_streaming_json` | `preserves_tool_call_metadata_received_only_on_toolcall_end` |
| `toolcall_end` for non-toolCall content | no event | gap: not separately asserted |
| `done` / `error` | maps `DoneReason`/`ErrorReason` to `StopReason` and carries usage | toolUse and error fixtures |
| unknown event type | `#[serde(other)] Unknown` → no event (TS logs a warning; Rust has no logger in this crate) | gap: warning not reproduced |
| `partial` object identity across events | Rust snapshots `partial` per event (owned values); JSON shape is identical | differential proxy fixture |

## Exclusions / discrepancies

- The TS `fetch` global becomes an injectable `Fetch` (defaults to `pi_ai::http::default_fetch`). This is a testability adaptation, not a behavior change.
- Sparse `contentIndex` values beyond `content.len()` are rejected with `Sparse proxy content index`; TypeScript would create a sparse array. Proxy servers emit dense indices; the typed Rust `AssistantMessage.content` cannot represent holes. Recorded as an explicit typed exclusion.
- `statusText` comes from the server response; `pi-ai`'s reqwest `Fetch` fills it from the canonical reason phrase because reqwest does not expose the raw phrase.
