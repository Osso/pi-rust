# Current session history search traceability

Frozen source: `packages/coding-agent/src/core/tools/search-current-session-history.ts` at `4f9e6f8a4` ([lock](../source-lock.json)). Rust: `crates/pi-coding-agent-core/src/core/tools/search_current_session_history.rs`. Contract: [spec](../../specs/search-current-session-history.md).

| Source symbol/branch | Rust | Proof |
|---|---|---|
| schema, description, label/name, snippet/guidelines, parallel mode | `parameters`, constants, definition | frozen `history:definition` exact JSON differential |
| `collectArgumentTexts`: keys, nested arrays, string/number/bool/null | `collect_argument_texts`, shared `js_number_to_string` | nested key/value/scalar fixtures; reopened-session numeric queries and `search_current_session_history_live_numbers_differential` prove `1.0` exclusion / `0.000001` inclusion / `1e-6` exclusion against frozen |
| `collectSearchTexts`, text/tool name/arguments, thinking/signature/image omission | `collect_search_texts` | assistant/tool fixtures, hidden thinking/signature no-match fixtures; image exclusion inspected |
| `collectMessageSearchTexts`: nonself tool results, bash command/output, inline summaries | `collect_message_search_texts` | result, bash command/output and compactionSummary fixtures |
| own tool calls/results excluded | both collection helpers | self-traffic no-match fixture, context retains full original content |
| `searchableEntry`: message/custom/compaction/branch summary (frozen `search-current-session-history.ts:93-118`) | `searchable_entry` reads the raw stored record (loaded entries stay `SessionEntry::Unknown`; no typed decode) | exact result JSON for each entry family; `repin_session_storage::persisted_history_tool_recovers_source_compacted_ancestor` (RED `lossless-targets-a600f578.log`: totalMatches 0 because raw records were skipped; GREEN `search-history-green.log`) |
| cursor, chronological page, neighboring context/compacted flag | existing execute pipeline | two real deployment matches plus summary; pages 1/2; limit, context, huge/leading-zero/invalid cursors |
| persisted guard / trimmed empty query / no match / inactive branch | execute guards and history reader | eight behavioral tests + differential empty-query/no-match |
| `renderCall`, `renderResult` | retained renderers | inspected against frozen; prior renderer differential is historical, not rerun as fresh pin proof |

Assistant match reproduced RED before production edits. `differential_session_tools::search_current_session_history_tool_differential` now GREEN with 25 query cases and registration comparison; eight native behavior tests GREEN at `3d4e6a32`. Own-tool-call-inclusion mutant killed by self-traffic mismatch. Old user-only contract/test retired. [History storage](session-storage-integration.md), [proof limits](../session-archive-history-proof.md).

Verifier corrections at `9bb38edf`: 30 reopened-session queries plus three live-manager numeric queries pass frozen differential. Numeric-format mutant killed by live `1.0` false positive; fixed-point false negative reproduced RED independently. Four-target evidence: [updated proof](../session-archive-history-proof.md#verifier-corrections-2026-10-21).

## Q18 native metadata checkpoint

Search-entry IDs/timestamps/type tags, context-ID sets and pagination cursor strings now use `JsString`. Cursor text deserializes losslessly, is explicitly rejected unless its UTF-16 units are ASCII digits, then crosses the numeric-offset boundary through the native number parser. Native history errors propagate through `SessionManagerError::message`; call rendering no longer formats native theme text with scalar `format!`.

`js_value_tests::raw_cursor_reaches_explicit_numeric_validation_without_json_narrowing` is added but unexecuted pending coding-library compilation. Earlier source proof above does not cover this checkpoint yet.

Typed Rust input exclusions remain explicit in [compatibility spec](../../specs/compatibility.md). Lowercasing remains inherited Unicode default mapping; no new locale waiver is claimed. Native generic serde UTF-16 representation edges are not expanded by this lane.
