# coding-agent child-process traceability

Pinned source: `packages/coding-agent/src/utils/child-process.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: behavior depends on process lifetime, pipes, and timing; proven behaviorally in `crates/pi-coding-agent/tests/child_process.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `child-process.ts:6-13` | Spawn a process (cross-spawn on Windows) | `utils::child_process::spawn_process`; focused `captures_output_and_exit_code`; Windows cross-spawn is a documented gap |
| `child-process.ts:19-23` | Synchronous spawn returning status/stdout/stderr | `spawn_process_sync`; focused sync test with cwd |
| `child-process.ts:29-100` | After exit, keep reading until both pipes end or the idle grace expires; re-arm on every chunk; report exit code; destroy pipes | `wait_for_child_process` with a 100 ms re-armed deadline; focused post-exit descendant and quiet-descendant tests |
| `child-process.ts:222-227` | Process errors reject | `spawn_process` returns `io::Error`; focused `spawn_failures_are_reported` |
| `child-process.ts:229-241` | Exit and close report the exit code; signal termination reports null | Focused signal test (`status == None`) |

## Discrepancies

- The source leaves stdout/stderr consumption to callers listening on the streams. Rust ownership prevents shared pipe listeners, so `wait_for_child_process` drains and returns stdout/stderr together with the exit status.
- `cross-spawn` Windows semantics (PATHEXT resolution and shebang handling) are not ported; `std::process::Command` is used on every platform.
- The grace timer is behaviorally proven with real subprocesses rather than virtual timers.

## Proof inventory

- Focused cases: 6 (output/status, post-exit writes, quiet descendant, signal, spawn failure, sync spawn).
