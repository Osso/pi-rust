# session-handle traceability

Pinned source: `packages/client/src/session-handle.ts`. Rust: `crates/pi-client/src/session_handle.rs` plus lease callbacks in `client.rs`.

| TypeScript export | Observable contract | Rust mapping and proof |
|---|---|---|
| `SessionLeaseMode` | `"shared" \| "exclusive"` | `SessionLeaseMode::{Shared, Exclusive}`; lease-mode test |
| `AcquireSessionOptions` | `{ mode }` | `AcquireSessionOptions { mode }` with `From<SessionLeaseMode>`; tests use both forms |
| `SessionLease` / `PiSessionHandle` | id, active, attached, snapshot, subscribe, onEvent, detach, dispose, prompt, steer, abort, setModel, setThinking | `SessionHandle` with the same operations; `SessionHandleCallbacks` supplies client behavior |
| `active` / `attached` | Both report whether the lease is usable and the session is attached | `SessionHandle::{active, attached}` → callback `is_attached` |
| `snapshot` | Current snapshot only while active | `SessionHandle::snapshot`; state tests |
| `subscribe` / `onEvent` | Throw when detached; unsubscribe functions | `subscribe`/`on_event`; state unsubscribe test |
| `detach` / `dispose` | Release the lease; dispose relinquishes on failure | `detach`/`dispose` → `LeaseCallbacks::release(false/true)`; session tests |
| `[Symbol.asyncDispose]` | Alias for `dispose` | Rust has no async-dispose protocol; `dispose()` is the explicit equivalent |
| `prompt`, `steer`, `abort`, `setModel`, `setThinking` | Send the command and return `result.session` | typed `SnapshotFuture`; prompt/abort/set-thinking tests |
| `SessionHandleCallbacks` | Internal callback contract | `SessionHandleCallbacks` trait implemented by `LeaseCallbacks` |

## Rust boundary

- Command methods return `Result<SnapshotFuture, PiError>` and register/send the request synchronously, matching the source's eager promise construction.
- `ReleaseFuture` wraps the shared release operation; repeated detach calls observe the same operation.

## Gaps / discrepancies

- TypeScript `async` methods convert synchronous assertion failures into rejections. Rust exposes them as immediate `Err` values; callers await only the successful future. Tests accept both at the call boundary.
- `Symbol.asyncDispose` has no Rust equivalent; `dispose()` is used directly.
