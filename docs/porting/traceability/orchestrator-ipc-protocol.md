# Orchestrator ipc-protocol

Frozen: `packages/orchestrator/src/ipc/protocol.ts` at [common pin](../source-lock.json).

| Surface | Rust | Proof |
| --- | --- | --- |
| Whole module | `crates/pi-orchestrator/src/ipc/protocol.rs` | `foundation::protocol_uses_js_numbers_key_order_and_newline; differential::frozen_protocol_storage_bidirectional_file_bytes` |

Parsers cast JSON without schema validation as frozen TS; shared coding-agent RPC types are re-exported.

Status: initial focused proof; complete branch matrix pending. Coverage remains todo until final evidence.
