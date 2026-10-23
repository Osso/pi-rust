# theme.ts traceability

## Common-pin native theme contract

`Theme.name/source_path`, ThemeInfo, registered/current/watched names and paths, load/get/set APIs, auto-setting names, export text and extension ThemeSetResult use `JsString`. JSON uses `parse_lossless` and `JsValue`/`JsObject`; variable keys/references and validation labels/errors retain raw units. No scalar/raw copies or early encode adapter. Actual read/existence/watch paths encode at filesystem operations only; numeric hex channels pass their native unit slices directly to the sole shared parseInt API.

`raw_theme_native.rs` tests native file metadata/registry/get-set, raw variable resolution/errors, and frozen source differential 24 file scenarios: high/low/pair × valid, slash-name error, raw variable validation error, missing reference, partial/invalid surrogate hex channels, circular reference and missing file. Paths themselves contain raw units and map to Node UTF8 only at file IO. Actual theme/controller/chalk/watch modules compile in the isolated harness; settings/source-info/highlighter/config fixtures bound proof to explicit theme files. No watcher/controller integration or full theme acceptance is claimed. `/tmp/q18-theme-native-source-green3.log` records GREEN. Parent owns SettingsManager native migration and whole-coding gate.

Source: `packages/coding-agent/src/modes/interactive/theme/theme.ts` (1290 lines). Rust: `crates/pi-coding-agent/src/modes/interactive/theme/mod.rs`. JSON asset identity, actual inclusion paths and unresolved schema distribution are mapped separately in [theme assets](theme-assets.md).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class Theme` | `struct Theme` | `theme.rs` probes, `differential_theme.rs` |
| `Theme.fg/bg/bold/italic/underline/inverse/strikethrough` | same methods | `theme_global_probe`, differential fg/bg |
| `Theme.getFgAnsi/getBgAnsi/getColorMode` | same methods | `theme_global_probe`, differential |
| `Theme.getThinkingBorderColor(level)` | `get_thinking_border_color(ThinkingLevel) -> StyleFn` | exact styled border outputs for xhigh/max/ultra in `differential_theme` |
| `Theme.getBashModeBorderColor()` | `get_bash_mode_border_color() -> StyleFn` | traceability-only |
| `getAvailableThemes()` | `get_available_themes() -> Result<Vec<String>, ThemeError>` | `theme_picker_probe` |
| `getAvailableThemesWithPaths()` | `get_available_themes_with_paths() -> Result<Vec<ThemeInfo>, ThemeError>` | `theme_picker_probe` |
| `loadThemeFromPath(path, mode?)` | `load_theme_from_path(&Path, Option<ColorMode>)` | `theme_validation_probe`, differential (22 cases) |
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
| `detectTerminalThemeForAuto` scheme early return before sequential background query (frozen:771–782) | `detect_terminal_theme_for_auto` | `differential_auto_theme` six delayed scenario traces; `theme_detection_probe` |
| `initTheme` fallback to dark, no watcher; fallback failure escapes | `init_theme` | `theme_global_probe`, `theme_missing_builtin_probe` |
| `setTheme` fallback + callback + error text; fallback failure escapes | `set_theme` | `theme_global_probe`, `theme_missing_builtin_probe` |
| `setThemeInstance` sets `<in-memory>` | `set_theme_instance` | `theme_global_probe` |
| theme file watcher (:901–984; directory watch,100ms debounce,reload,callback) | LocalSet event receiver + cancelable retained timer | `differential_theme_lifecycle`:10 exact real-file source/native snapshots; quiet-idle callback, basename filtering/sibling ignore, debounce, invalid/missing file retention, atomic replacement, watched-name registry key, stop/switch cancellation |

## Behavioral notes / deviations

- Module state remains `thread_local!` for non-`Send` UI callbacks. notify forwards raw file/error events to a LocalSet receiver; reload timers execute on that same UI loop, updating theme/registry and firing the callback without requiring theme access. No pending-on-getter path or per-event thread remains. Canceled receiver sends are normal watcher shutdown. Real-file RED/ GREEN: `/tmp/pi-rust-repin-lossless-interactive/theme-lifecycle-{red,green}.log`.
- `current_theme()` routes an uninitialized theme through the process-level uncaught handler with the TypeScript message (`Theme not initialized. Call initTheme() first.`); `try_current_theme()` exposes the error. `ThemeColor`/`ThemeBg` are enums, so `Unknown theme color` is unreachable (typed-API exclusion).
- Built-in themes are read without schema validation, exactly like `JSON.parse` in `getBuiltinThemes`.
- `getLanguageFromPath` keeps TypeScript's extension map semantics, including `Makefile` → `makefile` and `Dockerfile` → `dockerfile` (no-dot names are treated as extensions).
- Both highlighter catch boundaries include theme-formatter throws: direct highlight returns raw code; Markdown adapter returns mdCodeBlock-styled code. `syntax_formatter` resolves the live global theme lazily, matching the source proxy callbacks; sparse valid constructor palettes exercise actual error branches.

## Common-pin token reconciliation (2026-10-22)

Removed old-only `scrollbarThumb`, `searchMatchBg`, `searchMatchText` enums, schema keys, built-in asset entries and default-color mappings. Frozen `theme.ts` foreground/background token unions and assets contain none. Read-only trace across every `crates/**/src/**/*.rs` found no production consumer outside theme token conversion/default tables; no component rendering path required deletion or replacement. Removed the two old-only fullscreen-color tests and only those token requests from the differential. Other foreground/background assertions remain exact. Pre-removal differential RED was frozen `Unknown theme background color: scrollbarThumb` versus successful Rust rendering (`test-differential_theme-6183.log`).

### thinkingMax caller mapping

Common-pin `theme.ts:399–419` maps xhigh/max/ultra to `thinkingXhigh`. Rust's sole production `ThinkingMax` consumer was `Theme::get_thinking_border_color`; it now preserves that frozen mapping rather than dropping max/ultra rendering. Removed old-only token/schema/assets and the entire `with_theme_color_fallbacks` injection from creation/export. Built-in resolved colors now contain 51 entries, matching frozen.

Extended existing differential snapshots with complete styled borders for xhigh/max/ultra. RED `test-thinking-max-border-red.log` reproduced different max/ultra colors on dark built-in before implementation; existing missing-token case independently reproduced the injected token. No module-level exception or fallback added. The fixture still explicitly omits `thinkingMax` in its historical fallback-named case; it no longer requests an API absent from frozen.

## Gaps / exclusions

- Watcher lifecycle now has real-file source/native differential proof above; no watcher gap remains.
- `getThinkingBorderColor`'s `default` arm is unreachable (typed enum); `max`/`ultra` map to frozen `thinkingXhigh`.
- External detector RGB range is under audit: frozen OSC parser can yield NaN for an overflowing hex channel. This is not an approved exclusion.

## Constructor and callback boundary reconciliation

`differential_theme_api` compares both color modes using actual source class instances:15 direct color vectors (fractional/negative/large/NaN/Infinity/-0, signed/partial hex, empty and errors), raw UTF-16 fg/bg text, ANSI getters, partial registration before a slash-name throw, last duplicate value and locale-equal insertion order, callback TypeError with lone surrogate and caught dark fallback, sparse-palette raw-vs-Markdown highlighter error returns. ColorValue is f64; signed hex channels never pass through u8. Source schema still validates loaded indices as integer0..255.

`setRegisteredThemes` clears first, inserts incrementally, skips absent/empty names and keeps insertion slots on replacement (IndexMap). No transactional rollback is invented. `setTheme` catches callback throws and uses the error message, not its string prefix. Source accepts signed/partial parseInt channel forms (e.g. #-1ff00); preserved as a DISCREPANCY, not repaired.

Theme RGB-to-256 gray rounding now uses shared `pi_js_compat::number::math_round`; `pi-js-compat/tests/math_round.rs` compares14 Node binary64 tie/negative-zero/nonfinite fixtures. Logs: `theme-api-red.log` missing f64 domain; `theme-api-behavior-red.log` actual source failures; `theme-api-green.log` complete GREEN; `math-round-{red,green}.log` shared helper proof. Artifact root remains `/tmp/pi-rust-repin-lossless-interactive`.

## Native ThemeError values

`ThemeError` remains publicly re-exported from `theme`; its existing value/formatting block lives in `theme/error.rs` for independent actual-module proof. Every error text field is `JsString`; `message()->JsString` retains raw labels/messages/names. `Display` is a Node UTF8 diagnostic projection, not text authority. `SetThemeResult.error` is `Option<JsString>` and source error propagation uses `message()`.

`raw_theme_error` proves high/low/pair message assembly and explicit Display projection. Theme JSON/schema parsing and color/name input domains outside the error value remain unchanged; this proof must not be cited as complete raw Theme JSON acceptance.

## Q18 native UTF-16 integration (2026-10-22)

Theme file read errors receive the original native path before NodeFS error construction. This lane adds no scalar display or lookup path. No primary diagnostics at interactive6; runtime theme tests remain unexecuted.
