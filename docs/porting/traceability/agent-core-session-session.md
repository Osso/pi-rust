# session.ts

Source: common-pin `agent-core/src/harness/session/session.ts:1–273`.
Rust: `harness/session/{facade,context}.rs`.

| Export/branch | Proof |
|---|---|
| `Session` getters, append message/thinking/model/tools/compaction/custom/custom-message/label/name | `differential_tree_session::frozen_storage_facade_and_fork_contracts_match` |
| `moveTo`, optional branch summary, `getBranch`, `buildSessionContext` | Same differential |
| Compaction-kept history and model/thinking selection | `compaction::{builds_session_context_with_a_compaction_entry,tracks_model_and_thinking_level_changes_in_built_context}` |
| Raw custom text, null details and normalized names | Native-unit facade differential |

[Owned-copy approval and shared discrepancies](agent-core-tree-session.md). Partial: broader multiple-compaction/unknown-entry context matrix pending.
