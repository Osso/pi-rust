# startup-ui traceability

Source: `packages/coding-agent/src/cli/startup-ui.ts` (239 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/cli/startup_ui.rs`.

Status: done. Predicates, theme loading, the startup TUI, the selector dialog,
the text-input dialog, and the first-time-setup dialog are ported and tested.

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `shouldRunFirstTimeSetup` | `should_run_first_time_setup` (+ `_for_process`) | `first_time_setup_needs_official_distribution_experimental_flag_default_agent_dir_and_missing_settings` |
| `createStartupTui` | `create_startup_tui` | selector probes (create + start + clear) |
| `startStartupTui` | `start_startup_tui` | selector probes |
| `showStartupSelector` | `show_startup_selector` | `startup_selector_returns_the_selected_value`, `startup_selector_returns_none_when_cancelled` |
| `showFirstTimeSetup` | `show_first_time_setup` | `first_time_setup_persists_theme_and_analytics` (pty, real binary) |
| `showStartupInput` | `show_startup_input` | `startup_input_returns_the_typed_text`, `startup_input_returns_none_when_cancelled` |
| private `isOfficialDistribution` | `is_official_distribution` | `official_distribution_requires_all_three_identifiers` |
| private `loadThemes` | `load_themes` | `load_themes_skips_disabled_and_broken_entries_and_dedupes_names` |
| private `loadStartupThemes` | `load_startup_themes` | `load_startup_themes_uses_global_settings_only` |
| private `startStartupTui`/`applyDetectedStartupTheme`/`clearStartupTui` | `start_startup_tui`, `apply_detected_startup_theme`, `clear_startup_tui` | selector probes |
| — (theme detector adapter) | `StartupThemeDetector` (`TerminalBackgroundThemeDetector` + `TerminalAutoThemeDetector` over `TuiMainScreen`) | selector probes (100 ms query, env fallback) |

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | official package/app/config-dir triple | `official_distribution_requires_all_three_identifiers` |
| 2 | `PI_EXPERIMENTAL !== "1"` → no setup | `first_time_setup_...` |
| 3 | agent-dir override set → no setup | same |
| 4 | settings file exists → no setup | same |
| 5 | all conditions hold → setup | same |
| 6 | disabled resource / broken theme / duplicate name / enabled theme | `load_themes_...` |
| 7 | startup themes read global settings with `projectTrusted: false`, missing packages skipped | `load_startup_themes_uses_global_settings_only` |
| 8 | registered themes + terminal background + theme setting resolution + keybindings + cursor/clear-on-shrink | selector probes (`create_startup_tui`) |
| 9 | fixed theme setting skips auto detection; auto/unset runs the 100 ms query then `setTheme` | selector probes with no theme setting |
| 10 | selector value lookup by label; cancel resolves `undefined` | `startup_selector_returns_the_selected_value`, `..._none_when_cancelled` |
| 11 | clear (25 ms) + `ui.stop()` before resolving | selector probes |
| 12 | setup: start TUI, 100 ms theme detection, `setTheme`, preview re-render, submit persists theme/analytics + `flush`, cancel persists nothing, clear+stop | `first_time_setup_persists_theme_and_analytics` |
| 13 | input: submit resolves the typed text, cancel resolves `undefined`, clear+stop | `startup_input_returns_the_typed_text`, `startup_input_returns_none_when_cancelled` |

## Blocked parts

- **`ProjectTrustStartupUi` adapter** — `cli::project_trust::ProjectTrustStartupUi` returns
  `BoxFuture` (Send) and lives in `Arc<dyn ExtensionUiContext>` (Send + Sync), while the startup
  TUI is `Rc`-based. A Send-safe driver over the TUI is required before the adapter can be
  written; until then `create_project_trust_context` keeps its `startup_ui: None` seam.

## Deviations

- **Event loop** — the TypeScript TUI owns its loop; `show_startup_selector` polls
  `process_events`/`flush_pending` every 5 ms until the selector resolves, then clears (25 ms)
  and stops.
- **`applyDetectedStartupTheme`** — TypeScript runs it as `void` after `ui.start()`; the Rust
  detection path is synchronous (query + event polling), so `start_startup_tui` runs it inline
  before returning.
- **Theme query adapter** — `TuiMainScreen` queries settle through `process_events`, so the
  adapter polls until the query settles; a timeout surfaces as `None`, matching the source.
- **`shouldRunFirstTimeSetup`** takes the environment map explicitly because edition 2024 forbids
  a safe `std::env::set_var`; `should_run_first_time_setup_for_process` reads `std::env::vars()`.

## Proof

`cargo test -p pi-coding-agent --test startup_ui` (8 tests + 2 ignored probe entries; the
selector and input tests run the TUI in a child process with `PI_PACKAGE_DIR` set).

## Q18 native UTF-16 integration (2026-10-22)

Startup selector/input title/options/results/errors retain JsString through component callbacks. Startup theme resource names and paths remain native; OS setup errors are promoted once at their source. No primary diagnostics at interactive6; runtime tests remain unexecuted.

## Whole-module common-pin audit (2026-10-25)

All 239 frozen lines compared. Official identity/exact experimental gate/settings existence, global-only theme resources with skip-on-broken/dedupe, TUI/theme/keybinding setup, selector/input/first-time-setup completion and persistence map to the native implementation. Fixed empty custom-agent-directory truthiness: an empty environment value is not a custom directory. `first_time_setup_environment_truthiness_matches_frozen` compares30 actual-source cases (5 experimental values ×3 agent-dir values ×2 settings-file states); RED `/tmp/settings-models-startup-red.log`. Existing native startup selector/input/theme/resource fixtures and real-PTY first-time-setup persistence retain behavioral proof. The new picker terminal proof also exercises the same startup TUI boundary.

Gap: native theme detection pumps synchronously rather than the frozen detached promise; independent input/query completion scheduling is an excluded runtime oddity. Native default startup input has no timeout, so its component disposal has no timer side effect to reproduce. Output/selected values and persistence remain under exact parity.

GREEN: c4022e70d: /tmp/settings-models-startup-green.log (10 active tests;3 child helpers);69513786b: /tmp/settings-models-startup-pty-final.log (real PTY persistence). Targeted warning-mode clippy at69513786b: `/tmp/settings-models-checkpoint-clippy.log`, exit0; existing warnings at control_command.rs:55 and sessions_command.rs:136 are unchanged conversions, none on changed lines. Changed Rust/TS manually audited for readability; no in-scope violations.
