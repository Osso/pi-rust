# coding-agent ask-supervisor traceability

Pinned source: `packages/coding-agent/src/core/tools/ask-supervisor.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/ask_supervisor.rs` (`AskSupervisorTool`) plus
`crates/pi-coding-agent/src/supervisor/client.rs`.

| Source | Rust | Tests |
|---|---|---|
| `askSupervisorSchema` (`maxLength` 4000/8000) | `parameters()` | `ask_supervisor_bounds_advisory_input` |
| main-session guard | `assert_main_session` | `ask_supervisor_rejects_subagent_runtimes` |
| control DB + persisted session id guards | `require_control_db_path`, empty-id check | differential-free behavioral guard |
| `resolveSupervisorProjectForCwd` | existing `supervisor::project_resolver` | reused |
| `requestSupervisorDecision` (durable request, poll, retry with jitter, timeout error) | `supervisor::client::request_supervisor_decision` | `tests/supervisor_client.rs` (3 ported tests) |
| advisory response validation and `{answer, projectId, senderSessionId}` | `execute` | `ask_supervisor_returns_the_advisory_answer` |

## Gaps

- None.

## Renderers

Renderers ported: `render_call` (title), `render_result` (tool output). Covered by `differential_interactive_components.rs::tool_renderer_cases`.
- Differential is behavioral only: the request/response loop is timing-dependent
  (`setTimeout` polling); `tests/supervisor_client.rs` reproduces the source test cases at the
  real control-DB boundary.
