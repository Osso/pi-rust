# Orchestrator storage

Port of frozen `packages/orchestrator/src/storage.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] loadMachine/saveMachine/deleteMachine: absent, present, delete idempotence, corrupt JSON, TS→Rust→TS bytes: foundation/differential.
- [x] loadInstances/saveInstances/getInstance/upsertInstance/removeInstance: empty/default, insert/update order, replace/remove and unknown fields: foundation/differential.
- [x] read/open/mkdir/rm/write failures: errors.rs compares nine real FS failure messages to frozen, including private EACCES fixtures and /dev/full ENOSPC.

## How it works
- [Module traceability](../porting/traceability/orchestrator-storage.md)

## Implementation inventory
- `crates/pi-orchestrator/src/storage.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/foundation.rs`
- `crates/pi-orchestrator/tests/differential.rs`
- `crates/pi-orchestrator/tests/errors.rs`

## Known gaps (current cycle)
No additional in-slice gap beyond the proof boundary above.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
