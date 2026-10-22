# Orchestrator handler

Source: `packages/orchestrator/src/handler.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/handler.rs`.
Status: **partial**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| handleIpcRequest list/status/stop/rpc + unknown instance branches | frozen full child lifecycle differential; unknown status/stop/rpc/rpc_stream fixtures: supervisor |
| toInstanceSummary; openRpcStream | optional session fields, response/event/UI forwarding, no response leakage into event subscribers: supervisor |
| spawn success / successful rpc_stream ready dispatch | implemented using tested supervisor/summary code; explicit combined handler-success fixture not yet added |

## Explicit proof gaps
- Direct successful handleIpcRequest spawn and rpc_stream-ready dispatch branches lack dedicated frozen assertions.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).
