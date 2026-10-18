# Traceability: `packages/agent/src/harness/compaction/utils.ts`

Rust: `crates/pi-agent-core/src/harness/compaction/utils.rs`. Tests:
`compaction_utils.rs`, `file_operations.rs`, `differential_compaction.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## Exports and branches

| TypeScript | Rust | Tests |
|---|---|---|
| `FileOperations` (`Set<string>` x3) | `FileOperations` (`BTreeSet<String>` x3) | `creates_empty_file_operations`, differential |
| `createFileOps` | `create_file_ops` | `creates_empty_file_operations` |
| `extractFileOpsFromMessage` | `extract_file_ops_from_message` | three extraction tests, differential `prepare_branch_entries` |
| `computeFileLists` | `compute_file_lists` | `computes_sorted_read_only_and_modified_lists`, differential `compute_file_lists` |
| `formatFileOperations` | `format_file_operations` | `file_operations.rs`, differential `format_file_operations` |
| `serializeConversation` | `serialize_conversation` | `compaction_utils.rs`, differential `serialize_conversation` |
| `TOOL_RESULT_MAX_CHARS` (internal) | `TOOL_RESULT_MAX_CHARS` | truncation tests |
| `safeJsonStringify` (internal) | `safe_json_stringify` | tool-call serialization tests, differential |
| `truncateForSummary` (internal) | `truncate_for_summary` | truncated tool-result test, differential |
| `extractFullOutputPath` / `hasToolOutputTruncation` (internal) | `extract_full_output_path` / `has_tool_output_truncation` | full-output marker tests |
| `serializeToolResultForSummary` (internal) | `serialize_tool_result_for_summary` | full-output marker tests, differential |

## Behavior notes

- Sets map to `BTreeSet`; `computeFileLists` sorts both output lists, so insertion
  order is not observable. Read-only files exclude any modified path.
- Assistant tool calls are serialized as `name(key=JSON.stringify(value), ...)` with
  JSON object insertion order preserved (`serde_json` `preserve_order`).
- Tool results are truncated at 2 000 UTF-16 units with
  `[... N more characters truncated]`. A `details.fullOutputPath` marker is appended
  only when the serialized content does not already contain it and either the content
  exceeded the cap or `details.truncation.truncated === true`.
- Parts are joined with `\n\n`; empty user/tool-result text is omitted.

## Differential coverage

`serialize_conversation` cases cover text/tool-call messages, truncated tool results
with full-output paths, and custom/bash/branch/compaction messages after
`convertToLlm`. File-list and formatting cases cover sorted read-only/modified lists,
empty lists, and Unicode/NUL/newline content.

## Discrepancies and typed exclusions

1. **Lone surrogates.** `String.prototype.slice` can split a surrogate pair; Rust
   `String` cannot store lone surrogates, so truncation at such a boundary yields
   U+FFFD. This is the approved typed-API exclusion for unrepresentable JS strings.
2. **`JSON.stringify(undefined)` fallback.** The TypeScript helper returns
   `"undefined"` when `JSON.stringify` returns `undefined`; `serde_json::Value`
   cannot represent `undefined`, so only `[unserializable]` on serialization failure
   is reachable.
3. **Set insertion order.** TypeScript preserves insertion order in the `FileOperations`
   sets; the Rust accumulator is ordered by path. No exported function exposes set
   order (`computeFileLists` sorts), so this is not observable.
