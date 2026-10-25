# coding-agent session-manager traceability

## Whole-module re-pin audit (2026-10-25, in progress)

[Current proof ledger](../session-manager-repin-proof.md). Fresh frozen-source RED/GREEN covers removed-file append (Node append creates, without inventing a replacement header), empty control-DB sandbox truthiness, assignment-before-archive-guard even for in-memory managers, and original EISDIR/open and ENOTDIR/mkdir/error records. Reciprocal lifecycle proof compares JSONL byte strings (only allocated IDs/clocks substituted), imported bytes unchanged, tree/labels/settings/context, compaction update, TS reads of Rust appends, and all/none/from-entry forks. Initial-write EEXIST and fork's existing-directory guard were additional REDs. Remaining full-row proof and final checks are pending; historical partial claims below are not row closure.

AssistantMessage key-order deviation2026-10-23 ([AGENTS.md](../../../AGENTS.md):40): newly native-created assistants persist in the native fixed serializer order, with exact values/presence; frozen order varies by construction site. This does not waive raw TS-written records, whose key order remains exact. The timeout RPC process test compares actual newly written aborted records semantically and round-trips the actual TS-written aborted line byte-exactly (q15-thinking-timeout-rpc-persistence-approved-green.log). Other entry/JSON ordering contracts below remain unchanged.

Historical baseline: `packages/coding-agent/src/core/session-manager.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`. Current target is the [common source lock](../source-lock.json); only the explicitly recorded frozen4f9 deltas below have renewed bounded evidence, not the entire historical matrix.

## Shared native process cwd source (2026-10-23)

`current_cwd` now uses the [shared Node process authority](process-cwd.md) through paths::process_cwd and retains PathError→ConfigError→SessionManagerError sources. The former `.message` projection lost ENOENT/errno/uv_cwd metadata in a cold deleted directory; genuine frozen/source process RED→GREEN covers that boundary. Native now reads cwd directly after each deletion, including a warmed directory; Node's cached previous-path behavior is an approved gap. Broader resolve/normalize error projections are not accepted by this bounded helper proof.

## Common-pin truthiness audit (2026-10-23)

Bounded renewal at the [common pin](../source-lock.json), supporting the [session cwd re-port](session-cwd.md). This is not whole-module acceptance. `tests/session_manager_truthiness.rs` compares 14 real-filesystem cases against `oracle/session-manager-truthiness-oracle.mts`, which invokes the unmodified source methods.

| Frozen site | Correction / retained distinction | Proof |
|---|---|---|
| create/open/continueRecent/forkFrom/list sessionDir truthiness | empty strings choose the frozen default/parent directory; list's default path lookup does not create a directory | independent create/open/continue/fork/list REDs; constructed directory, actual continued ID, written fork and listing contents; missing-root listing checks no mkdir |
| listAll custom directory after normalization | normalized empty string follows default-root listing | actual default-root fixture returned instead of empty list, RED/GREEN |
| listArchived control-db and directory truthiness | empty DB returns before invalid directory normalization; empty directory leaves metadata unscoped | invalid file-URL directory + empty DB RED/GREEN |
| findMostRecentSession optional cwd | empty cwd does not enable filtering | unrelated actual process cwd versus stored fixture cwd, RED/GREEN |
| listMetadataSessions/cacheSessionMetadata/excludeKnownSubagentSessions | empty DB skips scope resolution/database work like frozen | public real-filesystem listing with empty DB retained in matrix; shared lower DB layer already tolerated empty paths, so no independent helper-only RED is claimed |
| open post-constructor cwdOverride | empty value does not perform the frozen truthy-only second assignment | empty selected cwd and reopen metadata differential; constructor nullish selection remains distinct |
| forkFrom fromEntryId | empty string copies all history, not only the latest branch path | two independent root branches: native formerly copied only entry-two, frozen copied both; RED/GREEN |
| setSessionFile precedingCwd | apply only truthy preceding cwd; delete native header-cwd fallback | direct setter RED preserves original cwd against a different header cwd; removing fallback also restores empty-override behavior |
| intentionally non-truthy sites | openReadOnly sessionDir uses nullish presence; id validates even empty; filterCwd keeps original !==undefined test; subagentName trim and metadata cwdOverride already matched | read-only empty-directory control case; existing ID/subagent/metadata tests retained; no blanket Some-to-truthy rewrite |

Logs: `session-manager-truthiness-{red,branched-red,green}.log`, `session-manager-set-file-cwd-red.log`, `session-manager-leading-blank-green.log` under `/tmp/pi-rust-repin-lossless-interactive/`. The development run records 48 existing tests GREEN plus one obsolete leading-blank cwd assertion; its corrected frozen expectation passed separately, without removing an assertion or rerunning the unchanged 48 tests.

**Frozen discrepancy classified: preserve for parity.** A leading-blank file is accepted by the full loader, but the initial first-physical-line header probe returns no header; open therefore retains process cwd rather than the later header cwd. `open-leading-blank` in the real-source matrix establishes this behavior. The old native fallback and old `reads_files_and_headers` expectation silently adopted the later header cwd; both are reconciled to the frozen behavior. No source delta is introduced.

Rust module: `crates/pi-coding-agent-storage/src/core/session_manager.rs`.
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
| `SessionInfo` | path/id/cwd/name/archived/parent/created/modified/counts/firstMessage only | `SessionInfo` (`created`/`modified` are epoch ms); `session_info_listing_uses_message_timestamps_and_skips_extension_prompts` |
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
| `SessionManager.list/listArchived/listAll` | metadata-backed fast path (control-db order, cwd/dir/archive scope, subagent/resident exclusion); populated-but-scoped-empty index returns empty without scanning/cache; only absent/empty index permits directory scan plus metadata cache, cwd filter, subagent/resident exclusion, modified-desc sort | `list` / `list_archived` / `list_all`; `lists_sessions_scoped_by_cwd`, `metadata_backed_listing_prefers_control_db_and_excludes_subagents`, `metadata_listing_scopes_by_cwd_and_returns_control_db_order` |
| `SessionManager.open` | resolves the header before construction; a persisted header keeps id/cwd/parent, a missing or empty file regenerates identity like the source | `open`; `open_preserves_a_freshly_persisted_header`, `reads_files_and_headers`, `rewrites_empty_files_and_rejects_non_session_files` |
| `getDefaultSessionDirPath` private helper | resolved cwd + agent dir | `get_default_session_dir_path` (public in Rust) |

## Gaps

- `SessionInfo.created`/`modified` are f64 Date time values, including NaN. Invalid dates survive construction; ISO conversion rejects them only at the frozen write boundary. Metadata reads no longer replace invalid dates with epoch0; Date/NaN sorting uses pinned V8 TimSort. [Full Date consumer/line mapping and proof](session-info-dates.md).
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

Archive-aware header/load/explicit-info/fork; filesystem resume discovery remains plain-only; mutable open/setSessionFile resolves stale path before guard; openReadOnly loads/migrates/indexes in memory without metadata writes. Message receipt field retains raw JSON including null and omission. Existing persist_for_recovery remains available; fsync/typed append are available and proved against the frozen receipt scanner; runtime claim/CAS integration remains owner work. Codec/fork trace: repin_session_storage. No crash-safe runtime delivery claim.

Receipt proof: source creates real records, mutates exposed entry refs to null/empty/string/object/valid arrays, persists; Rust retained compaction rewrite; same TS reads exact raw values. New typed append + persist_for_recovery + fsync consumed by same frozen acceptance scan; initial RED scan[] missed ref. No DB claim/CAS proof.

Bounded reverse scan now defers inactive payload loading until an uncompacted tree requires forward full read. Resource behavioral RED: real TS manager writes24 inactive3.5MiB Unicode records; fresh Rust child VmHWM delta114380KiB exceeded64MiB cap. Frozen source uses hasInactiveEntries, not retained inactive records. Compacted manager must retain only active cutoff plus chunk/largest-record memory.

Late truncation mutation: TS-generated210KiB Unicode archive minus final2bytes → frozen header null; prior Rust direct first-line stream returned header. Header now uses seekable full decode validation, and manager/fork share one outer spool before plaintext scan. Test `late_truncated_archive_header_does_not_hide_decode_failure`.

Full loader is now fallible (`Result<Vec<FileEntry>,SessionManagerError>`), missing path only yields empty; IO/decode errors propagate. Plain line streaming64/1MiB boundaries, archive uses full seekable spool; malformed records skipped. `load_session_context_entries` exports source header/context pair and requires existing actual filename; dynamic fixture tests source stale alias error vs explicit archive success without classifying it a defect. Parent-agent journal compile adapter propagates new error; old-pin-only journal still scheduled removal by owning lane.

Pin-only summary accounting removed: CompactionEntry/BranchSummaryEntry now preserve unknown incoming properties through generic flattened extra map; no typed usage field or append option/branch argument. `unknown_legacy_summary_usage_survives_without_a_typed_accounting_contract` source persists string-valued unknown usage; prior typed parse made compaction unknown and rewrite rejected it (RED). Authorized minimal AgentSession coupling removal drops branch-summary stats special case and summary usage propagation; tool-result/other stats remain unmodified and not claimed parity.

Mechanical source-absent read adaptation: footer add_entry_usage no longer reads typed compaction/branch-summary usage (two arms removed); frozen footer115–124 assistant-only loop is evidence. Existing tool-result accounting arm intentionally remains pending owning accounting lane.

Mutable suffix guard now uses source archive predicate `.zst` (not only `.jsonl.zst`); actual source mutable open of missing arbitrary.zst rejects, prior Rust allocated identity (RED). Read-only generic-zst recursion not classified or duplicated.

Historical raw-wire RED: source-generated escaped lone-surrogate mailbox path remained a message in TS while Value decoding dropped it. Resolved by the narrow lossless receipt/summary-extra parser and subsequent legacy Unknown carrier; not covered by the session-name waiver.

Audit correction via dynamic frozen API: filesystem list/listAll/continue-recent scan only.jsonl (source1240,1277); archive-aware exported buildSessionInfo is distinct from resume population. Restored plain-only filename filters; public build_session_info decodes explicit archives. `uncompressed_inactive_tree_and_archived_list_context_are_source_equivalent` covers plain+archived tree/context, explicit info and filesystem list; initial RED incorrectly widened Rust list returnedarchive vs TS[]. No source bug/waiver.

Narrow raw-wire implementation: receipt values and unknown summary properties are extracted from existing lossless JsValue AST before checked serde decode of remaining typed record; generic extra keys retain JsString, including unpaired UTF16. Known-entry and summary serializers emit lossless RawValue tokens; persistence and HTML payload serialize directly then normalize losslessly. No global TextContent/details/name representation change. Native raw field wrapper replaces Option<Value>; typed append signature unchanged. Existing serdeValue direct projections cannot represent unpaired strings and fail rather than corrupting them.

Lossless regression proofs now cover source escaped receipt load/append/rewrite/fork and source unknown unpaired summary keys/values rewrite/fork; archive HTML export payload parsed by same TS retains exact receipt tokens. Focused20-test batch at ef263439+new fixtures:19 passed; one cleanup test observed concurrent readers tempdirs, not own leak. Probe now runs in its own child TMPDIR to measure only its own lifecycle. Hard90 worker proof remains valid (spool source unchanged).

`parseSessionEntries` public in-memory path now shares the same lossless receipt/summary-extra parser; expanded source escaped-receipt test caught old Value-only path dropping message (RED1 vs2). No raw receipts are routed through Value before extraction in owned manager/history entrypoint paths.

Legacyv1 actual source fixture drops entry IDs/parents before persistence. `source_legacy_v1_read_only_migrates_context_without_mutating_original` reproduced Rust empty context vs TS user message. Migrated Unknown records now reify their newly valid typed shapes before context/indexing; original bytes remain unchanged during read-only load. Legacy raw-field retention is covered by the subsequent receipt/summary export proofs below; no legacy/raw waiver inferred.

Legacy/raw carrier now retains ordinary Map fields plus lossless receipt/summary extras in UnknownSessionEntry. Legacy ID/role changes edit only the ordinary map; reification serializes directly then shares the lossless parser, never converts raw fields through Value. Exact e1f7ab6b legacy receipt export RED reused;78fe6f2e focused GREEN. Legacy summary export proof additionally kills the Unknown-extra-dropping mutant (null/null vs source escaped UTF16 key/value tokens); restored-carrier GREEN1785172c; the focused storage integration also passes30/30 (unchanged hard90 boundary excluded under prior valid proof). One session_tool_output Unknown(Map::new().into()) constructor adaptation; no global content type rewrite. Public generic serde Decode still uses checked Value and is not claimed lossless; production manager/history use parse_session_file_entry. Active tool-output migration now shares that parser (b514ca25; source fixture RED then GREEN4403306e). No production direct FileEntry/SessionEntry serde decode remains; KnownSessionEntry::from_value receives fields after lossless extraction.

Authorized frozen selector delta: SessionInfo has first_message only; removed source-absent all_messages_text and total-transcript accumulator. Counts/activity still scan all records; text extraction stops after first non-extension user prompt. Selector matches id/name/first_message/cwd exactly source line28. Actual-source differential not-later-message RED before production (e1f7ab6b), GREENf0675811.

Metadata listing now consumes canonical control API (c7248cac via0063fe60): absent table alone returns None; populated-but-filtered-empty returns Some(empty) without filesystem scan/cache. Scope resolves cwd and directory prefix exactly at consumer boundary; DB owns query/subagent/order semantics. Actual-source scoped-empty RED at0063fe60, GREEN12b382a9. No duplicated SQL or compatibility query path.

Bounded merged evidence and unresolved consumer/classification limits: [session storage integration](session-storage-integration.md). No fresh gate or whole-module promotion.

## Q18 record authority (in progress)

Imported headers and entry objects now keep one `JsObject` authority; typed known-entry
views are transient `Cow` projections. Header mutations preserve unknown UTF-16 keys,
explicit nulls and insertion order. Known record strings use `JsString`, including
imported identities, labels and timestamps. Dedicated regressions: `lossless_session_records`.
Initial exact-record harness RED: both raw fixtures were silently discarded by the old
scalar deserializers. Full module/integration proof remains pending; this is not new-pin
whole-module parity evidence. Path API integration and persistence round trips remain open.

Q18 persistence serializer now serializes the raw header directly. Tool-output capping
mutates a persistence-only raw object copy, retaining arbitrary entry/message/content
fields and raw keys unless the pinned truncation algorithm explicitly removes them.
Dedicated `lossless_session_persistence` tests reproduced empty raw-header output and
uncapped imported tool results before implementation. Integration proof remains pending.

Q18 imported identity uses UTF-16 lookup/index keys and transient typed projections.
`FileEntry::Json` retains valid JSON primitives/arrays without invented wrappers. Such
values are parse/serialization proof only, not typed runtime-record inputs.

The v1 migration now resolves `firstKeptEntryIndex` in the original single-pass mutation
order and deletes the legacy field only for compaction records; a pinned differential
reproduced the former two-pass mismatch with a future raw target ID. The pinned TS
collision Set is never populated, whereas Rust currently tracks generated IDs: this
source discrepancy is awaiting user classification, not new-pin parity evidence.

Raw imported common-role messages remain complete objects in the context projection;
this proof does not establish downstream agent/provider interpretation of those raw
objects. Main integration owns that consumer gate. Native path integration remains open.

### Q18 native identity/path integration and provider payloads

SessionManager's session identity, leaf/index/tree identities, paths and cwd are now
`JsString`; pure path operations use the native Node-compatible path helpers. Filesystem
arguments encode at the actual binding. Legacy metadata API encoding is blocked below. `SessionHeader.fields` and
imported entry objects remain single authorities through fork/rechain/write.

`lossless_session_manager_io` exercises TS-created raw paths and records → Rust load,
lookup, tree, context, fork and append → TS reads, plus archived `.jsonl.zst` reads via
the real internal spool worker. Its non-compaction/empty-summary mutation case reproduced
the wrong validation order; the Rust mutation now follows the pinned source ordering.

The compaction record's `provider_native` is raw `Option<Box<JsValue>>`. Context projection
passes the object unchanged, retaining raw provider/api tags, null duration and invalid
date serialization as null. A TS-created raw provider payload reproduced silent context
loss before this correction. Typed `ProviderNativeContext` derivation explicitly returns
an error for opaque surrogate-containing tags under the approved scalar-metadata gap;
actual provider-consumer error propagation remains main-owned integration proof.

User approved the collision-set fix on 2026-10-21: TS delta commit
`6573c7139c4b1ab2215f4fb2defd717072e37cbc`; frozen source remains unchanged. Rust retains
collision tracking and adds a deterministic whole-migration collision/payload regression.
Earlier "awaiting classification" notes above are historical, superseded by this approval.
Full Q18/module coverage is not claimed. Exact-module harness excludes metadata/default
live configuration and resident listing via panic-on-use stubs; no control-DB proof.

Q18 follow-up regressions preserve source empty-string leaf truthiness and ensure fork
rewiring changes only `parentId`, never synthesizing a missing imported `id`. The
deterministic migration test now asserts frozen duplicate-ID discrepancy separately
from exact approved-delta equality using `selectApprovedModule`. Tool capping compares
exact pinned serializer output, not just size. Compaction updates compare TS mutation
output and preserve raw state on filesystem rewrite failure.

Reproducible standalone setup: `python oracle/lossless-session-records-harness.py`;
manifest `/tmp/pi-rust-repin-lossless-session-records/full/crates/pi-coding-agent/Cargo.toml`.
Target only the three new dedicated integration tests and `record_identity_tests`;
worker location comes from `Q18_HARNESS_PACKAGE_DIR`. Metadata, resident listing and
default live configuration are intentionally excluded via panic-on-use collaborators.

Native parser input accepts `impl Into<JsString>` and splits/trims UTF-16 directly;
no UTF-8 conversion is needed for literal unpaired units inside JSON strings/keys.
The new differential proves this source-valid API input (the previous scalar API
rejected it at compile time). Tree timestamp comparison now keeps JavaScript's NaN
comparator equality instead of replacing invalid raw timestamps with epoch zero;
a two-sibling pinned differential reproduced and covers the ordering correction.

### Unapproved metadata-boundary blocker (main-owned handoff)

The legacy `session_control_db` API still takes scalar path/ID/text arguments and
scalar `WritableSessionMetadata` fields. Manager-side encoding at those calls occurs
before that module's pure normalization/error construction, earlier than the actual
SQLite binding. This is **not an approved gap** and is not raw-path/error parity. Main
will widen those shared APIs and remove caller-side encodings, preserving raw strings
until DB-open/rusqlite params. The exact call-site inventory and proof ledger live in
`/tmp/pi-rust-repin-lossless-session-records/REPORT.md` for integration handoff.

Final bounded proof at `7219e8ab`: 17 dedicated exact-module tests pass, including
NaN-comparator timestamp RED→GREEN. This excludes actual metadata/default config and
provider-consumer execution. Standalone compilation emits two unused-function warnings
from unexercised shared paths/transcript exports; no warnings were suppressed. Manual
Rust/JS readability audit used because `rust-code-analysis-cli` is unavailable; inherited
long orchestration functions remain, so no whole-module readability pass is claimed.

Reloaded `message` records (`SessionEntry::Unknown`) project through `project_agent_message`:
role-typed `AgentMessage` decode, keeping the exact object as `Custom` only when it does not fit a
`Message` variant (source messages are untyped). Regression from the raw-authority change made every
reloaded user/assistant message `Custom`. RED `reloaded_message_records_project_typed_roles` and
`interactive_child_view` (session-records-red.log), GREEN 6/6 + 1/1 (session-records-green.log).

Consumer projection contract (Q18): reopened records are always `SessionEntry::Unknown`, so no production consumer
matches `SessionEntry::Known` directly. Consumers read `entry.known()`, `as_message()` or `message()`; message
records whose `message` object does not fit a typed role project it as `AgentMessage::Custom` (the same rule as
`message()`), so `known()`/`as_message()` never hide a loaded message record. Converted consumers: compaction and
branch summarization, cache stats, agent-session tree navigation and detached completion, tree selector, interactive
footer/mode, default footer, effort, codex-fast, run-plan, goal rendering, agents-core runtime/session
factories/parent journal. Proof: `lossless_session_projections` (RED `session-projections-red.log`: reopened
`prepare_branch_entries` empty; GREEN `projection-green.log`).

## Fallible UUID consumer migration (2026-10-25)

Parent-approved mechanical prerequisite: source :280–286 generateId and :290/:340 migration throws propagate through Result. Native generate-id/collision loop, v1 migration, branch-label construction and append/open/load preserve their exact existing ordering and partial state; only crypto failure is newly representable. Original NativeNodeError is boxed through the existing SessionManagerError authority, not converted to text or routed uncaught. System UUID uses the shared [crypto error contract](../../specs/crypto-random-failures.md). Positive caller fixtures must assert the returned Result; checkpoint proof remains open during this signature migration.
