# session.ts

Source: common-pin `agent-core/src/harness/session/session.ts:1–273`.
Rust: `harness/session/{facade,context}.rs`.

| Export/branch | Proof |
|---|---|
| `Session` getters, append message/thinking/model/tools/compaction/custom/custom-message/label/name | `differential_tree_session::frozen_storage_facade_and_fork_contracts_match` |
| `moveTo`, optional branch summary, `getBranch`, `buildSessionContext` | Same differential |
| Compaction-kept history and model/thinking selection | `compaction::{builds_session_context_with_a_compaction_entry,tracks_model_and_thinking_level_changes_in_built_context}` |
| Raw custom text, null details and normalized names | Native-unit facade differential |

[Owned-copy approval and shared discrepancies](agent-core-tree-session.md). Complete with the extended multiple-compaction context matrix below.

`multiple_compaction_contexts_and_duplicate_boundary_ids_match_frozen` adds nine direct frozen context scenarios: multiple/latest summaries; first-kept IDs before/at/after/missing from the prior range; duplicate compaction IDs; model/thinking/tool state carried across compaction; raw custom message text and branch summaries. GREEN `session-multiple-compaction-proof.log`. Source first-ID compaction lookup bug is retained: latest summary selection does not change findIndex's earlier duplicate-ID boundary. Existing unknown-entry/storage and all facade append/move/get operations remain in the broader differential. This closes the remaining context matrix.
