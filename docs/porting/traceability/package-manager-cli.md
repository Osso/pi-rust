# coding-agent package-manager-cli.ts traceability

Q18 native error consumers: model_catalog_refresh_outcome and refresh_model_catalogs return JsString errors; CommandEffects preserves those errors; captured CommandOutcome stdout/stderr and output-formatting helpers retain JsString through Chalk. No capture-time UTF8 encoding. `tests/lossless_provider_errors.rs` asserts exact raw catalog error prefix/units; currently compiler-blocked by concurrent coding-library dependencies. Earlier done status below does not claim Q18 integration proof.

Source: `packages/coding-agent/src/package-manager-cli.ts` (889 lines, pinned
revision in `docs/porting/source-lock.json`). Rust:
`crates/pi-coding-agent/src/package_manager_cli.rs`.

Status: done, with three explicit seams for pieces owned by blocked modules
(see *Documented seams*). Parsing/help/validation is byte-differentially tested;
the execution half is behaviorally tested over a scripted `PackageCommandEffects`
host, the real settings manager, and the real trust store.

Q15 stale execution-help assertion corrected to the frozen line135 description, `Update pi and installed packages.` Actual production help was already current; `package_command_execution::help_and_validation_stay_synchronous` RED→GREEN. No output or validation assertion removed.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `PackageCommand` | `PackageCommand` | parser tests |
| `PackageCommandRuntimeOptions` | `PackageCommandEffects` (host seam) | execution tests |
| `handleConfigCommand` | `handle_config_command` (validation) + `execute_config_command` | differential `handle_config`, `config_command_passes_one_resolved_resource_set_and_ignores_old_local_flag`, `config_old_local_flag_does_not_require_project_trust` |
| `handlePackageCommand` | `handle_package_command` (validation) + `execute_package_command` | differential `handle_package_command`, 25 execution tests |
| private `parsePackageCommand` | `parse_package_command` | parser matrix, differential |
| private `getPackageCommandUsage` | `package_command_usage` | differential |
| private `printPackageCommandHelp` | `package_command_help` | differential |
| private `printConfigCommandHelp` | `config_command_help` | differential |
| private `updateTargetIncludesSelf` / `...Extensions` | `update_target_includes_self` / `_extensions` | predicate test |
| private `reportProjectTrustWarnings` | `report_project_trust_warnings_lines` | `report_project_trust_warnings_are_yellow` |
| private `reportSettingsErrors` | `report_settings_errors` | compile path; not directly asserted (no seeded `SettingsManager` error in the harness) |
| private `formatPackage` / list block | `format_package_list` | list formatting test, differential |
| private `getCommandAppMode` | `command_app_mode` | `command_app_mode_requires_both_streams_to_be_terminals` |
| private `createCommandSettingsManager` | `create_command_settings_manager` | install/trust tests, `saved_project_trust_is_used_for_updates`, `extension_load_errors_become_trust_warnings` |
| private `refreshModelCatalogs` | `refresh_model_catalogs` + `model_catalog_refresh_outcome` | `model_catalog_refresh_outcome_maps_abort_and_provider_errors`; runtime call is network-bound (gap) |
| private `getSelfUpdatePlan` | `get_self_update_plan` + `self_update_plan` | `self_update_plan_decides_between_running_and_up_to_date`, `get_self_update_plan_reports_lookup_failures` |
| private `runSelfUpdate` | `run_self_update` | `run_self_update_reports_exit_codes_and_signals` |
| private `printSelfUpdateUnavailable` | `self_update_unavailable_lines` | `bun_binary_install_reports_the_download_instruction`, `self_update_helpers_render_the_expected_lines` |
| private `printSelfUpdateFallback` | `self_update_fallback_line` | same |
| private `printPnpmSelfUpdateMetadataHint` | `pnpm_self_update_metadata_hint_lines` | same |
| private `printSelfUpdateNote` | `self_update_note_lines` + `SelfUpdateNoteMarkdownTheme` | `self_update_note_renders_markdown_between_blank_lines` |
| private `prepareWindowsNpmSelfUpdate` | `prepare_windows_npm_self_update` | Windows-only branch (gap: not exercised on Linux) |

## Execution branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | help / invalid option / missing value / invalid argument / conflict / missing source | `help_and_validation_stay_synchronous` + differential |
| 2 | `update --models` → refresh, success line | `models_target_refreshes_catalogs_and_skips_settings` |
| 3 | `update --models` → refresh failure, `Error:` + exit 1 | `models_refresh_failure_sets_exit_code` |
| 4 | settings creation failure propagates as `Err` | `create_command_settings_manager` error path via corrupt trust store (project-trust tests) |
| 5 | project-trust warnings printed yellow | `extension_load_errors_become_trust_warnings` |
| 6 | local install/remove without trust → red message + exit 1 | `local_writes_require_trust` |
| 7 | `--approve` bypasses the trust gate | `approved_local_install_runs_even_with_project_resources` |
| 8 | install: progress lines then `Installed …` | `install_prints_progress_then_the_installed_line` |
| 9 | install failure: `Error:` + exit 1 | `install_failure_prints_the_error_and_sets_exit_code` |
| 10 | remove: `Removed …` / `No matching package found for …` | `remove_success_prints_the_removed_line`, `remove_reports_missing_packages` |
| 11 | list: table from configured packages | `list_prints_the_configured_package_table` |
| 12 | update default target: skipped-extensions note, release check, run, `Updated … from … to …` | `default_update_notes_skipped_extensions_and_runs_self_update` |
| 13 | already current → up-to-date line, no run | `up_to_date_release_prints_the_up_to_date_line` |
| 14 | `--force` reruns even when current | `force_runs_the_self_update_even_when_current` |
| 15 | self-update failure → `Error:`, fallback hint, exit 1 | `self_update_failure_prints_the_fallback_hint` |
| 16 | bun-binary install → download instruction + entrypoint | `bun_binary_install_reports_the_download_instruction` |
| 17 | release lookup failure → `Error:` + exit 1 | `release_lookup_failure_sets_exit_code` |
| 18 | `--extension <source>` → `Updated <source>` | `extension_updates_print_the_updated_line` |
| 19 | `--all` → extensions then self | `all_updates_extensions_then_self` |
| 20 | `update` uses the saved trust decision (`useSavedProjectTrustOnly`) | `saved_project_trust_is_used_for_updates` |
| 21 | config: one `DefaultPackageManager.resolve()` result from command settings, `exitCode = 0` | `config_command_passes_one_resolved_resource_set_and_ignores_old_local_flag` |
| 22 | old `-l` flag does not select a view or require trust; untrusted project resources remain excluded | `config_old_local_flag_does_not_require_project_trust` |
| 23 | config non-`config` argv → not handled | `config_command_passes_one_resolved_resource_set_and_ignores_old_local_flag` |
| 24 | self-update plan: newer/equal/force/renamed package/note | `self_update_plan_decides_between_running_and_up_to_date` |
| 25 | note renderer: blank note, Markdown block with blank lines | `self_update_note_renders_markdown_between_blank_lines` |
| 26 | `runSelfUpdate`: success, non-zero exit, spawn failure | `run_self_update_reports_exit_codes_and_signals` |

## Documented seams

- **`ProjectTrustExtensionsLoader`** — `DefaultResourceLoader.loadProjectTrustExtensions`
  is not ported yet (`core/resource-loader.ts` is partial). The trait receives
  `(cwd, agentDir, settingsManager)` and returns `Option<LoadExtensionsResult>`;
  `DefaultPackageCommandEffects` takes it as a constructor argument, so production
  wiring must pass the real loader when it lands. Until then, callers that pass a
  loader returning `None` lose extension-provided `project_trust` decisions.
- **`ConfigSelector`** — `execute_config_command` builds the exact
  `SelectConfigRequest` (flat `ResolvedPaths`, settings store, cwd, agent dir), matching
  frozen `package-manager-cli.ts:566–574`. No alternate global/project resolution,
  write scope, project-mode availability or missing-source skip handler remains. The concrete adapter is
  `cli::config_selector::TuiConfigSelector`, which runs the ported config TUI; see
  `config-selector.md`. `PackageCommandEffects` and `ConfigSelector` are not
  `Send`/`Sync` because the TUI is `Rc`-based (the command path is single-threaded).
- **Interactive trust prompt** — `create_command_settings_manager` takes an
  optional `startup_ui`. When it is `None` (current state, since `cli/startup-ui.ts`
  is blocked), an interactive `ask` prompt resolves as dismissed/untrusted, exactly
  like the `ProjectTrustStartupUi` seam in `cli/project-trust.ts`. Callers that can
  provide the startup UI get the TypeScript prompt behavior.

## Deviations and discrepancies

- **`stdout.columns`** — `printSelfUpdateNote` reads `process.stdout.columns` at
  print time; the Rust seam exposes `PackageCommandEffects::stdout_columns()`
  (the default host returns `None`, i.e. 80). Callers that can query a terminal
  width should return it.
- **Provider error order** — `refreshModelCatalogs` joins `result.errors` in
  `Map` insertion order; `ModelsRefreshResult.errors` is a `BTreeMap`, so the
  Rust message lists providers in sorted order. Recorded as a source
  discrepancy of the ported `pi-ai` result type.
- **Error stacks** — `reportSettingsErrors` prints `error.stack` when present;
  `SettingsError` has no stack, so only the message line is emitted.
- **`process.exit(0)`** — `handleConfigCommand` sets `exitCode = 0`; the outcome
  carries `exit_code: Some(0)` for the caller to apply.

## Proof

- `cargo test -p pi-coding-agent --test package_manager_cli` (8 tests)
- `cargo test -p pi-coding-agent --test package_command_execution` (31 tests)
- `cargo test -p pi-coding-agent --test differential_package_manager_cli` (2 tests, 36 argument cases)
- `cargo clippy -p pi-coding-agent --all-targets -- -D warnings`

## Q18 native command flow (2026-10-22)

Package command argv/source/options, captured diagnostics/help, settings/trust
paths/callbacks, config selections, self-update plans, and effects errors use
JsString. Only actual self-update process argv encode; model-catalog paths use
native joins. Config self-update Result failures propagate through command flow
and unavailable-instruction output instead of manufacturing a fallback.

Command-name recognition narrows only the ASCII command enum, returning no command
on unpaired input; the untouched native source/options continue when applicable.
Self-update comparison uses native version-check semantics, including invalid
semver's trimmed-string inequality fallback (not blanket no-match).

Raw source/invalid-option fixture written but unexecuted. replacement-cli-2
reached six CLI diagnostics: four old config dependencies and two local fixes
subsequently corrected; parent native config merge is needed for final diagnostic
proof. No runtime PASS or renewed frozen differential claim.

## Baseline conflict expectation (2026-10-25)

Frozen parsePackageCommand reports `--all cannot be combined with --self,
--extensions, or --extension`; the stale unit expectation additionally named
--models. The concrete expected string is replaced with the frozen value.
The JSON differential remains exact and no longer invokes the real config UI.

## Successor command-error correction (2026-10-25)

Whole-module audit found native list failures escaped the frozen catch, self-update read the merged rather than global npm command, and child failures used OS descriptions/numeric signals. Native REDs in `/tmp/settings-models-package-red.log` cover those boundaries; fixes preserve exact UTF-16 list errors, global-only npm selection, `spawn … ENOENT`, and `SIGTERM`. The stale lookup assertion now requires the full frozen wrapper including `: offline`, not a weakened assertion. Targeted GREEN: `/tmp/settings-models-package-green.log` (30 passing; scope fixture needed an existing package directory), `/tmp/settings-models-package-scope-green.log` (corrected real executable fixture passes). [Contract](../../specs/package-command-errors.md).
