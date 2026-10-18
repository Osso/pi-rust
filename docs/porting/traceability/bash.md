# coding-agent bash tool traceability

Pinned source: `packages/coding-agent/src/core/tools/bash.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::bash`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `bash.ts:30-48` | `resolveTimeoutMs` validation and maximum | `resolve_timeout_ms`; `rejects_invalid_timeouts`, differential timeout-validation cases. |
| `bash.ts:50-68` | Schema, prompt contribution, details/detach/background-job types | `bash_schema`, `BashToolDetails` fields in result details. |
| `bash.ts:86-127` | `BashOperations.exec` contract | `BashOperations`, `BashExecOptions`, `BashExecResult`. |
| `bash.ts:129-283` | Local operations: shell config, cwd access, stdin transport, output artifact, process-group spawn, streaming, timeout/abort kill, detach | `exec_local`, `create_local_bash_operations`; `bash_tool.rs` (stdin transport in `bash_stdin.rs`, environment, streaming, timeout, abort), `bash_stdin.rs`. |
| `bash.ts:285-330` | Spawn hooks and session-environment exposure | `shell_env` removes `PI_*`; **gap:** session values require the extension context, absent from `AgentToolExecutionContext`. `BashSpawnHook` is not ported. |
| `bash.ts:332-578` | Render components, detach runner helpers, completion polling | **Gap: renderers pending theme.ts**; runner-owned detach requires the unported `detached-job-runner` lifecycle controller. |
| `bash.ts:580-783` | Execute: prefix, accumulator, throttled updates, finish/snapshot, truncation notices, detached output, exit/abort/timeout mapping | `execute_bash`, `UpdateThrottle`, `format_output`; `timeout_and_abort_errors_include_the_full_output_path`, `trailing_newline_does_not_add_a_truncated_line`, `decodes_utf8_characters_split_across_chunks`, `coalesces_streaming_updates`, `reports_non_zero_exit_codes`, `prepends_the_command_prefix`, differential cases. |
| `bash.ts:784-864` | `renderCall`/`renderResult`/`createBashTool` | **Gap: renderers pending theme.ts**; `BashTool` is the `AgentTool` equivalent of `createBashTool`. |

Differential: `oracle/coding-agent-bash-tool-oracle.ts` (request `27800`, 7 cases) runs the real pinned tool and compares results after substituting the random `pi-bash-*.log` spill path.

## Discrepancies

- The runner-owned detach path (`useDetachedRunner`, `backgroundJobs`, `detachRegistry`) is not ported because `detached-job-runner.ts` and `detached-bash-runner.ts` are owned by other slices. `BashDetachOptions` exists for the local operations detach branch.
- Session environment variables are always stripped and never re-added; the extension context that supplies them is not available through `AgentToolExecutionContext`.
- Local shell executables containing a path separator are resolved against the execution `cwd` (matching Windows `spawn` and POSIX `execve` semantics for relative paths).
