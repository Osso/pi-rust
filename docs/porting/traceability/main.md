# main.ts traceability

Baseline process-fixture repair (2026-10-25): private package directories copy the pinned package.json instead of inventing version 0.84.1. The child-process fixture explicitly removes inherited AI_AGENT and asserts frozen/native `true||1`: the common pin no longer sets AI_AGENT. Source/native output remains exact, without normalization.

## Empty-session startup sweep (4f9e6f8a4)

Frozen :1063 calls [empty-session cleanup](empty-session-cleanup.md) after selecting controlDbPath, before createSessionManager, only when !noSupervisor. Native now uses the same placement. Actual CLI process RED reached --name validation without any runtime/provider work: source removed an orphan empty-main row, native retained it (`empty-cleanup-startup-red.log`). GREEN `empty-cleanup-startup-green-final.log`: ordinary startup sweeps before --name validation; valid --no-supervisor --no-session startup passes parsing/session setup and preserves the row until unknown-provider validation. No provider request occurs, no wider main acceptance inferred.

## Frozen relay delta (4f9e6f8a4)

`relay serve`/`relay connect` dispatch before resident/provider startup. Normal session startup starts [ensureRelayRunning](host-relay-resident.md) in an error-logged background task using global relay settings, except `--no-supervisor`. Flag parsing, standalone persistence diagnostics and invalid resident-console rejection precede startup; `tests/host_relay.rs` proves the boundary. Store/protocol/client/resident proof is linked from the [relay spec](../../specs/host-relay.md).

**Remaining Q14 main/CLI gap:** valid standalone-worker mode still uses the pre-existing orchestration/session runtime and self-restart handoff paths. Frozen additionally omits orchestration resources and restart handoff when `noSupervisor` is true. This slice does not port those unrelated main branches or claim full standalone parity. Historical whole-main done claims below are pre-repin evidence only.

Source: `packages/coding-agent/src/main.ts` (1,364 lines, pinned revision).

Rust: `crates/pi-coding-agent/src/main.rs` (module `main_runner`) and the `pi`
binary at `crates/pi-coding-agent/src/bin/pi.rs`.

Status: **done** for `main.ts`. Dispatch targets that live in unported modules
return an explicit gap error; they are listed under "Explicit gaps". Interactive
dispatch, the `--resume` picker, first-time setup, and first-party extension
registration are wired.

## Exported symbols

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `MainOptions` | `main_runner::MainOptions` | `tests/main.rs`, `tests/main_process.rs` |
| `main` | `main_runner::main` / `run_main_or_report_error` | process differentials (`tests/main_process.rs`, 16 incl. env/offline and embedded assets), pty tests (`tests/main_interactive.rs`, 4) |
| `runCliActionOrReportError` | `main_runner::run_cli_action_or_report_error` | `cli_action_wrappers_map_errors_to_exit_codes` |
| `runInteractiveActionOrReportTerminalError` | `main_runner::run_interactive_action_or_report_terminal_error` | `cli_action_wrappers_map_errors_to_exit_codes` |

## Private helpers

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `isTruthyEnvFlag` | `is_truthy_env_flag` | `truthy_env_flags_match_javascript_lowercasing` |
| `process.env.PI_OFFLINE`/`PI_SKIP_VERSION_CHECK` writes | `pi_js_compat::process_env::set` | `offline_environment_reaches_child_processes_differential` |
| `resolveAppMode` | `resolve_app_mode` | `app_mode_follows_mode_print_and_tty_state` |
| `toPrintOutputMode` | `to_print_output_mode` | `app_mode_follows_mode_print_and_tty_state` |
| `isPlainRuntimeMetadataCommand` | `is_plain_runtime_metadata_command` | `plain_runtime_metadata_commands_skip_stdout_takeover` |
| `collectSettingsDiagnostics` | `collect_settings_diagnostics` | `settings_diagnostics_prefix_scope_and_context` |
| `reportDiagnostics` | `report_diagnostics` | print/json process differentials |
| `refreshRpcModelCatalogs` | `refresh_rpc_model_catalogs` | rpc process differential |
| `runAuthCommand` | `run_auth_command` | `tests/main.rs` helpers; auth process paths deterministic |
| `prepareInitialMessage` | `prepare_initial_message` | `initial_message_concatenates_stdin_and_the_first_cli_message` |
| `findLocalSessionByExactId` | `find_local_session_by_exact_id` | `create_session_manager_honors_help_in_memory_and_session_ids` |
| `sessionBelongsToProject` | `session_belongs_to_project` | `session_project_membership_uses_the_git_common_directory` |
| `resolveSessionPath` | `resolve_session_path` | `session_paths_resolve_locally_globally_and_by_prefix` |
| `promptConfirm` | `prompt_confirm` | fork/prompt paths in `tests/main.rs` |
| `validateForkFlags` | `validate_fork_flags` / `fork_flag_conflict` | `fork_conflicts_list_flags_in_source_order`, fork-conflict differential |
| `validateSessionIdFlags` | `validate_session_id_flags` / `session_id_flag_conflict` | `session_id_conflicts_and_validation_match_the_source`, invalid-id differential |
| `openSessionOrExit` | `open_session_or_exit` | missing-session differential |
| `forkSessionOrExit` | `fork_session_or_exit` | fork-conflict differential |
| `createSessionManager` | `create_session_manager` | `create_session_manager_honors_help_in_memory_and_session_ids` |
| `buildSessionOptions` | `build_session_options` | `build_session_options_prefers_cli_model_and_explicit_thinking`, `build_session_options_uses_scoped_models_and_tool_flags` |
| `resolveCliPaths` | `resolve_cli_paths` | `cli_paths_resolve_local_entries_only` |
| `promptForMissingSessionCwd` | `prompt_for_missing_session_cwd` | interactive-only; uses `showStartupSelector` |
| `firstPartyExtensionFactory` | `first_party_extension_factory` | `first_party_factory_list_partitions_the_source_names`, `first_party_factories_load_and_register_agent_tools` |
| `runtimeExtensionFactory` | `InlineExtension::Factory` | extension loader tests |
| `createFirstPartyExtensionFactories` | `create_first_party_extension_factories` (registry-backed, emitted in source order; takes the startup `Fetch`) | `first_party_factory_list_partitions_the_source_names`, `first_party_factories_load_and_register_agent_tools`, `extensions_command_lists_the_ported_first_party_extensions` |
| `builtInExtensions` | `built_in_extensions` | package/config command paths; `extensions` inventory excludes it |
| `createRuntime` (closure in `main`) | `create_runtime_factory` / `create_runtime` | `runtime_factory_creates_services_and_reports_missing_models`, print/json differentials |
| `selectSession` call | `select_session_for_process` / `MainSessionsLoader` | `resume_flag_opens_the_session_picker_and_cancels_with_escape`, `resume_flag_selects_a_session_and_starts_interactive_mode` |
| `showFirstTimeSetup` call | `cli::startup_ui::show_first_time_setup` | `first_time_setup_persists_theme_and_analytics` |
| `readPipedStdin` | `read_piped_stdin` | print differential with argv messages; stdin path covered by initial-message test |
| `sessionStartEvent` (runtime request) | `session_start_event` | print/json differentials |

## Behavior coverage

- Entry order matches `cli.ts`: internal detached runner, HTTP dispatcher, then
  `main`. The binary uses `cli_entry::run_cli_main` so the dispatcher step is shared.
- `main` order: resident-console parse, `architect`/`supervisor` dispatch, timings,
  package/config commands, auth (twice, as in the source), Windows quarantine,
  bootstrap settings, login/control/debug/sessions commands, args parse, self-restart
  handoff, first-party extension factories, diagnostics, version, export, app-mode
  resolution, fork/session-id validation, worktree, migrations, startup settings,
  first-time-setup guard, session manager, missing-cwd handling, restart notice,
  `--name`, stdout takeover, trust resolution, per-cwd runtime factory, runtime
  creation, dead-runtime reconciliation, help/list commands, piped stdin, initial
  message, theme, deprecation warnings, runtime diagnostics, no-model check,
  startup-benchmark guard, RPC catalog refresh, then print/json/RPC dispatch.
- `createRuntime` restores the shared `MultiAgentStore` from the session manager,
  resolves project trust with the per-cwd cache, builds loader options, resolves the
  model scope, applies CLI model/thinking/tools options and `--api-key`, and creates
  the orchestrator session with the shared store, runtime handles, session-start
  event, and shared-channel wake handler.
- Print/JSON/RPC dispatch calls the ported `modes` implementations. Interactive
  dispatch maps `InteractiveModeOptions` (migrated providers, model fallback,
  auto-trust cwd, initial message/images/messages, claimed control message,
  verbose, TUI mode, control DB path) and calls
  `modes::interactive::run_interactive_mode`, which owns its `LocalSet`. The
  `PI_STARTUP_BENCHMARK` branch constructs `InteractiveMode` directly, calls
  `init()`, waits 150 ms, `stop()`, then prints timings.
- `--offline` writes `PI_OFFLINE=1`/`PI_SKIP_VERSION_CHECK=1` through the
  `pi_js_compat::process_env` overlay; every child spawn builds its environment
  from `process_env::vars()`.
- First-party factories are registered in an `ExtensionRegistry` under their
  `<first-party:...>` ids and emitted in `main.ts` order. `register_small_extensions`
  (source order), `register_agents_extensions`, and `register_bundled_extensions`
  provide the factories; `pyrun` is re-registered with the multi-agent Pi request
  handler. `SmallExtensionsOptions` carries the shared fast-mode authority, the
  startup HTTP client, and the multi-agent store. Package/config commands receive
  only `options.extensionFactories`, as in frozen `main.ts`; the old-only hidden
  `llama.cpp` built-in was removed with `extensions/index.ts`.
- `--resume` builds `SessionManager::{list,list_all,list_archived}` loaders,
  validates the selection with `assert_main_session_runtime_available`, and maps
  cancel/exit to `No session selected`/exit 0.
- First-time setup runs when `should_run_first_time_setup` holds and persists
  theme/analytics through `SettingsManager`.

## Explicit gaps

- `--supervisor` / `--architect` resident console is not wired; the `architect` /
  `supervisor` subcommands call the ported service entries.
- Extension HTTP (`codex-usage`) captures the startup dispatcher client; the
  settings-driven idle timeout applied after runtime creation is not visible to
  already-registered factories (Rust has no global fetch).
- `core/debug-repl.ts` is a documented gap (`node:vm`). The `debug` extension is
  registered; its controller provider returns the gap error
  (`Debug REPL is not ported (core/debug-repl.ts)`) from `enable`/`disable`
  instead of a fake REPL (`debug_repl_provider_reports_the_documented_gap`).
- First-party factories: all 29 names are registered in source order, including
  the approved native `session-autoname` extension. `pi extensions` lists it;
  `bundled_extensions_commands::registers_all_small_extensions_under_source_ids`
  asserts the 23 small-extension IDs in order and their loaded tools/commands.
  `FIRST_PARTY_EXTENSION_NAMES` is the reference list; tests assert the factory
  list, the loaded extension order, and the `pi extensions` inventory. The
  `agents-core` factory is built with the production child/attached session
  factories, the shared store/handles, and the interactive agent-view selector
  slot; `create_agent_session` receives the same store and the shared-channel
  wake callback. The session mutation-target resolver is bound to the shared
  store and runtime handles (`bind_agent_session_mutation_target_resolver`).
- Interactive multi-agent options are wired: `InteractiveModeOptions` carries
  the shared store, `wakeWaitAgentsAfterSteering`, `steerMultiAgent`,
  `cancelMultiAgent`, and the agent-view selector slot; the mode fills the slot
  with the async bridge selector. `bindInteractiveModeSessionMutationTargetResolver`
  has no Rust analogue because the session-level resolver bound here already
  targets the selected child through the store and runtime handles.
- `selectSession` keybindings: `create_startup_tui` installs
  `KeybindingsManager::create()`; the component receives a clone. The source
  installs the same manager in both places.
- First-time setup and the picker use `ProcessTerminal`; the pty tests drive the
  real binary, so terminal wiring is covered.

## Deviations

- `process.title` and `process.emitWarning` are not applied; both have no Rust
  analogue. `PI_CODING_AGENT`/`AI_AGENT` from `cli.ts` are written to the
  `pi_js_compat::process_env` overlay by `cli_entry::run_cli_main`
  (`apply_process_env`), matching the source order (after the internal-runner
  gate, before the dispatcher).
- `applyHttpProxySettings` has no process-global equivalent. Settings `httpProxy`
  is not written into the environment; the HTTP client reads the ambient proxy
  environment instead. `resolve_http_proxy_settings` captures the source decision.
- `--version` and package constants come from the embedded pinned `package.json` unless `PI_PACKAGE_DIR` is set (approved deviation 2026-10-18; see config.md).
- The JSON mode serializes `startedAt`/`finishedAt` as floats; the differential
  normalizes these duration fields (allowed substitution).

## Source discrepancies

- `runAuthCommand` is invoked twice in the source (before and after package/config
  commands). The Rust orchestration preserves both calls.
- `format_package_list` previously returned a trailing newline, which produced a
  blank line through `console_log`; fixed to match `console.log` output. This was a
  Rust port bug, not a source discrepancy.
- `tests/main_process.rs::write_models` writes a `models.json` literal that is
  missing its root closing brace. Both the pinned TypeScript CLI and the Rust
  port fail to parse it identically, so the existing print/json/RPC differentials
  never reach the provider. Fixing the literal exposes a real json-mode
  divergence (TypeScript emits the completed assistant message in `message_start`
  with `partialArgs`/`streamIndex`; Rust emits an empty pending message followed
  by deltas). The provider-reaching test uses `write_valid_models`; the json-mode
  divergence is reported, not fixed here.

## v15 startup name (approved 2026-10-20)

`--name` calls `SessionManager::set_session_name` per `c248b0067`, not `appendSessionInfo`.
`tests/startup_session_name.rs` ports both process-boundary cases: trimmed name persists in
metadata even when later model selection fails; empty name exits 1 without a name. Neither
path writes JSONL session-info entries.

## Embedded package metadata re-pin (2026-10-25)

`crates/pi-coding-agent/package.json` is refreshed byte-for-byte from the
read-only common pin, not regenerated. The existing asset identity test was
RED on the old embedded package metadata; its exact comparison is retained.
Built-in theme files are checked in the same test.
