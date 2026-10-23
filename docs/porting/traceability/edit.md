# coding-agent edit tool traceability

## Current-pin cycle (open)

Source `coding-agent/src/core/tools/edit.ts` at immutable4f9e6f8a4. [Current tool contract](../../specs/coding-agent-edit-tool.md). Historical rows below are not current-pin closure.

`prepareEditArguments`: fourteen actual-frozen JSON fixtures now compare values and JSON.stringify. Legacy merge RED (`edit-prepare-order-red.log`) proves an existing edits key moved after unrelated fields. Native now clones the existing array without deleting its slot before override, matching object-rest/spread semantics. GREEN1/0 (`edit-prepare-order-green.log`), including parsed non-arrays, malformed JSON, raw surrogate text and __proto__ data. Mutation-through-input identity follows the approved owned-value rule; execution/preview/reuse audit remains pending.

Access-error catch: fifteen actual-frozen cases in `differential_edit_operation_errors` reproduce lost native name/code and bad String(error) formatting (`edit-operation-errors-red.log`). Native inspects the original IO-carried rejection before the actual catch projects a new ToolError message. Null/false/object/array codes and throwing primitive conversion match; access-abort wins, read/write failure wins over abort. GREEN1/0 (`edit-operation-errors-green.log`); whole queued/default-filesystem/preview closure pending.

Pinned source: `packages/coding-agent/src/core/tools/edit.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::edit`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `edit.ts:14-40` | Schemas for `edits[]` and the tool | `edit_schema`. |
| `edit.ts:42-54` | Prompt snippet/guidelines | Not part of `AgentTool`; owned by the system-prompt slice. |
| `edit.ts:56-77` | `EditToolDetails` (`diff`, `patch`, optional `firstChangedLine`) and `EditOperations` | `EditOperations` + `LocalEditOperations`; details asserted by the differential and `edit_replaces_text_and_reports_diff_and_patch`. |
| `edit.ts:79-89` | Local operations read/write/access with R_OK+W_OK | `LocalEditOperations::access` uses `access(2)` with `READ_OK|WRITE_OK`. |
| `edit.ts:96-124` | `prepareEditArguments`: parse JSON-string `edits`, merge legacy top-level `oldText`/`newText` after existing edits, drop legacy keys | `prepare_edit_arguments`; `prepare_arguments_accepts_json_strings_and_legacy_fields`. `create_edit_tool_definition` forwards it through `create_tool_definition_from_agent_tool` (`edit_definition_forwards_prepare_arguments`). |
| `edit.ts:126-131` | `validateEditInput` requires a non-empty edits array | `parse_edit_input`; `edit_rejects_empty_and_overlapping_edits`. |
| `edit.ts:143-296` | Render components, preview computation, and diff formatting | `EditPreview`, `get_renderable_preview_input`, `request_edit_preview` (background thread), `fill_edit_call_component`; `render_call` reuses the last call component and `render_result` rebuilds it in place after applying the result diff/settled-error, matching `buildEditCallComponent` in `renderResult`. The call component is passed through `ToolRenderContext.call_component` instead of the source's `state.callComponent`. Proof: `edit_tool_preview` (pending header, on-disk diff, error body, args reset, settled-result rebuild through `ToolExecutionComponent`) and differential `27950` (6 cases: success, CRLF, missing file, no-match error, matching-diff suppression, error text). |
| `edit.ts:318-330` | Queue, abort checks before/after awaits, access failure text `Could not edit file: <path>. Error code: <code>.` | `execute_edit`; `edit_reports_enoent_for_missing_files`. Errors without an errno report the original message (`Could not edit file: broken.txt. Error: disk offline.`), matching the source's `String(error)` branch; `edit_reports_the_original_message_for_unknown_access_errors`. |
| `edit.ts:332-352` | Read, strip BOM, detect line ending, normalize to LF, apply edits, restore BOM/line endings, write | `edit_preserves_crlf_and_bom`; fuzzy/duplicate/overlap behavior is covered by the shared `edit_diff` port. |
| `edit.ts:353-365` | Diff with 4 context lines, unified patch, `Successfully replaced N block(s) in <path>.` | `generate_diff_string`, `generate_unified_patch`, `edit_replaces_text_and_reports_diff_and_patch`. |
| `edit.ts:366-442` | `renderCall`/`renderResult`/`createEditTool` | `create_edit_tool_definition` exposes both renderers with `renderShell: "self"`; `EditTool` is the `AgentTool` equivalent of `createEditTool`; `constrainedSampling` mirrors `getExperimentalToolSampling()` (`tool_definitions_expose_experimental_constrained_sampling`). |

Differential: `oracle/coding-agent-edit-write-tool-oracle.ts` (request `27300`, 4 edit cases including success, multi-edit, ambiguity, and ENOENT) compares result JSON, error text, and resulting file contents. Request `27301` compares `definition.prepareArguments` outputs for legacy fields, JSON-string edits, invalid JSON, non-object input, and pass-through cases. `oracle/coding-agent-edit-preview-oracle.ts` (request `27950`) renders the pending header, the completed on-disk preview, and result output for 6 cases byte-for-byte.

## Gaps / discrepancies

- The source keeps the rendered `Box` in `context.state.callComponent`; Rust passes it as
  `ToolRenderContext.call_component` and rebuilds it in place on the result. Observable output is
  identical (differential `27950`, `edit_settled_result_rebuilds_the_rendered_call_component`).
- `renderResult` returns a fresh result `Container` instead of clearing `context.lastComponent`;
  output is identical.
- `renderDiff`'s optional `filePath` argument is ignored by the source itself (`_options`), so the
  Rust `render_diff(diff_text)` signature matches.
- `formatEditCall` falls back to `path` only when `file_path` is missing or null, matching
  `str(args?.file_path ?? args?.path)`; non-string values are typed out.
