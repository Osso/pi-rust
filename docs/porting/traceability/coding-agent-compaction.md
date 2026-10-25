# Traceability: `packages/coding-agent/src/core/compaction/compaction.ts` and `index.ts`

## Common-pin acquisition correction (2026-10-22)

Frozen `compaction.ts:697–708` has no summarization retry. Acquisition and `stream.result()` rejection now propagate; removed the old retry/callback API, generated request session IDs and fabricated assistant-error path. [Exact bounded proof and caller coverage](stream-acquisition.md). Existing usage-returning fields remain outside this bounded correction. Evidence below is historical and does not authorize removed APIs or claim new-pin whole-module parity.

Rust: `crates/pi-coding-agent-tools/src/core/compaction.rs` (with
`compaction/utils.rs`, `compaction/branch_summarization.rs`). Tests:
`tests/compaction.rs`, `tests/compaction_generation.rs`,
`tests/differential_compaction.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

The coding-agent copy diverges from `packages/agent/src/harness/compaction/compaction.ts`
([agent compaction](compaction.md)): `SessionEntry` instead of `Entry`, budget +
protected-suffix cut-point selection, goal-reminder filtering, provider-native
materialization, `source` metadata, and a different `shouldCompact` overload.
Bodies that are identical are reused: `estimateTokens`, `estimateContextTokens`,
`calculateContextTokens`, `CompactionSettings`, `DEFAULT_COMPACTION_SETTINGS`,
`ContextUsageEstimate`, and file-operation helpers.

## Exported surface

| TypeScript | Rust | Tests |
|---|---|---|
| `CompactionDetails` | `CompactionDetails` | `compacts_with_file_operations_and_source_details` |
| `CompactionSourceInfo` | `CompactionSourceInfo`, `CompactionSourceType` (`local` / `openai_remote`) | same; `value_round_trips_through_serde` |
| `CompactionResult<T>` | `CompactionResult<T = Value>` | same |
| `CompactionSettings` | re-export from `pi_agent_core` | `decides_compaction_from_settings_and_explicit_threshold`; differential |
| `DEFAULT_COMPACTION_SETTINGS` | re-export | differential fixtures |
| `calculateContextTokens` | re-export `pi_ai::utils::estimate::calculate_context_tokens` | `calculates_total_context_tokens_from_usage`; differential |
| `getLastAssistantUsage` | `get_last_assistant_usage` (SessionEntry) | 3 tests; skipped by differential |
| `ContextUsageEstimate` | re-export `pi_agent_core::...::ContextUsageEstimate` | `estimates_context_from_last_non_zero_usage_anchor`; differential |
| `estimateContextTokens` | re-export `estimate_context_tokens` | same |
| `shouldCompact` | `should_compact(..., Option<f64>)` | threshold tests; differential (5 cases) |
| `estimateTokens` | re-export `estimate_tokens` | `estimates_message_tokens_with_chars_over_four`; differential (10 roles) |
| `CutPointResult` | `CutPointResult` (`turn_start_index: i64`, `-1` = none) | cut-point tests; differential |
| `findTurnStartIndex` | `find_turn_start_index` | `finds_turn_start_index_...`; differential |
| `findCutPoint` | `find_cut_point` | 8 cut-point tests; differential (4 fixtures) |
| `completeSummarization` | `complete_summarization` (+ `..._without_callbacks`) | every generation test uses it; retry test |
| `GenerateSummaryWithUsageOptions` | `GenerateSummaryWithUsageOptions<'a>` | prompt/usage/retry tests |
| `MaterializeCompactionSummaryResult` | `MaterializeCompactionSummaryResult` (`Aborted` / `Summary`) | 4 materialization tests |
| `MaterializeCompactionSummaryOptions` | `MaterializeCompactionSummaryOptions<'a>` | same |
| `materializeCompactionSummary` | `materialize_compaction_summary` | same |
| `generateSummary` | `generate_summary` | exact-request-shape, retry, error tests |
| `generateSummaryWithUsage` | `generate_summary_with_usage` | previous-summary/custom-focus test |
| `CompactionPreparation` | `CompactionPreparation` | 8 preparation tests; differential (6 fixtures) |
| `prepareCompaction` | `prepare_compaction` | same |
| `compact` | `compact` | 4 compact tests |
| `index.ts` re-exports | module `pub use` block | all of the above |

## Internal branches

| TypeScript | Rust | Notes |
|---|---|---|
| `extractFileOperations` | `extract_file_operations` | Skips `fromHook` compaction details; adds tool-call paths. |
| `getMessageFromEntry` | `get_message_from_entry` | message / custom_message / branch_summary / compaction (`durationMs` only). |
| `getMessageFromEntryForCompaction` | `get_message_from_entry_for_compaction` | Drops compaction entries and extension goal reminders (`Work toward…`, `Continue working toward…`). |
| `combineUsage` | `combine_usage` | Optional `cacheWrite1h`/`reasoning` only when either side present. |
| `estimateEntryTokens` | `estimate_entry_tokens` | |
| `findContextMessageCountCutIndex` | `find_context_message_count_cut_index` | 3 protected messages. |
| `findTokenBoundedCutIndex` | `find_token_bounded_cut_index` | 10 000 protected tokens. |
| `findNearestCutPointAtOrAfter` | `find_nearest_cut_point_at_or_after` | Falls back to the requested index (diverges from agent copy). |
| `findProtectedSuffixStartIndex` | `find_protected_suffix_start_index` | |
| `isTurnStartEntry` / `isActiveTurnContinuationEntry` | same names | branch/custom messages start turns; assistant/tool results continue. |
| `findActiveTurnFloorIndex` | `find_active_turn_floor_index` | |
| `findProtectedSuffixCutIndex` | `find_protected_suffix_cut_index` | Applies active-turn floor only when an earlier cut point exists. |
| `findBudgetCutIndex` | `find_budget_cut_index` | Uses `sessionEntryToContextMessages` sums; `keepRecentTokens`. |
| `findValidCutPoints` | `find_valid_cut_points` | Skips compaction; custom messages and `isCutPointMessage` entries. |
| `includeAdjacentMetadata` | `include_adjacent_metadata` | Extends the kept suffix over non-context entries. |
| `describeCutPoint` | `describe_cut_point` | |
| `createSummarizationOptions` | `create_summarization_options` | `maxTokens`, signal, api key, headers, env. |
| `getSummarizationMaxTokens` | `get_summarization_max_tokens` | `floor(0.8 * reserveTokens)` capped at `model.maxTokens`. |
| `buildCompactionMaterializationMessages` | `build_compaction_materialization_messages` | Checkpoint (with `providerNative`) + materialization instruction. |
| `readCompactionMaterializationResponse` | `read_compaction_materialization_response` | Aborted / error text / non-empty text-join. |
| `buildSummaryPrompt` / `buildSummaryRequest` | same names | Exact prompts; no 300 000-char cap (coding-agent source has none). |
| `generateSummaryWithOptions` | `generate_summary_with_options` | `Summarization failed: …` message. |
| `buildHistorySummaryOptions` | `build_history_summary_options` | |
| `buildTurnPrefixSummaryRequest` | `build_turn_prefix_summary_request` | `floor(0.5 * reserveTokens)`. |
| `interpretTurnPrefixSummaryResponse` | `interpret_turn_prefix_summary_response` | `Turn prefix summarization failed: …`. |
| `generateSplitTurnSummary` | `generate_split_turn_summary` | `history\n\n---\n\n**Turn Context (split turn):**\n\nprefix`. |
| `generateCompactionSummary` | `generate_compaction_summary` | |
| `completeSummarization` request | `complete_request` | Uses `streamFn` when supplied, otherwise `completeSimple`; stream failures become error assistant messages so retry sees them. |

## Prompt constants

`SUMMARIZATION_PROMPT`, `UPDATE_SUMMARIZATION_PROMPT`,
`COMPACTION_MATERIALIZATION_PROMPT`, and `TURN_PREFIX_SUMMARIZATION_PROMPT` are
module-private and copied byte-for-byte. `generate_a_summary_with_the_exact_request_shape`
asserts the full summarization prompt; split-turn and materialization tests
assert the other two; branch tests assert the shared system prompt.

## Gaps / discrepancies

- `getLastAssistantUsage` is not exercised by the differential oracle (it takes
  `SessionEntry[]`; the behavioural tests cover it).
- `completeSummarization` has no oracle: it performs network/stream I/O. It is
  covered behaviourally through scripted `StreamFn`s (request shape, cache
  retention `none`, fresh session id, retry, abort, error mapping).
- No source bugs found in this module.

2026-10-25: removed the frozen-absent AI text helper; the two summary paths now exclude whitespace-only text exactly as pinned compaction.ts:720–723/768–771. Pinned branch-summarization.ts:363–366 does not exclude whitespace, and that behavior is retained. Behavioral whitespace/U+FEFF RED→GREEN is in `compaction_generation::generates_a_summary_with_the_exact_request_shape`.

## Re-pin percentage threshold (2026-10-25)

Parent assigned this module and coding-agent branch-summarization to the agent-session lane. Frozen :157–172/268–280 defines a coding-agent-specific `CompactionSettings` with optional `thresholdPercent`; harness settings are a separate three-field interface and remain unchanged. Native coding-agent settings now preserve the percentage, validate it before checking `enabled`, and apply percentage ≥ boundary before model `autoCompactionThreshold` ≥ boundary before legacy reserve-token strict > boundary. `should_compact` is fallible; session consumers propagate failures and forward the model threshold. Session settings projection retains the percentage. Existing coding-agent fixture literals and the extension preparation carrier were updated mechanically. `differential_should_compact` adds inclusive percentage boundary, model precedence and disabled cases; actual-frozen RED `/tmp/repin-compaction-threshold-red.log` proves three mismatches in the old predicate. Whole-module audit is not yet closed.

## Re-pin protected-suffix cut point (2026-10-25)

Frozen :543–585 ignores `_keepRecentTokens` and selects only the protected suffix; removed native's old budget-cut pass and max(budget, protected) selection. Existing common-pin `differential_find_cut_point_and_turn_start` reproduced parent's baseline failure: frozen retains index2 while native retained index4 for a large custom message and keepRecentTokens1 (`/tmp/repin-compaction-cut-red.log`). Added a custom-turn-start vector: frozen returns index2/start2/splittrue; native returned start-1/splitfalse (`red2.log`). Native now tests only an actual user message for the special nonsplit result, as frozen does. DISCREPANCY: a selected custom/branch/bash turn start is itself marked split with turnStartIndex equal to cutIndex; source behavior is preserved literally. Diagnostic-only oracle probes were removed without source changes.

## Re-pin generation request/result contract (2026-10-25)

New injected-stream frozen differential (`suite_session_1::differential_compaction_generation`) compares complete captured model/context/options and returned values/errors for history/update/blank focus, split-turn summaries and empty output. RED `/tmp/repin-compaction-generation-red.log` showed missing continuation guidance, old `Additional focus` wording and native-only usage result fields. Ported frozen :660–679 instructions verbatim, trimmed optional focus and previous-summary truthiness, passed custom focus to turn-prefix summarization, and made empty history/prefix text fail with frozen messages. Removed `SummaryWithUsage`, `generate_summary_with_usage`, usage aggregation and `CompactionResult.usage`; options are now `GenerateSummaryOptions` and generation returns only text. Removed the old public root export. Native result serialization no longer has a hidden null-on-details-serialization failure. Test-clock-only request timestamps and compaction durations normalize to zero; maxTokens uses JS-number serialization, not field removal. Mechanical fallout: remote-compaction result constructor, interactive test result constructor, extension result fixtures, acquisition/lossless fixture option names. No new dependency or compatibility alias.

The update-summary template now also matches frozen622–658: retain only still-current goals/context and remove superseded state. The first generation GREEN probe exposed the older PRESERVE-all template; branch/split cases already passed. The same complete request comparison covers the corrected update template.

Added ten actual-frozen numeric percentage validation/enabled combinations (0/-1/101 reject even when disabled;0.5/100 accepted), plus preparation-settings round-trip for37.5%. These supplement the inclusive-boundary and percentage/model/reserve precedence vectors.

Focused helper batch after the source changes:36/37 passed; the sole failure was the old budget unit fixture, whose keepRecentTokens1/2 expectations contradicted frozen protected-suffix behavior. It now asserts exact index1/start0/splittrue for both budgets plus full-result equality. Generation batch15/15 passed. An initial branch filter selected zero tests in suite_session_1 and is not evidence; native branch tests belong to suite_core_1.
