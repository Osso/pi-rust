# memory-repo.ts

Source: common-pin `agent-core/src/harness/session/memory-repo.ts:1–50`.
Rust: `harness/session/memory.rs` (`InMemorySessionRepo`).

| Export/branch | Proof |
|---|---|
| `create/list/open/delete`, empty list, insertion order, repeated delete and missing open | `differential_tree_repository` |
| Replacement of an existing ID without moving map position | Same differential |
| `fork`, all entries, before/at target, missing/non-user/empty target | Same differential plus shared selector matrix |

GREEN: `tree-repository-green.log`. [Shared representation and owned-copy approval](agent-core-tree-session.md).

Automatic/default IDs are now exercised through the complete create/fork/open/list/delete workflow for both memory and JSONL repositories, alongside explicit-ID replacement. Frozen initial UUID samples and a fixed clock drive the native seams without normalizing output IDs. GREEN `session-repository-auto-id-proof.log`, four full orchestrations. This closes the remaining automatic-ID branch proof; no production change was needed.
