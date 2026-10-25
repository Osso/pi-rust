# coding-agent list-sessions traceability

Pinned source: `packages/coding-agent/src/core/tools/list-sessions.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent-tools/src/core/tools/list_sessions.rs` (`ListSessionsTool`).

| Source | Rust | Tests |
|---|---|---|
| `listSessionsSchema` (`include_ended` optional) | `parameters()` | differential |
| control DB requirement | `require_control_db_path` | `list_sessions_requires_a_control_database_path` |
| `includeEnded: params.include_ended ?? true` | `SessionDirectoryOptions::include_ended` | `excludes_ended_rows_when_include_ended_is_false`, differential |
| `formatSessionLine` | `format_session_line` (native output; `JSON.stringify` goal via lossless serializer) | list differential text |
| `No sessions found.` / numbered lines | `execute` | differential |

Differential: `tests/differential_session_tools.rs::list_sessions_tool_differential`
(empty, ended, ended-hidden).

## Q18 native checkpoint

Against common pin `4f9e6f8a4`, session IDs/name/cwd and numbered output retain UTF-16; goal JSON quoting uses the lossless serializer, and details use native JSON. `native_output_tests::session_line_keeps_raw_identity_name_and_cwd_and_quotes_goal` asserts exact raw output, unexecuted until coding-library compilation. Legacy differential/render proofs above must be rerun in-crate; no Q18 acceptance claim.

## Renderers

Renderers ported: `render_call` (title), `render_result` (tool output). Covered by `differential_interactive_components.rs::tool_renderer_cases`.
