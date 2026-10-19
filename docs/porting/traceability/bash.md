# coding-agent bash tool traceability

Pinned source: `packages/coding-agent/src/core/tools/bash.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::bash`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `bash.ts:30-48` | `resolveTimeoutMs` validation and maximum | `resolve_timeout_ms`; `rejects_invalid_timeouts`, differential timeout-validation cases. |
| `bash.ts:50-68` | Schema, prompt contribution, details/detach/background-job types | `bash_schema`, `BashToolDetails` fields in result details. |
| `bash.ts:86-127` | `BashOperations.exec` contract | `BashOperations`, `BashExecOptions`, `BashExecResult`. |
| `bash.ts:129-283` | Local operations: shell config, cwd access, stdin transport, output artifact, process-group spawn, streaming, timeout/abort kill, detach | `exec_local`, `create_local_bash_operations`; `bash_tool.rs` (stdin transport in `bash_stdin.rs`, environment, streaming, timeout, abort), `bash_stdin.rs`. |
| `bash.ts:285-330` | Spawn hooks and session-environment exposure | `BashSpawnContext`, `BashSpawnHook`, `BashSessionEnvironmentProvider`, `resolve_spawn_context`; `exposes_session_metadata_as_pi_environment_variables`, `omits_session_metadata_when_session_environment_is_not_exposed`, `applies_the_spawn_hook_to_command_cwd_and_environment`. The agent session supplies the provider. |
| `bash.ts:332-578` | Render components, detach runner helpers, completion polling | `format_bash_call`/`render_call` and `render_result` (expanded/collapsed preview via `visual_truncate`, truncation warnings, footer stripping, elapsed label) are covered by renderer differential cases. `render_call` records `startedAt` and `render_result` records `endedAt` in the shared `ToolRenderState` (`edit_tool_preview::bash_call_records_the_start_and_result_records_the_end`); the source's one-second `setInterval` invalidation is covered by the tool-execution timer (`poll`). Runner-owned detach is implemented in `execute_runner_owned_bash`/`observe_foreground_bash_runner`/`activate_detached_bash_runner`; process-level proof lives in `tests/detached_bash_runner.rs` because the tool launches the compiled internal runner entry. |
| `bash.ts:580-783` | Execute: prefix, accumulator, throttled updates, finish/snapshot, truncation notices, detached output, exit/abort/timeout mapping | `execute_bash`, `UpdateThrottle`, `format_output`; `timeout_and_abort_errors_include_the_full_output_path`, `trailing_newline_does_not_add_a_truncated_line`, `decodes_utf8_characters_split_across_chunks`, `coalesces_streaming_updates`, `reports_non_zero_exit_codes`, `prepends_the_command_prefix`, differential cases. |
| `bash.ts:784-864` | `renderCall`/`renderResult`/`createBashTool` | `render_call`, `render_result`, `create_bash_tool_definition`/`create_bash_tool`; differential expanded, collapsed, and truncated result cases. |

Differential: `oracle/coding-agent-bash-tool-oracle.ts` (request `27800`, 7 cases) runs the real pinned tool and compares results after substituting the random `pi-bash-*.log` spill path.

## Discrepancies

- The tool-level runner-owned detach path (`useDetachedRunner`, `backgroundJobs`, `detachRegistry`) is wired, including restored-tool-call replay, but integration tests cannot exercise it end-to-end: `launch_detached_bash_runner` starts the compiled binary's internal runner mode, and the workspace has no `pi` binary target. The runner loop itself is proven with real processes in `tests/detached_bash_runner.rs`.
- `renderCall`/`renderResult` read and write `startedAt`/`endedAt` through the shared
  `ToolRenderState` (`ToolRenderContext.state`), matching the source's mutable `state` object. The
  source's per-row `setInterval(() => context.invalidate(), 1000)` is not created by the renderer;
  the tool-execution component's driver-polled one-second timer already invalidates the row while
  the call is partial.
- `PI_*` session values come from `BashToolOptions.session_environment`, supplied by `AgentSession::rebuild_base_tool_definitions` (live session id/file, model provider/id, thinking level). The source reads the same values from the extension context inside `execute`; the provider is the typed equivalent because Rust core tools do not receive the extension context. Proof: `agent_session::bash_tool_exposes_live_session_environment_variables`.
- Local shell executables containing a path separator are resolved against the execution `cwd` (matching Windows `spawn` and POSIX `execve` semantics for relative paths).
