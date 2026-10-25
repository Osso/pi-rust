# Orchestrator ipc-server

Source: `packages/orchestrator/src/ipc/server.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/ipc/server.rs`.
Status: **done** (realistic-input user-observable parity). [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| startIpcServer; IpcRequestHandler; stale/live socket | bidirectional IPC, live rejection, stale removal, bind and unlink error identity: ipc/errors |
| ordinary request/error/rpc_stream setup and ordered stream queue | fragmented JSONL, unknown stream, parse/handler error, pending-command event delivery: ipc/supervisor |
| Server.close | real frozen differential: removes listening socket; established stream survives and responds: errors |
| approved coalesced-frame post-pin fix | frozen stall pinned separately; byte-equal ordered/partial/coalesced matrix vs locked f11463802 delta: ipc |

Approved delta: [f11463802 lock](../delta-lock.json), server SHA256 `392a5982cdd40ba6880bc314d8cbc077f8b5467874ef7f7cb77b682fc0fbfe5d`. Frozen source remains selected for the exact stall discrepancy assertion; only the explicit delta test sets `PI_APPROVED_DELTA=1`. Ordinary requests still close after one response.

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.

## Verifier correction (2026-10-21)

`queued_command_survives_disconnect_like_frozen` now completes one in-flight side effect after socket close, matching frozen. `frame_whitespace.rs` proves 31 code points for both ordinary and established-stream framing vs Node trim+JSON.parse. New actual-source all_queued_commands_finish_in_order_after_disconnect_like_frozen fixture sends three coalesced commands after the stream-ready handshake, disconnects after the first starts, and compares ordered completed side effects. GREEN at 0b0d4f638 (`/tmp/runtime-services-ipc-queued.log`).

Prior unconditional done claim withdrawn. Seven regression mutants (exit EOF, queue abort, four trim sites, successful-body read) were killed by behavioral failures, not compiler failures.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).

Whole frozen module re-read in the runtime-services lane (2026-10-25). No platform-fault or event-scheduling equivalence is claimed.
