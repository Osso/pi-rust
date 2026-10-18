# coding-agent list-sessions traceability

Pinned source: `packages/coding-agent/src/core/tools/list-sessions.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/list_sessions.rs` (`ListSessionsTool`).

| Source | Rust | Tests |
|---|---|---|
| `listSessionsSchema` (`include_ended` optional) | `parameters()` | differential |
| control DB requirement | `require_control_db_path` | `list_sessions_requires_a_control_database_path` |
| `includeEnded: params.include_ended ?? true` | `SessionDirectoryOptions::include_ended` | `excludes_ended_rows_when_include_ended_is_false`, differential |
| `formatSessionLine` | `format_session_line` (`JSON.stringify` goal via `serde_json::to_string`) | list differential text |
| `No sessions found.` / numbered lines | `execute` | differential |

Differential: `tests/differential_session_tools.rs::list_sessions_tool_differential`
(empty, ended, ended-hidden).

## Gaps

- None.

## Renderers

Renderers ported: `render_call` (title), `render_result` (tool output). Covered by `differential_interactive_components.rs::tool_renderer_cases`.
