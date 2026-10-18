# coding-agent change-working-directory traceability

Pinned source: `packages/coding-agent/src/core/tools/change-working-directory.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/change_working_directory.rs`
(`ChangeWorkingDirectoryTool`).

| Source | Rust | Tests |
|---|---|---|
| `changeWorkingDirectorySchema` | `parameters()` | differential |
| `readNonEmptyTarget` (path keeps whitespace, id trimmed) | same | whitespace test, blank-path test, differential |
| `normalizeTarget` (exactly one of path/id) | same | `requires_exactly_one_of_path_or_id`, differential |
| `assertExistingDirectory` | same | missing/non-directory tests |
| `loadTargetCwd` (path resolve, id → `findSessionFileById` + header cwd) | same | id/path tests, differential |
| current-session id guard | same | `rejects_a_current_session_id_prefix`, differential |
| `relocateAfterToolResult` | `ExtensionContext::relocate_after_tool_result`; `ExtensionRunner::deliver_tool_result_relocation` | `schedules_relocation_only_for_its_tool_call_id`, differential relocation cases |
| result text/details/`terminate` | `execute` | differential |

Differential: `tests/differential_session_tools.rs::change_working_directory_tool_differential`
(path, whitespace path, id prefix, current id, missing, both, neither).

## Gaps

- None.

## Renderers

Renderers ported: `render_call` (path/session target), `render_result` (tool output). Covered by `differential_interactive_components.rs::tool_renderer_cases`.
- Real runtime relocation/restart lifecycle tests (`suite/change-working-directory-tool.test.ts`
  process cases) belong to the agent-session/runtime slices; this slice ports the tool's own
  validation, resolution, and relocation call.
