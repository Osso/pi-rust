# Coding-agent compaction traceability

Frozen: `4f9e6f8a4`, `core/compaction/compaction.ts` (1120 lines) and `index.ts` (7 lines), whole-module audit 2026-10-25.
Rust: `crates/pi-coding-agent-tools/src/core/compaction.rs`.

## Export and branch map

| Frozen surface / branches | Native representation | Proof |
| --- | --- | --- |
| `CompactionDetails`, `CompactionSourceInfo`, `CompactionResult<T>` | Matching records; no obsolete usage field | Native generation serialization; complete frozen split/non-split result comparison |
| `CompactionSettings`, `DEFAULT_COMPACTION_SETTINGS` | Coding-agent-specific four-field settings; harness interface unchanged | Frozen predicate/preparation settings vectors |
| `calculateContextTokens`, `ContextUsageEstimate`, `estimateContextTokens`, `estimateTokens` | Shared compatible helpers | `differential_token_calculation_and_estimation`; native usage/role tests |
| `getLastAssistantUsage`: backwards valid assistant search, skip aborted/error/zero | `get_last_assistant_usage` | Frozen empty/user-only/valid-with-invalid-tail matrix; native three usage tests |
| `shouldCompact`: validate percentage before disabled; percentage then model threshold then reserve | Fallible `should_compact` | Ten valid precedence/boundary cases; ten validation/enabled combinations |
| `findTurnStartIndex`, `CutPointResult`, `findCutPoint` | Literal protected-suffix selection; unused budget argument retained | Frozen cut/turn matrix including custom turn starts; native metadata/active-turn/token-cap tests |
| `CompactionPreparation`, `prepareCompaction` | Latest boundary, first-kept lookup, history/prefix partition, goal-reminder filtering, cumulative file tracking | Seven frozen preparation fixtures including percentage37.5; native repeated/active-turn/goal-reminder fixtures |
| `generateSummary` | `GenerateSummaryOptions` carries the positional Rust inputs; text-only result | Frozen complete model/context/options/result comparison: history/update/empty previous/trimmed or blank focus/empty output |
| `MaterializeCompactionSummaryOptions`, `MaterializeCompactionSummaryResult`, `materializeCompactionSummary` | Matching options; native aborted/summary enum | Frozen complete checkpoint request and result, aborted, whitespace rejection, missing checkpoint; four native materialization cases |
| `compact` | Text-only history/prefix composition, file suffix, UUID check, duration/source/details result | Frozen split/history-only/no-history/empty-prefix request and result matrices; native generation tests |
| `index.ts` | Existing module re-exports | All differential helpers import the actual frozen index |

Internal extraction, valid cut points, protected three-message/10000-token suffix, active-turn floor, adjacent metadata, source prompts, token budgets and request construction were compared branch by branch. Acquisition and result rejection remain fallible; existing acquisition/lossless tests cover those boundaries. All generation tests inject streams; no provider network is used.

## Re-pin corrections

- Removed the old budget-cut pass: frozen ignores `_keepRecentTokens`. Parent's cut-point baseline is reproduced in `/tmp/repin-compaction-cut-red.log`; frozen retains index2, old native retained index4. `red2.log` proves custom turn-start classification differed too. Both are now GREEN.
- Added percentage settings, validation ordering, inclusive boundaries and model-threshold forwarding. `/tmp/repin-compaction-threshold-red.log` records the old predicate mismatches. Session wiring and trailing-message ordering are mapped in [agent-session](agent-session.md).
- Copied frozen current-state update wording and continuation guidance; trim custom focus and suppress empty previous-summary tags. Prefix summaries receive the same focus. Empty history/prefix output now raises the source error.
- Removed `SummaryWithUsage`, `generate_summary_with_usage`, usage aggregation, old options naming and result usage. No compatibility alias remains. Mechanical fixture/export fallout is listed in the [proof ledger](../agent-session-repin-proof.md).

DISCREPANCY: frozen marks a selected custom/branch/bash turn start as split with `turnStartIndex == firstKeptEntryIndex`; this counterintuitive result is retained literally and tested.

Gap: adversarial/wrong-typed inputs, OS faults, numeric extremes and runtime scheduling oddities follow the repository parity exclusion; existing lossless carriers remain intact.

## Proof

At `0671e013f` plus fixture-only `880c13455`: helper batch36 passes plus its corrected stale-budget unit1 pass; generation batch15 passes, expanded frozen generation4 passes, native branch14 passes, last-usage frozen matrix1 pass. Warning-mode targeted library/test Clippy exits0 with zero changed-line findings. Commands, exact scopes and logs: [proof ledger](../agent-session-repin-proof.md). No workspace or independent final-gate claim.
