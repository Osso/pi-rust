# theme-controller.ts traceability

Source: `packages/coding-agent/src/modes/interactive/theme/theme-controller.ts` (139 lines). Rust: `crates/pi-coding-agent/src/modes/interactive/theme/theme_controller.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class InteractiveThemeController` | `struct InteractiveThemeController` | gap: constructor requires a TUI + settings manager; exercised by the interactive-mode slice |
| `constructor(ui, settingsManager, showError, onChanged)` | `new(Rc<dyn ThemeUi>, Rc<RefCell<SettingsManager>>, show_error, on_changed)` | traceability-only |
| `rebindTui()` | `rebind_tui()` | traceability-only |
| `applyFromSettings()` | `apply_from_settings()` | traceability-only |
| `setThemeName(name, showError?)` | `set_theme_name(&str, bool)` | traceability-only |
| `setThemeInstance(theme)` | `set_theme_instance(Arc<Theme>)` | traceability-only |
| `preview(settingOrName)` | `preview(&str)` | traceability-only |
| `disableAutoSync()` | `disable_auto_sync()` | traceability-only |
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

- No test in this slice: the controller needs a TUI and a `SettingsManager` instance; the TypeScript package has no dedicated controller test either. Wiring is covered when `modes/interactive/interactive-mode.ts` is ported.
