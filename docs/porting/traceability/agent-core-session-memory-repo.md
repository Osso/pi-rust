# memory-repo.ts

Source: common-pin `agent-core/src/harness/session/memory-repo.ts:1–50`.
Rust: `harness/session/memory.rs` (`InMemorySessionRepo`).

| Export/branch | Proof |
|---|---|
| `create/list/open/delete`, empty list, insertion order, repeated delete and missing open | `differential_tree_repository` |
| Replacement of an existing ID without moving map position | Same differential |
| `fork`, all entries, before/at target, missing/non-user/empty target | Same differential plus shared selector matrix |

GREEN: `tree-repository-green.log`. [Shared representation and owned-copy approval](agent-core-tree-session.md).
