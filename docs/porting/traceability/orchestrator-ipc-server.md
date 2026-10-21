# Orchestrator ipc-server
Frozen `packages/orchestrator/src/ipc/server.ts`; [source lock](../source-lock.json).

| Exports/branches | Proof |
| --- | --- |
| sendIpcRequest / startIpcServer | `tests/ipc.rs`: TS client → Rust server; Rust client → TS server |
| partial JSONL, ordinary request and rpc_stream, sequential coalesced commands | `fragmented_jsonl_and_live_socket_rejection` |
| live socket rejected, stale socket removed | `fragmented_jsonl_and_live_socket_rejection`, `rust_client_to_frozen_ts_server_and_coalesced_stream_boundary` |
| initial coalesced stream command waits for new data | last test pins frozen discrepancy, pending user classification |
| parser/handler exception, EOF client failure, UI/event callbacks | implemented; additional behavioral fixtures pending |

Rust: `crates/pi-orchestrator/src/ipc/server.rs`. Coverage remains todo until final branch matrix. No live services used; all sockets under sandbox temp directories.

Streaming concurrency: `session_events_are_written_while_rpc_command_is_pending` reproduces the Rust draft buffering regression (RED); independent sequential command worker now permits socket writes during pending RPCs, matching frozen callback behavior. Targeted GREEN pending.

Additional `tests/errors.rs` Unix pipe differentials: ENOENT/ECONNREFUSED/EACCES connect, libuv missing-parent bind→EACCES, bind permission, stale-file unlink permission, read ECONNRESET; normal Server.close unlinks listening socket but preserves established stream requests. Native draft failed these boundaries before shared Node diagnostics and explicit stop-accepting lifecycle corrections.

## Approved post-pin coalesced-line delta (2026-10-21)
User classifies frozen coalesced-frame handling as a TS bug. `rust_client_to_frozen_ts_server_pins_approved_coalesced_discrepancy` keeps the exact common-pin stall assertion. Native `approved_delta_*` tests require immediate ordered drain after handshake, multiple buffered frames and blank-prefix handling. Ordinary request connections still end after their one response. Rust RED reproduced both stalls before the fix. Immutable TS delta commit/checkout and delta-lock entry are parent-owned and pending; no alternate TS source is selected or changed here.

Approved TS delta f11463802d385787413b3c434dd71d171a58ac37 is now independently verified and locked: server SHA256 392a5982cdd40ba6880bc314d8cbc077f8b5467874ef7f7cb77b682fc0fbfe5d. Oracle selects only this server module through approved-delta.mjs with PI_APPROVED_DELTA=1; ordinary peers explicitly remove the selector. `approved_locked_ts_delta_matches_native_coalesced_and_partial_frames_byte_for_byte` covers blank-prefixed ordinary one-response EOF, coalesced handshake + ordered commands, incomplete tail retention and later coalesced continuation. Common-pin frozen stall remains a separate exact expected discrepancy.
