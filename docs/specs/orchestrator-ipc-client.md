# Orchestrator ipc-client

Port of frozen `packages/orchestrator/src/ipc/client.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] Frame padding uses ECMAScript String.trim, not Rust Unicode trim.
- [x] sendIpcRequest: TS client→Rust server and Rust client→frozen TS server; first JSONL response and fragment buffering: ipc.rs.
- [x] connect/read/EOF failure: ENOENT/ECONNREFUSED/EACCES/read ECONNRESET exact Node text and EOF message: errors.rs/ipc.rs.
- [x] response parse and socket write diagnostics: shared JS parser and Node stream error helpers; malformed request/pipe-write fixtures cover common helpers.

## How it works
- [Module traceability](../porting/traceability/orchestrator-ipc-client.md)

## Implementation inventory
- `crates/pi-orchestrator/src/ipc/client.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/ipc.rs`
- `crates/pi-orchestrator/tests/errors.rs`

## Known gaps (current cycle)
No additional in-slice gap beyond the proof boundary above.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
