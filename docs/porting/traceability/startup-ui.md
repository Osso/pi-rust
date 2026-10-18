# startup-ui traceability

Source: `packages/coding-agent/src/cli/startup-ui.ts` (239 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/cli/startup_ui.rs`.

Status: partial. The distribution/setup predicates and theme loading are ported and tested.
The TUI construction and the selector/input/first-time-setup dialogs are blocked.

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `shouldRunFirstTimeSetup` | `should_run_first_time_setup` (+ `_for_process`) | `first_time_setup_needs_official_distribution_experimental_flag_default_agent_dir_and_missing_settings` |
| `createStartupTui` | blocked | — |
| `startStartupTui` | blocked | — |
| `showStartupSelector` | blocked | — |
| `showFirstTimeSetup` | blocked | — |
| `showStartupInput` | blocked | — |
| private `isOfficialDistribution` | `is_official_distribution` | `official_distribution_requires_all_three_identifiers` |
| private `loadThemes` | `load_themes` | `load_themes_skips_disabled_and_broken_entries_and_dedupes_names` |
| private `loadStartupThemes` | `load_startup_themes` | `load_startup_themes_uses_global_settings_only` |

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | official package/app/config-dir triple | `official_distribution_requires_all_three_identifiers` |
| 2 | `PI_EXPERIMENTAL !== "1"` → no setup | `first_time_setup_...` |
| 3 | agent-dir override set → no setup | same |
| 4 | settings file exists → no setup | same |
| 5 | all conditions hold → setup | same |
| 6 | disabled resource skipped | `load_themes_...` |
| 7 | broken theme skipped (`catch`) | same |
| 8 | duplicate theme name skipped | same |
| 9 | enabled theme loaded | same |
| 10 | `loadStartupThemes` reads global settings with `projectTrusted: false` and skips missing packages | `load_startup_themes_uses_global_settings_only` |

## Blocked parts

- **`createStartupTui` / `startStartupTui` / `applyDetectedStartupTheme` / `clearStartupTui`** —
  need a `TerminalAutoThemeDetector` adapter over `TuiMainScreen` (the theme module exposes the
  trait, but no TUI adapter exists yet) plus `TuiMainScreen::with_options` wiring. `init_theme`
  also resolves built-in themes from the package directory, so an in-process test would need the
  package-dir probe used by `tests/theme.rs`.
- **`showStartupSelector`** — `ExtensionSelectorComponent` is ported, but driving it requires the
  TUI event loop and `startStartupTui`; no fake terminal exists in the workspace.
- **`showStartupInput` / `showFirstTimeSetup`** — blocked on `ExtensionInputComponent` and
  `FirstTimeSetupComponent` (`modes/interactive/components/`), both not ported.

`shouldRunFirstTimeSetup` takes the environment map explicitly because edition 2024 forbids a
safe `std::env::set_var`; `should_run_first_time_setup_for_process` reads `std::env::vars()` for
production callers.

## Proof

`cargo test -p pi-coding-agent --test startup_ui` (4 tests).
