# coding-agent search-current-session-history traceability

Pinned source: `packages/coding-agent/src/core/tools/search-current-session-history.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/search_current_session_history.rs`
(`SearchCurrentSessionHistoryTool`).

| Source | Rust | Tests |
|---|---|---|
| `searchCurrentSessionHistorySchema` | `parameters()` | differential |
| `collectSearchTexts` (recursive strings) | `collect_search_texts` | differential |
| `searchableEntry` (message/custom_message/compaction/branch_summary) | `searchable_entry` | 7 behavioral tests |
| `parseCursor` (`^\d+$`) | `parse_cursor` | `rejects_an_invalid_cursor`, differential |
| `selectedIndexes` (context window) | `selected_indexes` | neighbor test, differential |
| `requirePersistedSession` | `require_persisted_session` | `requires_a_persisted_current_session` |
| matching (`toLocaleLowerCase` literal `includes` over user text) | `execute` (`str::to_lowercase`) | user-only, multiline, pagination tests |
| `compacted` via `buildContextEntries` ids | same | compaction-hidden test, differential |
| `JSON.stringify(details, null, 2)` / no-match text | `serde_json::to_string_pretty` | differential byte comparison |

Differential: `tests/differential_session_tools.rs::search_current_session_history_tool_differential`
(match with context, assistant content excluded, pagination page 1/2, no match, invalid cursor).

## Gaps

- `renderCall`/`renderResult` need `Theme`.
- `String.prototype.toLocaleLowerCase` is ported as `str::to_lowercase` (Unicode default case
  mapping). Node's default locale on the pinned host is `en-US`; locale-specific case folding
  (e.g. Turkish `İ`) is an approved deviation already used by `pi-agent-core::search`.
