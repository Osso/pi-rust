# Orchestrator supervisor
Port frozen `packages/orchestrator/src/supervisor.ts`.

## What it must do
- [x] Preserve spawn, list, status, RPC and stop handler envelopes.
- [x] Persist session identity and remove stopped live records.
- [x] Forward stream responses/events/UI and preserve raw event-only subscription.
- [x] Recover online/starting persisted entries as stopped.
- [ ] Prove complete failure/recovery/subscription ordering matrix.

## How it works
- [Traceability](../porting/traceability/orchestrator-supervisor.md)

## Implementation inventory
- `crates/pi-orchestrator/src/supervisor.rs`: source module port.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/supervisor.rs`

## Known gaps (current cycle)
- [ ] Complete remaining branch proof.

## Out of scope
Live provider and Radius calls.
