# server/types.ts traceability

Source: `packages/server/src/types.ts`. Rust: `crates/pi-server/src/types.rs`.

| TS export | Rust |
|---|---|
| `PiServerOptions` | `PiServerOptions` (listeners, frame/handshake limits, server id, error observer) |
| `MaybePromise` | `BoxFuture` |
| `PromptInput` / `SteerInput` | `PromptInput`; `SteerInput = PromptInput` |
| `CreateSessionOptions` | `CreateSessionOptions` |
| `PiSessionRuntimeEvent` | `PiSessionRuntimeEvent::{Snapshot, Progress(Box<TranscriptProgress>), Error}` |
| `PiSessionRuntime` | `PiSessionRuntime` trait (snapshot, phase, prompt/steer/abort, model/thinking, subscribe, dispose) |
| `PiServerService` | `PiServerService` trait (list sessions/models, create/open session) |
| `SessionRuntime` / `SessionRuntimeEvent` aliases | same traits/enum |

## Proof
All methods are exercised by `tests/sessions.rs` (durable ids, attach/detach, prompt/steer/abort, model/thinking, disposal, restart restore) and `tests/conformance.rs` (errors, progress, terminal runtime errors).

## Notes
- `Progress` boxes its payload to keep the event enum small; observable values are unchanged.
- Subscription returns `RuntimeSubscription`, which unsubscribes on explicit call or drop.
