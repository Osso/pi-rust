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
| `relocateAfterToolResult` | `ToolContext::relocate_after_tool_result` / `RelocateAfterToolResult` | relocate capture tests |
| result text/details/`terminate` | `execute` | differential |

Differential: `tests/differential_session_tools.rs::change_working_directory_tool_differential`
(path, whitespace path, id prefix, current id, missing, both, neither).

## Gaps

- `renderCall`/`renderResult` need `Theme`.
- Real runtime relocation/restart lifecycle tests (`suite/change-working-directory-tool.test.ts`
  process cases) belong to the agent-session/runtime slices; this slice ports the tool's own
  validation, resolution, and relocation call.
