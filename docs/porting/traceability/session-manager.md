# coding-agent session-manager traceability

Pinned source: `packages/coding-agent/src/core/session-manager.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust module: `crates/pi-coding-agent/src/core/session_manager.rs`.
Focused tests: `crates/pi-coding-agent/tests/session_manager.rs` (34).
Differential: `crates/pi-coding-agent/tests/differential_session_manager.rs` (8) against `oracle/session-manager-oracle.ts`, requests `41000..=41062`.

The session file is the shared contract with TypeScript Pi. Request `41000` proves byte-identical serialization of every entry shape (including omitted optional fields, `parentId: null`, legacy `firstKeptEntryIndex`, unknown entry types, and multibyte content). Requests `41001..=41013` compare parsed entries, context messages, headers, and loaded entries as exact JSON strings in source order (the oracle performs no sorting or canonicalization). Requests `41020..=41030` prove cross-implementation reads: TS writes → Rust reads, Rust writes → TS reads, and Rust appends that TS reads back. Request `41060..=41062` proves interleaved TS/Rust appends on one file. The migration differential substitutes only the randomly generated ids in place, then compares exact serialized strings.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `CURRENT_SESSION_VERSION` | `3` | `CURRENT_SESSION_VERSION`; header writes in `persists_only_after_the_first_assistant_message`, differential `41000` |
| `MAX_KEPT_PRE_COMPACTION_BYTES` | `20_000` | `MAX_KEPT_PRE_COMPACTION_BYTES`; capped suffix cases in `drops_orphaned_tool_results_from_capped_pre_compaction_suffix` |
| `SessionHeader` | `type:"session"`, optional `version`/`parentSession`, `id`, `timestamp`, `cwd` | `SessionHeader`; missing optional fields are omitted (`41000`) |
| `NewSessionOptions` | optional id/parentSession, subagent flags | `NewSessionOptions`; `custom_session_ids_round_trip`, fork cases |
| Entry interfaces (`message`, `thinking_level_change`, `model_change`, `compaction`, `branch_summary`, `custom`, `custom_message`, `label`, `session_info`) | camelCase wire records with exact field order and omitted `undefined` | `KnownSessionEntry` + per-entry structs; differential `41000` byte comparison |
| `SessionEntry` / `FileEntry` | known entries plus preserved unknown shapes | `SessionEntry::{Known,Unknown}` / `FileEntry::{Header,Entry}`; unknown fixture in `41000` |
| `SessionContext` | `{messages, thinkingLevel, model}` | `SessionContext`; `builds_context_without_compaction` |
| `SessionInfo` | path/id/cwd/name/archived/parent/created/modified/counts/text | `SessionInfo` (`created`/`modified` are epoch ms); `session_info_listing_uses_message_timestamps_and_skips_extension_prompts` |
| `SessionTreeNode` | entry/children/label/labelTimestamp | `SessionTreeNode`; `tree_traversal_and_branching`, `labels_and_tree_nodes` |
| `assertValidSessionId` | anchored `[A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?` | `assert_valid_session_id`; `validates_session_ids` |
| `migrateSessionEntries` | v1→v2 id/parent chain + `firstKeptEntryIndex`; v2→v3 `hookMessage`→`custom` | `migrate_session_entries`; `migrates_v1_entries_and_is_idempotent`, differential `41050` |
| `parseSessionEntries` | trim, skip blank/malformed lines | `parse_session_entries`; `parses_entries_skipping_malformed_lines`, differential `41001` |
| `getLatestCompactionEntry` | last compaction in list | `get_latest_compaction_entry`; exercised by context builds |
| `sessionEntryToContextMessages` | message/custom_message/branch_summary/compaction projections; missing content array normalized | `session_entry_to_context_messages`; differential `41002..=41003` |
| `buildContextEntries` | leaf path, latest compaction, kept entries from `firstKeptEntryId` | `build_context_entries`; `builds_compacted_context_with_summary_and_kept_messages`, differential `41002..=41003` |
| `buildSessionContext` | path settings, capped kept messages, branch summaries | `build_session_context`; compaction/branch/orphan tests, differential `41002..=41003` |
| `getDefaultSessionDir` / `getDefaultSessionDirPath` | `--<cwd with /\\: → ->--` under `<agentDir>/sessions` | `get_default_session_dir` / `get_default_session_dir_path`; `default_session_dir_encodes_cwd`, differential `41013` |
| `loadEntriesFromFile` | full scan, malformed lines skipped, first entry must be a session header | `load_entries_from_file`; `reads_files_and_headers`, differential `41010` |
| `readSessionHeader` | first line only, `type:"session"` + string id | `read_session_header`; `reads_files_and_headers`, differential `41011` |
| `findMostRecentSession` | newest `.jsonl` by mtime, optional cwd filter | `find_most_recent_session`; `finds_most_recent_session_by_mtime_and_cwd`, differential `41012` |
| `SessionManager.create/open/continueRecent/inMemory/forkFrom` | persisted/in-memory construction, explicit ids, fork parent + active slice | constructors; `persists_only_after_the_first_assistant_message`, `continues_most_recent_session`, `forks_only_the_active_compacted_slice`, `fork_selection_all_none_and_from` |
| `setSessionFile` | reverse-scan v3 active slice; repair empty file; reject non-session file | `set_session_file`; `loads_active_compacted_slice`, `rewrites_empty_files_and_rejects_non_session_files`, `active_slice_errors_and_truncated_tail` |
| `newSession` | uuidv7 id or validated custom id, timestamped file name, no file until assistant | `new_session`; `custom_session_ids_round_trip`, persistence test |
| `isPersisted/getCwd/getSessionDir/usesDefaultSessionDir/getSessionId/getSessionFile/getMetadataControlDbPath` | accessors | same-named methods; used across tests (`getMetadataControlDbPath` blocked with the control-db gap) |
| `readPersistedSessionSettings` | model/sandbox/thinking from control db | `read_persisted_session_settings`; `control_db_metadata_persists_model_and_thinking_level` |
| `isSubagentSession/getSubagentName` | subagent flags | `is_subagent_session` / `get_subagent_name`; fork test |
| `getSessionGoalJson*`, `set/clearSessionGoalJson` | control-db goal payload, in-memory fallback | `get_session_goal_json` / `set_session_goal_json` / `clear_session_goal_json` |
| `setSessionModel/ThinkingLevel` | control-db session settings | `set_session_model` / `set_session_thinking_level`; metadata test |
| `setMetadataControlDbPath`, `restorePersistedMetadata`, `writeMetadataSnapshot` | metadata cache and incremental writes | `set_metadata_control_db_path` / `restore_persisted_metadata` / `write_metadata_snapshot`; metadata test |
| `relocate` | move default-dir file, preserve bytes/header, reject existing destination | `relocate`; `relocation_preserves_the_complete_jsonl`. Control-data relocation is blocked |
| `persistForRecovery` | write before first assistant | `persist_for_recovery`; exercised by branch dedup test |
| `appendMessage/ThinkingLevelChange/ModelChange/Compaction/CustomEntry/SessionInfo/CustomMessageEntry/LabelChange` | leaf-child append, persist, id generation | same-named methods; tree/label/save-entry tests |
| `updateCompactionSummary` | trim, non-empty, drop provider-native fields, restore on write failure | `update_compaction_summary`; two focused tests |
| `getLeafId/getLeafEntry/getEntry/getChildren/getLabel/getBranch/buildContextEntries/buildSessionContext/getHeader/getEntries/getTree` | traversal and defensive copies | same-named methods; tree/label tests |
| `branch/resetLeaf/branchWithSummary/createBranchedSession` | leaf pointer, branch summary, path extraction with label rewiring | same-named methods; `tree_traversal_and_branching`, `branched_session_*` tests |
| `SessionManager.list/listArchived/listAll` | directory scan, cwd filter, metadata-backed fast path, archived/subagent exclusion | `list` / `list_archived` / `list_all`; `lists_sessions_scoped_by_cwd`. Metadata path, archived listing, and subagent exclusion are **blocked** on `ca-control-db`; the file scan is used |
| `getDefaultSessionDirPath` private helper | resolved cwd + agent dir | `get_default_session_dir_path` (public in Rust) |

## Gaps

- Control-db metadata writes/reads are implemented, but metadata-backed `list`/`listAll`/`listArchived` and subagent exclusion still scan files. `relocate` moves the session file but not control-db rows (`relocate_session_control_data` is ported and unused). `setSessionSandboxProfile` persistence (writeSessionSandboxProfile/clearPersistedSessionSandboxProfile) is not wired here; sdk restores the profile read-only.
- `SessionInfo.created`/`modified` are epoch milliseconds (`i64`) instead of JavaScript `Date`; invalid timestamps map to `0` instead of an invalid Date.
- Listing loads session files sequentially; TypeScript uses at most 10 concurrent loads. Observable only through progress-callback interleaving.
- `truncateMessageToFit` measures JavaScript UTF-16 lengths. Rust truncates at whole-char boundaries, so a cut inside a surrogate pair yields one fewer character than `String#slice` would; the source data (text content) does not rely on lone surrogates.
- Typed boundary: a `type:"session"` first line whose header fields do not match `SessionHeader` (for example a non-string `timestamp`) is rejected, where TypeScript only requires `type` and string `id`. Unknown *entry* types are preserved as raw JSON.

## Discrepancies

- `SessionHeader` records the parsed JSON key order and replays it when serializing, so migrated v1 headers append `version` exactly like TypeScript. `serde_json::Map` uses swap-removal, so the legacy `firstKeptEntryIndex` conversion uses `shift_remove` to preserve TypeScript's deletion order.
- `migrateToolResultSessionFiles` traversal order differs (stack pop vs. deterministic walk); the report counters are order-independent.
