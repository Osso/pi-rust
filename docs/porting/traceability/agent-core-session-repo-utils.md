# repo-utils.ts

Source: common-pin `agent-core/src/harness/session/repo-utils.ts:1–51`.
Rust: `harness/session/repo_utils.rs`.

| Export/branch | Proof |
|---|---|
| `createSessionId`, `createTimestamp` | Frozen deterministic clock/UUID storage differential |
| `getEntriesToFork`: current leaf, before/at target, missing target, empty ID | `differential_tree_session::frozen_storage_facade_and_fork_contracts_match` |

[Shared representation and proof artifacts](agent-core-tree-session.md). Repository orchestration proof tracked in its owning modules.
