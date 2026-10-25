# Pyrun extension (`extensions/pyrun/src`)

Port group: `crates/pi-coding-agent-extensions/src/bundled_extensions/pyrun/`.
Registered as `<first-party:pyrun>`; bwrap profiles use the ported
`bundled_extensions::bwrap::backend` helpers.

| Source | Rust | Tests |
|---|---|---|
| `runner.ts`: options resolution, spawn, JSONL correlation, progress, `pi_request`, exit/stderr, abort, dispose | `runner::PyrunRunnerClient` | `pyrun_extension` (fake runner on PATH); differential `differential_pyrun` against real `pyrun-jsonl` and the TS client |
| `eval-tool.ts`: console caps, result formatting, progress formatting, capability snapshot, executor | `eval_tool::*` | `caps_and_formats_console_output`, `formats_result_values_like_the_tool_output`, `formats_progress_updates_in_priority_order`, `formats_the_canonical_tool_result` |
| `index.ts`: builtin Pi dispatcher, parameter validation, tool schema/renderers, executor state per sandbox profile/cwd, session shutdown | `index::*` | `registers_pyrun_eval_with_the_canonical_schema`, `dispatches_builtin_pi_requests`, `validates_dispatch_parameters_like_the_source`, `tool_definition_prepares_a_uuidv7_agent_id` |
| `detached-bridge.ts`: request parse/validate, request/response mailbox persistence | `detached_bridge::*` | `detached_pyrun_bridge` (durable request/response round trip, exact-identity response, non-string/empty `toolCallId` rejection); exercised through the detached runner bridge round-trip (`owns_evaluation_output_and_commits_one_exact_terminal_input`) |
| `detached-runner.ts`: manifest checksum, activation, invocation, runner settlement/finalization, control commands, bridge round-trip | `detached_runner::*` | `detached_pyrun` manifest/activation tests plus `uses_the_pi_internal_runner_mode`, `keeps_the_source_entry_invocation_for_node`, `requires_an_entry_path_when_not_compiled`; `detached_pyrun_runner_process` (`owns_evaluation_output_and_commits_one_exact_terminal_input`, `answers_status_requests_while_the_evaluation_runs`, `adopts_the_cancelling_revision_and_finalizes_aborted`, `launches_an_independent_runner_process_that_owns_the_evaluation`, `pi_binary_dispatches_the_detached_pyrun_runner_mode`, `pi_binary_reports_a_runner_failure_to_the_runner_error_file`); differential `differential_pyrun_detached` (manifest/activation bytes, read-back, validation errors) |
| `detached-runner-entry.ts` | `internal_runner::DETACHED_PYRUN_RUNNER_MODE` dispatch + `run_detached_pyrun_runner` in `bin/pi.rs` | `detached_pyrun_runner_process` real-process tests above; `cli_entry` dispatch tests |
| `detached-evaluation.ts`: artifact root, foreground launch, manifest, observation loop, progress consumption, detach, bridge requests, restore terminal/lost runtime, finalization | `detached_evaluation::*` | `detached_pyrun_evaluation` (`replays_a_complete_terminal_result_without_launching_replacement_code`, `replays_a_complete_terminal_error_with_preceding_console_output`, `ignores_an_incomplete_trailing_record_when_reporting_a_lost_runtime`, `rejects_an_artifact_collision_when_the_params_differ`, `rejects_a_manifest_written_for_another_agent`, `rejects_multiple_terminal_records`, `settles_a_cancelling_job_to_aborted_instead_of_rerunning_it`, `settles_a_legacy_cancelling_job_without_a_tool_call_id`, `returns_the_persisted_cancelling_snapshot_when_no_activation_was_written`, `requires_the_tool_lifecycle_start_timestamp`, `resumes_a_live_runner_and_reports_its_terminal_result`, `detaches_to_a_handle_while_the_runner_keeps_running`, `aborts_a_foreground_runner_when_the_signal_is_cancelled`) |

## Branch mapping (detached runner / evaluation)

- Launch manifest write: `write_detached_pyrun_launch_manifest`; checksum is
  `sha256(JSON.stringify(unsigned))` with JS number formatting for `startedAt`
  and `version` (`pi_js_compat::number::js_number_value`).
- Manifest read: version `3`, finite `startedAt`, and checksum checks; a
  mismatch throws `Invalid detached Pyrun launch manifest: <path>`.
- Activation write/read: atomic `.<tmp>` + fsync + rename + directory fsync;
  a malformed activation file throws (like `JSON.parse`).
- Runner invocation: compiled mode only (`--internal-detached-pyrun-runner
  <manifest>`), executable is the current binary.
- Runner settlement: `evaluate` races the 25ms control poll; cancel commands
  abort the evaluation, `respond` settles pending bridge requests, `status`
  replies with `{outputPath, pendingRequestCount, state: "running"}`.
- Runner finalization: one `result`/`error` artifact record, then
  `finalize_detached_job_with_retry` with `completed`/`failed`/`aborted`.
- Artifact restore: complete lines only (a trailing fragment is ignored); more
  than one terminal record throws; a dead runner with no terminal record
  formats `lost_runtime` with the preserved console output; a live runner
  resumes the existing artifacts.
- Cancelling settlement: a `cancelling` persisted job correlated by
  `toolCallId` (or a legacy manifest without one) is killed, the output file is
  created when missing, and the job is finalized `aborted` with reason
  `Cancelled before session resume`.
- Observation loop: 25ms artifact polling, 1 MiB read cap, dense console
  batching at 100 console records, 3s foreground liveness checks, detach handle
  registration, and abort-token cancellation (once) through
  `controller.cancel` or `terminate_foreground_runner`.

## Deviations / gaps

- The foreground launch path (`runDurableDetachablePyrunEvaluation` without a
  restorable manifest) is ported but not exercised end-to-end: launching the
  compiled runner requires the `pi` binary as `current_exe`, and a cargo test
  binary cannot dispatch the internal runner mode. The observation loop,
  foreground completion, detach, abort, restore/replay, cancelling settlement,
  and the real runner process through `pi` are covered.
- The TypeScript `writeActivation` injection point exists only to test
  activation-persistence failure; Rust has no equivalent test seam. The failure
  branch (register, write activation fails, kill runner, return `false`) is
  ported.
- `detached-runner-entry.ts` (standalone script) throws `Detached Pyrun runner
  requires a launch manifest path`; the compiled binary dispatches through
  `src/internal-runner.ts`, whose message is `Internal detached runner
  <mode> requires a launch manifest path` (same as `internal-runner.ts`).
- `.runner-error` contains the message, not the Node stack (`error.stack ??
  error.message`); Rust has no stack.
- `pi_request` results are `Option<Value>`: `None` is TS `undefined` (not
  handled; the dispatcher falls through) and `Some(Value::Null)` is a handled
  JSON `null`. The runner writes `{}` for `None` and `{"result":null}` for
  `Some(Value::Null)`, matching `JSON.stringify`. Tests:
  `pi_request_undefined_and_null_results_match_typescript_wire_format`,
  `custom_pi_request_handlers_treat_null_as_handled`.
- A progress callback failure rejects the pending evaluation (the TS
  `try`/`catch` around `pending.onProgress`); this is why
  `PyrunRunnerClient::evaluate` takes a fallible progress callback.
- Legacy manifests written without `toolCallId` deserialize as `None` and
  correlate with any replayed call; a detached bridge request from such a
  launch omits `toolCallId` (TS `undefined`) and is rejected by
  `parse_detached_pyrun_bridge_request` like any other missing id.
- A missing tool call id interpolates as `undefined` in the artifact-collision
  message, matching the TypeScript template literal.
- The bwrap sandbox path uses the approved pi-internal fs worker; see
  `bwrap-extension.md`.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).


Callable-status consumer adaptation (2026-10-24): create_pi_capability_snapshot and create_canonical_pyrun_eval_params return Result so the frozen footerData.getExtensionStatuses read propagates the original callback failure. The detached launch helper carries CallbackError to HandlerError::from; existing test data unwraps success. No launch/activation/order, process or detached-runtime behavior change. [Status contract](../../specs/extension-status.md).

## Re-pin detached-output (2026-10-25)

| Frozen symbol / branch | Native proof | Status |
| --- | --- | --- |
| `PyrunArtifactWriter.appendProgress`: immediate <=25 MiB head; permanent tail mode; FIFO drop to 50 MiB bound | `pyrun_capped_artifacts_match_frozen_bytes_and_preserve_live_offsets`, small and 56 MiB progress inputs | done |
| `finish`: terminal reserve, 256-byte marker reserve, omitted byte/record counters, ordered tail, terminal wins above cap | same test, exact SHA256/byte lengths against actual frozen writer including 51 MiB terminal | done |
| Live append-only offsets | exact pre-settlement head length and unchanged output prefix | done |
| Runner progress and terminal writes | shared writer owned by settlement; no legacy uncapped output path | ported; runner process regression pending |
| ENOSPC, exotic filesystem fault identity and numeric extremes | repository parity policy | gap |

Writer differential: `/tmp/bundled3-pyrun-writer-green.log`.

## Re-pin detached-progress (2026-10-25)

| Frozen symbol / branch | Native proof | Status |
| --- | --- | --- |
| `isConsoleProgressUpdate`; console buffering, first stream/stream change; pending/dense threshold of 100 | `pyrun_progress_batches_across_reads_and_flushes_like_frozen` compares snapshots with frozen, including 60+60 records across two reads | done |
| First pending flush timer; no timer reset on subsequent updates; timed flush | same paused-time differential, sparse and dense cases | done |
| Non-console, terminal and truncation records flush; last result retained | same status/truncation/result cases | done |
| `close` cancels timer and discards unreported updates | same close-before-deadline case | done |
| Observer integration | one accumulator across polls, close on every return; removed old per-read batching | ported; observer regression pending |
| Callback throw from timer | process-uncaught authority; synchronous consume failures return the error | runtime failure-shape gap under repository policy |

Differential: `/tmp/bundled3-pyrun-progress-green.log`.

## Re-pin detached-evaluation (2026-10-25)

| Frozen surface | Native implementation / proof | Status |
| --- | --- | --- |
| Artifact root/manifest correlation, cancellation settlement, replay/error/lost runtime, script existence | existing real control-DB replay/collision/cancellation tests | proven |
| Restore ignores truncation markers | `repin_replays_capped_artifacts_without_treating_truncation_as_terminal` RED/GREEN | proven |
| Restore diagnostic before lost runtime; live diagnostic before liveness; empty diagnostic default; completion marker | `repin_restored_runner_error_writes_failure_before_lost_runtime`, `repin_observed_runner_error_wakes_and_keeps_the_original_diagnostic` RED/GREEN, live child terminated | proven |
| Read cursor: 1 MiB bounded reads, fragmentation, immediate drain before liveness/wait | seek/read chunk replaces whole-file read; existing fragmented UTF-16 test; explicit unread-byte state | ported |
| Watch-driven wakeups, 1-second timer, watcher error close and pending notification permit | native fs-watch utility + Notify, watcher dropped on completion/error, real live-runner test | proven normal path |
| Progress flush timing | one 1-second accumulator across observation reads; separate frozen progress differential | proven |
| Foreground/detached observation, abort, detached signal removal | real live child resume/detach/abort tests; activated ownership excludes further signal handling | proven |
| Initial launch via compiled current executable; activation write-failure injection | existing foreground-launch/activation-seam **proof gaps**, not parity approvals | gap |
| Watch setup/runtime OS faults, exotic I/O error shapes and scheduling races | repository exclusion policy | gap |

All sixteen observation/replay tests passed: `/tmp/bundled3-evaluation-green.log`.
No live provider or user state was used.
