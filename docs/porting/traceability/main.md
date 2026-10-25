# Coding-agent main

Frozen: `packages/coding-agent/src/main.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (1,420 lines).
Native: `crates/pi-coding-agent/src/main.rs`, binary `src/bin/pi.rs`.

**Done at the user-observable parity bar (2026-10-25).** Whole-module audit and existing tests, with native regressions for discrepancies; no new main differential or proof matrix. [Contracts](../../specs/main-startup.md), [session cwd](../../specs/session-cwd.md), [HTTP authority](http-dispatcher.md).

## Exports

- `MainOptions` → `main_runner::MainOptions`: native inline extension factories; factory/load and runtime tests in `tests/main.rs`.
- `main` → `main_runner::main`: `tests/main_process.rs` exercises the actual executable and existing frozen CLI process fixtures; `tests/main_interactive.rs` exercises real terminals.
- `runCliActionOrReportError` → `run_cli_action_or_report_error`: native exit-code/error behavior test.
- `runInteractiveActionOrReportTerminalError` → corresponding native helper: terminal-error classification, callback, and original error propagation tests.

## Whole-module audit

- Startup order: relay commands; read-only terminal export; standalone validation; resident consoles; architect/supervisor services; offline environment; bootstrap settings/proxy/HTTP configuration; package/config/login/control/debug/sessions dispatch; restart handoff; coordination and first-party factories; diagnostics/version/export/catalog refresh. Existing process tests cover exact stdout/stderr and early-exit ordering. Terminal export and resident/relay implementation proof remains in their own module traces.
- Session setup: app-mode/stdout selection, RPC file rejection, fork/session-ID validation, worktree/migrations, startup settings/first setup, session-dir precedence, empty-session sweep, selection, metadata binding, missing-cwd recovery, interrupted tools/restart notice, name validation and incoming-message claim. Native helper, process and PTY tests cover these paths, including standalone skipping restart/coordination/sweep/reconciliation.
- Runtime creation: per-cwd trust cache and startup UI, exact resource-loader options/extension flags, shared auth storage, diagnostic aggregation, scoped/CLI model and thinking/tool/API-key choices, internal orchestrator capability, shared-channel callback and mutation resolver. `tests/main.rs` exercises concrete services, supplied trust contexts, scoped choices, and repeated runtime creation.
- Final dispatch: runtime HTTP settings, help/model/tool/extension lists, piped stdin and file/image preparation, theme/deprecation/runtime diagnostics, missing models/benchmark guards, background relay, RPC/interactive/print modes, terminal errors and cleanup. Existing process/PTY tests cover metadata/list output, print/JSON/RPC, embedded assets, offline child environment, resume, setup, shutdown/restart and fatal import.
- First-party factory list follows frozen order and command availability. The extra openai-remote-compact registration was removed with parent approval. Frozen still ships its extension module, so that module, tests and coverage remain.

## Successor corrections

- Missing `.jsonl.zst` names are transcript paths. Cross-project `--session` opens the original rather than prompting/forking. Explicit and picker-selected archives restore through the control DB before opening. Native RED/GREEN: `/tmp/settings-models-main-sessions-{red,green}.log`.
- Removed the old-only new-session warning and cross-project confirmation workflow.
- Global httpProxy is applied before bootstrap configuration and after runtime settings load, preserving existing environment values including empty strings. A real child with `--version` proves this. The factory owns one AuthStorage so runtime-only API keys survive recreation. Native RED/GREEN: `/tmp/settings-models-main-runtime-{red,green}.log`.
- Main and package trust prompts now use the existing startup selector on a process terminal. The adapter propagates Result and runs the Rc TUI on one blocking worker/current-thread runtime with read-only settings. Both real PTY tests assert the saved Trusted decision: `/tmp/settings-models-trust-pty-{red,green}.log`.
- The old fatal-import PTY fixture did not answer frozen's confirmation. It now waits for the actual confirmation and sends Enter; its exit-code-1 assertion is unchanged. No production import behavior changed.

## Proof

- Native main helpers/runtime: 27 tests, `/tmp/settings-models-main-merged-native.log`.
- Actual process fixtures: 20 tests, `/tmp/settings-models-main-merged-process.log`.
- Main PTY subset: all 11 tests pass after final parent integration in `/tmp/settings-models-main-merged-pty.log`.
- Proxy/default HTTP and settings persistence proof: [HTTP](http-dispatcher.md), [settings](settings-manager.md).
- Final checkpoint/revision ledger: [successor proof](../settings-models-successor-proof.md).

## Approved gaps

Debug REPL remains the existing approved gap; Windows-only branches and adversarial/exotic inputs, numeric extremes, OS/runtime fault shapes and scheduling races follow AGENTS.md. Existing native-extension, embedded-asset and stack-text approvals remain unchanged. No new semantic deviation is introduced.
