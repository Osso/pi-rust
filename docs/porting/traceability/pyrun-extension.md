# Pyrun extension (`extensions/pyrun/src`)

Port group: `crates/pi-coding-agent/src/bundled_extensions/pyrun/`.
Registered as `<first-party:pyrun>`; bwrap profiles use the ported
`bundled_extensions::bwrap::backend` helpers.

| Source | Rust | Tests |
|---|---|---|
| `runner.ts`: options resolution, spawn, JSONL correlation, progress, `pi_request`, exit/stderr, abort, dispose | `runner::PyrunRunnerClient` | `pyrun_extension` (fake runner on PATH); differential `differential_pyrun` against real `pyrun-jsonl` and the TS client |
| `eval-tool.ts`: console caps, result formatting, progress formatting, capability snapshot, executor | `eval_tool::*` | `caps_and_formats_console_output`, `formats_result_values_like_the_tool_output`, `formats_progress_updates_in_priority_order`, `formats_the_canonical_tool_result` |
| `index.ts`: builtin Pi dispatcher, parameter validation, tool schema/renderers, executor state per sandbox profile/cwd, session shutdown | `index::*` | `registers_pyrun_eval_with_the_canonical_schema`, `dispatches_builtin_pi_requests`, `validates_dispatch_parameters_like_the_source`, `tool_definition_prepares_a_uuidv7_agent_id` |
| `detached-bridge.ts`: request parse/validate, request/response mailbox persistence | `detached_bridge::*` | compiled; exercised through detached runner settlement paths |
| `detached-runner.ts`: manifest checksum, activation, invocation, runner settlement/finalization, control commands, bridge round-trip | `detached_runner::*` | `detached_pyrun` manifest/activation/invocation tests; runner settlement requires a registered control-db job |
| `detached-runner-entry.ts` | `internal_runner::DETACHED_PYRUN_RUNNER_MODE` dispatch + `run_detached_pyrun_runner` | manifest tests; entry dispatch owned by the bootstrap |
| `detached-evaluation.ts`: artifact root, foreground launch, manifest, observation loop, progress consumption, detach, bridge requests, restore terminal/lost runtime, finalization | `detached_evaluation::*` | manifest/restore helpers covered indirectly; foreground observation is OS/timing dependent and not differentially compared |

## Deviations / gaps

- Detached foreground evaluation has no end-to-end test: it needs a persisted
  `MultiAgentStore` session, control-db metadata row, lifecycle coordinator, and
  a real pi binary for the internal runner mode. The manifest/activation
  protocol and checksum are tested; the observation loop and cancelling-settle
  path are ported but unverified at the process boundary.
- `detached-evaluation.ts` restore path is simplified: terminal and
  lost-runtime restoration are implemented, but the `cancelling` settlement and
  artifact-collision handling for every legacy manifest shape are not fully
  reproduced.
- The TypeScript extension passes `PyrunRunnerOptions` verbatim into the launch
  manifest; Rust serializes the same fields with `camelCase` names.
- `pi_request` handlers return `Value`; a handler returning JSON `null` is
  treated as "not handled", matching the TS `undefined` convention. A real
  capability returning `null` is indistinguishable (source has no such case).
- The bwrap sandbox path uses the approved pi-internal fs worker; see
  `bwrap-extension.md`.
