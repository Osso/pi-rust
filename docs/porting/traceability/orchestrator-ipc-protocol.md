# Orchestrator ipc-protocol

Source: `packages/orchestrator/src/ipc/protocol.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/ipc/protocol.rs`.
Status: **done**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| encodeMessage; parseRequestLine/parseResponseLine | JS number and integer-key order, LF framing, cast-only success and SyntaxError messages: foundation/differential + IPC errors |
| request interfaces/RequestMap/OrchestratorRequest | native OrchestratorRequest enum; six request discriminators; raw parser stays JSON-cast compatible |
| response interfaces/ResponseMap/ResponseFor; RpcClientMessage/RpcServerMessage/ProtocolMessage | native JSON response/event envelopes; shared coding-agent RpcCommand/RpcResponse/UI types reused, not copied; handler/stream fixtures exercise wire values |

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.
