# Frozen branch-summarization.ts

Source: [common pin](../source-lock.json), `agent-core/src/harness/compaction/branch-summarization.ts:1–263`.
Rust: `crates/pi-agent-core/src/harness/compaction/branch_summarization.rs`.

| Export/branch | Proof |
|---|---|
| `collectEntriesForBranchSummary`: common ancestor, abandoned chronological path, absent old leaf | `branch_summarization.rs` and native-tree `differential_compaction` |
| `prepareBranchEntries`: reverse budget, tool-result exclusion, summary overshoot, prior file details | Same whole frozen differential and focused tests |
| `generateBranchSummary`: empty result, prompt, custom append/replace, context budget, fixed maxTokens, preamble/files | Same differential and 13 focused tests |
| Failed/aborted provider responses and raw UTF-16 error messages | Same differential plus `lossless_provider_errors` |
| `BranchSummaryDetails`, preparation/result records | Public mutation test and output differential |

Frozen has no retry or usage result: transient provider failure returns immediately; remaining queued response proves no retry. Requests do not add cache overrides. Old lane fixtures now move the native tree leaf.

Provider timestamps now retain f64; the temporary checked conversion has been removed ([integration proof](message-timestamps.md)). [Owned session getter approval](agent-core-tree-session.md).

GREEN artifacts: `tree-compaction-dev2.log`, `tree-compaction-oracle-dev2.log`, `tree-native-boundaries-dev.log` under `/tmp/pi-rust-repin-delta-agent-core/`.

`branch_summary_paths` closes the remaining error/path matrix:11 direct frozen/native cases cover absent/empty old leaf short-circuit, missing old/target IDs, corrupt old/target ancestors, disconnected roots, same target, ancestor navigation in both directions, and a custom storage whose getEntry disappears after path snapshots. Exact returned entries/common ancestor and SessionError name/code/message are asserted. GREEN `branch-summary-paths-final-green.log`; initial fixture compile log only lacked the explicit SessionMetadata generic. No production behavior changed or source failure was normalized.

