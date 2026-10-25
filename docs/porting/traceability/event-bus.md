# coding-agent event-bus traceability

Pinned source: `packages/coding-agent/src/core/event-bus.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

Proof: focused behavioral tests in `crates/pi-coding-agent/tests/event_bus.rs` (delivery, channel isolation, unsubscribe, clear, failure isolation, recursive emit, unsubscribe during delivery). `differential_event_bus_errors` executes actual frozen listeners/catches and compares original Error/code/raw-string/null/lookalike-object values across the boundary; no JS identity claim is needed.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `EventBus.emit` | Synchronous fan-out of `data` to listeners of one channel | `EventBus::emit` snapshots matching handlers under the lock and delivers without it; `delivers_emitted_data_to_channel_listeners`, `listeners_can_unsubscribe_from_inside_a_delivery` |
| `EventBus.on` | Registers a handler and returns an unsubscribe function | `EventBus::on`/`on_handler` return `Subscription`; `unsubscribe_removes_only_that_listener` |
| `EventBusController.clear` | Removes every listener | `EventBusController::clear`; `clear_drops_every_listener` |
| `safeHandler` | A throwing handler is caught, logged, and does not stop later listeners | `emit` logs the original CallbackError, returns native diagnostics with `error`, and continues; retained failure-isolation test plus4actual-source catch histories |
| `emitter.off` on unsubscribe | Unsubscribing twice is harmless; delivery continues for others | `Subscription::unsubscribe` (idempotent by construction); unsubscribe test |

## Exclusions

- Handlers/RequestHandlers use `Result<(), CallbackError>`; payloads/channels/original throws retain native UTF-16. RequestFuture's existing result contract is unchanged.
- The bus itself logs at frozen19-23 using the shared original-throw reporter, retaining Error name/message/properties and JSON values; V8 stack frames remain omitted under the approved gap. Native `HandlerError.error` retains the original failure.4fresh differential cases and9retained tests GREEN; [current integration checkpoint](../ui-notify-proof.md) records compile/lint/type/inventory proofs. [Contract](../../specs/fallible-ui-notifications.md).
- `EventEmitter`'s `newListener`/`removeListener`/`maxListeners` machinery has no caller in this module and is not ported.

## Discrepancies

None.

## Approved native request slot (2026-10-22)

`EventRequest` holds native `JsValue` data, optional `Arc<dyn Any + Send + Sync>` context, and an optional `RequestFuture`. `on_request` joins ordinary listeners in registration order; `emit_request` runs every listener synchronously and reports errors exactly like `emit`. A later assignment overwrites an earlier future, including when the assigning handler subsequently errors. Extension wrappers retain stale-runtime checks and tracked unsubscribe. This implements frozen codex-usage's mutable promise fields without loading TypeScript extensions.

Proof: `requests_preserve_context_unclaimed_and_rejected_futures` and `request_claims_run_every_listener_and_last_assignment_wins` (raw UTF-16 payload/channel/errors, ordinary/request interleaving, claim then error, last claim, unclaimed, context, rejection, unsubscribe).
