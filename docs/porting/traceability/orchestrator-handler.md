# Orchestrator handler

Source: `packages/orchestrator/src/handler.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/handler.rs`.
Status: **done** (realistic-input user-observable parity). [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| handleIpcRequest list/status/stop/rpc + unknown instance branches | frozen full child lifecycle differential; unknown status/stop/rpc/rpc_stream fixtures: supervisor |
| toInstanceSummary; openRpcStream | optional session fields, response/event/UI forwarding, no response leakage into event subscribers: supervisor |
| spawn success / successful rpc_stream ready dispatch | Frozen full child-lifecycle oracle now dispatches spawn and rpc_stream through actual handleIpcRequest and compares spawn_result/rpc_ready summaries, including ignored provider/model fields. GREEN at c654ffcbe (`/tmp/runtime-services-handler-success.log`). |

## Explicit proof gaps
- All realistic exported dispatch/stream branches mapped above; adversarial request shapes are outside the parity bar.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).

Whole frozen module re-read in the runtime-services lane (2026-10-25). No platform-fault or event-scheduling equivalence is claimed.
