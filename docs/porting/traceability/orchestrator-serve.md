# Orchestrator serve
Frozen `packages/orchestrator/src/serve.ts` at [common pin](../source-lock.json).

| Exports/branches | Proof |
| --- | --- |
| Whole module | tests/supervisor.rs: startup logs and frozen SIGTERM cleanup; native idempotent shutdown and startup-failure socket cleanup |
| cli rpc-stream / serve Radius-enabled logs and failure signals | implemented; remaining focused fixtures pending |

Parent-approved native boundary: library-only package (no frozen bin entry), async serve returns Result after signal cleanup, CLI run returns exit code (signal shutdown 0, propagated errors 1). JS uncaughtException/unhandledRejection map to propagated native errors → embedding exit 1; no process-global panic hooks. Child packaging uses sibling pi --mode rpc. Frozen source remains immutable.

Approved frozen discrepancies (parent 2026-10-21): duplicate in-flight ID retains the original pending resolver even after disposal; Rust orphan sender list releases only with instance lifetime. Malformed child JSON propagates through native fatal channel to serve Err/CLI exit 1, with ordinary server/supervisor/Radius/socket cleanup, not a new immediate send rejection. Real-child RED/GREEN and frozen subprocess differential fixtures cover both.
