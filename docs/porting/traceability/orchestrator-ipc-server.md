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
