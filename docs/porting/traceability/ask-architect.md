# coding-agent ask-architect traceability

## Q18 native input/details

Common-pin checkpoint: message/project-path/sender ID fields retain `JsString`; lexical path validation is native, canonicalization encodes only at filesystem entry and decodes its OS result. Result details use the native serializer. `lossless_architect_input` records raw ingress/details units, unexecuted while the coding library is blocked; earlier scalar differential evidence below is not renewed acceptance.

Pinned source: `packages/coding-agent/src/core/tools/ask-architect.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/ask_architect.rs` (`AskArchitectTool`).

| Source | Rust | Tests |
|---|---|---|
| `askArchitectSchema` | `parameters()` | differential |
| subagent rejection | `execute` | `ask_architect_rejects_subagent_runtimes` |
| control DB + persisted session id guards | `require_control_db_path`, empty-id check | differential error paths |
| `resolveProjectPath` (trim/absolute check, `realpathSync`, directory check) | `resolve_project_path` (`fs::canonicalize`) | invalid-path test, canonical-symlink differential |
| `postArchitectRequest` | `post_architect_request` | default/canonical tests + differential state |
| `createAskArchitectToolDefinition` (name/label/description, prompt snippet/guidelines, `executionMode: sequential`) | `create_ask_architect_tool_definition` | `differential_tools_index` (`symbol`-level definition parity covers every built-in; ask_architect is not in the index) |
| `renderCall` / `renderResult` | `render_call` / `render_result` (shared `render_text_component`/`render_tool_output`) | `tool_renderers` pattern |
| `AskArchitectToolInput`, `AskArchitectToolDetails` | `AskArchitectToolInput`, `AskArchitectToolDetails` (`requestId` number, `senderSessionId`) | differential result details |

Differential: `tests/differential_session_tools.rs::ask_architect_tool_differential`
(canonical symlink, default cwd, relative path, missing path) compares results and pending rows.

## Gaps

- `cause` on the thrown realpath error is not representable in `ToolError`; the message matches.
