# agent-session-runtime.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session_runtime.rs`.

## Common-pin re-port in progress (2026-10-23)

[Contract](../../specs/agent-session-runtime.md). Historical inventory below is
not whole-module acceptance. Parent cleared the cloneable handle migration after
confirming worker-thread relocation consumers: agent-core `agent_loop.rs:43/64`
uses `tokio::spawn`; `agent_session.rs` activates the prepared relocation from
AgentEnd; runtime relocation handlers are Send futures. Send carriers remain.

| Frozen boundary | Native checkpoint | Proof |
|---|---|---|
| :167–185 acquire/runLifecycleTransition FIFO | one private Tokio owned mutex permit; no outer runtime operation lock | actual frozen prepare/new/activate vs real native concurrent lifecycle test |
| :369–408 prepare retains permit through activation and releases before continue | owned permit stored with prepared relocation; released on activation success/error | `prepared_relocation_holds_lifecycle_gate_until_activation` RED `runtime-lifecycle-red.log` (new completed early), GREEN `runtime-lifecycle-green.log` |
| constructor/apply bind runtime actions | cloneable Arc-inner handle, short state mutex, weak handlers; obsolete share wrapper deleted | same real-runtime gate test compiles constructor/weak handlers |
| :418–420 reload under lifecycle gate | runtime reload forwarding added under the same permit | integration proof pending |
| beforeInvalidate/beforeRestart/rebind ordering | existing Send callbacks retained; interactive installation/deletion of adopt_session pending | not accepted yet |
| whole source branch coverage | working-directory recovery, archived resume/trust propagation, restart cleanup, teardown and creation side effects still auditing | not accepted yet |

No state mutex guard crosses an await. Callers clone the runtime handle directly;
print/RPC/interactive/main and harnesses no longer own an outer operation mutex.
This checkpoint preserves existing standalone gates, fork-guard removal and
CallbackError carriers. Logs: `/tmp/pi-rust-repin-lossless-interactive/`.


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
| `bindSessionRuntimeActions` | `AgentSessionRuntime::bind_session_runtime_actions` in constructor/apply | `bound_runtime_actions_relocate_through_the_extension_context` |
| `emitBeforeSwitch` / `emitBeforeFork` | `emit_before_switch` / `emit_before_fork` | cancellation test |
| `teardownCurrent` | `teardown_current` (+ `teardown_current_with`) | shutdown event test |
| `finishSessionReplacement` | `finish_session_replacement` | `withSession` receives `ReplacedSessionContext` |
| `runDetachedJobArtifactCleanup` | `run_detached_job_artifact_cleanup` | invoked by `create_agent_session_runtime` |

## Port fixes

- Common-pin fork correction2026-10-23: remove the old-only file-exists/unsaved-session guard. Frozen runtime.ts:568–583 opens the manager then calls createBranchedSession, so a fork containing only restriction context followed by clone reports `Entry <leafId> not found`. Full RPC differential exercises this persisted fork→clone boundary and asserts both complete serialized errors using each side's captured leaf ID and the identical literal error template; no error output is normalized or removed.

- Standalone runtime creation/resume/import gates and proof: [standalone slice](standalone-worker.md).

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
- Constructor binds runtime-owned extension actions through the one Arc-inner
  handle. `apply` re-binds them on the replacement session. The obsolete outer
  operation mutex and `share_agent_session_runtime` wrapper are deleted.
  Activation propagates relocation failures before continuing.
- Callers: `main` startup, print/RPC modes, and the interactive mode all take the
  shared runtime handle, so extension `ctx.relocate`, `ctx.relocateAfterToolResult`,
  and the tool-result activation path reach the runtime.

## Gaps

- Project-trust context factories and `assertMainSessionRuntimeAvailable` refresh
  bookkeeping are not part of the factory options.
- `teardownCurrent` does not flush pending session entries beyond `abort()`.

## v15 import/fork names (approved 2026-10-20)

Name paths follow `c248b0067` ([reference lock](../source-lock.json)).
Copied imports remove stale destination metadata after copying; lexically identical imports
return before copy/removal. Forks reset name state rather than inheriting a parent name.
Proof: `agent_session_runtime::import_does_not_inherit_stale_metadata_or_historical_jsonl_names`,
`fork_does_not_inherit_the_parent_session_name`, existing lexical/symlink import test.
