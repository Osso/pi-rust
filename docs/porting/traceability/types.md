# types traceability

Pinned source: `packages/client/src/types.ts`. Rust: `crates/pi-client/src/types.rs`.

| TypeScript export | Observable contract | Rust mapping and proof |
|---|---|---|
| `ConnectionState` | `"disconnected" \| "connecting" \| "connected"` | `ConnectionState::{Disconnected, Connecting, Connected}` with `as_str`; state-sequence assertion in `connection::rejects_pending_requests_on_close...` |
| `ConnectionStateChange` | State plus optional error | `ConnectionStateChange { state, error: Option<PiError> }`; connection listener tests |
| `Unsubscribe` | Callable that removes a listener | `Box<dyn Fn() + Send + Sync>`; `state::reduces_only_authoritative_snapshots_and_supports_unsubscribe` |
| `ListenerErrorHandler` | Receives listener failures | `Arc<dyn Fn(PiError) + Send + Sync>`; `connection::reports_subscriber_failures_without_changing_connection_state` |
| `PiClientOptions.transportFactory` | Factory used for every connection attempt | `PiClientOptions::transport_factory` |
| `PiClientOptions.maxFrameLength?` | Optional outbound/inbound frame limit | `PiClientOptions::max_frame_length: Option<u32>`; frame-limit tests |
| `PiClientOptions.onListenerError?` | Diagnostics callback | `PiClientOptions::on_listener_error` |
| `CreateSessionOptions` | Optional cwd/name/model/thinkingLevel | `CreateSessionOptions` mapped to `CreateCommandInput` |

## Rust boundary

- `PiClientOptions::{new, with_transport_factory, with_max_frame_length, with_listener_error}` provide builder construction.
- Listener closures are `Arc` values so they can be cloned across state snapshots and transport callbacks.

## Gaps / discrepancies

- `maxFrameLength` is typed `u32`; JavaScript numbers above `2^32-1`, non-integers, and `NaN` are excluded by the typed API. Zero is rejected at construction.
- Optional TypeScript fields map to `Option`.
