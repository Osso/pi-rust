# Coding-agent login command traceability

Pinned source: `packages/coding-agent/src/cli/login-command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/login_command.rs`.

Q18 native errors: LoginIo stdout/stderr take &JsString; read_line errors, BrowserCliOpener failures and handle_login_command errors are JsString. OAuth runtime errors propagate message data to captured stderr; StdioLoginIo alone encodes at write_all. Added `raw_oauth_error_reaches_captured_stderr_without_display_projection`; execution blocked by concurrent coding-library dependencies, so not new-pin runtime proof.

Interactive process module. Behavioral proof with a scripted `LoginIo` and a registered test OAuth provider: `tests/login_command.rs` (7 tests, ports of `test/login-command.test.ts`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `login-command.ts:41-56` | Provider list formats `  <id padEnd 20> <name>` for OAuth providers only | `returns_nonzero_and_prints_provider_list_for_unknown_providers`, usage test |
| `login-command.ts:57-61` | Usage text to stderr, exit 0 for `--help`, 1 otherwise | `prints_usage_for_help_and_unknown_flags` |
| `login-command.ts:63-100` | Non-TTY prompt reader consumes buffered lines; TTY reads one line per prompt after writing it | `LoginIo` + `StdioLoginIo`; scripted-line test |
| `login-command.ts:101-118` | Select prompt prints the message and numbered options, trims the answer, defaults to the first option, accepts an index or exact id | Scripted lines `"2"` → `second` |
| `login-command.ts:119-136` | Prompt formatting (`message (placeholder): `) and select delegation | Scripted prompt assertions |
| `login-command.ts:137-160` | Info links, auth URL/device-code instructions, progress messages, browser-cli queueing | `runs_oauth_login_interactions...`, `reports_login_failures...` |
| `login-command.ts:162-177` | Login runs with browser tasks awaited after the flow | Browser tests |
| `login-command.ts:179-216` | Unknown provider, success, and failure lines with chalk colors and exit codes | Unknown-provider, failure, success tests |
| `login-command.ts:218-249` | Argument parsing: provider, help flag, `--browser-cli`, unknown flags | Usage test |

## Semantics

- `run_login_command` returns the exit code; `handle_login_command` returns `LoginCommandOutcome { handled, exit_code }` instead of mutating `process.exitCode`.
- `AuthPromptKind::Select` carries typed options; other kinds format `message (placeholder): `.
- `StdioLoginIo` buffers piped stdin on the first prompt (matching the source's `collectInputLines`) and reads one line at a time on a TTY via `spawn_blocking`.
- Browser opening defaults to `browser-cli open <url>` through `utils::child_process`, with the source's exit-code message.

## Gaps

- TTY readline editing/history is not reproduced; prompts read raw lines. Node's readline `question` behavior for terminal echo is outside the ported surface.
- `openUrlWithBrowserCli` is exercised through injection; the real `browser-cli` process is not spawned in tests.

## Discrepancies

None found.

Q18 native CLI auth prompt/credential text stays `JsString` through IO callbacks; browser URL encodes at spawn, not presentation. Behavioral proof status: [lossless auth](lossless-auth.md); coding-agent tests unexecuted until library compiles.
