# Orchestrator ipc-protocol

Port of frozen `packages/orchestrator/src/ipc/protocol.ts`.

## What it must do
- [x] Preserve tested JSON/path/record contract against frozen source.
- [ ] Complete all source branches and exported-symbol proof.

## How it works
- [Traceability](../porting/traceability/orchestrator-ipc-protocol.md)

## Implementation inventory
- `crates/pi-orchestrator/src/ipc/protocol.rs`: module port.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/foundation.rs`
- `crates/pi-orchestrator/tests/differential.rs`

## Known gaps (current cycle)
- [ ] Parsers cast JSON without schema validation as frozen TS; shared coding-agent RPC types are re-exported.

## Out of scope
Live host state and network.
