# Traceability: `packages/coding-agent/src/core/compaction/utils.ts`

Rust: `crates/pi-coding-agent/src/core/compaction/utils.rs`. Tests:
`tests/compaction_utils.rs`, `tests/differential_compaction.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

The TypeScript file is a near-copy of `packages/agent/src/harness/compaction/utils.ts`
([agent-core utils](compaction-utils.md)). File-operation tracking and the
`SUMMARIZATION_SYSTEM_PROMPT` constant are byte-identical and are re-exported
from `pi_agent_core`; only `serializeConversation`'s tool-argument serialization
diverges, so that single function is ported here on top of a shared
argument-serializer hook.

## Exported surface

| TypeScript | Rust | Tests |
|---|---|---|
| `FileOperations` | re-export `pi_agent_core::...::FileOperations` | `differential_serialize_conversation_and_file_lists` |
| `createFileOps` | re-export `create_file_ops` | same |
| `extractFileOpsFromMessage` | re-export `extract_file_ops_from_message` | same; branch-preparation differential |
| `computeFileLists` | re-export `compute_file_lists` | same |
| `formatFileOperations` | re-export `format_file_operations` | same; `compaction_generation::compacts_with_file_operations_and_source_details` |
| `serializeConversation` | `serialize_conversation` (coding-agent argument truncation) | 7 tests in `compaction_utils.rs`; differential over six messages |
| `SUMMARIZATION_SYSTEM_PROMPT` | re-export from `pi_agent_core` | prompt assertions in `compaction_generation.rs`, branch tests |

## Internal branches

| TypeScript | Rust | Notes |
|---|---|---|
| `truncateForSummary` | `pi_agent_core::...::utils::truncate_for_summary` | Shared; UTF-16 length and `[... N more characters truncated]` marker. |
| `serializeToolCallArgument` | `serialize_tool_call_argument` | `JSON.stringify` then 1000-char truncation. `JSON.stringify(undefined)` cannot occur because Rust `serde_json::Value` has no undefined. |
| `serializeToolResultForSummary` | shared agent-core helper | 2000-char truncation plus `[Output truncated. Full output: …]` rules. |
| `serializeConversation` loop | `pi_agent_core::...::utils::serialize_conversation_with_arguments` | Shared body; coding-agent passes the truncating argument serializer, agent-core passes `safe_json_stringify`. |
| `TOOL_CALL_ARGUMENT_MAX_CHARS` / `TOOL_RESULT_MAX_CHARS` | `TOOL_CALL_ARGUMENT_MAX_CHARS` / `TOOL_RESULT_MAX_CHARS` | 1000 / 2000. |

## Deviations / gaps

- `FileOperations` uses `BTreeSet`, so iteration is lexicographic rather than
  JavaScript `Set` insertion order. Every exported consumer sorts
  (`computeFileLists`) or membership-tests, and the differential fixtures use
  ascending insertion order; raw iteration order is an inherited deviation from
  the agent-core port, not observable through these exports' outputs.
- Truncation counts UTF-16 units but stores Rust `String`, so a cut inside a
  surrogate pair yields U+FFFD instead of a lone surrogate (inherited approved
  deviation from the agent-core helper).
