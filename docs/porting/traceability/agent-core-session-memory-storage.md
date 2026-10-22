# memory-storage.ts

Source: common-pin `agent-core/src/harness/session/memory-storage.ts:1–131`.
Rust: `harness/session/{memory,state,types}.rs`.

| Export/branch | Proof |
|---|---|
| `InMemorySessionStorage`, metadata, entries, entry lookup, find, leaf, labels, path | `differential_tree_session::frozen_storage_facade_and_fork_contracts_match` |
| Initial invalid leaf, missing path and missing move target | Same differential |
| ID generation and collision avoidance | Same differential uses 2048 frozen deterministic UUID samples |
| Duplicate append log / last index wins | `repin_tree_session` RED→GREEN |

[Shared representation, discrepancy and approval](agent-core-tree-session.md). Partial: malformed cyclic parent graphs not exercised.
