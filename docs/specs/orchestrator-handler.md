# Orchestrator handler

Port of frozen `packages/orchestrator/src/handler.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] handleIpcRequest list/status/stop/rpc + unknown instance branches: frozen full child lifecycle differential; unknown status/stop/rpc/rpc_stream fixtures: supervisor.
- [x] toInstanceSummary; openRpcStream: optional session fields, response/event/UI forwarding, no response leakage into event subscribers: supervisor.
- [ ] Direct successful handleIpcRequest spawn and rpc_stream-ready dispatch branches lack dedicated frozen assertions.

## How it works
- [Module traceability](../porting/traceability/orchestrator-handler.md)

## Implementation inventory
- `crates/pi-orchestrator/src/handler.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/supervisor.rs`
- `crates/pi-orchestrator/tests/ipc.rs`

## Known gaps (current cycle)
- [ ] Direct successful handleIpcRequest spawn and rpc_stream-ready dispatch branches lack dedicated frozen assertions.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
