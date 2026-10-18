# Coding-agent debug command traceability

Pinned source: `packages/coding-agent/src/cli/debug-command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/debug_command.rs`.

Process module (control DB lookup, Unix socket). Behavioral proof with injected attach: `tests/debug_command.rs` (4 tests, ports of `test/debug-command.test.ts`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `debug-command.ts:14-21` | Ignore non-`debug` argv; `--help`/`-h` prints usage to stdout with exit 0 | `prints_debug_command_help_successfully`, `reports_missing_sessions_and_invalid_usage` |
| `debug-command.ts:22-25` | Any other shape prints usage to stderr with exit 1 | `reports_missing_sessions_and_invalid_usage` |
| `debug-command.ts:27-33` | Require a live `ok` health row; otherwise print `Session <id> is not running.` and exit 1 | `reports_missing_sessions_and_invalid_usage` |
| `debug-command.ts:35-45` | Socket path is `<agentDir>/debug/<pid>.sock`; attach failures print the wrapped message and exit 1 | `attaches_to_the_debug_socket_owned_by_the_exact_live_session`, `reports_attach_failures_with_the_source_message` |
| `debug-command.ts:54-84` | Handshake JSON line, then pipe stdin/stdout to the socket until close | `attach_debug_repl` (Unix std sockets); live socket path not exercised by tests |

## Gaps

- `core/debug-repl.ts` is a documented project gap (`node:vm`). Only its pure `getDebugSocketPath` helper is reproduced here; the debug REPL server itself stays unported.
- `attach_debug_repl` uses blocking std I/O inside the async default; the CLI awaits it as the sole foreground operation.

## Discrepancies

None found.
