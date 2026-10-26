# coding-agent broadcast traceability

Pinned source: `packages/coding-agent/src/core/tools/broadcast.ts` (common pin `4f9e6f8a4`).

**Removed by user decision, 2026-10-26.** The native `broadcast` tool is not part of the target; this is a deliberate gap, not parity. The frozen source inventory row remains. The implementation and proofs below describe the former port, not the removal's verification.

Former Rust: `crates/pi-coding-agent-tools/src/core/tools/broadcast.rs` (`BroadcastTool`).

| Source | Rust | Tests |
|---|---|---|
| `broadcastSchema` (message, session_ids, cwd, name, status enum) | `parameters()`, `BroadcastParams` | tool metadata test |
| control DB + persisted session file guards | `execute` | `broadcast_requires_a_persisted_session_file` |
| `broadcastToSessions` with filters/sender identity | `broadcast_to_sessions` | `broadcast_rejects_an_empty_message`, differential |
| sent/failed/skipped counts and `formatBroadcastResult` (`JSON.stringify(error)`) | `execute`, `format_broadcast_result` | differential |
| `No sessions matched.` | `execute` | differential |

Differential: `tests/differential_session_tools.rs::broadcast_tool_differential`
(skipped-dead, filtered) plus the directory-level broadcast differential.

## Q18 native checkpoint

Against common pin `4f9e6f8a4`, message/filter IDs, cwd/name/status inputs and numbered result/filter rendering retain UTF-16 units. Error JSON quoting uses the lossless serializer; directory-level human error storage remains the session-health owner's pending migration. `native_filter_tests::broadcast_filters_accept_raw_units_without_repair` records raw DTO and status behavior, unexecuted until coding-library compilation. Legacy differential/render proofs above must be rerun in-crate; no Q18 acceptance claim.

## Renderers

Renderers ported: `render_call` (title + dim filters), `render_result` (tool output). Covered by `differential_interactive_components.rs::tool_renderer_cases`.
