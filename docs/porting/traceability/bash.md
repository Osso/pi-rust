# coding-agent bash tool traceability

## Current-pin cycle (open)

Frozen `4f9e6f8a4`; [contract](../../specs/bash-tool.md). Custom execution RED (`bash-core-red-2.log`) proves six mismatches: empty prefix adds a newline, both hook-cwd cases use the old cwd, inherited PI_* values disappear, timeout mapping keeps extra colon segments, and an empty detached log path becomes a detail. Native now follows prefix/path truthiness, passes the hook cwd to both local/runner paths, splits the timeout token like frozen and inherits the ordinary shell environment unchanged.18 actual-frozen cases plus retained real spawn-hook test GREEN2/0 (`bash-core-green.log`), including exact initial/data/final updates, raw human strings, split UTF-8, custom NaN/Infinity/negative timeouts and null/nonzero exits.

Parent cleared mechanical removal of the old session provider in `agent_session.rs`; frozen has no BashSessionEnvironment/provider or exposure option. Removed old-only tests (not skipped):

- `tests/bash_tool_detach.rs::exposes_session_metadata_as_pi_environment_variables`
- `tests/bash_tool_detach.rs::omits_session_metadata_when_session_environment_is_not_exposed`
- `tests/agent_session.rs::bash_tool_exposes_live_session_environment_variables`

Their former injection/stripping contract is absent from frozen; the new inherited-environment differential covers current behavior. Checkpoint `d13646ce` + `fe427eac`: workspace all-target0 errors/0 warnings (`bash-core-clean-workspace.*`), owned bash source clippy0 (`bash-core-clean-clippy.*`), package-global525 library/570 lib-test lint findings remain parent-owned. Canonical protected fixture typecheck0 (`bash-core-types.log`). Four identity-borrow/conversion findings and the retired test's sole macro import were removed without suppression. Numeric API prerequisite: frozen `BashOperations.exec` returns `number | null`; former `Option<i32>` cannot express valid fractional/nonfinite callbacks (`bash-exit-domain-api-red.log`). Widened tool and required executor/UI carriers to `Option<f64>`, with exact integer process-code conversions and no callback coercion. Subsequent actual-frozen26-case behavioral RED proves Rust exponent/Infinity spelling differs; shared JS number spelling makes the same26 GREEN (`bash-exit-domain-behavior-red-2.log`, `bash-exit-domain-green.log`). Parent cleared mechanical bwrap/SSH/RPC/session/UI consumers; binary64 paths no longer narrow in restored UI history. Ten direct executor numeric returns and seven bounded component frames are GREEN (`bash-exit-executor-2.log`, `bash-exit-ui.log`), including direct preservation of negative-zero sign. All-target found one remaining old executor assertion literal (`Some(3)`), mechanically corrected; refreshed all-target/scoped-lint checkpoint remains pending. Whole local/runner/detach/render/API audit remains open. Historical mappings below do not establish current-pin parity.

## Common-pin metadata correction

Frozen `4f9e6f8a4` `packages/coding-agent/src/core/tools/bash.ts:612–617` returns name/label/description/promptSnippet/parameters without `promptGuidelines`. Native `src/core/tools/bash.rs:69,1526–1532` retained the old PI-environment guidance and assigned it conditionally. Actual Cargo `differential_tools_index` RED at `61f810a6` (`fixture-tool-support-test.log`) reports that exact array versus frozen null. Removed the old text and assignment; environment-exposure execution behavior is unchanged. GREEN pending.

## Historical mapping

Source below: `packages/coding-agent/src/core/tools/bash.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`; these line mappings are not renewed common-pin parity evidence.

Rust: `pi_coding_agent::core::tools::bash`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `bash.ts:30-48` | `resolveTimeoutMs` validation and maximum | `resolve_timeout_ms`; `rejects_invalid_timeouts`, differential timeout-validation cases. |
| `bash.ts:50-68` | Schema, prompt contribution, details/detach/background-job types | `bash_schema`, `BashToolDetails` fields in result details. |
| `bash.ts:86-127` | `BashOperations.exec` contract | `BashOperations`, `BashExecOptions`, `BashExecResult`. |
| `bash.ts:129-283` | Local operations: shell config, cwd access, stdin transport, output artifact, process-group spawn, streaming, timeout/abort kill, detach | `exec_local`, `create_local_bash_operations`; `bash_tool.rs` (stdin transport in `bash_stdin.rs`, environment, streaming, timeout, abort), `bash_stdin.rs`. Detach returns the source default result (`jobId: "background"`, no `logPath`), flushes durable output once, keeps the timeout armed so the backgrounded process is still killed, and removes the artifact directory when the command finishes in the foreground. Proof: `local_operations_detach_uses_the_default_result`, `local_operations_remove_artifacts_when_not_detached`, `detach_keeps_the_timeout_active`. |
| `bash.ts:285-330` | Historical spawn hooks/session-environment exposure | Current `BashSpawnContext`, `BashSpawnHook`, `resolve_spawn_context`; retained `applies_the_spawn_hook_to_command_cwd_and_environment`. Legacy session provider/exposure and tests removed in current-pin checkpoint above. Runner-owned shell-path behavior remains under audit. |
| `bash.ts:332-578` | Render components, detach runner helpers, completion polling | `format_bash_call`/`render_call` and `render_result` (expanded/collapsed preview via `visual_truncate`, truncation warnings, footer stripping, elapsed label) are covered by renderer differential cases. `render_call` reuses the last `Text` component and records `startedAt`; `render_result` records `endedAt` in the shared `ToolRenderState` (`edit_tool_preview::bash_call_records_the_start_and_result_records_the_end`); the source's one-second `setInterval` invalidation is covered by the tool-execution timer (`poll`). Runner-owned detach is implemented in `execute_runner_owned_bash`/`observe_foreground_bash_runner`/`activate_detached_bash_runner`; process-level proof lives in `tests/detached_bash_runner.rs` because the tool launches the compiled internal runner entry. Detached results keep the truncation footer (`detached_output_keeps_the_truncation_footer`). |
| `bash.ts:580-783` | Execute: prefix, accumulator, throttled updates, finish/snapshot, truncation notices, detached output, exit/abort/timeout mapping | `execute_bash`, `UpdateThrottle`, `format_output`; `timeout_and_abort_errors_include_the_full_output_path`, `trailing_newline_does_not_add_a_truncated_line`, `decodes_utf8_characters_split_across_chunks`, `coalesces_streaming_updates`, `reports_non_zero_exit_codes`, `prepends_the_command_prefix`, differential cases. The throttle tracks `updateDirty`, emits only dirty final snapshots, and never fires its timer after completion (`finishes_without_an_extra_update`, `does_not_emit_updates_after_completion`). |
| `bash.ts:784-864` | `renderCall`/`renderResult`/`createBashTool` | `render_call`, `render_result`, `create_bash_tool_definition`/`create_bash_tool`; differential expanded, collapsed, and truncated result cases. `constrainedSampling` mirrors `getExperimentalToolSampling()` (`tool_definitions_expose_experimental_constrained_sampling`). `BashToolDetachRegistry` is a Rust alias for the shared `ToolDetachRegistry`. |

Differential: `oracle/coding-agent-bash-tool-oracle.ts` (request `27800`, 7 cases) runs the real pinned tool and compares results after substituting the random `pi-bash-*.log` spill path.

## Historical discrepancies (pending current-pin audit)

- `BashDetachOptions.adopt` is not modeled: it receives a live `ChildProcess`, exit promise, and timeout handle that the typed Rust API cannot expose. No source caller sets it; the default detach result is ported.
- `bash-close-hang-windows.test.ts` exercises a Windows close-event hang; the crate depends on `rustix`/`nix` and implements Unix process groups, so there is no Windows execution path.
- The tool-level runner-owned detach path (`useDetachedRunner`, `backgroundJobs`, `detachRegistry`) is wired, including restored-tool-call replay, but integration tests cannot exercise it end-to-end: `launch_detached_bash_runner` starts the compiled binary's internal runner mode, and the workspace has no `pi` binary target. The runner loop itself is proven with real processes in `tests/detached_bash_runner.rs`.
- `renderResult` rebuilds its `BashResultRenderComponent` each render instead of caching visual lines on the component; observable output is identical.
- `createBashTool` cannot copy `promptSnippet`/`promptGuidelines` onto the tool because `AgentTool` has no prompt fields; the definition created by `createBashToolDefinition` carries them.
- `renderCall`/`renderResult` read and write `startedAt`/`endedAt` through the shared
  `ToolRenderState` (`ToolRenderContext.state`), matching the source's mutable `state` object. The
  source's per-row `setInterval(() => context.invalidate(), 1000)` is not created by the renderer;
  the tool-execution component's driver-polled one-second timer already invalidates the row while
  the call is partial.
- Local shell executables containing a path separator are resolved against the execution `cwd` (matching Windows `spawn` and POSIX `execve` semantics for relative paths).
