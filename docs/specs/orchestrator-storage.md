# Orchestrator storage

Port of frozen `packages/orchestrator/src/storage.ts`.

## What it must do
- [x] Preserve tested JSON/path/record contract against frozen source.
- [ ] Complete all source branches and exported-symbol proof.

## How it works
- [Traceability](../porting/traceability/orchestrator-storage.md)

## Implementation inventory
- `crates/pi-orchestrator/src/storage.rs`: module port.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/foundation.rs`
- `crates/pi-orchestrator/tests/differential.rs`

## Known gaps (current cycle)
- [ ] Malformed file and I/O failures propagate; additional failure fixtures pending.

## Out of scope
Live host state and network.
