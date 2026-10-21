# Orchestrator config

Port of frozen `packages/orchestrator/src/config.ts`.

## What it must do
- [x] Preserve tested JSON/path/record contract against frozen source.
- [ ] Complete all source branches and exported-symbol proof.

## How it works
- [Traceability](../porting/traceability/orchestrator-config.md)

## Implementation inventory
- `crates/pi-orchestrator/src/config.rs`: module port.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/foundation.rs`
- `crates/pi-orchestrator/tests/differential.rs`

## Known gaps (current cycle)
- [ ] Bun virtual filesystem detection is false in the native port; package metadata uses frozen package version.

## Out of scope
Live host state and network.
