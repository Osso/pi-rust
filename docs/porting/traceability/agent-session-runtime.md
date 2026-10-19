# agent-session-runtime.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session_runtime.rs`.

| TS | Rust | Tests |
| --- | --- | --- |
| `AgentSessionRuntime` | `AgentSessionRuntime` | `tests/agent_session_runtime.rs` (5) |
| `createAgentSessionRuntime` | `create_agent_session_runtime` | same |
| `CreateAgentSessionRuntimeFactory` | `CreateAgentSessionRuntimeFactory` | same |
| `switchSession` | `switch_session` | cancellation test |
| `newSession` | `new_session` | before-switch/shutdown/replaced-context test |
| `fork` | `fork` | invalid entry + before-fork cancellation |
| `relocate` | `relocate` | cwd + `cwd_changed` note |
| `restart` | `restart` | process path injected; default restarter uses `core::self_restart` |
| `importFromJsonl` | `import_from_jsonl` | import test |
| Session replacement re-binds the extension runner's `sessionManager` | `create_runtime` builds a new `AgentSession`, whose `build_runtime` passes the new shared handle | `session_replacement_rebinds_the_shared_extension_handle` |
| `dispose` | `dispose` | — |
| `prepare/activateToolResultRelocation` | same | — |
| `emitBeforeSwitch` / `emitBeforeFork` | `emit_before_switch` / `emit_before_fork` | cancellation test |
| `teardownCurrent` | `teardown_current` (+ `teardown_current_with`) | shutdown event test |
| `finishSessionReplacement` | `finish_session_replacement` | `withSession` receives `ReplacedSessionContext` |
| `runDetachedJobArtifactCleanup` | `run_detached_job_artifact_cleanup` | invoked by `create_agent_session_runtime` |

## Gaps

- Extension runner relocate handlers (`set_relocate_handler`,
  `set_tool_result_relocation_handlers`) are not bound by the runtime; the prepare/
  activate methods exist but no extension can trigger them yet.
- Project-trust context factories and `assertMainSessionRuntimeAvailable` refresh
  bookkeeping are not part of the factory options.
- `teardownCurrent` does not flush pending session entries beyond `abort()`.
