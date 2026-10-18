# main.ts traceability

Source: `packages/coding-agent/src/main.ts` (1,364 lines, pinned revision).

Status: **partial** — startup helpers are ported and tested; `main()` orchestration,
the `pi` binary target, and process-level differentials land with the consolidated
`master` that contains the print/RPC modes, `DefaultResourceLoader`, and
`self-restart` (in-flight slices ca-rpc, ca-resource-loader/ca-agent-session-2).

## Exported symbols

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `MainOptions` | `main_runner::MainOptions` | compile-time use in `tests/main.rs` |
| `runCliActionOrReportError` | `main_runner::run_cli_action_or_report_error` | `cli_action_wrappers_map_errors_to_exit_codes` |
| `runInteractiveActionOrReportTerminalError` | `main_runner::run_interactive_action_or_report_terminal_error` | `cli_action_wrappers_map_errors_to_exit_codes` |
| `main` | pending (orchestration commit) | pending process differential |

## Private helpers

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `isTruthyEnvFlag` | `is_truthy_env_flag` | `truthy_env_flags_match_javascript_lowercasing` |
| `resolveAppMode` | `resolve_app_mode` | `app_mode_follows_mode_print_and_tty_state` |
| `toPrintOutputMode` | `to_print_output_mode` | `app_mode_follows_mode_print_and_tty_state` |
| `isPlainRuntimeMetadataCommand` | `is_plain_runtime_metadata_command` | `plain_runtime_metadata_commands_skip_stdout_takeover` |
| `collectSettingsDiagnostics` | `collect_settings_diagnostics` | `settings_diagnostics_prefix_scope_and_context` |
| `reportDiagnostics` | `report_diagnostics` | covered by the orchestration process differential (color depends on TTY) |
| `refreshRpcModelCatalogs` | `refresh_rpc_model_catalogs` | pending RPC process differential |
| `runAuthCommand` | `run_auth_command` | pending auth process differential |
| `prepareInitialMessage` | `prepare_initial_message` | `initial_message_concatenates_stdin_and_the_first_cli_message` |
| `findLocalSessionByExactId` | `find_local_session_by_exact_id` | `create_session_manager_honors_help_in_memory_and_session_ids` |
| `sessionBelongsToProject` | `session_belongs_to_project` | `session_project_membership_uses_the_git_common_directory` |
| `resolveSessionPath` | `resolve_session_path` | `session_paths_resolve_locally_globally_and_by_prefix` |
| `promptConfirm` | `prompt_confirm` | pending `--session` global-fork process differential |
| `validateForkFlags` | `validate_fork_flags` / `fork_flag_conflict` | `fork_conflicts_list_flags_in_source_order` |
| `validateSessionIdFlags` | `validate_session_id_flags` / `session_id_flag_conflict` | `session_id_conflicts_and_validation_match_the_source` |
| `openSessionOrExit` | `open_session_or_exit` | pending process differential |
| `forkSessionOrExit` | `fork_session_or_exit` | pending process differential |
| `createSessionManager` | `create_session_manager` | `create_session_manager_honors_help_in_memory_and_session_ids` |
| `buildSessionOptions` | `build_session_options` | `build_session_options_prefers_cli_model_and_explicit_thinking`, `build_session_options_uses_scoped_models_and_tool_flags` |
| `resolveCliPaths` | `resolve_cli_paths` | `cli_paths_resolve_local_entries_only` |
| `promptForMissingSessionCwd` | pending (`showStartupSelector` needs a terminal) | pending interactive slice |
| `firstPartyExtensionFactory` | `first_party_extension_factory` | `first_party_factory_list_partitions_the_source_names`, `first_party_factories_load_and_register_agent_tools` |
| `runtimeExtensionFactory` | pending (bundled extension registry) | pending |
| `createFirstPartyExtensionFactories` | `create_first_party_extension_factories` (bundled modules present on master) | `first_party_factory_list_partitions_the_source_names`, `first_party_factories_load_and_register_agent_tools` |
| `readPipedStdin` | pending (orchestration commit) | pending process differential |

## Explicit gaps

- `main()` orchestration is pending the consolidated `master`. The call order is
  recorded from the source and will be implemented verbatim: resident-console
  dispatch, `architect`/`supervisor` service dispatch, timings reset, package/config
  commands, auth, migrations, args, self-restart handoff, extension factories,
  version/export, mode resolution, fork/session-id validation, worktree, settings,
  session manager, trust, runtime creation, help/list commands, stdin, initial
  message, theme, diagnostics, model checks, mode dispatch.
- `--resume` currently returns an explicit error: `selectSession` is blocked on
  `SessionSelectorComponent` (`cli/session-picker.ts`). This is a recorded gap, not
  a fallback.
- Interactive mode dispatch will return an explicit error naming the unported
  `interactive-mode.ts` (per the slice brief) once `main()` lands.
- `--supervisor` / `--architect` resident console and the `architect`/`supervisor`
  subcommands are blocked on `cli/resident-console-command.ts` UI and the
  `architect/main.ts` / `supervisor/main.ts` service loops.
- `core/debug-repl.ts` is a documented gap (`node:vm`); the `debug` extension
  factory cannot be registered until a Rust-native REPL exists.
- Bundled extension factories: `agents-core`, `agent-viewer`, and `agents-mailbox`
  are wired from `src/bundled_extensions/`. `FIRST_PARTY_EXTENSION_NAMES` records
  the full source list and `PENDING_FIRST_PARTY_EXTENSIONS` records the names whose
  factories are still in flight (approval-controls, ask-secret, browser-cli,
  claude-bash-hook, claude-memory-enrich, claude-memory-session-end, codex-fast,
  codex-image-generation, codex-usage, codex-web-search, debug, default-footer,
  docs-tree-context, effort, goal, bwrap, pyrun, loop, openai-remote-compact,
  run-plan, safe, spec-validation, session-archive, self-restart, session-id).
  A test enforces that the registered and pending lists partition the source list.
- Production child/attached agent session factories are blocked on agent-session
  multi-agent identity options; the registered agents extensions therefore expose
  the documented "child orchestration unavailable" path.
- `MultiAgentStore::restore_from_session_manager` needs a `SessionPersistence`
  implementation for `SessionManager` (in flight with session metadata); until it
  lands the store is created empty.

## Source-port discrepancies

- `bundled_extensions::agents_core::runtime::multi_agent_extension` mirrors the
  TypeScript `runtime.ts` default export, which registers core, viewer, and
  mailbox tools. `main.ts` imports the package index default export
  (`agents-core/src/index.ts`), which calls `registerAgentsCoreTools` only. The
  first-party factory uses `register_agents_core_tools`; calling
  `multi_agent_extension` would register duplicate tools and fail extension
  loading when all three factories are present.

## Source discrepancies

- `runAuthCommand` is invoked twice in the source (before and after package/config
  commands). The Rust orchestration will preserve both calls.
- `sessionManager.setMetadataControlDbPath`, `bindExtensions`,
  `bindAgentSessionMutationTargetResolver`, and `createMultiAgentRuntimeHandles`
  are in-flight dependencies; the orchestration will use the merged APIs rather
  than local substitutes.
