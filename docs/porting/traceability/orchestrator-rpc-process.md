# Orchestrator rpc-process

Source: `packages/orchestrator/src/rpc-process.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/rpc_process.rs`.
Status: **partial**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| RpcProcessInstance/createRpcProcessInstance | native spawn method; sibling pi --mode rpc and cwd proved by copied embedding test with sandbox fake sibling |
| send; handleLine; handleUiResponse; pending requests | concurrent IDs, events/UI routing, duplicate ID orphan lifetime, exit/disposal errors, EOF-tail and Node UTF-8 decoding: rpc_process |
| setUiRequestHandler/onEvent/onExit and unsubscribes | frozen Set identity, repeated callback deduplication and unsubscribe counts: rpc_process; raw supervisor stream tests |
| spawn/stdin/exit/dispose | six real spawn failure messages, closed stdin EPIPE, SIGKILL/code errors, disposal pending rejection and fatal→serve cleanup: errors/rpc_process/supervisor |

Duplicate in-flight IDs intentionally keep the overwritten original resolver pending through disposal; orphan senders release only with instance lifetime. Malformed child JSON propagates to serve fatal cleanup without an invented immediate rejection. Both preserve parent-approved frozen behavior.

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.

## Verifier correction (2026-10-21)

Exit state/notifications/disposal follow child exit, independently of inherited-pipe EOF. Verifier regression and Unicode child-frame matrix tracked; renewed proof pending, not an unconditional done claim.
