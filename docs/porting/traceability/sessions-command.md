# Coding-agent sessions command traceability

Pinned source: `packages/coding-agent/src/cli/sessions-command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/sessions_command.rs`.

Process module (session files, control DB, filesystem migration). Behavioral proof with injected effects: `tests/sessions_command.rs` (6 tests, ports of `test/sessions-command.test.ts`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `sessions-command.ts:19-24` | Ignore non-`sessions` argv; route `truncate-tool-output` and `archive`; other subcommands print help (stdout only for `--help`/`-h`) with exit code 0/1 | `prints_help_and_ignores_non_sessions_commands` |
| `sessions-command.ts:25-43` | Truncate requires exactly one argument, prints the report, and reports migration errors on stderr with exit code 1 | `reports_tool_result_truncation_through_the_migration_command`, `reports_migration_errors_and_sets_a_failing_exit_code` |
| `sessions-command.ts:45-71` | Archive refreshes metadata, computes the cutoff (`now - days*86_400_000`), archives, and prints the exact count sentence | `archives_sessions_using_a_five_day_default_cutoff`, `accepts_an_explicit_day_cutoff` |
| `sessions-command.ts:73-78` | `parseDays`: default 5, `--older-than <days>`, finite and positive, else invalid | `rejects_invalid_archive_arguments` |
| `sessions-command.ts:80-94` | Writable metadata fields and ISO timestamps | Default `refresh_metadata` implementation uses `SessionManager::list_all` + `write_session_metadata`; exercised by the session-control-db slice |
| `sessions-command.ts:96-101` | Truncation report sentence, including singular/plural and skipped counts | Truncate tests |
| `sessions-command.ts:103-106` | Help text | `prints_help_and_ignores_non_sessions_commands` |

## Semantics

- Exit codes are returned (`SessionsCommandOutcome.exit_code`) instead of mutating `process.exitCode`.
- `days` stays `f64` because the source accepts fractional `Number` values; the cutoff truncates like `Date` millisecond storage, and the message uses `js_number_to_string`.
- The default `control_db_path` is `getControlDbPath(getUserStateRoot())`, matching the source's no-argument call.

## Gaps

- Default `refresh_metadata`/`archive_older_than`/`truncate_tool_output` bodies run real filesystem/SQLite code; their behavior is covered by the session-manager, session-control-db, and session-tool-output slices.

## Discrepancies

None found.
