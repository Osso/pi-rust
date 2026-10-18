# client traceability

Pinned source: `packages/client/src/client.ts`. Rust: `crates/pi-client/src/client.rs`.

| TypeScript symbol / branch | Observable contract | Rust mapping and proof |
|---|---|---|
| `PiClient` construction | Creates connection and state; validates frame limit | `PiClient::new`; frame-limit test |
| `PiClient.connect(options)` static | Connect then dispose on failure | `PiClient::connect_new` (name differs because Rust cannot overload an inherent static and method named `connect`); `disposal::connects_through_its_ownership_factory` |
| `disposed`, `connectionState`, `connected`, `snapshot` | Read-only views | same-named Rust methods; disposal/connection tests |
| `connect` | Reject when disposed; reset state when disconnected; delegate | `PiClient::connect`; reconnect tests |
| `reconnect` | Alias for connect | `reconnect`; reconnect tests |
| `disconnect(reason?)` | Default `"Client disconnected"` or explicit error | `disconnect`, `disconnect_with_reason`; session invalidation test |
| `subscribe` / `onEvent` | Reject when disposed | `subscribe`/`on_event` with `assert_not_disposed`; state tests |
| `onConnectionStateChange` | Register/unregister state listener | `on_connection_state_change`; sequence test |
| `listSessions` | Send `list`, return sessions | `list_sessions`; request tests |
| `createSession(options)` | Send `create`, reserve exclusive lease, build handle | `create_session`; branch mapped (no source test) |
| `attachSession` | Shared acquisition | `attach_session`; session tests |
| `acquireSession` | Reserve mode, serialize behind detachment/reconciliation, attach once | `acquire_session`, `begin_attach`, `complete_attach`; shared/exclusive and serialized reacquisition tests |
| attach deduplication | Concurrent acquires share one attach request | `AttachmentGate`; implemented, no source test for two concurrent acquires |
| attach failure | Restore forgotten snapshot and release lease | `complete_attach` error branches; lower-revision test exercises restore on success path |
| `request` | Reject disposed/disconnected; encode frame; register correlation; send | `start_request`; request tests |
| response with no request | Fail connection with `Response has no matching request` | `handle_message`; branch mapped |
| response command mismatch | Reject request and fail connection | `handle_message`; `rejects_mismatched_response...` |
| typed server error | Reject with `PiServerError` | `handle_message` error branch; typed request-error test |
| successful response | Apply result then resolve | `handle_message`; delayed snapshot test |
| lease reservation conflicts | `PiSessionOwnershipError` with exact messages | `reserve_session_lease`; lease-mode test |
| lease release counting | Detach only on final lease | `release_session_lease`, `LeaseCallbacks::release`; shared-detach test |
| release failure | Restore active lease, keep cleanup flag on dispose failure | `LeaseCallbacks::release` catch branches; releasing-failure test |
| release while releasing | Commands fail with detached; second detach reuses operation | `LeaseState`, shared `SharedOperation`; releasing-failure test |
| session removal | Invalidate leases before applying event | `handle_message` event branch; branch mapped |
| disconnect state change | Clear attachments, invalidate leases, reject pending, notify listeners | `handle_connection_state_change`; reconnect/pending tests |
| `dispose` | Idempotent; reject pending, disconnect, dispose state, invalidate leases, clear listeners | `dispose`; disposal tests |
| listener failures | Reported without corrupting protocol/transport | `report_listener_error`; subscriber-failure tests |

## Differential coverage

`differential_client::differential_scripted_client_session` compares:
- every outbound frame (hex) for hello, list, attach, prompt, detach;
- connection-state sequence `connecting, connected, disconnected`;
- observed session revision notifications, prompt result revision, post-prompt snapshot, post-detach state.

## Gaps / discrepancies

- Static `PiClient.connect` is `PiClient::connect_new`; Rust forbids a static and instance method with the same name.
- Source requests that fail frame encoding reject a promise; Rust returns `Err` from the command method before any frame is sent. The source test accepts either boundary.
- Source `Promise` resolution is a microtask; Rust resolves futures on the next poll. Scripted differential output is identical.
- `createSession` has no upstream test; its request/lease branches are mapped but not behaviorally exercised.
