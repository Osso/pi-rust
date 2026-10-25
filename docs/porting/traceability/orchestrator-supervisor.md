# Orchestrator supervisor

Source: all 354 lines of `packages/orchestrator/src/supervisor.ts` at the [common pin](../source-lock.json), `4f9e6f8a4`. Rust: `crates/pi-orchestrator/src/supervisor.rs`.
Status: **done**, whole-module audit completed 2026-10-25 under the lighter-proof rule. [Native boundary approvals](../orchestrator-proof.md) and [lossless integration](lossless-orchestrator.md).

| Export/branch | Audit and behavioral proof |
| --- | --- |
| OrchestratorSupervisor/supervisor; update/get/list live and persisted records | One native supervisor with weak Radius coordinator; owned shallow-record equivalents; existing spawn/list/status/stop persistence and frozen lifecycle proof |
| setStatus/updateRecord; bind/clear RPC resources | Field insertion/undefined omission, lastSeenAt, resource ids, event/exit/UI subscription lifetime audited; existing lifecycle and shared callback-identity tests |
| recoverAfterRestart; failSpawn; unexpected exit; shutdown | Ordered recovery and disconnect before persistence, error→stopped failed spawn, unexpected exit→error and live removal, snapshot of shutdown ids audited; native failure/recovery tests |
| openRpcStream/handleRpc | Captured process, shared callback Set identity, last UI handler, conditional UI cleanup, response-before-metadata-refresh and response return audited; frozen stream/lifecycle proof |
| metadata refresh | Exactly new_session, switch_session, fork, clone, set_session_name, prompt; get_state success classifier and optional sessionFile omission audited; existing child initial-state and RPC tests. No new exhaustive fixture matrix required by lighter proof |
| Radius resource acquisition/cleanup | Register after session sync, disconnect before RPC disposal, failure/finally ordering audited; enabled-Radius real-IPC lifecycle and native failed-disconnect test |

## Successor proof

`/tmp/runtime-successor-orchestrator.log`: 10 existing supervisor tests PASS; two native runtime-services tests PASS; eight IPC tests PASS, including both frozen wire directions and the existing approved coalesced-frame delta comparison; one exact-UTF-16 real IPC test PASS; two existing protocol/storage/config differentials PASS. No new frozen differential or matrix was added.

The new native `enabled_radius_serve_registers_serves_ipc_and_disconnects_in_order` covers real fake-child spawn, machine/Pi registration, persisted metadata, IPC list and ordered Pi/machine disconnect. `radius_disconnect_failure_preserves_frozen_stop_finally_and_skipped_rpc_disposal` proves the failure boundary without leaving an owned child running.

## Discrepancies / gaps

- DISCREPANCY (preserved frozen bug): if Radius disconnect rejects, cleanup stops before RPC disposal, but stopInstance's finally still removes the live and persisted record. A child can remain running and unreachable through the supervisor. The native regression retains a separate test-only process handle and explicitly disposes that child afterward; product behavior is not repaired.
- GAP: malformed/adversarial records, cycles, numeric extremes and exceptional OS/runtime/scheduling fault shapes use the approved input/runtime gap. Exact UTF-16 carriers remain intact.

No realistic-input implementation difference was found in the successor audit; no production supervisor change was needed.
