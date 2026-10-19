# rpc-entry traceability

Pinned source: `packages/coding-agent/src/rpc-entry.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/rpc_entry.rs`. Proof: `tests/rpc_entry.rs` (3 tests).

| TypeScript step | Observable behavior | Rust mapping and proof |
|---|---|---|
| `process.title = \`${APP_NAME}-rpc\`` | Process title | **gap** (same platform-specific gap as `cli.ts`; documented in `cli-entry.md`) |
| `process.env.PI_CODING_AGENT = "true"`, `process.env.AI_AGENT = "pi"` | Environment pair written to the `pi_js_compat::process_env` overlay | `RPC_PROCESS_ENV`; `rpc_process_env_matches_the_typescript_entry_point` |
| `process.emitWarning = () => {}` | Suppress Node warnings | not ported (no Rust analogue) |
| `configureHttpDispatcher()` | Default idle timeout | `configure_http_dispatcher(DEFAULT_HTTP_IDLE_TIMEOUT_MS)`; the fetch is returned to the caller |
| `main(["--mode", "rpc", ...process.argv.slice(2)])` | `--mode rpc` precedes the process arguments; `main` is not awaited | `RPC_MODE_ARGS` + `run_rpc_entry`; `rpc_entry_prepends_the_rpc_mode_arguments_and_returns_the_configured_fetch` |

## Exclusions and deviations

- TypeScript fires `main` without awaiting it, so rejections surface as unhandled rejections; the Rust
  entry awaits `main` and propagates the error (`rpc_entry_propagates_main_errors`). The future
  `main.ts` port owns the `runCliActionOrReportError` wrapper.
- `process.title` and `emitWarning` are documented gaps shared with `cli.ts`.
