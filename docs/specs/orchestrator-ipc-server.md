# Orchestrator ipc-server

Port of frozen `packages/orchestrator/src/ipc/server.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] Accepted stream requests finish in order after socket disconnect; close removes callbacks but does not cancel accepted work. Frame padding uses ECMAScript String.trim.
- [x] startIpcServer; IpcRequestHandler; stale/live socket: bidirectional IPC, live rejection, stale removal, bind and unlink error identity: ipc/errors.
- [x] ordinary request/error/rpc_stream setup and ordered stream queue: fragmented JSONL, unknown stream, parse/handler error, pending-command event delivery: ipc/supervisor.
- [x] Server.close: real frozen differential: removes listening socket; established stream survives and responds: errors.
- [x] approved coalesced-frame post-pin fix: frozen stall pinned separately; byte-equal ordered/partial/coalesced matrix vs locked f11463802 delta: ipc.

## How it works
- [Module traceability](../porting/traceability/orchestrator-ipc-server.md)

## Implementation inventory
- `crates/pi-orchestrator/src/ipc/server.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/ipc.rs`
- `crates/pi-orchestrator/tests/errors.rs`
- `crates/pi-orchestrator/tests/supervisor.rs`

## Known gaps (current cycle)
No additional in-slice gap beyond the proof boundary above.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
