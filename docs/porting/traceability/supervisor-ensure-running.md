# supervisor/ensure-running.ts

Pin: [source-lock](../source-lock.json), `4f9e6f8a4`. Contract: [spec](../../specs/supervisor-ensure-running.md). Whole-module audit completed 2026-10-25 under the lighter-proof rule.

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

Shared `probe_resident_console` sends `probe`, never `attach`. Caller-provided lock delays leave existing callers' default schedule unchanged. [Transport common-pin audit](resident-console-transport.md) retains reciprocal wire proof.

## Successor audit

All 228 frozen lines audited: invocation/env construction, initial compatible reuse, resolved-path in-process startup sharing and cleanup, lock acquisition/reprobe/release, incompatible external refusal and owned termination, detached spawn, target verification, exit/readiness deadlines and timer normalization. Native compiled invocation is the approved executable-as-entrypoint mapping. Late setsid exec failures remain the explicit approved gap above; no acknowledgement protocol or fallback was added.

Fixed one discrepancy with native RED→GREEN: a regular file at the control DB parent path now fails before launch as frozen `mkdirSync` does, preserving EEXIST message/code/syscall/path. `supervisor_startup_errors::existing_file_parent_fails_before_launch_with_node_mkdir_error` also proves the file is unchanged and launch is not called.

Proof `/tmp/runtime-successor-ensure-{native,scenarios}.log`: five standalone tests PASS (one parent-invoked subprocess helper ignored); three Core5 tests PASS, including the existing seven-case frozen startup scenario and SystemError/client-message proof. The real detached resident and independent starters exercise process lifetime and the actual lock boundary.

- GAP: malformed/adversarial inputs, numeric/timer extremes and exotic OS/runtime faults remain the approved one-line input/runtime gap.
- DISCREPANCY (preserved frozen behavior): an already compatible initial probe bypasses startup coordination and locks; readiness waits only for matching version, not the old instance id or a fresh launch identity.
