# Settings manager

## Contract

Frozen source: `4f9e6f8a4`, `core/settings-manager.ts` and `core/compaction-threshold.ts`.

- Effective settings overlay global, trusted project, then session overrides. Object values merge one level only; grandchildren replace, arrays/null replace. The source's recursive-merge comment is misleading; executable spread governs.
- Queue scoped immutable write snapshots in source order; flush/reload settle earlier writes. Reread the scope under lock, overwrite only modified root fields or marked immediate children, and preserve unknown root/nested values and independent writes. Recheck project trust when executing a queued task; record each failed task once and continue later tasks. Source automatic task draining is required, not an approved synchronous-write deviation. Native shared mutations use `lock_settings_manager` / `SettingsManagerGuard`: release the mutex before scheduling one owned worker for pending snapshots, without requiring a Tokio runtime. The worker retains the shared manager until drained and resets scheduling on exit, including unwind. Each queued task catches backend unwinds as a scoped settings error and continues later snapshots, without retry or rollback. Worker creation is fallible: failure records an explicit error against the earliest queued scope, clears scheduling, and leaves snapshots queued for a later release/flush; guard destruction must not panic on OS spawn failure. `flush` remains a synchronous barrier under that same mutex; standalone borrowed managers still require explicit flush. Raw `Arc<Mutex<_>>::lock` is not the shared mutation API. Process exit still requires an explicit barrier; detached work cannot outlive the process. Existing queue/websocket/skills/approval/retry migrations remain unchanged.
- Steering defaults to `all`. Retry base/cap default to 30000/300000 ms; each present value must be a safe integer in `[0,2147483647]`, cap at least base. Invalid values produce getter errors, not defaults.
- Compaction percentage is optional, finite, greater than zero and at most 100; present null/invalid values error. Idle compaction defaults true. Both transcript-hide flags default false. Shell paths are returned literally; session directories retain source normalization.
- Agent-profile reads clone configured JSON without whole-profile validation; built-ins apply only when the configured entry is absent or null. Consumers project fields only when accessed. A truthy non-string model fails at model matching with `modelReference.trim is not a function`; unrelated malformed tools must not erase a valid configured context.
- `reminders.deliverySessionId` is an optional UTF-16 string in the merged settings object. The Rust accessor returns it without normalization; absent or null values return none. Global and trusted project objects merge under the ordinary one-level rule, and unknown reminder fields survive unrelated writes. There is no dedicated setter in TypeScript.
- `markdown.mermaid` selects `off`, `final`, or `streaming`; missing or unrecognized values read as `streaming`. The setter writes the global immediate child and preserves other markdown fields; trusted project settings override it.
- Settings presence does not implement retry timing, compaction triggering/scheduling, display suppression, relay startup, or reminder delivery. Those behaviors belong to their separate module contracts.

## Staged exclusions

The old-only cache settings APIs and typed package autoload field have been removed. Legacy JSON keys remain ordinary unknown data. PackageFilter reuses the existing lossless ModelProperties extras/key-order authority; typed edits replace known values in place. No old-only interpretation or parallel property-order carrier remains.

## Proof

See [traceability](../porting/traceability/settings-manager.md). Only isolated disposable settings are used; no live config or provider state.

## Successor error parity (2026-10-25)

- [x] A directory at settings.json reports the Node read error during load and queued save; later scope writes continue.
- [x] Invalid timeout text uses JS String rather than JSON serialization; native regression is recorded in `settings_manager.rs`.

- [x] Package filter get/set round-trips preserve unknown properties and original key order. Typed edits replace known fields in place. Existing reciprocal TS/Rust persistence proof includes this case.
