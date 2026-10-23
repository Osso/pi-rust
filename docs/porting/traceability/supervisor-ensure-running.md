# supervisor/ensure-running.ts

Pin: [source-lock](../source-lock.json). Contract: [spec](../../specs/supervisor-ensure-running.md).

| Surface / branch | Implementation | Proof |
|---|---|---|
| SUPERVISOR_AUTOSTART_ENV; getSupervisorStartLockPath; resolveSupervisorLaunchInvocation | `supervisor/ensure_running.rs` | Actual-frozen invocation differential, compiled/script flags/env and raw lock-path units |
| ensureSupervisorRunning; coordinated startup; under-lock reprobe; compatible reuse | same | Concurrent native starters; real owned detached resident; seven actual-frozen startup branch scenarios |
| missing/external/incompatible residents; launch rejection; readiness/exit timeout | same | Seven source differential scenarios; real incompatible owned Pi process replacement |
| default probe/launch/terminate and identity validation | shared resident transport/runtime-process | Real Python resident fixture at a verified Pi entrypoint path; detached session, cwd/env, reuse/replacement and cleanup |
| startup lock retry schedule, stale/release protocol | shared `utils/lockfile.rs::acquire_with_retry_delays` | `supervisor_ensure_running::separate_process_starters_recover_stale_lock_and_launch_once`: two independent test processes, abandoned stale lock, one real detached resident launch, released lock, owned process cleanup |
| target missing/nonexecutable | preflight + shared Node spawn error formatter | Real native launch boundary ENOENT/EACCES tests |
| late target exec failure/race | **gap**, approved2026-10-23 in [AGENTS.md](../../../AGENTS.md) | setsid(1) cannot report ENOEXEC or post-preflight disappearance as spawn errors; exact ENOENT/EACCES preflight remains. No acknowledgement protocol. |
| homedir SystemError propagation and client Error.message catch | NativeNodeError through startup/coordinator/dependency carriers | `supervisor_startup_errors`: concrete homedir SystemError fixture with constructor/properties, actual-frozen injected rejection propagation, and client reason excludes Error-name prefix. Does not force a live OS passwd failure. |

Shared `probe_resident_console` uses existing connect/parser and sends `probe`, never `attach`. Caller-provided lock delays leave existing callers' default schedule unchanged. Wider transport numeric/parser deltas remain owned by that module, not fabricated here.
