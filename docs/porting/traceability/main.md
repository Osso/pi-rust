# main.ts traceability

Source: `packages/coding-agent/src/main.ts` (1,364 lines, pinned revision).

Rust: `crates/pi-coding-agent/src/main.rs` (module `main_runner`) and the `pi`
binary at `crates/pi-coding-agent/src/bin/pi.rs`.

Status: **done** for `main.ts`. Dispatch targets that live in unported modules
return an explicit gap error; they are listed under "Explicit gaps".

## Exported symbols

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `MainOptions` | `main_runner::MainOptions` | `tests/main.rs`, `tests/main_process.rs` |
| `main` | `main_runner::main` / `run_main_or_report_error` | process differentials (`tests/main_process.rs`, 11) |
| `runCliActionOrReportError` | `main_runner::run_cli_action_or_report_error` | `cli_action_wrappers_map_errors_to_exit_codes` |
| `runInteractiveActionOrReportTerminalError` | `main_runner::run_interactive_action_or_report_terminal_error` | `cli_action_wrappers_map_errors_to_exit_codes` |

## Private helpers

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `isTruthyEnvFlag` | `is_truthy_env_flag` | `truthy_env_flags_match_javascript_lowercasing` |
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
| `createFirstPartyExtensionFactories` | `create_first_party_extension_factories` | same first-party tests |
| `createRuntime` (closure in `main`) | `create_runtime_factory` / `create_runtime` | `runtime_factory_creates_services_and_reports_missing_models`, print/json differentials |
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
  dispatch returns the explicit gap error for `interactive-mode.ts`.

## Explicit gaps

- `--resume` returns an explicit error: `cli/session-picker.ts` `selectSession` is
  unported (`SessionSelectorComponent`).
- Interactive mode dispatch returns an explicit error naming
  `modes/interactive/interactive-mode.ts` until the `ca-interactive-mode` slice lands.
- `--supervisor` / `--architect` resident console and the `architect`/`supervisor`
  subcommands are not wired; the service loops are owned by the hosts slice.
- `core/debug-repl.ts` is a documented gap (`node:vm`), so the `debug` extension
  factory is not registered.
- First-party factories: `agents-core`, `agent-viewer`, `agents-mailbox`, `goal`,
  `bwrap`, and `pyrun` are wired. The Pyrun factory constructs the multi-agent Pi
  request handler from the extension API and falls through to builtin Pyrun
  requests for unowned methods. `FIRST_PARTY_EXTENSION_NAMES` lists the source
  names and `PENDING_FIRST_PARTY_EXTENSIONS` the remaining factories
  (approval-controls, ask-secret, browser-cli, claude-bash-hook,
  claude-memory-enrich, claude-memory-session-end, codex-fast,
  codex-image-generation, codex-usage, codex-web-search, debug, default-footer,
  docs-tree-context, effort, loop, openai-remote-compact, run-plan, safe,
  spec-validation, session-archive, self-restart, session-id). A test enforces
  that registered and pending names partition the source list.
- `bindAgentSessionMutationTargetResolver` is not ported; the store is restored but
  mutation-target resolution for interactive views is skipped.
- Production child/attached session factories remain unavailable (documented
  "child orchestration unavailable" path).

## Deviations

- `process.title`, `process.emitWarning`, and the `PI_CODING_AGENT`/`AI_AGENT`
  environment writes from `cli.ts` are not applied. Edition 2024 makes
  `std::env::set_var` unsafe and the workspace forbids unsafe code;
  `cli_entry::CLI_PROCESS_ENV` records the values for the in-flight
  `pi_js_compat::process_env` overlay. Apply it at startup once that module lands.
  `process.title`/`emitWarning` have no Rust analogue.
- `applyHttpProxySettings` has no process-global equivalent. Settings `httpProxy`
  is not written into the environment; the HTTP client reads the ambient proxy
  environment instead. `resolve_http_proxy_settings` captures the source decision.
- `--version` reads `package.json` from `PI_PACKAGE_DIR` (or the executable
  directory), matching the deployed layout. Process differentials stage that file.
- The JSON mode serializes `startedAt`/`finishedAt` as floats; the differential
  normalizes these duration fields (allowed substitution).

## Source discrepancies

- `runAuthCommand` is invoked twice in the source (before and after package/config
  commands). The Rust orchestration preserves both calls.
- `format_package_list` previously returned a trailing newline, which produced a
  blank line through `console_log`; fixed to match `console.log` output. This was a
  Rust port bug, not a source discrepancy.
