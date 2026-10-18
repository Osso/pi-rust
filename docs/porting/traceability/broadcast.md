# coding-agent broadcast traceability

Pinned source: `packages/coding-agent/src/core/tools/broadcast.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/broadcast.rs` (`BroadcastTool`).

| Source | Rust | Tests |
|---|---|---|
| `broadcastSchema` (message, session_ids, cwd, name, status enum) | `parameters()`, `BroadcastParams` | tool metadata test |
| control DB + persisted session file guards | `execute` | `broadcast_requires_a_persisted_session_file` |
| `broadcastToSessions` with filters/sender identity | `broadcast_to_sessions` | `broadcast_rejects_an_empty_message`, differential |
| sent/failed/skipped counts and `formatBroadcastResult` (`JSON.stringify(error)`) | `execute`, `format_broadcast_result` | differential |
| `No sessions matched.` | `execute` | differential |

Differential: `tests/differential_session_tools.rs::broadcast_tool_differential`
(skipped-dead, filtered) plus the directory-level broadcast differential.

## Gaps

- None.

## Renderers

Renderers ported: `render_call` (title + dim filters), `render_result` (tool output). Covered by `differential_interactive_components.rs::tool_renderer_cases`.
