# coding-agent self-restart traceability

Pinned source: `packages/coding-agent/src/core/self-restart.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/core/self_restart.rs`. Tests: `crates/pi-coding-agent/tests/self_restart.rs` (20 behavioral, including a real `execve` child).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `ENV_SELF_RESTART_REQUEST/SESSION/PROMPT/OLD_PID` | Constants | env tests |
| `consumeSelfRestartRequest` (always deletes env, marker required, pid validity/ownership) | `consume_self_restart_request` | leaked-marker, exec-in-place, spawn-handoff, unrelated-pid tests |
| `waitForSelfRestartParentExit` (5 s deadline, 25 ms poll) | `wait_for_self_restart_parent_exit` + `SelfRestartWaitDependencies` | wait/skip tests |
| `applySelfRestartRequest` | `apply_self_restart_request` | no-handoff and resume tests |
| `appendSelfRestartNotice` / `formatRestartNotice` | `append_self_restart_notice`, `format_restart_notice` | current-pid and old→new pid notice tests; no-prompt skip |
| `getSelfRestartArgs` (Bun virtual entrypoints) | `get_self_restart_args`, `is_bun_virtual_entrypoint` | bun-virtual test |
| `buildSelfRestartEnv` | `build_self_restart_env` | env test |
| `execSelfRestart` / `restartCurrentProcess` | `exec_self_restart`, `restart_current_process` using `CommandExt::exec` | exec-in-place test asserts argv0 `pi` and restart env through a real child process |
| `spawnSelfRestart` (wait / unref) | `spawn_self_restart` | spawn wait test, immediate-return test, missing executable error test |

Gaps / deviations:

- `restartCurrentProcess` returns `std::io::Error` when `exec` fails; TS throws the `execve returned after replacing the process image` sentinel only because its `execve` is injectable. The Rust test uses a real child process instead of a mock.
- `spawnSelfRestart` returns `Result<i32>`; `waitForExit: false` drops the child (Node `unref`) and returns `Ok(0)`.
- `appendSelfRestartNotice` takes `current_pid` explicitly for testability; production passes `std::process::id()`.
- The TS title-fallback test depends on SessionManager listing; session listing behavior is covered by `session-manager` tests, not duplicated here.
