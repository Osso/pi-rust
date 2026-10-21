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
