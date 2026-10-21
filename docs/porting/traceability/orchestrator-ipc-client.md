# Orchestrator ipc-client

Source: `packages/orchestrator/src/ipc/client.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/ipc/client.rs`.
Status: **partial**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| sendIpcRequest | TS client→Rust server and Rust client→frozen TS server; first JSONL response and fragment buffering: ipc.rs |
| connect/read/EOF failure | ENOENT/ECONNREFUSED/EACCES/read ECONNRESET exact Node text and EOF message: errors.rs/ipc.rs |
| response parse and socket write diagnostics | shared JS parser and Node stream error helpers; malformed request/pipe-write fixtures cover common helpers |

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.

## Verifier correction (2026-10-21)

JS framing whitespace regression uses shared js_trim; renewed frame matrix is tracked in frame_whitespace.rs. Prior done claim withdrawn; broader framing/platform coverage is not exhaustive.
