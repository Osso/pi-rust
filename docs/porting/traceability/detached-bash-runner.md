# coding-agent detached-bash-runner traceability

## Current-pin atomic writer prerequisite

[Contract](../../specs/bash-atomic-writer.md). Parent cleared whole frozen writer270–295. `bash-atomic-writer-red-2.log` proves eleven native differences across twelve real source/native histories: ordinary OS messages instead of Node diagnostics; ignored close failures; incorrect write/fsync-vs-close precedence; rename/directory-sync errors. The initial red log faulted the oracle's own snapshot close and is INVALID evidence; snapshots now occur after disabling the source fault.

| Actual frozen/native scenario | Observable proof |
|---|---|
| Success | Complete JSON+LF bytes, mode0600, no temporary file |
| Temporary path is a directory | Original EISDIR/open message and unchanged directory |
| Write EIO; write EIO + close EBADF | Empty temporary file; close overrides write only in combined case |
| Write-file close EBADF | Written temporary file retained, final absent |
| File fsync EIO; file close-after-fsync EBADF; both | Written temporary retained; exact pathless diagnostics and finally-close precedence |
| Rename EIO | Exact source/destination Node message; written temporary retained |
| Directory fsync EIO; directory close-after-fsync EBADF; both | Final file already installed; exact pathless diagnostics and close precedence |

Native sequence is now write/close → reopen read-only/fsync/finally-close → rename → directory-open/fsync/finally-close. Existing native `fsync_directory` uses the same helper and returns its existing module error carrier; builtin caller audit finds only this writer. Node fd-only fsync now omits the empty pathname through shared authority, not string surgery. `bash-atomic-writer-green-2.log`: GREEN12 plus21 actual real-runner histories, including writer-only activation.json.tmp rejection with unchanged activation/registry/update traces. No new queue/fallback/production seam or generic-error replacement. `e2db715d` + lifecycle-getter65b9a908; parent49b0d18f mergedf954d244. Workspace all-target0errors/0warnings (`bash-atomic-writer-workspace.*`), compatclippy0 and ownedwriter/bash0; package-global1075 remain (`bash-atomic-writer-clippy-*`). Strict protected3fixturecompiler0/Biome0 (`bash-atomic-writer-types.log`), format0 and changed-line Rust/TS readability clean. Retained complete launch-file/checksum6, restored7, spill18 and durable-descriptor2 histories GREEN after C-fixture changes (`bash-atomic-writer-retained-green.log`). Inventory PASS686/186679,403done/19gap/87partial/177todo (`bash-atomic-writer-inventory.log`). External schema row below remains parent-owned.

## Current-pin numeric prerequisite (whole runner still open)

Frozen `4f9e6f8a4` manifest numbers are not integers: the actual writer accepts finite fractions and includes them in JSON/checksums. `differential_bash_manifest_numbers` first proves native rejection of ±1.25 versus frozen success; the native input/manifest timeoutMs carrier now preserves binary64 and uses shared JS number serialization, retaining TS integer-manifest bytes. Bash launch no longer truncates milliseconds. A real-runner RED (`negative_manifest_timeout_expires_before_the_payload_can_complete`) proves the previous unsigned-absolute conversion moved a past deadline into the future. Runtime deadlines now use shared `js_now_millis()+timeoutMs`, matching frozen171/183-185 without abs, coercion or duration panic.

| Remaining current-pin boundary | State |
| --- | --- |
| null/malformed external manifest-schema behavior, including checksum-valid source records beyond the typed carrier | Pending under whole detached-bash-runner ownership; explicitly excluded from this approved finite-number prerequisite. |

## Historical mapping (not renewed current-pin proof)

Pinned source: `packages/coding-agent/src/core/detached-bash-runner.ts` and `detached-bash-runner-entry.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent-tools/src/core/detached_bash_runner.rs` plus the internal-runner dispatch in `internal_runner.rs`. Tests: `crates/pi-coding-agent/tests/detached_bash_runner.rs` (10 tests, real processes, `/proc`, signals, files).

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

Atomic manifest writes also use JS stringify: `launch_manifest_enumerates_numeric_environment_names_like_javascript` covers env keys 1/2/10. Activation identities share that writer; filesystem error propagation is unchanged.

## Retained UTF-8 boundary (2026-10-26)

`read_detached_bash_activation` and `read_detached_bash_launch_manifest` retain strict reads: machine-written activation/checksummed launch metadata; invalid bytes require corruption (parent-approved OS/data-fault gap, 2026-10-26). Frozen `detached-bash-runner.ts:75,252` reads UTF-8 with replacement; this is an exclusion, not equivalent invalid-byte behavior. See the [complete file-read audit](../utf8-decoding-sweep.md).
