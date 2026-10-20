# agent-session-runtime.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session_runtime.rs`.

| TS | Rust | Tests |
| --- | --- | --- |
| `AgentSessionRuntime` | `AgentSessionRuntime` | `tests/agent_session_runtime.rs` (7) |
| `createAgentSessionRuntime` | `create_agent_session_runtime` | same |
| `CreateAgentSessionRuntimeFactory` | `CreateAgentSessionRuntimeFactory` | same |
| `switchSession` | `switch_session` | cancellation test |
| `newSession` | `new_session` | before-switch/shutdown/replaced-context test |
| `fork` | `fork` | invalid entry + before-fork cancellation |
| `relocate` | `relocate` | cwd + `cwd_changed` note |
| `restart` | `restart` | process path injected; default restarter uses `core::self_restart` |
| `importFromJsonl` | `import_from_jsonl` | `import_from_jsonl_switches_to_the_imported_session`, `import_through_a_symlinked_session_dir_preserves_the_source_file` |
| Session replacement re-binds the extension runner's `sessionManager` | `create_runtime` builds a new `AgentSession`, whose `build_runtime` passes the new shared handle | `session_replacement_rebinds_the_shared_extension_handle` |
| `dispose` | `dispose` | `tests/agent_session_runtime.rs::dispose_emits_session_shutdown_quit_before_invalidating_the_session` |
| `prepare/activateToolResultRelocation` | same | `bound_runtime_actions_relocate_through_the_extension_context` |
| `bindSessionRuntimeActions` | `AgentSessionRuntime::bind_session_runtime_actions` + `share_agent_session_runtime` | `bound_runtime_actions_relocate_through_the_extension_context` |
| `emitBeforeSwitch` / `emitBeforeFork` | `emit_before_switch` / `emit_before_fork` | cancellation test |
| `teardownCurrent` | `teardown_current` (+ `teardown_current_with`) | shutdown event test |
| `finishSessionReplacement` | `finish_session_replacement` | `withSession` receives `ReplacedSessionContext` |
| `runDetachedJobArtifactCleanup` | `run_detached_job_artifact_cleanup` | invoked by `create_agent_session_runtime` |

## Port fixes

- `fork` captures the current session file before locking the shared
  `SessionManager`; the earlier Rust body called `AgentSession::session_file()`
  while holding the manager guard for `newSession`, which deadlocked on the
  in-memory fork path. The TypeScript source reads `sessionFile` through the
  manager getter, so this is a port bug, not a source discrepancy. Covered by
  `ui_tests::fork_selector_forks_from_the_selected_user_message`.

## Behavior

- `dispose` emits `session_shutdown` with reason `quit` before
  `beforeSessionInvalidate` and `session.dispose()`, matching the source.
- `importFromJsonl` compares `path.resolve(destinationPath)` with the resolved
  input path lexically (`resolve_path` with tilde expansion disabled) instead of
  canonicalizing; resolution errors propagate. The source uses
  `resolve(destinationPath) !== resolvedPath`. When the lexical paths differ but
  both names resolve to the same file, the copy is a no-op, matching
  `copyFileSync`; `std::fs::copy` alone would truncate the source.
- `share_agent_session_runtime` wraps a freshly created runtime and binds the
  runtime-owned extension actions, matching the TypeScript constructor's
  `bindSessionRuntimeActions`. `apply` re-binds them on the replacement session
  using the stored shared handle. The activate handler propagates relocation
  failures like the source (`activateToolResultRelocation` is awaited).
- Callers: `main` startup, print/RPC modes, and the interactive mode all take the
  shared runtime handle, so extension `ctx.relocate`, `ctx.relocateAfterToolResult`,
  and the tool-result activation path reach the runtime.

## Gaps

- Project-trust context factories and `assertMainSessionRuntimeAvailable` refresh
  bookkeeping are not part of the factory options.
- `teardownCurrent` does not flush pending session entries beyond `abort()`.
