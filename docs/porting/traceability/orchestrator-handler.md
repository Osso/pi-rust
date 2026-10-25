# Orchestrator handler

Source: `packages/orchestrator/src/handler.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/handler.rs`.
Status: **partial**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| handleIpcRequest list/status/stop/rpc + unknown instance branches | frozen full child lifecycle differential; unknown status/stop/rpc/rpc_stream fixtures: supervisor |
| toInstanceSummary; openRpcStream | optional session fields, response/event/UI forwarding, no response leakage into event subscribers: supervisor |
| spawn success / successful rpc_stream ready dispatch | Frozen full child-lifecycle oracle now dispatches spawn and rpc_stream through actual handleIpcRequest and compares spawn_result/rpc_ready summaries, including ignored provider/model fields. Execution pending at commit. |

## Explicit proof gaps
- New combined success fixture pending execution; no unapproved behavioral change.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).
