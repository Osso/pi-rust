# coding-agent edit tool traceability

Pinned source: `packages/coding-agent/src/core/tools/edit.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::edit`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `edit.ts:14-40` | Schemas for `edits[]` and the tool | `edit_schema`. |
| `edit.ts:42-54` | Prompt snippet/guidelines | Not part of `AgentTool`; owned by the system-prompt slice. |
| `edit.ts:56-77` | `EditToolDetails` (`diff`, `patch`, optional `firstChangedLine`) and `EditOperations` | `EditOperations` + `LocalEditOperations`; details asserted by the differential and `edit_replaces_text_and_reports_diff_and_patch`. |
| `edit.ts:79-89` | Local operations read/write/access with R_OK+W_OK | `LocalEditOperations::access` uses `access(2)` with `READ_OK|WRITE_OK`. |
| `edit.ts:96-124` | `prepareEditArguments`: parse JSON-string `edits`, merge legacy top-level `oldText`/`newText` after existing edits, drop legacy keys | `prepare_edit_arguments`; `prepare_arguments_accepts_json_strings_and_legacy_fields`. |
| `edit.ts:126-131` | `validateEditInput` requires a non-empty edits array | `parse_edit_input`; `edit_rejects_empty_and_overlapping_edits`. |
| `edit.ts:143-296` | Render components, preview computation, and diff formatting | **Gap:** `renderDiff` from `components/diff.ts` is not ported, so `renderCall`/`renderResult` remain pending; preview computation reuses `edit_diff::compute_edits_diff`. |
| `edit.ts:318-330` | Queue, abort checks before/after awaits, access failure text `Could not edit file: <path>. Error code: <code>.` | `execute_edit`; `edit_reports_enoent_for_missing_files`. |
| `edit.ts:332-352` | Read, strip BOM, detect line ending, normalize to LF, apply edits, restore BOM/line endings, write | `edit_preserves_crlf_and_bom`; fuzzy/duplicate/overlap behavior is covered by the shared `edit_diff` port. |
| `edit.ts:353-365` | Diff with 4 context lines, unified patch, `Successfully replaced N block(s) in <path>.` | `generate_diff_string`, `generate_unified_patch`, `edit_replaces_text_and_reports_diff_and_patch`. |
| `edit.ts:366-442` | `renderCall`/`renderResult`/`createEditTool` | **Gap:** renderers need `components/diff.ts`; `EditTool` is the `AgentTool` equivalent of `createEditTool`. |

Differential: `oracle/coding-agent-edit-write-tool-oracle.ts` (request `27300`, 4 edit cases including success, multi-edit, ambiguity, and ENOENT) compares result JSON, error text, and resulting file contents.

## Discrepancies

- None observed.
