# Traceability: `packages/coding-agent/src/core/compaction/branch-summarization.ts`

## Common-pin acquisition correction (2026-10-22)

Frozen `branch-summarization.ts:349–353` awaits acquisition/result and propagates rejection. Rust now returns Result for those failures, without converting them to a provider-error record; removed old retry/callback parameters and the callback-less alternate wrapper. [Bounded proof](stream-acquisition.md): native acquisition rejection, 14 branch tests, SDK/agent-session consumers. Historical evidence below is not a whole-module re-pin claim; existing usage fields await their owning re-port.

Rust: `crates/pi-coding-agent-tools/src/core/compaction/branch_summarization.rs`. Tests:
`tests/branch_summarization.rs`, `tests/differential_compaction.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

Compared with `packages/agent/src/harness/compaction/branch-summarization.ts`
([agent branch summarization](branch-summarization.md)), this copy reads
`SessionEntry`/`ReadonlySessionManager`, returns a plain result object instead of
`Result`, handles `custom_message` entries, filters `fromHook` details, and calls
`completeSummarization` with coding-agent auth options.

## Exported surface

| TypeScript | Rust | Tests |
|---|---|---|
| `BranchSummaryResult` | `BranchSummaryResult` (all-optional fields, serde camelCase) | 6 generation tests |
| `BranchSummaryDetails` | `BranchSummaryDetails` | preparation tests |
| `FileOperations` re-export | module re-export | preparation differential |
| `BranchPreparation` | `BranchPreparation` | 5 preparation tests; differential (4 fixtures) |
| `CollectEntriesResult` | `CollectEntriesResult` | 2 collection tests; differential on a real session file |
| `GenerateBranchSummaryOptions` | `GenerateBranchSummaryOptions<'a>` | all generation tests |
| `collectEntriesForBranchSummary` | `collect_entries_for_branch_summary` | collection tests; differential |
| `prepareBranchEntries` | `prepare_branch_entries` | preparation tests; differential |
| `generateBranchSummary` | `generate_branch_summary` (+ `generate_branch_summary_default`) | generation tests |

## Internal branches

| TypeScript | Rust | Notes |
|---|---|---|
| `getMessageFromEntry` | `get_message_from_entry` | Skips `toolResult`; converts custom/branch/compaction entries; ignores settings/metadata entries. |
| first pass over entries | inline in `prepare_branch_entries` | Cumulative `readFiles`/`modifiedFiles` from non-hook branch summaries. |
| newest-to-oldest budget walk | inline in `prepare_branch_entries` | `tokenBudget == 0` means unlimited; summary entries may exceed the budget only below 90 %. |
| `BRANCH_SUMMARY_PREAMBLE` / `BRANCH_SUMMARY_PROMPT` | same constants | Asserted by the preambled-summary and custom-instruction tests. |
| instruction selection | `match (replace_instructions, custom_instructions)` | Replace / append / default. |
| request construction | inline | System prompt, `<conversation>` block, `maxTokens: 2048`, cache retention `none` via `complete_summarization`. |
| aborted / error handling | inline | `{aborted:true}` and `{error: …}` results. |
| file-list suffix | `compute_file_lists` + `format_file_operations` | Asserted in the preambled-summary test. |

## Gaps / discrepancies

- `generateBranchSummary` is not differential: it performs network I/O. The
  scripted `StreamFn` tests assert the captured system prompt, prompt text,
  `maxTokens`, cache retention, usage, preamble, and file-list suffix.
- `BranchSummaryResult` uses `Option` fields with `skip_serializing_if`, so an
  aborted result serializes as `{"aborted":true}` exactly like the source.
- No source bugs found.

## Re-pin generation result and empty instructions (2026-10-25)

Frozen :32–38/305–381 returns summary/readFiles/modifiedFiles, never usage. Removed native `BranchSummaryResult.usage` and corrected empty custom instruction truthiness for both replace/append modes. `differential_compaction_generation::branch_generation_matches_frozen_result_and_instruction_truthiness` invokes the actual source module with an injected stream and compares the full prompt/options/model and full result across missing/empty/nonempty custom instructions and replace modes. RED `/tmp/repin-compaction-generation-red.log` exposed the extra usage property. Existing source whitespace behavior remains unchanged: branch summaries join whitespace text; history and turn-prefix summaries filter whitespace-only blocks.

The full-result frozen generation matrix also covers whitespace preservation, provider-error default text and aborted result shapes; native branch tests14/14 pass in suite_core_1.
