# Orchestrator index

Port of frozen `packages/orchestrator/src/index.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] all export-star modules: native index.rs reexports config/handler/IPC/process/serve/storage/supervisor/types; root imports exercised by errors and process-boundary fixtures.
- [x] no Radius export-star and no runtime logic: Radius remains a module dependency, not part of native index reexports; no standalone executable behavior to differentiate.

## How it works
- [Module traceability](../porting/traceability/orchestrator-index.md)

## Implementation inventory
- `crates/pi-orchestrator/src/index.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/errors.rs`
- `crates/pi-orchestrator/tests/rpc_process.rs`

## Known gaps (current cycle)
No additional in-slice gap beyond the proof boundary above.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
