# Orchestrator RPC process
Port frozen process wrapper, with parent-approved native child packaging.

## What it must do
- [x] Correlate concurrent IDs and dispatch events/UI like frozen TS.
- [x] Reject pending sends on disposal or child exit.
- [x] Refuse sends after exit and await child termination.
- [ ] Prove remaining line/error/subscription branches.

## How it works
- [Traceability](../porting/traceability/orchestrator-rpc-process.md)

## Implementation inventory
- `crates/pi-orchestrator/src/rpc_process.rs`: child transport.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/rpc_process.rs`

## Known gaps (current cycle)
- [ ] Complete branch proof.

## Out of scope
Live agent/provider calls; tests launch only fake child.
