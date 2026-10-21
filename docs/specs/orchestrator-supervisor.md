# Orchestrator supervisor

Port of frozen `packages/orchestrator/src/supervisor.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] OrchestratorSupervisor/supervisor; update/get/list live and persisted records: native runtime shares one supervisor and weak Radius coordinator; spawn/list/status/stop persistence and frozen lifecycle differential.
- [x] recoverAfterRestart; failSpawn; unexpected RPC exit; shutdown: online/starting recovery, error preservation, failed spawn stopped record, unexpected exit error record and removed live entry: supervisor.
- [x] openRpcStream; handleRpc; bindings and clone semantics: frozen event/UI/response lifecycle, shared callback identity and close semantics; child request correlation and initial session sync.
- [ ] Exhaustive metadata-refresh command/changed-identity matrix and Radius-failure cleanup paths remain unproven.

## How it works
- [Module traceability](../porting/traceability/orchestrator-supervisor.md)

## Implementation inventory
- `crates/pi-orchestrator/src/supervisor.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/supervisor.rs`
- `crates/pi-orchestrator/tests/rpc_process.rs`

## Known gaps (current cycle)
- [ ] Exhaustive metadata-refresh command/changed-identity matrix and Radius-failure cleanup paths remain unproven.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
