# coding-agent ask-architect traceability

Pinned source: `packages/coding-agent/src/core/tools/ask-architect.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/ask_architect.rs` (`AskArchitectTool`).

| Source | Rust | Tests |
|---|---|---|
| `askArchitectSchema` | `parameters()` | differential |
| subagent rejection | `execute` | `ask_architect_rejects_subagent_runtimes` |
| control DB + persisted session id guards | `require_control_db_path`, empty-id check | differential error paths |
| `resolveProjectPath` (trim/absolute check, `realpathSync`, directory check) | `resolve_project_path` (`fs::canonicalize`) | invalid-path test, canonical-symlink differential |
| `postArchitectRequest` | `post_architect_request` | default/canonical tests + differential state |

Differential: `tests/differential_session_tools.rs::ask_architect_tool_differential`
(canonical symlink, default cwd, relative path, missing path) compares results and pending rows.

## Gaps

- `renderCall`/`renderResult` need `Theme`.
- `cause` on the thrown realpath error is not representable in `ToolError`; the message matches.
