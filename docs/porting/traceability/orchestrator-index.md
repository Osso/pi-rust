# Orchestrator index

Source: `packages/orchestrator/src/index.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/index.rs`.
Status: **done**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| all export-star modules | native index.rs reexports config/handler/IPC/process/serve/storage/supervisor/types; root imports exercised by errors and process-boundary fixtures |
| no Radius export-star and no runtime logic | Radius remains a module dependency, not part of native index reexports; no standalone executable behavior to differentiate |

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.

Native UTF-16 integration: [lossless orchestrator proof](lossless-orchestrator.md).
