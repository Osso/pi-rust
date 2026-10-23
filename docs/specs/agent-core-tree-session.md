# Agent-core tree sessions

## Contract

- Match frozen `4f9e6f8a4` agent-core session modules and harness compaction, not retired `packages/agent` lane APIs.
- Persist v3 session headers and append-only tree entries with native UTF-16 strings and arbitrary JSON values. Preserve unknown entry types, unknown fields, explicit nulls and source ordering.
- Keep leaf selection, parent traversal, labels, session names, compaction-kept history, active tools, model and thinking state consistent with frozen session behavior.
- Return owned getter values (approved 2026-10-22); JavaScript mutation-through-reference is excluded.
- JSONL listing filters only frozen invalid-header cases; accepted invalid dates retain exact pinned V8 date-difference sort order. Directory/symlink/extension and header-only behavior follow the [repository trace](../porting/traceability/agent-core-session-jsonl-repo.md).
- Expose memory/JSONL create, list, open, delete and fork behavior with frozen errors. Preserve classified frozen duplicate-ID behavior rather than introducing uniqueness repairs.
- Preserve native Node promise rejection independently of returned FileError and SessionError; do not prefix or reclassify a rejected filesystem operation.
- Compaction retains history by `firstKeptEntryId`; no embedded retained tails, lane records, retry extension or usage-returning core summary API.
- Timestamp-string summary/custom creators apply native Date.parse, including NaN. Provider message timestamps preserve the full JavaScript numeric domain through f64; no checked i64 conversion remains.

## Proof

[Module coverage and explicit remaining cases](../porting/traceability/agent-core-tree-session.md); [compaction](../porting/traceability/compaction.md); [branch summary](../porting/traceability/branch-summarization.md).
