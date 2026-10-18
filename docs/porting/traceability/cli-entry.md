# cli.ts traceability

Source: `packages/coding-agent/src/cli.ts` (25 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/cli_entry.rs`.

## Exported symbols

| TS step | Rust | Tests |
| --- | --- | --- |
| `runInternalDetachedRunner(args)` gate | `run_internal_detached_runner` call in `run_cli_entry` | `cli_entry_stops_after_an_internal_runner_handles_the_process` |
| `process.title = APP_NAME` | not ported (gap) | — |
| `process.env.PI_CODING_AGENT = "true"`, `process.env.AI_AGENT = "pi"` | `CLI_PROCESS_ENV` | `cli_process_env_matches_the_typescript_entry_point` |
| `process.emitWarning = () => {}` | not ported (no Rust analogue) | — |
| `configureHttpDispatcher()` | `configure_http_dispatcher(DEFAULT_HTTP_IDLE_TIMEOUT_MS)`; the fetch is returned | `cli_entry_runs_main_and_returns_the_configured_fetch` |
| `runCliActionOrReportError(() => main(args))` | `main(args).await`; errors propagate to the caller | `cli_entry_propagates_main_errors` |

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | internal runner handled → skip env/dispatcher/main | `cli_entry_stops_after_an_internal_runner_handles_the_process` |
| 2 | no internal runner → configure dispatcher, run `main` | `cli_entry_runs_main_and_returns_the_configured_fetch` |
| 3 | `main` rejects → error reaches the caller | `cli_entry_propagates_main_errors` |
| 4 | dispatcher configuration fails → error propagates | `configure_http_dispatcher` invalid-timeout test (`tests/http_dispatcher.rs`) |

## Documented gaps and deviations

- **`process.title`** — needs a platform-specific syscall (Linux `prctl(PR_SET_NAME)`) and a
  new dependency. Not ported; the process keeps the executable's name.
- **`process.emitWarning`** — Node-only API with no Rust equivalent; Rust does not emit Node
  warnings, so nothing is suppressed.
- **Environment variables** — edition 2024 makes `std::env::set_var` unsafe and this workspace
  denies unsafe code. The pairs are exported as `CLI_PROCESS_ENV`; the binary bootstrap must
  apply them before running `main` (TypeScript sets them inline).
- **Global dispatcher** — TypeScript replaces `globalThis.fetch`. Rust has no global fetch, so
  `run_cli_entry` returns the configured `Arc<dyn Fetch>`; the caller passes it to the model
  runtime.
- **`runCliActionOrReportError`** — defined in `main.ts` (not yet ported); the entry point
  propagates the error instead of printing `Error: …` and setting `exitCode = 1`.
