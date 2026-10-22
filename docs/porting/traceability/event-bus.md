# coding-agent event-bus traceability

Pinned source: `packages/coding-agent/src/core/event-bus.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

Proof: focused behavioral tests in `crates/pi-coding-agent/tests/event_bus.rs` (delivery, channel isolation, unsubscribe, clear, failure isolation, recursive emit, unsubscribe during delivery). Differential testing is not applicable: handlers are JavaScript closures and `node:events` object identity cannot cross the JSONL oracle boundary.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `EventBus.emit` | Synchronous fan-out of `data` to listeners of one channel | `EventBus::emit` snapshots matching handlers under the lock and delivers without it; `delivers_emitted_data_to_channel_listeners`, `listeners_can_unsubscribe_from_inside_a_delivery` |
| `EventBus.on` | Registers a handler and returns an unsubscribe function | `EventBus::on`/`on_handler` return `Subscription`; `unsubscribe_removes_only_that_listener` |
| `EventBusController.clear` | Removes every listener | `EventBusController::clear`; `clear_drops_every_listener` |
| `safeHandler` | A throwing handler is caught, logged, and does not stop later listeners | `emit` returns `Vec<HandlerError>` and continues; `one_failing_listener_does_not_stop_the_others` |
| `emitter.off` on unsubscribe | Unsubscribing twice is harmless; delivery continues for others | `Subscription::unsubscribe` (idempotent by construction); unsubscribe test |

## Exclusions

- Handlers are `Fn(&JsValue) -> Result<(), JsString> + Send + Sync` instead of JavaScript closures; payloads, channels and errors retain native UTF-16.
- The source logs handler failures with `console.error`; the Rust bus returns them from `emit` so the owning runtime can log them. Delivery order and failure isolation are unchanged.
- `EventEmitter`'s `newListener`/`removeListener`/`maxListeners` machinery has no caller in this module and is not ported.

## Discrepancies

None.

## Approved native request slot (2026-10-22)

`EventRequest` holds native `JsValue` data, optional `Arc<dyn Any + Send + Sync>` context, and an optional `RequestFuture`. `on_request` joins ordinary listeners in registration order; `emit_request` runs every listener synchronously and reports errors exactly like `emit`. A later assignment overwrites an earlier future, including when the assigning handler subsequently errors. Extension wrappers retain stale-runtime checks and tracked unsubscribe. This implements frozen codex-usage's mutable promise fields without loading TypeScript extensions.

Proof: `requests_preserve_context_unclaimed_and_rejected_futures` and `request_claims_run_every_listener_and_last_assignment_wins` (raw UTF-16 payload/channel/errors, ordinary/request interleaving, claim then error, last claim, unclaimed, context, rejection, unsubscribe).
