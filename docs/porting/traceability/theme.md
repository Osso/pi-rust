# theme.ts traceability

Source: `packages/coding-agent/src/modes/interactive/theme/theme.ts` (1337 lines). Rust: `crates/pi-coding-agent/src/modes/interactive/theme/mod.rs`. Shipped JSON: `crates/pi-coding-agent/theme/{dark,light,theme-schema}.json` (served from `config::get_themes_dir()` = `PI_PACKAGE_DIR/theme`).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class Theme` | `struct Theme` | `theme.rs` probes, `differential_theme.rs` |
| `Theme.fg/bg/bold/italic/underline/inverse/strikethrough` | same methods | `theme_scrollbar_probe`, differential fg/bg |
| `Theme.getFgAnsi/getBgAnsi/getColorMode` | same methods | `theme_global_probe`, differential |
| `Theme.getThinkingBorderColor(level)` | `get_thinking_border_color(ThinkingLevel) -> StyleFn` | traceability-only (callback shape); colors covered by `get_fg_ansi` differential |
| `Theme.getBashModeBorderColor()` | `get_bash_mode_border_color() -> StyleFn` | traceability-only |
| `getAvailableThemes()` | `get_available_themes() -> Result<Vec<String>, ThemeError>` | `theme_picker_probe` |
| `getAvailableThemesWithPaths()` | `get_available_themes_with_paths() -> Result<Vec<ThemeInfo>, ThemeError>` | `theme_picker_probe` |
| `loadThemeFromPath(path, mode?)` | `load_theme_from_path(&Path, Option<ColorMode>)` | `theme_validation_probe`, `theme_scrollbar_probe`, differential (22 cases) |
| `getThemeByName(name)` | `get_theme_by_name(&str) -> Option<Arc<Theme>>` | `theme_global_probe` |
| `parseAutoThemeSetting(setting)` | `parse_auto_theme_setting(Option<&str>)` | `theme_detection_probe`, differential helpers |
| `resolveThemeSetting(setting, terminalTheme)` | `resolve_theme_setting(...)` | both |
| `detectTerminalBackgroundFromEnv(options?)` | `detect_terminal_background_from_env(&BTreeMap)` | both |
| `detectTerminalBackgroundTheme({ui, timeoutMs, env})` | `detect_terminal_background_theme(&dyn TerminalBackgroundThemeDetector, Duration, env)` | `theme_detection_probe` |
| `detectTerminalThemeForAuto({ui, timeoutMs, env})` | `detect_terminal_theme_for_auto(&dyn TerminalAutoThemeDetector, ...)` | `theme_detection_probe` |
| `getThemeForRgbColor(rgb)` | `get_theme_for_rgb_color(RgbColor)` | both |
| `getDefaultTheme()` | `get_default_theme()` | `theme_global_probe` (via `init_theme(None)` path) |
| `theme` (global Proxy) | `current_theme() -> Arc<Theme>` / `try_current_theme() -> Result<Arc<Theme>, ThemeError>` / `with_theme` | `theme_missing_builtin_probe`, all probes that render |
| `setRegisteredThemes(themes)` | `set_registered_themes(Vec<Arc<Theme>>)` | `theme_global_probe` (registered lookup path covered by differential load) |
| `initTheme(name?, enableWatcher?)` | `init_theme(Option<&str>, bool) -> Result<(), ThemeError>` | `theme_global_probe` (missing-theme fallback), `theme_missing_builtin_probe`, `main_process::missing_builtin_theme_differential` |
| `setTheme(name, enableWatcher?)` | `set_theme(&str, bool) -> Result<SetThemeResult, ThemeError>` | `theme_global_probe`, `theme_missing_builtin_probe` |
| `setThemeInstance(theme)` | `set_theme_instance(Arc<Theme>)` | `theme_global_probe` |
| `onThemeChange(callback)` | `on_theme_change(Rc<dyn Fn()>)` | `theme_global_probe` |
| `stopThemeWatcher()` | `stop_theme_watcher()` | traceability-only |
| `getResolvedThemeColors(name?)` | `get_resolved_theme_colors(Option<&str>) -> Result<Vec<(String,String)>>` | `theme_resolved_probe` |
| `isLightTheme(name?)` | `is_light_theme(Option<&str>)` | `theme_language_probe` |
| `getThemeExportColors(name?)` | `get_theme_export_colors(Option<&str>) -> ThemeExportColors` | `theme_export_probe` |
| `highlightCode(code, lang?)` | `highlight_code(code, Option<&str>) -> Vec<String>` | not differential: highlight.js output is covered by the existing `syntax_highlight` port; language validation branch covered by `get_language_from_path` tests |
| `getLanguageFromPath(path)` | `get_language_from_path(&str)` | `theme_language_probe`, differential helpers |
| `getMarkdownTheme()` | `get_markdown_theme() -> Box<dyn MarkdownTheme>` | construction only; rendering belongs to pi-tui markdown |
| `getSelectListTheme()` | `get_select_list_theme()` | used by selector components |
| `getEditorTheme()` | `get_editor_theme()` | used by editor components |
| `getSettingsListTheme()` | `get_settings_list_theme()` | used by settings selector |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| `hexToRgb`: length != 6 / NaN channel | `hex_to_rgb` → `InvalidHexColor` | differential `bad-hex`, extra-color case |
| `rgbTo256` cube/gray distance, `spread < 10` | `rgb_to_256` | differential `dark256`/`full-vars-256` |
| `fgAnsi`/`bgAnsi`: empty, number, `#`, invalid | same match arms | differential full-vars (empty/0/15/231/232/255) |
| `resolveVarRefs`: literal, circular, missing, recursion | same | differential `circular`, `missing-var`, `full-vars` alias chain |
| `withThemeColorFallbacks` (`??` per optional token) | `with_theme_color_fallbacks` | `theme_scrollbar_probe`, differential `fallbacks` |
| `createTheme` bg key partition | `create_theme` | differential all cases |
| TypeBox validation (required + union errors, 8-error iterator cap) | `validate_theme_json` + `MAX_VALIDATION_ERRORS` | differential `missing`, `bad-types`, `many-bad`, `required-truncated`, `no-name*` |
| `assertThemeNameIsValid` (only on parsed files/registration) | `assert_theme_name_is_valid` | differential `slash` |
| `parseThemeJsonContent` JSON.parse failure | `ThemeError::Parse` with `SyntaxError: <V8 message>` | differential `invalid-json`/`truncated-json`/`trailing-json`/`empty-json` |
| `getBuiltinThemes` read/parse failure | `ThemeError::Io` (Node fs text) / `ThemeError::Syntax` (raw V8 message, `label()` = `SyntaxError`) | `theme_missing_builtin_probe`, `main_process::{missing,invalid}_builtin_theme_differential` |
| `getCustomThemeInfos` `readdirSync` failure | `ThemeError::Io` (`scandir`) | `theme_missing_builtin_probe` (absent dir is empty, matching `existsSync`) |
| builtin cache (`BUILTIN_THEMES`) | `ThemeState.builtin` thread-local cache | `theme_picker_probe`, `theme_resolved_probe` |
| custom theme discovery, invalid ignored | `get_custom_theme_infos` | `theme_picker_probe` (broken.json ignored) |
| duplicate-name dedupe + `localeCompare` sort | `get_available_themes_with_paths` | `theme_picker_probe` |
| registered theme with/without `sourcePath` | `load_theme_json` | `NoSourcePath` branch traceability-only |
| `getThemeExportColors` error swallowing | `get_theme_export_colors` | `theme_export_probe` (missing theme → defaults) |
| `ansi256ToHex` basic/cube/gray | `ansi256_to_hex` | `theme_export_probe` (24 → `#005f87`), `theme_resolved_probe` |
| COLORFGBG last-valid-index scan | `get_color_fg_bg_background_index` | differential helpers |
| luminance thresholds | `get_rgb_color_luminance` | differential helpers |
| `detectTerminalThemeForAuto` color-scheme preference + fallback | `detect_terminal_theme_for_auto` | `theme_detection_probe` |
| `initTheme` fallback to dark, no watcher; fallback failure escapes | `init_theme` | `theme_global_probe`, `theme_missing_builtin_probe` |
| `setTheme` fallback + callback + error text; fallback failure escapes | `set_theme` | `theme_global_probe`, `theme_missing_builtin_probe` |
| `setThemeInstance` sets `<in-memory>` | `set_theme_instance` | `theme_global_probe` |
| theme file watcher (100 ms debounce, reload, callback) | `start_theme_watcher`/`schedule_theme_reload` | gap: timing-based, not tested; reload applied on next `current_theme()` access |

## Behavioral notes / deviations

- Module state is `thread_local!` because it holds non-`Send` UI callbacks (`onThemeChange`). The watcher thread parses reloads into a `Mutex` slot that the main thread applies on the next `current_theme()` call; TypeScript applies it from a timer callback. Observable theme values match; the refresh point differs.
- `current_theme()` routes an uninitialized theme through the process-level uncaught handler with the TypeScript message (`Theme not initialized. Call initTheme() first.`); `try_current_theme()` exposes the error. `ThemeColor`/`ThemeBg` are enums, so `Unknown theme color` is unreachable (typed-API exclusion).
- Built-in themes are read without schema validation, exactly like `JSON.parse` in `getBuiltinThemes`.
- `getLanguageFromPath` keeps TypeScript's extension map semantics, including `Makefile` → `makefile` and `Dockerfile` → `dockerfile` (no-dot names are treated as extensions).
- `highlightCode`'s `try/catch` fallback is not reproduced: the Rust `syntax_highlight` port returns a `String` and cannot throw. The markdown-theme variant's styled fallback is likewise unreachable.

## Gaps / exclusions

- Theme file watcher behavior is implemented but not covered by a test (timing/filesystem events); `notify` is already a crate dependency.
- `getThinkingBorderColor`'s `default` arm is unreachable (typed enum); `max`/`ultra` map to `thinkingMax`.
