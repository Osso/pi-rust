# self-restart traceability

Source: `packages/coding-agent/extensions/self-restart/src/index.ts` (32 lines).
Rust: `crates/pi-coding-agent/src/bundled_extensions/self_restart.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_commands.rs` (`self_restart_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `RESTART_NOTICE` | `RESTART_NOTICE` | differential restart options |
| `/restart` command | `register_command` handler | differential restart call |
| `restart_self` tool metadata/schema (`Type.Object({})`) | `ToolDefinition` | registration differential |
| promptGuidelines (2 lines) | literal | registration differential |
| `execute` restart + result text/details | tool execute | differential tool result |

No gaps.

## Core self-restart (`packages/coding-agent/src/core/self-restart.ts`)

Rust: `crates/pi-coding-agent/src/core/self_restart.rs`.
Tests: `crates/pi-coding-agent/tests/self_restart.rs`, `self_restart_exec_env.rs`.

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `consumeSelfRestartRequest` (marker, session, prompt, old-pid validation, deletion) | `consume_self_restart_request` | 6 behavioral tests |
| `waitForSelfRestartParentExit` (pid/current, 5 s deadline, 25 ms poll) | `wait_for_self_restart_parent_exit` | injected clock/alive/sleep tests |
| `applySelfRestartRequest` | `apply_self_restart_request` | args-override tests |
| `appendSelfRestartNotice` / `formatRestartNotice` | `append_self_restart_notice` | notice tests, exec and spawn PID variants |
| `restartCurrentProcess` → `execve` (pid/foreground group preserved) | `restart_current_process` → `exec_self_restart` → `nix::unistd::execve` | real `/bin/sh` exec child test; failed-exec error-kind test |
| `spawnSelfRestart` fallback (cwd, env, stdio inherit, optional wait/unref) | `spawn_self_restart` | spawn/wait and immediate-return tests |
| `buildSelfRestartEnv` over `process.env` | `build_self_restart_env` over `pi_js_compat::process_env::vars()` | env tests; non-UTF-8 lossy test in `process_env`; non-Unicode restart child test |
| `getSelfRestartArgs` / Bun virtual entrypoint filter | `get_self_restart_args` | argv tests |

Deviation: the TS source calls `process.execve` directly. Rust previously used
`CommandExt::exec`, which swaps the process-global `environ` under a read lock and races with
concurrent environment readers (root cause and proof in
[test-infrastructure.md](test-infrastructure.md#6-self_restart-suite-corruption-flake-root-cause-commandext-exec)).
`nix::unistd::execve` gives the same `execve` semantics (same PID, no PATH search) without touching
`environ`. Non-UTF-8 environment entries follow Node's lossy decoding via `process_env::vars()`.
