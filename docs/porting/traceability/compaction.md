# Traceability: `packages/agent/src/harness/compaction/compaction.ts`

Rust: `crates/pi-agent-core/src/harness/compaction/mod.rs`. Tests: `compaction.rs`,
`differential_compaction.rs`. Reused helpers live in `compaction/utils.rs` and
`harness/messages.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## Q18 native human-error consumer proof

Against frozen `4f9e6f8a4`, `lossless_provider_errors` proves raw failed/aborted provider completions retain native UTF16 with exact `Summarization failed: ` prefix and aborted identity, plus retry scheduled/finished callbacks. CompactionError.message is JsString; only Display encodes. Earlier module evidence below retains its original pin and is not upgraded by this bounded proof.

## Exported surface

| TypeScript | Rust | Tests |
|---|---|---|
| `CompactResult<T>` | `CompactResult` (`usage: Option<Usage>`, `details: Option<CompactionDetails>`, `duration_ms: Option<f64>`) | `returns_a_compaction_result_with_file_details`, differential `compact` |
| `CompactionPreparation` | `CompactionPreparation` | all `prepare_*` tests, differential `prepare_compaction` |
| `CompactionDetails` | `CompactionDetails` | `compaction_details.rs`, `returns_a_compaction_result_with_file_details` |
| `CompactionSettings` | `CompactionSettings` (`Clone + Copy + PartialEq`) | `compaction_settings.rs`, `checks_compaction_threshold` |
| `DEFAULT_COMPACTION_SETTINGS` | `DEFAULT_COMPACTION_SETTINGS` | `default_compaction_settings_match_the_source_values` |
| `ContextUsageEstimate` | `ContextUsageEstimate` | `context_usage_estimate.rs`, `estimates_tokens_and_context_usage_across_supported_message_roles` |
| `GenerateSummaryWithUsageOptions` | `GenerateSummaryOptions` (callbacks split into `*_with_callbacks`) | `generate_*` tests, differential `generate_summary` |
| `CutPointResult` | `CutPointResult` | `cut_point_result.rs`, cut-point tests |
| `calculateContextTokens` | `calculate_context_tokens` (re-exported from `pi_ai::utils::estimate`) | `calculates_total_context_tokens_from_usage`, differential |
| `completeSimpleWithRetries` | `complete_simple_with_retries` | exercised by every faux-backed test; retry path by branch differential |
| `getLastAssistantUsage` | `get_last_assistant_usage` | `estimates_tokens_...`, differential |
| `estimateContextTokens` | `estimate_context_tokens` | `estimates_tokens_...`, differential |
| `shouldCompact` | `should_compact` | `compaction_settings.rs`, `checks_compaction_threshold` |
| `estimateTokens` | `estimate_tokens` | `estimates_tokens_...`, differential (all roles) |
| `findTurnStartIndex` | `find_turn_start_index` | `covers_cut_point_and_turn_start_edge_cases`, differential |
| `findCutPoint` | `find_cut_point` | four suffix tests, edge cases, differential |
| `SUMMARIZATION_SYSTEM_PROMPT` | `SUMMARIZATION_SYSTEM_PROMPT` | `summarization_system_prompt.rs`, prompt differential |
| `generateSummary` | `generate_summary` (+ `generate_summary_with_callbacks`) | `preserves_the_string_result...`, differential |
| `generateSummaryWithUsage` | `generate_summary_with_usage` (+ `..._with_callbacks`) | prompt/usage/error tests, differential |
| `prepareCompaction` | `prepare_compaction` | six preparation tests, differential |
| `serializeConversation` (re-export) | `serialize_conversation` (re-export of `utils`) | `compaction_utils.rs`, differential |
| `compact` | `compact` (+ `compact_with_callbacks`) | five compact tests, differential |

## Internal branches

| TypeScript | Rust | Notes |
|---|---|---|
| `safeJsonStringify` | `utils::safe_json_stringify` | `serde_json::to_string`; `[unserializable]` on failure. |
| `extractFileOperations` | `extract_file_operations` | Prior compaction `details` plus message tool calls. |
| `getMessageFromEntry` | `get_message_from_entry` | Message, branch summary, compaction summary (`durationMs`); other entries `None`. |
| `getMessageFromEntryForCompaction` | `get_message_from_entry_for_compaction` | Compaction entries excluded. |
| `combineUsage` | `combine_usage` | Optional `cacheWrite1h`/`reasoning` preserved only when either side has them. |
| `getAssistantUsage` / `getLastAssistantUsageInfo` | `get_assistant_usage` / `get_last_assistant_usage_info` | Skips aborted/error and zero-token usage. |
| `estimateTextAndImageContentChars` | `pi_ai::utils::estimate::estimate_text_and_image_content_tokens` | Reused; image block counts 4800 chars. |
| `estimateEntryTokens` | `estimate_entry_tokens` | |
| cut-index helpers | `find_context_message_count_cut_index`, `find_token_bounded_cut_index`, `find_nearest_cut_point_at_or_after`, `find_protected_suffix_start_index`, `find_active_turn_floor_index`, `find_protected_suffix_cut_index`, `find_valid_cut_points`, `is_turn_start_entry`, `is_active_turn_continuation_entry` | 3-message / 10 000-token protected suffix, latest active turn floor. |
| `buildSummaryPrompt` | `build_summary_prompt` | Exact `SUMMARIZATION_PROMPT` / `UPDATE_SUMMARIZATION_PROMPT`, `<previous-summary>` block, 300 000-char conversation cap. |
| `buildSummaryRequest` | `build_summary_request` | `floor(0.8 * reserveTokens)` clamped to `model.maxTokens`; reasoning only for reasoning models with a non-`off` level. |
| `generateSummaryWithOptions` | `generate_summary_with_options` | Aborted/error message mapping. |
| `generateTurnPrefixSummary` | `generate_turn_prefix_summary` | `floor(0.5 * reserveTokens)` cap; split-turn suffix text. |
| virtual retained entries | `virtual_retained_entries` | Rebuilds previous retained tail as synthetic message entries. |

## Prompt constants

`SUMMARIZATION_PROMPT`, `UPDATE_SUMMARIZATION_PROMPT`, and
`TURN_PREFIX_SUMMARIZATION_PROMPT` are module-private constants copied byte-for-byte
from the source. Prompt text is proven by the differential `generate_summary` and
`compact` cases, which compare the exact strings sent to the model.

## Differential coverage

`differential_compaction.rs` compares against the pinned module:

- token estimates for user text/blocks, assistant text/thinking/tool calls, tool results with images, custom, bash execution, branch/compaction summaries, unknown roles;
- `calculateContextTokens`, `getLastAssistantUsage`, `estimateContextTokens`;
- cut points (alternating, huge suffix, 24 tool cycles, compaction boundary) and turn starts;
- six `prepareCompaction` fixtures including prior summary, retained tail, split turn, file details, compaction-last and empty;
- conversation serialization with tool-call JSON and truncated tool results;
- `generateSummaryWithUsage` prompt text, `maxTokens`, reasoning, cache retention, usage, and error/abort messages;
- `compact` split-turn usage combination, file details, max-token clamp, reasoning, error paths;
- `FileLists`, file-op formatting, and branch preparation.

## Discrepancies and typed exclusions

1. **Stream failure mapping (typed API).** `Models::complete_simple` returns
   `Result<_, StreamFailure>`; the TypeScript `completeSimple` rejects instead. The
   Rust `retry_assistant_call` producer is infallible, so a transport/auth failure is
   converted to an error `AssistantMessage` (`stopReason: "error"`, `errorMessage` =
   failure message). Unreachable through the faux provider; no TS test covers it.
2. **UTF-16 truncation.** `String.prototype.slice` counts UTF-16 units. Rust truncates
   by UTF-16 units too, but a boundary that splits a surrogate pair yields U+FFFD
   because `String` cannot store lone surrogates. Only reachable with astral characters
   exactly at the 300 000-unit conversation cap.
3. **`keepRecentTokens` is unused.** The pinned `findCutPoint` ignores its
   `_keepRecentTokens` parameter; the Rust signature keeps the parameter and ignores
   it. Not "fixed" to avoid a semantic deviation.
4. **`summary || "No summary generated"` is unreachable** after the non-empty branch
   preamble; preserved.
5. **Indices remain `f64`.** `CutPointResult` and `ContextUsageEstimate` keep the
   source-shaped `number` fields (existing type proofs); integer narrowing stays
   deferred, and internal indexing narrows only internally produced values.
6. **Timestamp fallbacks.** Virtual retained entries use `message.timestamp()`
   (valid typed messages always carry one); the `unwrap_or(0)` fallback is
   unreachable for valid input.
7. **Callbacks.** TypeScript accepts optional retry callbacks on the options object;
   Rust cannot default a generic parameter in a struct literal, so the no-callback
   path is `generate_summary`/`compact` and the full path is
   `*_with_callbacks` with `NoRetryCallbacks` as the no-op type.
8. **`durationMs`** is wall-clock and is not compared differentially; unit tests
   assert non-negativity.

No source bugs were found. The `findCutPoint` ignored parameter and the dead
`|| "No summary generated"` fallback are recorded as preserved source behavior.
