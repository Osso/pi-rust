# rpc-entry traceability

Pinned source: `packages/coding-agent/src/rpc-entry.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

Rust: `crates/pi-coding-agent/src/rpc_entry.rs`. Proof: `tests/rpc_entry.rs` (3 tests).

| TypeScript step | Observable behavior | Rust mapping and proof |
|---|---|---|
| `process.title = \`${APP_NAME}-rpc\`` | Process title | **gap** (same platform-specific gap as `cli.ts`; documented in `cli-entry.md`) |
| `process.env.PI_CODING_AGENT = "true"` | Environment write applied to the `pi_js_compat::process_env` overlay | `RPC_PROCESS_ENV`; `rpc_process_env_matches_the_typescript_entry_point` |
| `process.emitWarning = () => {}` | Suppress Node warnings | not ported (no Rust analogue) |
| `configureHttpDispatcher()` | Default idle timeout | `configure_http_dispatcher(DEFAULT_HTTP_IDLE_TIMEOUT_MS)`; the fetch is returned to the caller |
| `main(["--mode", "rpc", ...process.argv.slice(2)])` | `--mode rpc` precedes the process arguments; `main` is not awaited | `RPC_MODE_ARGS` + `run_rpc_entry`; `rpc_entry_prepends_the_rpc_mode_arguments_and_returns_the_configured_fetch` |

## Exclusions and deviations

- TypeScript fires `main` without awaiting it, so rejections surface as unhandled rejections; the Rust
  entry awaits `main` and propagates the error (`rpc_entry_propagates_main_errors`). The future
  `main.ts` port owns the `runCliActionOrReportError` wrapper.
- `process.title` and `emitWarning` are documented gaps shared with `cli.ts`.

## Whole-module common-pin audit (2026-10-25)

All entrypoint statements revalidated against 4f9e6f8a4. Removed old-only AI_AGENT=pi mutation; absent, empty and inherited AI_AGENT stay unchanged. `entry_environment::entrypoints_preserve_inherited_ai_agent_like_frozen` loads each actual frozen entry with invalid read-only export arguments (returns before filesystem/provider startup), and compares the environment with the real Rust entry wrapper in fresh child processes. RED `/tmp/settings-models-entry-env-red.log` proves the old mutation. Six-process GREEN and targeted clippy follow this commit. Existing native entry tests cover main handoff, mode prefix, short circuit and error propagation.

Gap: Node process.title/warning APIs and unhandled-rejection scheduling have no identical native runtime form; retained runtime exclusions. No live providers or network startup are used by the new differential.
