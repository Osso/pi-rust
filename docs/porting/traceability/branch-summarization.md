# Traceability: `packages/agent/src/harness/compaction/branch-summarization.ts`

Rust: `crates/pi-agent-core/src/harness/compaction/branch_summarization.rs`. Tests:
`branch_summarization.rs`, `differential_compaction.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## Q18 native human-error consumer proof

Against frozen `4f9e6f8a4`, `lossless_provider_errors::branch_failures_preserve_raw_error_and_source_defaults` proves native failed/aborted provider text, exact `Branch summary failed: ` prefix, missing-message defaults, and empty-message preservation. BranchSummaryError.message is JsString; only Display encodes. Earlier module evidence below retains its original pin and is not upgraded by this bounded proof.

## Exports and branches

| TypeScript | Rust | Tests |
|---|---|---|
| `BranchSummaryResult` | `BranchSummaryResult` | generation tests, differential |
| `BranchSummaryDetails` | `BranchSummaryDetails` | `branch_summary_details_fields_are_publicly_mutable` |
| `FileOperations` (re-export) | `FileOperations` (from `utils`) | `file_operations.rs` |
| `BranchPreparation` | `BranchPreparation` | preparation tests, differential |
| `CollectEntriesResult` | `CollectEntriesResult` | collection tests, differential |
| `GenerateBranchSummaryOptions` | `GenerateBranchSummaryOptions` (+ `..._with_callbacks`) | generation tests, differential |
| `collectEntriesForBranchSummary` | `collect_entries_for_branch_summary` | both TS cases, differential |
| `getMessageFromEntry` (internal) | `get_message_from_entry` | preparation tests |
| `prepareBranchEntries` | `prepare_branch_entries` | four preparation tests, differential |
| `BRANCH_SUMMARY_PREAMBLE` (internal) | `BRANCH_SUMMARY_PREAMBLE` | summary-generation test |
| `BRANCH_SUMMARY_PROMPT` (internal) | `BRANCH_SUMMARY_PROMPT` | prompt differential |
| `generateBranchSummary` | `generate_branch_summary` | five generation tests, differential |

## Behavior notes

- `collectEntriesForBranchSummary` returns `{entries: [], commonAncestorId: null}` when
  the previous leaf is absent. Otherwise it builds the old path set, scans the target
  path for the first shared id (deepest common ancestor), walks the old leaf to that
  ancestor through `getEntry`, reverses to chronological order, and raises
  `SessionError("invalid_entry", "Entry <id> not found")` for a missing entry.
- `prepareBranchEntries` first accumulates `details.readFiles`/`details.modifiedFiles`
  from branch-summary entries, then walks entries newest-to-oldest. Message entries
  except `toolResult` become messages; branch/compaction entries become their summary
  messages. With a positive budget, the walk stops at the first message that does not
  fit, except that a compaction/branch-summary entry is still included when the
  accumulated total is below 90% of the budget.
- `generateBranchSummary` defaults `reserveTokens` to 16 384 and
  `contextWindow` to 128 000 when the model reports 0. An empty selection returns
  `"No content to summarize"` without a model call. The request uses
  `maxTokens: 2048`, the summarization system prompt, and the
  `<conversation>…</conversation>` prompt. The result prefixes
  `BRANCH_SUMMARY_PREAMBLE` and appends the file-operation sections.
- Aborted responses produce `BranchSummaryError("aborted", errorMessage || "Branch summary aborted")`;
  error responses produce `"summarization_failed"` with
  `Branch summary failed: <errorMessage || "Unknown error">`.

## Differential coverage

`differential_compaction.rs` compares collection (branch + no previous leaf),
preparation with/without budgets and summary details, and generation for basic,
custom, replaced instructions, empty selection, error, aborted, and small-context
budgets — including the exact prompt text, `maxTokens`, cache retention, usage, and
file lists.

## Discrepancies and typed exclusions

1. **Optional signal.** TypeScript requires an `AbortSignal`; Rust takes
   `Option<&CancellationToken>` (`None` never aborts). This is the typed mapping used
   across the port, not a behavioral change.
2. **Typed options.** `replaceInstructions` is `bool` (default `false`) and
   `reserveTokens` is `Option<f64>` (default 16 384); TypeScript uses optional
   properties with the same defaults.
3. **Callbacks.** As in `compaction.ts`, the no-callback path is
   `generate_branch_summary`; `generate_branch_summary_with_callbacks` exposes the
   generic retry-callback path.
4. **`usage` is `Option<Usage>`** because the empty-selection result has no provider
   call; the differential compares `null` in that case.
5. **Timestamp fallbacks.** Summary messages use `Date.now()` equivalents; no
   timestamp is part of the compared prompt text.
