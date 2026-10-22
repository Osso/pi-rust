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

31 padding code points (24 trimmed; 7 rejected), exact values/errors vs Node trim+JSON.parse across real sockets: `frame_whitespace.rs`. Full frozen module BOM boundary is independently demonstrated server-side in `verifier_regressions.rs`; the full client Unicode matrix is a language-semantic oracle, not a full-module differential. Arbitrary Unicode chunk splits remain unproved.

Prior unconditional done claim withdrawn. Seven regression mutants (exit EOF, queue abort, four trim sites, successful-body read) were killed by behavioral failures, not compiler failures.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).
