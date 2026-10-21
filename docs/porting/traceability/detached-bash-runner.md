# coding-agent detached-bash-runner traceability

Pinned source: `packages/coding-agent/src/core/detached-bash-runner.ts` and `detached-bash-runner-entry.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/core/detached_bash_runner.rs` plus the internal-runner dispatch in `internal_runner.rs`. Tests: `crates/pi-coding-agent/tests/detached_bash_runner.rs` (10 tests, real processes, `/proc`, signals, files).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `DETACHED_BASH_RUNNER_MODE` | Re-export of `internal_runner::DETACHED_BASH_RUNNER_MODE` | `detached_runner_process_entry`, launch test |
| `DETACHED_BASH_LAUNCH_VERSION = 3` | `DETACHED_BASH_LAUNCH_VERSION` | manifest round-trip in every runner test |
| `DetachedBashLaunchManifestData` / manifest checksum | `DetachedBashLaunchManifestData`, `write_detached_bash_launch_manifest`, `hash_json` | `owns_payload_exit_and_commits_one_exact_terminal_input` |
| `writeDetachedBashActivation` / `readDetachedBashActivation` | `write_detached_bash_activation` / `read_detached_bash_activation` | activation write in runner tests, `reads_activation_only_when_the_file_exists` |
| `getDetachedBashRunnerInvocation` (compiled mode) | `get_detached_bash_runner_invocation` | `launches_an_independent_runner_that_remains_the_payload_parent` |
| `launchDetachedBashRunner` (`detached: true`, HOME/PATH only, stdio ignore) | `launch_detached_bash_runner` via `setsid(1)` | independent-runner test asserts payload `PPid` equals runner pid |
| `runDetachedBashRunner`: manifest wait, gated payload, identity, release | `run_detached_bash_runner`, `wait_for_detached_bash_launch_manifest` | terminal/foreground tests |
| `waitForDetachedBashExit`: status responses, cancel adoption, timeout signal | `wait_for_detached_bash_exit`, `signal_payload_group` | `adopts_the_cancelling_revision_terminates_the_payload_group_and_finalizes_aborted`, `answers_status_requests_while_the_payload_runs` |
| Foreground completion (`cancelReason`, `exitCode`, `signal`, `timedOut`) | `DetachedBashForegroundCompletion`, `signal_name` | `completes_a_foreground_run_without_registering_a_background_job` |
| `detachedBashOutcome` (success, signal/exit failure, timeout, abort) | `detached_bash_outcome` | terminal test asserts `Process exited successfully.`; cancellation asserts aborted |
| `finalizeDetachedJobWithRetry` / retry delay 250 ms | `finalize_detached_job_with_retry`, `finalize_detached_job_with_retry_delay`, `DetachedFinalizeError` | `does_not_retry_non_database_terminal_finalization_failures`, `retries_the_same_terminal_input_after_transient_database_failures`, `default_retry_delay_is_250_milliseconds` |
| `appendArtifactRecord` / `readJsonLines` (pyrun shared helpers) | `append_artifact_record`, `read_json_lines`, `JsonLinesChunk` | consumed by the pyrun slice; shared bodies are identical |
| `retainControlDbConnection` | Re-export of `session_control_db::retain_control_db_connection` | control-db tests |
| `terminateForegroundRunner` | `terminate_foreground_runner` | manifest-failure cleanup, cancellation |
| `detached-bash-runner-entry.ts` (argv[2], `.runner-error`) | `internal_runner::run_internal_detached_runner` | `internal_runner` tests + `detached_runner_process_entry` |

Gaps / deviations:

- Node `spawn(..., { detached: true })` is replaced by `setsid(1)` (approved deviation, same as `detached-job-bootstrap`). `setsid` may fork when the caller is already a process-group leader; `read_process_identity` then fails exactly where the TS `readProcessIdentity` would.
- `getDetachedBashRunnerInvocation` has no `--experimental-strip-types` branch: the Rust runner is compiled and always uses the internal mode; `entry_path` selects a wrapper executable.
- `hashJson` uses `serde_json` serialization. Manifest `env` is a `BTreeMap`, so the key order (and therefore the checksum) differs from a JS object built from `process.env`. Rust-written manifests are only consumed by the Rust runner; cross-implementation manifest exchange is not supported (recorded deviation).
- Signal names cover the Linux signal range 1–31; signals outside that range serialize as `null`.

## JS-value bytes (2026-10-20)

`hash_json` and `append_artifact_record` use shared JS stringify, matching TS hash inputs and artifact lines. `js_value_artifacts` checks concrete bytes/digest; numeric Pyrun manifest differential checks shared checksum integration. [Boundary proof](../js-value-semantics-proof.md).
