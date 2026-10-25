# Self-restart requests and process replacement

Port all 303 lines of frozen `packages/coding-agent/src/core/self-restart.ts`, including request consumption, parent wait, persisted results/notices and process handoff. [Traceability](../porting/traceability/self-restart.md) records source boundaries and proofs; [runtime contract](agent-session-runtime.md) governs lifecycle preparation.

## What it must do

### Requests and persisted state

- [x] Consume and remove restart environment variables before child creation; require the marker and nonempty session, and reject an invalid or unrelated supplied old PID.
- [x] Skip absent/current/zero parent waits; poll the selected live parent until exit or the frozen deadline.
- [x] Apply the handoff to Args, clearing provider/model/thinking and other session/input overrides exactly like frozen; no handoff leaves Args unchanged.
- [x] Append pending restart results in call order with the first matching detached job, then truthy restart notices with the correct same/spawn PID text. Keep ordinary interruption outcomes distinct from restart_self success.

### Process replacement

- [x] Replace the supplied old PID with the current PID before restart callbacks.
- [x] Call an available execve with current executable, argv0, filtered restart arguments and restart environment; propagate its original native/raw rejection. A returned execve raises frozen's exact error.
- [x] Explicitly absent execve calls spawnSelfRestart with only `waitForExit:false`, forwards its exit code to exit, preserves either callback rejection, and raises frozen's exact error if exit returns.
- [x] Exit/return callbacks retain the full JS number domain, including fractions, negative zero, non-finite and out-of-i32 values. Default process exit validates integer/safe-integer bounds with Node-exact errors and applies V8 Int32Value semantics.
- [x] Preserve arbitrary UTF-16 argument/environment values and native/raw callback rejections until the actual OS boundary.
- [x] Preserve JavaScript environment key ordering, including overwritten keys and numeric keys.
- [x] Real default exec/spawn validate NUL arguments/environment with exact native TypeError fields; oversized argv preserves each backend's exact errno/code/message/syscall/path/string.
- [x] Real exec clears stdio FD_CLOEXEC before the syscall, restores saved flags on failure, and reports fcntl failures after restoring earlier streams.
- [x] Spawn with current executable/cwd and inherited stdio; optional unref runs only for no-wait; waited null exits become zero. Preserve spawn/unref/awaited child errors.
- [x] Built-in runtime restart defaults to actual exec-in-place, preserving kernel PID, without injected process hooks.
- [x] No-wait default child launch errors remain uncaught after the zero return; preserve native code/errno/syscall/path/spawnargs and exit 1.
- [x] Unobserved native Error events retain their constructor/properties; raw JSON events become Node's ERR_UNHANDLED_ERROR with original context.
- [x] Actual successful default spawn runs without injected process hooks, creates another kernel PID and preserves env/cwd/session; deleted environment entries stay deleted.

## How it works

- [Module traceability and syscall boundary](../porting/traceability/self-restart.md)

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/self_restart.rs`: one dependency carrier, source dispatch and native process boundaries.
- `crates/pi-coding-agent-core/src/core/agent_session_runtime.rs`: built-in process restarter selects the source exec-first function.
- `crates/pi-coding-agent-storage/src/core/self_restart/native_execve.rs`: supplementary Node POSIX errno/stdio boundary.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/self_restart_process.rs`: actual frozen module with the same injected callbacks.
- `crates/pi-coding-agent/tests/support/self_restart_numeric.rs`: numeric callback domain and actual default-exit process/error differential.
- `crates/pi-coding-agent/tests/self_restart.rs`: argument/environment and actual process regression fixtures.
- `crates/pi-coding-agent/tests/self_restart_exec_env.rs`: explicit-environment exec safety and non-Unicode environment spawn.
- `crates/pi-coding-agent/tests/support/runtime_restart.rs`: actual constructor-default source/native process restart.
- `crates/pi-coding-agent-storage/src/core/self_restart.rs::process_tests`: real spawn backend failures.

## Known gaps (current cycle)

- [x] Whole-module realistic-branch audit is complete; current code reuses unchanged source/process evidence rather than rerunning the old numeric/fault matrices. [Audit and exact scope](../porting/traceability/self-restart.md#whole-module-audit-closure-2026-10-25).
- [x] Scoped acceptance: process16/16, legacy22/22+3/3, runtime44/44 plus parent-invoked isolated child, backend/outside-callers/abort regression; strict TS/Biome/fmt, workspace0 errors/0 warnings, owned-hunk clippy0 and locked inventory PASS. Full unrelated clippy debt remains explicit.

## Out of scope

- Invalid JavaScript runtime input types excluded by typed Rust APIs; Windows branches remain the recorded platform gap.
- Replacement Node bootstrap re-marks stdio FD_CLOEXEC; native Rust bootstrap does not. Approved runtime-oddity gap, 2026-10-23. Success proves physical inheritance/targets/PID/output; failure restore/fcntl flags remain exact. See [source and proof](../porting/traceability/self-restart.md#native-execve-boundary).
- Old native executable override is removed: frozen public restart functions always select `process.execPath`.
