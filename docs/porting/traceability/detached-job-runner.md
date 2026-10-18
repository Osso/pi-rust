# coding-agent detached-job-runner traceability

Pinned source: `packages/coding-agent/src/core/detached-job-runner.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/core/detached_job_runner.rs`. Tests: `crates/pi-coding-agent/tests/detached_job_runner.rs` (13 behavioral).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `DetachedJobOwnershipIdentity`, `DetachedJobOutcome`, `DetachedJobTerminalInput`, `DetachedJobTerminalOutput` | Re-exported from `core::session_control_db` (`lifecycle.rs`, ported by the control-db slice) | `binds_preallocated_artifacts_ownership_projection_and_exact_finalization` |
| `DetachedJobArtifacts`, `ReservedDetachedJob`, `DetachedJobOwnership` | Structs | artifact tests below |
| `DetachedJobArtifactExistsError` | `DetachedJobError::ArtifactExists` (message `Detached job artifact directory already exists: <dir>`) | `rejects_an_existing_detached_job_directory_before_runner_launch` |
| `createDetachedJobArtifacts` | `create_detached_job_artifacts` (recursive `0700`) | `builds_an_immutable_terminal_input_from_the_completed_output_artifact`, `artifact_paths_are_scoped_under_the_detached_jobs_root` |
| `reserveDetachedJobArtifacts` | `reserve_detached_job_artifacts` (`EEXIST` → `ArtifactExists`) | `rejects_an_existing_detached_job_directory_before_runner_launch` |
| `detachedJobDirectory` id validation (`""`, `/`, `\`) | `detached_job_directory` | `rejects_job_ids_that_escape_their_artifact_root` |
| `createDetachedJobTerminalInput` (fsync, SHA-256 hex, size) | `create_detached_job_terminal_input` + `sha256_hex` | `builds_an_immutable_terminal_input_from_the_completed_output_artifact` asserts `f7047c…` and size 14 |
| `DetachedJobLifecycleController.allocateJobId` | `allocate_job_id` | `binds_preallocated_artifacts_ownership_projection_and_exact_finalization` |
| `cancel` (coordinator + store publication) | `cancel` | `requests_detached_cancellation_and_publishes_cancelling_state` |
| `createArtifacts` / `reserveJob` (retry on existing directory) | `create_artifacts` / `reserve_job` | `reserves_distinct_jobs_without_reusing_retained_artifact_directories` |
| `findBashJobByToolCallId` (active-first, then `updatedAt` descending locale compare) | `find_bash_job_by_tool_call_id` | `finds_the_active_bash_attempt_for_a_repeated_tool_call` |
| `launchBash` (reserve, spawn runner, identity, manifest, kill on failure) | `launch_bash` | `preserves_retained_artifacts_when_launch_manifest_persistence_fails` |
| `markDetached` | `mark_detached` | covered by lifecycle-coordinator tests plus `mark_detached` wiring |
| `observe` (dead-identity recovery gated on session/owner/session-path) | `observe` | `recovers_an_active_job_when_its_persisted_runner_identity_is_no_longer_alive`, `keeps_an_active_job_running_when_its_exact_persisted_runner_identity_is_alive`, `observes_and_publishes_a_terminal_snapshot_committed_by_an_external_runner` |
| `register` (background child, output file ref first, exact ownership) | `register` | `binds_preallocated_artifacts_ownership_projection_and_exact_finalization` |
| `detachedJobArtifactRoot` (session stem, empty → throw) | `detached_job_artifact_root` | `isolates_reused_agent_ids_by_supervisor_session`, `artifact_paths_are_scoped_under_the_detached_jobs_root` |
| `terminateDetachedRunner` (group then pid, ESRCH ignored) | `detached_bash_runner::terminate_foreground_runner` | `preserves_retained_artifacts_when_launch_manifest_persistence_fails` (runner is killed) |
| `DetachedJobRunnerContract` | `DetachedJobRunnerContract` trait | compile-time surface; no TS call sites in this module |

Gaps / deviations:

- `createDetachedJobArtifacts` uses `DirBuilder::mode(0o700)`; Node applies `mode` subject to umask identically.
- `register` takes a typed `RegisterDetachedJobInput`; JS `agentType` string is `DetachedJobAgentType`.
- `launchBash` accepts an injected `WriteBashLaunchManifest` hook (used by tests) instead of module mocking.
- `observe` returns `Result<Option<AgentSnapshot>>`; TS throws on control-db errors, so the error is preserved rather than swallowed.
