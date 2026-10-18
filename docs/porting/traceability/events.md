# Harness events traceability

Pinned source: `packages/agent/src/harness/events.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-agent-core/src/harness/events.rs`. Behavioral port of `packages/agent/test/harness/events.test.ts`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `events.ts:1-15` | `RunStartEvent`/`RunEndEvent` tagged union; outcome is `completed \| aborted \| failed`; `HarnessEventType`/`HarnessEventOfType` derive from `type` | `HarnessEvent` internally tagged `type` with `run_start`/`run_end`; `RunEndOutcome` closed enum; `event_type()` returns the literal. `tests/harness_events.rs` asserts payload JSON and literals; differential event scripts compare emitted JSON. |
| `events.ts:16` | Listener receives one event; return value may be a promise and is not awaited | `HarnessEventListener = Arc<dyn Fn(&HarnessEvent) + Send + Sync>`; `emit` invokes synchronously and ignores results. Differential reentrant script covers nested emit. |
| `events.ts:24-27` | `on(type, listener)` registers only future events and returns an unsubscribe function | `HarnessEventBus::on` returns `EventSubscription`; `unsubscribe` removes the exact listener id. Differential scripts `on`/`off`. |
| `events.ts:34-46` | One listener set per type; unsubscribe removes the wrapper and drops empty sets | Listener vectors keyed by `HarnessEventType` with id removal. Focused test unsubscribes and re-emits. |
| `events.ts:49-56` | `emit` delivers to type listeners only, then to every watcher; async results are not awaited | Direct listeners first, then watch states; no lock is held while invoking callbacks. |
| `events.ts:58-84` | `watch` registers before capturing the snapshot, buffers until `start`, flushes reentrantly while still buffering, then delivers live events; `unsubscribe` drops buffered events | `watch` registers the state before calling the capture closure; `start` loops over buffered events while the listener remains unset; `unsubscribe` clears the buffer and listener. Focused snapshot-gap test and differential `emitDuringSnapshot` scripts. |
| `events.ts:86-88` | `WatchHandle.snapshot` is the value captured at registration | `WatchHandle::snapshot()`; focused and differential snapshot comparisons. |

## Approved Rust boundary

- Listener results are discarded because Rust callbacks are synchronous; the source `void listener(event)` has the same observable effect.
- `WatchHandle::start` takes `&mut self`; the source handle is a mutable object. Repeated `start` calls are accepted like the source.
- Dropping `EventSubscription`/`WatchHandle` does not unsubscribe; callers invoke `unsubscribe` explicitly, matching the source function contract.

## Proof

- Focused: `tests/harness_events.rs` (4 tests).
- Differential: `oracle/harness-core-oracle.ts` operation `event_bus`, fixtures `14000..=14005` in `tests/differential_harness_core/events.rs`.
- Gaps: none.
