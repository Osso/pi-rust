# claude-memory-session-end traceability

Source: `packages/coding-agent/extensions/claude-memory-session-end/src/index.ts` (61 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/claude_memory_session_end.rs`.
Tests: unit tests in that module (`launches_index_file_detached_and_logs_the_start`,
`logs_spawn_failures_without_failing`) and
`crates/pi-coding-agent/tests/bundled_extensions_context.rs`
(`claude_memory_session_end_skips_ephemeral_sessions`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `formatLogLine` (`new Date().toISOString()`) | `format_log_line`, `utils::iso_time::now_iso_string` | log assertion |
| `appendLogLine` (mkdir recursive, append) | `append_log_line` | log assertion |
| `reportLaunchFailure` (message + fallback console.error) | `report_launch_failure` | spawn-failure test |
| `launchIndex` cache path (`XDG_CACHE_HOME` or `~/.cache`) | `launch_index` via `pi_js_compat::process_env` | implementation |
| open log, write start line, detached spawn, ignore spawn error | `launch_index_with` | unit tests with injected executable |
| `session_shutdown` unpersisted-session skip | handler | integration test |
| `PI_CLAUDE_MEMORY_SESSION_END_HOOK` legacy env | unused in source | source ignores it (test sets it) |

Gaps / discrepancies:
- The executable is the source's hardcoded `/home/osso/.cargo/bin/claude-memory`.
  Tests call `launch_index_with` with a fake script; the extension itself never runs
  the real binary in tests.
- Detach uses `process_group(0)`; Node `detached: true` also calls `setsid`.
- When `HOME` is unset the fallback cache path is `/.cache`; Node's `os.homedir()`
  resolves the passwd entry instead. Documented edge case.
