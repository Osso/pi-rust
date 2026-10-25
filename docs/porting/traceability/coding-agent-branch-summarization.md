# Coding-agent branch summarization traceability

Frozen: `4f9e6f8a4`, `core/compaction/branch-summarization.ts` (380 lines), whole-module audit 2026-10-25.
Rust: `crates/pi-coding-agent-tools/src/core/compaction/branch_summarization.rs`.

| Frozen surface / branches | Native | Proof |
| --- | --- | --- |
| `BranchSummaryResult`, `BranchSummaryDetails`, `BranchPreparation`, `CollectEntriesResult`, `GenerateBranchSummaryOptions`, `FileOperations` export | Matching records/options; owned values under repository approval | Native fixtures and full frozen result serialization |
| `collectEntriesForBranchSummary`: no old leaf; deepest common ancestor; backwards walk and chronological result | `collect_entries_for_branch_summary` | Two native collection tests; actual frozen session-file collection differential |
| Entry conversion: omit tool results/metadata, include custom/branch/compaction entries | `get_message_from_entry` | Native entry fixtures and frozen preparation vectors |
| `prepareBranchEntries`: cumulative non-hook summary file lists, newest-first budget, extract tool files before budget check, summary overflow allowed below90%, stop at first overflow | `prepare_branch_entries` | Native budget/90%/hook/custom/compaction tests; four frozen preparation fixtures |
| `generateBranchSummary`: context-window/default budget; no content; default/append/replace instructions; empty custom instructions falsey | `generate_branch_summary` | Native14-test group; eight full frozen generation scenarios |
| Request options and completion: system prompt/conversation/preamble,2048 tokens, cache none, supplied auth/headers/env/signal/stream, acquisition failures | Existing injected-stream boundary | Frozen complete model/context/options snapshots; native acquisition tests |
| Aborted/error/empty error fallback; join all text including whitespace; file lists and suffix | Matching result branches | Full frozen aborted/error/whitespace/success result comparison |

Removed native-only `usage` from the result and corrected empty custom instruction truthiness. Frozen history compaction filters whitespace-only blocks; this branch module does not, and retains the preamble even for whitespace-only output. No old usage wrapper or retry path remains.

Gap: adversarial/wrong-typed inputs, OS faults, numeric extremes and runtime scheduling oddities follow the repository parity exclusion; native owned returns use the standing approval.

Proof: native `suite_core_1 branch_summarization::`14/14; frozen generation matrix and preparation/collection differentials pass. Exact revisions, commands and logs: [proof ledger](../agent-session-repin-proof.md). No network or workspace final gate.
