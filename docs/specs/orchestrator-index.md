# Orchestrator index
Port frozen `packages/orchestrator/src/index.ts`.

## What it must do
- [x] Preserve focused frozen command/output/lifecycle contract.
- [ ] Complete remaining stream/error branches.

## How it works
- [Traceability](../porting/traceability/orchestrator-index.md)

## Implementation inventory
- `crates/pi-orchestrator/src/index.rs`: module port.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/cli.rs`
- `crates/pi-orchestrator/tests/supervisor.rs`

## Known gaps (current cycle)
- [ ] Remaining branch proof; no unconditional coverage promotion.

## Out of scope
New executable packaging: frozen package.json declares no bin.
