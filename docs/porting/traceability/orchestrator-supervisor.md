# Orchestrator supervisor

Source: `packages/orchestrator/src/supervisor.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/supervisor.rs`.
Status: **partial**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| OrchestratorSupervisor/supervisor; update/get/list live and persisted records | native runtime shares one supervisor and weak Radius coordinator; spawn/list/status/stop persistence and frozen lifecycle differential |
| recoverAfterRestart; failSpawn; unexpected RPC exit; shutdown | online/starting recovery, error preservation, failed spawn stopped record, unexpected exit error record and removed live entry: supervisor |
| openRpcStream; handleRpc; bindings and clone semantics | frozen event/UI/response lifecycle, shared callback identity and close semantics; child request correlation and initial session sync |
| metadata refresh set and Radius cleanup failure paths | all six frozen refresh commands implemented; exhaustive changing-identity and Radius-failure cleanup matrices remain unproven |

## Explicit proof gaps
- Exhaustive metadata-refresh command/changed-identity matrix and Radius-failure cleanup paths remain unproven.
