# coding-agent event-bus traceability

Pinned source: `packages/coding-agent/src/core/event-bus.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: focused behavioral tests in `crates/pi-coding-agent/tests/event_bus.rs` (delivery, channel isolation, unsubscribe, clear, failure isolation, recursive emit, unsubscribe during delivery). Differential testing is not applicable: handlers are JavaScript closures and `node:events` object identity cannot cross the JSONL oracle boundary.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `EventBus.emit` | Synchronous fan-out of `data` to listeners of one channel | `EventBus::emit` snapshots matching handlers under the lock and delivers without it; `delivers_emitted_data_to_channel_listeners`, `listeners_can_unsubscribe_from_inside_a_delivery` |
| `EventBus.on` | Registers a handler and returns an unsubscribe function | `EventBus::on`/`on_handler` return `Subscription`; `unsubscribe_removes_only_that_listener` |
| `EventBusController.clear` | Removes every listener | `EventBusController::clear`; `clear_drops_every_listener` |
| `safeHandler` | A throwing handler is caught, logged, and does not stop later listeners | `emit` returns `Vec<HandlerError>` and continues; `one_failing_listener_does_not_stop_the_others` |
| `emitter.off` on unsubscribe | Unsubscribing twice is harmless; delivery continues for others | `Subscription::unsubscribe` (idempotent by construction); unsubscribe test |

## Exclusions

- Handlers are `Fn(&Value) -> Result<(), String> + Send + Sync` instead of JavaScript closures; the data type is `serde_json::Value`.
- The source logs handler failures with `console.error`; the Rust bus returns them from `emit` so the owning runtime can log them. Delivery order and failure isolation are unchanged.
- `EventEmitter`'s `newListener`/`removeListener`/`maxListeners` machinery has no caller in this module and is not ported.

## Discrepancies

None.
