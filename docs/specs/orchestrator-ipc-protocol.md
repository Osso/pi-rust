# Orchestrator ipc-protocol

Port of frozen `packages/orchestrator/src/ipc/protocol.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] encodeMessage; parseRequestLine/parseResponseLine: JS number and integer-key order, LF framing, cast-only success and SyntaxError messages: foundation/differential + IPC errors.
- [x] request interfaces/RequestMap/OrchestratorRequest: native OrchestratorRequest enum; six request discriminators; raw parser stays JSON-cast compatible.
- [x] response interfaces/ResponseMap/ResponseFor; RpcClientMessage/RpcServerMessage/ProtocolMessage: native JSON response/event envelopes; shared coding-agent RpcCommand/RpcResponse/UI types reused, not copied; handler/stream fixtures exercise wire values.

## How it works
- [Module traceability](../porting/traceability/orchestrator-ipc-protocol.md)

## Implementation inventory
- `crates/pi-orchestrator/src/ipc/protocol.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/foundation.rs`
- `crates/pi-orchestrator/tests/differential.rs`
- `crates/pi-orchestrator/tests/ipc.rs`
- `crates/pi-orchestrator/tests/supervisor.rs`

## Known gaps (current cycle)
No additional in-slice gap beyond the proof boundary above.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
