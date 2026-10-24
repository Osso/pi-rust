# Empty session cleanup

Port frozen `core/empty-session-cleanup.ts`. [Traceability](../porting/traceability/empty-session-cleanup.md).

## Requirements

- [x] Explicit removal returns for absent/empty control DB or session file, same resolved target, resident IDs/directories or any message entry.
- [x] Otherwise remove control metadata first; unlink a still-existing transcript second. Preserve native failure records and prior side effects.
- [x] Sweep only DB-selected empty main candidates whose file is absent, which are not residents and whose health has no truthy live Pi PID.
- [x] Select the complete abandoned set before removing any rows; return the number selected.
- [x] Runtime teardown calls removal after beforeInvalidate and before dispose; cleanup failure prevents disposal. CLI startup calls sweep before session selection/name validation unless noSupervisor is set.

## Tests

`teardown_removes_a_persisted_empty_session_before_disposal` executes actual frozen teardown/cleanup/SessionManager/SQLite against a real native runtime. Genuine RED: native disposed but left the empty file and metadata, while source removed both. Runtime removal and metadata-first native unlink failure are GREEN, alongside the20-scenario module matrix and28/28 Runtime regression. Full actual CLI process RED→GREEN confirms sweep before name validation; valid standalone startup reaches model validation without sweeping. Scoped workspace/TS/Biome pass and owned clippy/readability findings are recorded in the traceability doc; global clippy debt is not claimed resolved.

## Exclusions

Not general session deletion, archive policy or sweeping all files. Process liveness is the shared runtime-process authority. Runtime oddities follow [project rules](../../AGENTS.md).
