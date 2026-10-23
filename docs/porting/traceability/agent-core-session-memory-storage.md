# memory-storage.ts

Source: common-pin `agent-core/src/harness/session/memory-storage.ts:1–131`.
Rust: `harness/session/{memory,state,types}.rs`.

| Export/branch | Proof |
|---|---|
| `InMemorySessionStorage`, metadata, entries, entry lookup, find, leaf, labels, path | `differential_tree_session::frozen_storage_facade_and_fork_contracts_match` |
| Initial invalid leaf, missing path and missing move target | Same differential |
| ID generation and collision avoidance | Same differential uses 2048 frozen deterministic UUID samples |
| Duplicate append log / last index wins | `repin_tree_session` RED→GREEN |

[Shared representation, discrepancy and approval](agent-core-tree-session.md). Complete with the bounded source-cycle characterization below.

The expanded storage/facade differential covers append of an invalid leaf followed by getLeafId rejection, recovery through a later append, trimmed/cleared labels, and duplicate-ID log-versus-index/path behavior (`session-storage-mutation-proof.log`). The existing deterministic facade writes exercise the100-collision/full-UUID branch.

DISCREPANCY preserved: self-parent and two-entry parent cycles never settle in frozen getPathToRoot. `session_cycle_characterization` compares owned frozen/native subprocesses after readiness with a20ms bounded non-settlement probe, then terminates/reaps them. This is bounded observed non-settlement plus literal traversal-loop comparison, not a timing guarantee or cycle repair. GREEN `session-cycle-proof.log`; no cycle guard was added. The formerly unexercised cyclic branch is now characterized.
