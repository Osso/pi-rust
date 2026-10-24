# Self-restart process dependencies

Port frozen `packages/coding-agent/src/core/self-restart.ts:231-305` process handoff behavior. [Traceability](../porting/traceability/self-restart.md) records source boundaries and proofs; [runtime contract](agent-session-runtime.md) governs lifecycle preparation.

## What it must do

- [x] Replace the supplied old PID with the current PID before restart callbacks.
- [x] Call an available execve with current executable, argv0, filtered restart arguments and restart environment; propagate its original native/raw rejection. A returned execve raises frozen's exact error.
- [x] Explicitly absent execve calls spawnSelfRestart with only `waitForExit:false`, forwards its exit code to exit, preserves either callback rejection, and raises frozen's exact error if exit returns.
- [x] Preserve arbitrary UTF-16 argument/environment values and native/raw callback rejections until the actual OS boundary.
- [ ] Preserve JavaScript environment key ordering, including overwritten keys and numeric keys.
- [x] Spawn with current executable/cwd and inherited stdio; optional unref runs only for no-wait; waited null exits become zero. Preserve spawn/unref/awaited child errors.
- [x] Built-in runtime restart defaults to actual exec-in-place, preserving kernel PID, without injected process hooks.
- [x] No-wait default child launch errors remain uncaught after the zero return; preserve native code/errno/syscall/path/spawnargs and exit 1.
- [x] Actual successful default spawn runs without injected process hooks, creates another kernel PID and preserves env/cwd/session; deleted environment entries stay deleted.

## How it works

- [Module traceability and syscall boundary](../porting/traceability/self-restart.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/self_restart.rs`: one dependency carrier, source dispatch and native process boundaries.
- `crates/pi-coding-agent/src/core/agent_session_runtime.rs`: built-in process restarter selects the source exec-first function.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/self_restart_process.rs`: actual frozen module with the same injected callbacks.
- `crates/pi-coding-agent/tests/self_restart.rs`: argument/environment and actual process regression fixtures.
- `crates/pi-coding-agent/tests/self_restart_exec_env.rs`: explicit-environment exec safety and non-Unicode environment spawn.
- `crates/pi-coding-agent/tests/support/runtime_restart.rs`: actual constructor-default source/native process restart.
- `crates/pi-coding-agent/src/core/self_restart.rs::process_tests`: real spawn backend failures.

## Known gaps (current cycle)

- [ ] Finish environment-order/native validation and unhandled non-Error event proofs; scoped gates.

## Out of scope

- Invalid JavaScript runtime input types excluded by typed Rust APIs; Windows branches remain the recorded platform gap.
- Old native executable override is removed: frozen public restart functions always select `process.execPath`.
