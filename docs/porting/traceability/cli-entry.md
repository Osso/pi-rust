# cli.ts traceability

Source: `packages/coding-agent/src/cli.ts` (25 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/cli_entry.rs`.

## Exported symbols

| TS step | Rust | Tests |
| --- | --- | --- |
| `runInternalDetachedRunner(args)` gate | `run_internal_detached_runner` call in `run_cli_entry` | `cli_entry_stops_after_an_internal_runner_handles_the_process` |
| `process.title = APP_NAME` | not ported (gap) | — |
| `process.env.PI_CODING_AGENT = "true"` | `CLI_PROCESS_ENV` + `apply_process_env` in `run_cli_main` (`pi_js_compat::process_env` overlay) | `cli_process_env_matches_the_typescript_entry_point`, `run_cli_main_exports_the_typescript_process_environment_to_children` |
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
  denies unsafe code. `run_cli_main` applies `CLI_PROCESS_ENV` to the
  `pi_js_compat::process_env` overlay (same position as the source: after the
  internal-runner gate, before the dispatcher); child spawns read the overlay.
- **Global dispatcher** — TypeScript replaces `globalThis.fetch`. Rust has no global fetch, so
  `run_cli_entry` returns the configured `Arc<dyn Fetch>`; the caller passes it to the model
  runtime.
- **`runCliActionOrReportError`** — defined in `main.ts` (`main_runner::run_main_or_report_error`);
  the entry point propagates the error and the binary prints `Error: …` with exit code 1.

## Whole-module common-pin audit (2026-10-25)

All entrypoint statements revalidated against 4f9e6f8a4. Removed old-only AI_AGENT=pi mutation; absent, empty and inherited AI_AGENT stay unchanged. `entry_environment::entrypoints_preserve_inherited_ai_agent_like_frozen` loads each actual frozen entry with invalid read-only export arguments (returns before filesystem/provider startup), and compares the environment with the real Rust entry wrapper in fresh child processes. RED `/tmp/settings-models-entry-env-red.log` proves the old mutation. Six-process GREEN and targeted clippy follow this commit. Existing native entry tests cover main handoff, mode prefix, short circuit and error propagation.

Gap: Node process.title/warning APIs and unhandled-rejection scheduling have no identical native runtime form; retained runtime exclusions. No live providers or network startup are used by the new differential.

GREEN at 5f55e77a4: six actual-source/native child cases plus 10 CLI/internal-runner and3 RPC tests, `/tmp/settings-models-entry-env-green-final.log`. Targeted coding-agent library warning-mode clippy exit0, no entrypoint warnings (`/tmp/settings-models-entry-clippy.log`). Changed Rust/TS manually audited for readability; no violations.
