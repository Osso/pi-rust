# coding-agent detached-job-lifecycle traceability

Pinned source: `packages/coding-agent/src/core/detached-job-lifecycle.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: controller lives in `crates/pi-coding-agent-tools/src/core/detached_job_runner.rs` (the TS controller factory and runner types are one coupled module). Tests: `crates/pi-coding-agent/tests/detached_job_runner.rs`, `crates/pi-coding-agent/tests/detached_bash_runner.rs`.

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `createDetachedJobLifecycleController` | `DetachedJobLifecycleController::new(DetachedJobLifecycleControllerOptions)` | every controller test |
| `findPersistedBashJob` filter (parent, `Bash output` ref, worker/result tool call) and sort | `find_persisted_bash_job`, `bash_job_matches`, `compare_persisted_bash_jobs` | `finds_the_active_bash_attempt_for_a_repeated_tool_call` |
| `launchDetachedBashJob` | `launch_detached_bash_job` | `preserves_retained_artifacts_when_launch_manifest_persistence_fails`, `launches_an_independent_runner_that_remains_the_payload_parent` |
| `reserveDetachedJob` retry loop | `reserve_detached_job` | `reserves_distinct_jobs_without_reusing_retained_artifact_directories` |
| `detachedJobArtifactRoot` | `detached_job_artifact_root` | session-isolation test |
| `terminateDetachedRunner` | `detached_bash_runner::terminate_foreground_runner` | manifest-failure test |
| `registerDetachedJob` (output ref first, coordinator prepare/commit, identity) | `register_detached_job` | `binds_preallocated_artifacts_ownership_projection_and_exact_finalization` |
| `observe` recovery gate | `observe_detached_job`, `should_recover_dead_child` | dead/alive/other-session/terminal observe tests |
| `cancel` / `markDetached` store publication | `cancel`, `mark_detached` | cancellation test |

Gaps / deviations:

- The controller is constructed with `Arc<LifecycleCoordinator>`/`Arc<MultiAgentStore>` instead of JS object references.
- `launchBash` returns `Result`; TS throws. Messages match (`Could not own detached bash job: <error>`, `Detached job session path must have a file name`, `Detached job ownership identity is incomplete`).
- Launch relies on `setsid(1)` (approved deviation) and `read_process_identity` after spawn, exactly like the TS `readProcessIdentity(runnerPid)`.
