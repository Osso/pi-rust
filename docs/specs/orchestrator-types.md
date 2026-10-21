# Orchestrator types

Port of frozen `packages/orchestrator/src/types.ts`.

## What it must do
- [x] Preserve tested JSON/path/record contract against frozen source.
- [ ] Complete all source branches and exported-symbol proof.

## How it works
- [Traceability](../porting/traceability/orchestrator-types.md)

## Implementation inventory
- `crates/pi-orchestrator/src/types.rs`: module port.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/foundation.rs`
- `crates/pi-orchestrator/tests/differential.rs`

## Known gaps (current cycle)
- [ ] Typed invalid JS input exclusions apply; optional record fields omitted, not null.

## Out of scope
Live host state and network.
