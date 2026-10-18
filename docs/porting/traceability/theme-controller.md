# theme-controller.ts traceability

Source: `packages/coding-agent/src/modes/interactive/theme/theme-controller.ts` (139 lines). Rust: `crates/pi-coding-agent/src/modes/interactive/theme/theme_controller.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class InteractiveThemeController` | `struct InteractiveThemeController` | `tests/theme_controller.rs` (fake `ThemeUi` + real `SettingsManager`) |
| `constructor(ui, settingsManager, showError, onChanged)` | `new(Rc<dyn ThemeUi>, Rc<RefCell<SettingsManager>>, show_error, on_changed)` | `applies_explicit_and_preview_themes` |
| `rebindTui()` | `rebind_tui()` | `rebinds_tui_listeners` |
| `applyFromSettings()` | `apply_from_settings()` | `auto_sync_follows_terminal_color_scheme`, `detects_and_persists_terminal_theme` |
| `setThemeName(name, showError?)` | `set_theme_name(&str, bool)` | `applies_explicit_and_preview_themes` |
| `setThemeInstance(theme)` | `set_theme_instance(Arc<Theme>)` | traceability-only |
| `preview(settingOrName)` | `preview(&str)` | `applies_explicit_and_preview_themes` |
| `disableAutoSync()` | `disable_auto_sync()` | `auto_sync_follows_terminal_color_scheme` |
| `getTerminalTheme()` | `get_terminal_theme()` | traceability-only |

## Branches

| TypeScript branch | Rust branch |
|---|---|
| auto setting → detect color scheme, enable sync, pick light/dark | `apply_from_settings` first arm |
| explicit setting → apply with error display | second arm |
| no setting → OSC 11/COLORFGBG detection, persist on high confidence | third arm |
| `applyThemeName` fallback to `dark` + error text | `ControllerCore::apply_theme_name` |
| `setAutoSync` early return | `ControllerCore::set_auto_sync` |
| color-scheme listener only when auto-sync enabled | `ControllerCore::apply_terminal_theme` |
| `preview` uses resolved setting, falls back to active name | `preview` |

## Behavioral notes

- `ui` is represented by the `ThemeUi` trait: `invalidate`, `request_render`, color-scheme notification toggling, listener registration/removal, and the two OSC queries. The TypeScript object is duck-typed, so this is a structural mapping, not a new abstraction.
- `queryTerminalColorScheme` remains optional through the defaulted trait method, matching `ui.queryTerminalColorScheme?.()`.
- Detection calls are synchronous in Rust (the pi-tui query API is poll-based), so the "start both queries, prefer color scheme" concurrency is expressed as a sequential color-scheme query followed by background detection. Result selection is identical.

## Gaps / exclusions

- `setThemeInstance` and `getTerminalTheme` are exercised only through the shared fake-TUI probe; the TypeScript package has no dedicated controller test.
- The `ThemeUi` implementation for the real TUI belongs to the interactive-mode slice.
