# Settings manager

## Contract

Frozen source: `4f9e6f8a4`, `core/settings-manager.ts` and `core/compaction-threshold.ts`.

- Effective settings overlay global, trusted project, then session overrides. Object values merge one level only; grandchildren replace, arrays/null replace. The source's recursive-merge comment is misleading; executable spread governs.
- Queue scoped immutable write snapshots in source order; flush/reload settle earlier writes. Reread the scope under lock, overwrite only modified root fields or marked immediate children, and preserve unknown root/nested values and independent writes. Recheck project trust when executing a queued task; record each failed task once and continue later tasks. Source automatic task draining is required, not an approved synchronous-write deviation. Existing queue/websocket/skills/approval/retry migrations remain unchanged.
- Steering defaults to `all`. Retry base/cap default to 30000/300000 ms; each present value must be a safe integer in `[0,2147483647]`, cap at least base. Invalid values produce getter errors, not defaults.
- Compaction percentage is optional, finite, greater than zero and at most 100; present null/invalid values error. Idle compaction defaults true. Both transcript-hide flags default false. Shell paths are returned literally; session directories retain source normalization.
- Settings presence does not implement retry timing, compaction triggering/scheduling, display suppression, or relay startup. Those consumer slices remain pending.

## Staged exclusions

Main's Phase1 sequencing retains existing old-only UI settings APIs until coupled Phase3 consumer deletion. They are not frozen-source parity. Full-module completion remains open while those APIs/typed fields remain. Package autoload interpretation belongs to the resource/package slice; unknown bytes must survive unrelated settings writes.

## Proof

See [traceability](../porting/traceability/settings-manager.md). Only isolated disposable settings are used; no live config or provider state.
