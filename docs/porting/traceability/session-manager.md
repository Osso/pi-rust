# coding-agent session-manager traceability

Pinned source: `packages/coding-agent/src/core/session-manager.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust module: `crates/pi-coding-agent/src/core/session_manager.rs`.
Focused tests: `crates/pi-coding-agent/tests/session_manager.rs` (39).
Differential: `crates/pi-coding-agent/tests/differential_session_manager.rs` (9) against `oracle/session-manager-oracle.ts`, requests `41000..=41070`.

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
| `isPersisted/getCwd/getSessionDir/usesDefaultSessionDir/getSessionId/getSessionFile/getMetadataControlDbPath` | accessors | same-named methods; used across tests |
| `readPersistedSessionSettings` | model/sandbox/thinking from control db; throws on control-db errors; empty provider/model ids count as unset (JS truthiness) | `read_persisted_session_settings` returns `Result`; `control_db_metadata_persists_model_and_thinking_level`, `control_db_metadata_errors_propagate`, `metadata_empty_paths_and_model_fields_use_js_truthiness` |
| `isSubagentSession/getSubagentName` | subagent flags | `is_subagent_session` / `get_subagent_name`; fork test |
| `getSessionGoalJson*`, `set/clearSessionGoalJson` | control-db goal payload, in-memory fallback, throws on control-db errors | `get_session_goal_json` / `get_session_goal_json_for_session` / `set_session_goal_json` / `clear_session_goal_json` (all `Result`); `control_db_metadata_errors_propagate`, `metadata_empty_paths_and_model_fields_use_js_truthiness`, differential `41070` |
| `setSessionModel/ThinkingLevel` | control-db session settings, throws on errors | `set_session_model` / `set_session_thinking_level` (`Result`); `control_db_metadata_persists_model_and_thinking_level`, `control_db_metadata_errors_propagate` |
| `setSessionSandboxProfile` | recovery persist, metadata snapshot, then write/clear the per-session profile; `false` without a control db or session file | `set_session_sandbox_profile`; `set_session_sandbox_profile_persists_and_clears` |
| `setMetadataControlDbPath`, `restorePersistedMetadata`, `writeMetadataSnapshot` | metadata cache and incremental writes; throws on control-db errors; `cwdOverride` uses JS truthiness | `set_metadata_control_db_path` / `restore_persisted_metadata` / `write_metadata_snapshot`; metadata tests + `metadata_empty_paths_and_model_fields_use_js_truthiness` |
| `_appendEntry` → `writeMetadataSnapshot` when `affectsSessionInfo` | message appends refresh control-db metadata; historical session_info does not | `append_entry`; `appending_messages_writes_the_metadata_snapshot` |
| `relocate` | move default-dir file, preserve bytes/header, reject existing destination, relocate control-db rows, final metadata snapshot | `relocate`; `relocation_preserves_the_complete_jsonl`, `relocation_moves_control_db_rows` |
| `persistForRecovery` | write before first assistant | `persist_for_recovery`; exercised by branch dedup test |
| `appendMessage/ThinkingLevelChange/ModelChange/Compaction/CustomEntry/CustomMessageEntry/LabelChange` | leaf-child append, persist, id generation | same-named methods; tree/label/save-entry tests |
| `updateCompactionSummary` | trim, non-empty, drop provider-native fields, restore on write failure | `update_compaction_summary`; two focused tests |
| `getLeafId/getLeafEntry/getEntry/getChildren/getLabel/getBranch/buildContextEntries/buildSessionContext/getHeader/getEntries/getTree` | traversal and defensive copies | same-named methods; tree/label tests |
| `branch/resetLeaf/branchWithSummary/createBranchedSession` | leaf pointer, branch summary, path extraction with label rewiring | same-named methods; `tree_traversal_and_branching`, `branched_session_*` tests |
| `SessionManager.list/listArchived/listAll` | metadata-backed fast path (control-db order, cwd/dir/archive scope, subagent exclusion), else directory scan plus metadata cache, cwd filter, subagent exclusion, modified-desc sort | `list` / `list_archived` / `list_all`; `lists_sessions_scoped_by_cwd`, `metadata_backed_listing_prefers_control_db_and_excludes_subagents`, `metadata_listing_scopes_by_cwd_and_returns_control_db_order` |
| `SessionManager.open` | resolves the header before construction; a persisted header keeps id/cwd/parent, a missing or empty file regenerates identity like the source | `open`; `open_preserves_a_freshly_persisted_header`, `reads_files_and_headers`, `rewrites_empty_files_and_rejects_non_session_files` |
| `getDefaultSessionDirPath` private helper | resolved cwd + agent dir | `get_default_session_dir_path` (public in Rust) |

## Gaps

- `SessionInfo.created`/`modified` are epoch milliseconds (`i64`) instead of JavaScript `Date`; invalid timestamps map to `0` instead of an invalid Date.
- Listing loads session files sequentially; TypeScript uses at most 10 concurrent loads. Observable only through progress-callback interleaving.
- `truncateMessageToFit` measures JavaScript UTF-16 lengths. Rust truncates at whole-char boundaries, so a cut inside a surrogate pair yields one fewer character than `String#slice` would; the source data (text content) does not rely on lone surrogates.
- Typed boundary: a `type:"session"` first line whose header fields do not match `SessionHeader` (for example a non-string `timestamp`) is rejected, where TypeScript only requires `type` and string `id`. Unknown *entry* types are preserved as raw JSON.

## Discrepancies

- `AgentSession`'s approval reviewer reads the goal inside an `Option`-returning hook, so it cannot reject like the source. A control-db goal read error blocks the tool call with the error message (fail closed) instead of rejecting the hook promise.
- `open` on a session path whose header was never persisted (missing or empty file) regenerates the session id and drops the metadata name exactly like the source; callers that need identity must persist the header first (`persistForRecovery`). The agents-core attach test now persists like the TypeScript test and asserts the stable id/name.

- `SessionHeader` records the parsed JSON key order and replays it when serializing, so migrated v1 headers append `version` exactly like TypeScript. `serde_json::Map` uses swap-removal, so the legacy `firstKeptEntryIndex` conversion uses `shift_remove` to preserve TypeScript's deletion order.
- `migrateToolResultSessionFiles` traversal order differs (stack pop vs. deterministic walk); the report counters are order-independent.

## Metadata-authoritative names (approved 2026-10-20)

Name paths use the [common source lock](../source-lock.json); former c248b0067 approval to re-validate against 4f9e6f8a4.
`set_session_name` normalizes and writes metadata, `get_session_name` returns cached metadata
state, and `has_session_name_state` distinguishes never named from explicitly cleared.
`appendSessionInfo` is removed; `SessionInfoEntry` remains parseable history only.

| Branch | Tests in `session_manager.rs` |
|---|---|
| Compacted JSONL names ignored; no pre-cutoff session-info retention | `summarized_jsonl_session_info_is_not_authoritative` |
| Restore metadata name and relocated cwd independently of active slice | `restores_the_sqlite_session_name_without_loading_pre_cutoff_session_info` |
| Restore explicit clear and disable autoname | `restores_an_explicit_name_clear_without_making_the_session_autoname_eligible` |
| Reset then rehydrate names on file switch | `rehydrates_the_sqlite_session_name_when_switching_files` |
| Fork/new session clears cached name state | `does_not_carry_a_session_name_into_a_branched_session` |

Source consequence: agents-core path-based attach opens a manager without attaching its metadata
DB; the name therefore stays unset and display name uses `Session <id>`. This slice does not add
a new metadata lookup to that unchanged source path.

Empty-file initialization returns after `new_session`/header recovery without rehydrating stale metadata, matching the source early return. RED→GREEN proof: `opening_an_empty_file_resets_names_without_restoring_stale_metadata`.

## Frozen4f9e6f8a4 storage delta (pending whole-module proof)

Archive-aware header/load/list/fork; mutable open/setSessionFile resolves stale path before guard; openReadOnly loads/migrates/indexes in memory without metadata writes. Message receipt field retains raw JSON including null and omission. Existing persist_for_recovery remains available; fsync/typed append integration pending. Codec/fork trace: repin_session_storage. No crash-safe runtime delivery claim.

Receipt proof: source creates real records, mutates exposed entry refs to null/empty/string/object/valid arrays, persists; Rust retained compaction rewrite; same TS reads exact raw values. New typed append + persist_for_recovery + fsync consumed by same frozen acceptance scan; initial RED scan[] missed ref. No DB claim/CAS proof.

Bounded reverse scan now defers inactive payload loading until an uncompacted tree requires forward full read. Resource behavioral RED: real TS manager writes24 inactive3.5MiB Unicode records; fresh Rust child VmHWM delta114380KiB exceeded64MiB cap. Frozen source uses hasInactiveEntries, not retained inactive records. Compacted manager must retain only active cutoff plus chunk/largest-record memory.

Late truncation mutation: TS-generated210KiB Unicode archive minus final2bytes → frozen header null; prior Rust direct first-line stream returned header. Header now uses seekable full decode validation, and manager/fork share one outer spool before plaintext scan. Test `late_truncated_archive_header_does_not_hide_decode_failure`.

Full loader is now fallible (`Result<Vec<FileEntry>,SessionManagerError>`), missing path only yields empty; IO/decode errors propagate. Plain line streaming64/1MiB boundaries, archive uses full seekable spool; malformed records skipped. `load_session_context_entries` exports source header/context pair and requires existing actual filename; dynamic fixture tests source stale alias error vs explicit archive success without classifying it a defect. Parent-agent journal compile adapter propagates new error; old-pin-only journal still scheduled removal by owning lane.

Pin-only summary accounting removed: CompactionEntry/BranchSummaryEntry now preserve unknown incoming properties through generic flattened extra map; no typed usage field or append option/branch argument. `unknown_legacy_summary_usage_survives_without_a_typed_accounting_contract` source persists string-valued unknown usage; prior typed parse made compaction unknown and rewrite rejected it (RED). Authorized minimal AgentSession coupling removal drops branch-summary stats special case and summary usage propagation; tool-result/other stats remain unmodified and not claimed parity.

Mechanical source-absent read adaptation: footer add_entry_usage no longer reads typed compaction/branch-summary usage (two arms removed); frozen footer115–124 assistant-only loop is evidence. Existing tool-result accounting arm intentionally remains pending owning accounting lane.

Mutable suffix guard now uses source archive predicate `.zst` (not only `.jsonl.zst`); actual source mutable open of missing arbitrary.zst rejects, prior Rust allocated identity (RED). Read-only generic-zst recursion not classified or duplicated.

Open raw-wire regression: source-generated escaped lone-surrogate mailbox path remains a message in TS; current Value decoding drops it (source_escaped_surrogate_receipt_remains_a_persisted_message RED). Not covered by session-name waiver; narrow lossless representation pending.

Audit correction via dynamic frozen API: filesystem list/listAll/continue-recent scan only.jsonl (source1240,1277); archive-aware exported buildSessionInfo is distinct from resume population. Restored plain-only filename filters; public build_session_info decodes explicit archives. `uncompressed_inactive_tree_and_archived_list_context_are_source_equivalent` covers plain+archived tree/context, explicit info and filesystem list; initial RED incorrectly widened Rust list returnedarchive vs TS[]. No source bug/waiver.

Narrow raw-wire implementation: receipt values and unknown summary properties are extracted from existing lossless JsValue AST before checked serde decode of remaining typed record; generic extra keys retain JsString, including unpaired UTF16. Known-entry and summary serializers emit lossless RawValue tokens; persistence and HTML payload serialize directly then normalize losslessly. No global TextContent/details/name representation change. Native raw field wrapper replaces Option<Value>; typed append signature unchanged. Existing serdeValue direct projections cannot represent unpaired strings and fail rather than corrupting them.

Lossless regression proofs now cover source escaped receipt load/append/rewrite/fork and source unknown unpaired summary keys/values rewrite/fork; archive HTML export payload parsed by same TS retains exact receipt tokens. Focused20-test batch at ef263439+new fixtures:19 passed; one cleanup test observed concurrent readers tempdirs, not own leak. Probe now runs in its own child TMPDIR to measure only its own lifecycle. Hard90 worker proof remains valid (spool source unchanged).
