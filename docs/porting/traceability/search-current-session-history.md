# Current session history search traceability

Frozen source: `packages/coding-agent/src/core/tools/search-current-session-history.ts` at `4f9e6f8a4` ([lock](../source-lock.json)).
Rust: `crates/pi-coding-agent/src/core/tools/search_current_session_history.rs`.

| Source | Rust | Proof |
|---|---|---|
| schema, metadata/guidance, renderers | parameters/constants/definition/renderers | existing renderer differential; registration coverage pending |
| `collectArgumentTexts` | `collect_argument_texts` | nested keys, arrays, scalar differential fixtures |
| `collectSearchTexts`, `collectMessageSearchTexts` | corresponding Rust helpers | text/tool/custom/summary/bash and self/thinking exclusion fixtures |
| `searchableEntry` | `searchable_entry` | message/custom/compaction/branch summary fixtures |
| cursor, pagination, neighboring context/compaction | existing execute pipeline | page 1/2, context, invalid cursor differential |
| persisted guard, empty query | existing execute guards | existing behavioral tests |

`differential_session_tools::search_current_session_history_tool_differential`: assistant match RED recorded before implementation; expanded frozen fixtures GREEN pending. Old user-only contract and its evidence retired. History storage: [integration trace](session-storage-integration.md). Rust typed inputs retain the documented invalid-JS-input exclusions; locale edge remains inherited, not new parity evidence.
