# Coding-agent control command traceability

Pinned source: `packages/coding-agent/src/cli/control-command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/control_command.rs`.

Process module (control SQLite database, process signals). Behavioral proof with injected effects: `tests/control_command.rs` (7 tests, ports of `test/control-command.test.ts`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `control-command.ts:17-19` | Ignore non-`control` argv | `ignores_non_control_commands_and_prints_help` |
| `control-command.ts:23-36` | `send` queues one incoming message, prints `queued <id>`, optionally signals `SIGHUP` and prints `signaled <pid>` | `queues_a_control_message_and_signals_a_running_process` |
| `control-command.ts:38-56` | `restart` requires a live `ok` health row; signals the recorded pid; unknown/dead sessions print the exact message and exit 1; signal failures print the wrapped message | `restarts_a_running_session_by_exact_session_id`, `reports_restart_of_unknown_or_dead_sessions` |
| `control-command.ts:58-71` | `last` prints the stored last message content; `path` prints the control DB path | `prints_the_last_assistant_reply`, `prints_the_state_root_control_database_path` |
| `control-command.ts:73-76` | Help goes to stdout for `--help`/`-h` (exit 0) and stderr otherwise (exit 1) | `ignores_non_control_commands_and_prints_help` |
| `control-command.ts:78-97` | `parseSendCommand`: `--pid` must be a positive integer; remaining args join with spaces and trim; empty message is invalid | `rejects_invalid_send_arguments` |
| `control-command.ts:99-102` | `parseRestartCommand`: exactly `--session-id <non-empty>` | `reports_restart_of_unknown_or_dead_sessions` |

## Semantics

- `signal_process` defaults to `rustix::process::kill_process(pid, Signal::HUP)`; errors surface as their OS message.
- The default control DB path and the signal function are trait methods so tests inject both without mutating process env.

## Discrepancies

None found.
