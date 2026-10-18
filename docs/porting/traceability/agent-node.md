# Agent node entrypoint traceability

Pinned source: `packages/agent/src/node.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`. Rust: `crates/pi-agent-core/src/node.rs`.

| TypeScript source | Observable contract | Rust item and proof |
|---|---|---|
| `node.ts:1` | Re-export `NodeExecutionEnv` | `node::NodeExecutionEnv` (`pub use crate::harness::env::NodeExecutionEnv`) |
| `node.ts:2` | Re-export the full package index | `pub use crate::*;`; crate-root tests and differential cover the re-exported surface |

## Gaps / discrepancies

- None. Rust has no separate package export map; `pi_agent_core::node` is the
  equivalent module path. The node environment itself is covered by
  `harness-nodejs-env.md`.
