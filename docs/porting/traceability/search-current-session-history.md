# Current session history search traceability

Frozen source: `packages/coding-agent/src/core/tools/search-current-session-history.ts` at `4f9e6f8a4` ([lock](../source-lock.json)). Rust: `crates/pi-coding-agent/src/core/tools/search_current_session_history.rs`. Contract: [spec](../../specs/search-current-session-history.md).

| Source symbol/branch | Rust | Proof |
|---|---|---|
| schema, description, label/name, snippet/guidelines, parallel mode | `parameters`, constants, definition | frozen `history:definition` exact JSON differential |
| `collectArgumentTexts`: keys, nested arrays, string/number/bool/null | `collect_argument_texts` | nested key/value/scalar query fixtures; JS numeric enumeration unit fixture updated |
| `collectSearchTexts`, text/tool name/arguments, thinking/signature/image omission | `collect_search_texts` | assistant/tool fixtures, hidden thinking/signature no-match fixtures; image exclusion inspected |
| `collectMessageSearchTexts`: nonself tool results, bash command/output, inline summaries | `collect_message_search_texts` | result, bash command/output and compactionSummary fixtures |
| own tool calls/results excluded | both collection helpers | self-traffic no-match fixture, context retains full original content |
| `searchableEntry`: message/custom/compaction/branch summary | `searchable_entry` | exact result JSON for each entry family |
| cursor, chronological page, neighboring context/compacted flag | existing execute pipeline | two real deployment matches plus summary; pages 1/2; limit, context, huge/leading-zero/invalid cursors |
| persisted guard / trimmed empty query / no match / inactive branch | execute guards and history reader | eight behavioral tests + differential empty-query/no-match |
| `renderCall`, `renderResult` | retained renderers | inspected against frozen; prior renderer differential is historical, not rerun as fresh pin proof |

Assistant match reproduced RED before production edits. `differential_session_tools::search_current_session_history_tool_differential` now GREEN with 25 query cases and registration comparison; eight native behavior tests GREEN at `3d4e6a32`. Own-tool-call-inclusion mutant killed by self-traffic mismatch. Old user-only contract/test retired. [History storage](session-storage-integration.md), [proof limits](../session-archive-history-proof.md).

Typed Rust input exclusions remain explicit in [compatibility spec](../../specs/compatibility.md). Lowercasing remains inherited Unicode default mapping; no new locale waiver is claimed. Native generic serde UTF-16 representation edges are not expanded by this lane.
