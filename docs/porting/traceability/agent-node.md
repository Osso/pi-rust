# Traceability: `packages/agent-core/src/node.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/node.rs`.

| Frozen export | Rust | Proof |
|---|---|---|
| NodeExecutionEnv | pub use crate::harness::env::NodeExecutionEnv | workspace all-targets compilation; environment runtime parity belongs to its slice |
| index.ts exports | pub use crate::* | root forwarding compiles; extra old-only root exports remain for owning slices |

No dedicated node-only runtime test or whole export-surface parity claim. See [index](agent-index.md) and [slice-1 ledger](agent-core-api-repin.md).
