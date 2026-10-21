# Orchestrator cli
Frozen `packages/orchestrator/src/cli.ts` at [common pin](../source-lock.json).

| Exports/branches | Proof |
| --- | --- |
| Whole module | tests/cli.rs: help/version/usage/unknown exit codes and list/spawn/status/stop/rpc request JSON stdout compared to frozen CLI subprocess |
| cli rpc-stream / serve Radius-enabled logs and failure signals | implemented; remaining focused fixtures pending |

Parent-approved native boundary: library-only package (no frozen bin entry), async serve returns Result after signal cleanup, CLI run returns exit code (signal shutdown 0, propagated errors 1). JS uncaughtException/unhandledRejection map to propagated native errors → embedding exit 1; no process-global panic hooks. Child packaging uses sibling pi --mode rpc. Frozen source remains immutable.
