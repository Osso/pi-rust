# `components/settings-selector.ts`

Authority: common-pin851-line module. [Contract](../../specs/interactive-settings-selector.md). Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/settings_selector.rs`.

## Exported symbols and branches

| Surface | Frozen behavior | Proof |
| --- | --- | --- |
| SettingsConfig / SettingsCallbacks | native UTF-16 theme names; f64 numeric fields/callbacks; raw warning object | `differential_settings_selector` numeric/config/raw cases |
| SettingsSelectorComponent / getSettingsList | inherited Container mounts the same live SettingsList; image capability gates two rows; exact search/navigation/order/toggles | constructor/navigation, each setting toggle, inherited children/live-getter cases |
| SelectSubmenu | retained title/description/hint styles, shared selection/cancel callbacks, inherited invalidation | thinking/theme sequences and theme-preview color change |
| WarningSettingsSubmenu | object spreads retain unknown keys/UTF-16; toggle updates currentWarnings before callback; reopening retains last state | warning-unknown-raw-state sequence |
| ThemeSubmenu | preferred theme, raw theme names, single/automatic transitions, per-mode retained text, preview/rollback/apply, light/dark terminal choice | thinking-submenu, theme-preview-colors-rollback, theme-automatic-apply, automatic-to-single (both terminal appearances), raw-theme-and-no-preview |
| callback failure | changed setting survives callback exception | callback-error source/native sequence |
| timeout labels | same finite/nonfinite/fractional display helper as interactive callback | numeric-config HTTP cases; existing helper differential retained |

`oracle/settings-selector-oracle.mts` imports the frozen module and the pinned TUI/keybinding singleton. Test requests preserve raw UTF-16 via unit arrays/JSON. RED `test-settings-selector-red-2.log` and `...red-3.log` exposed missing rows, numeric narrowing, dropped warnings, theme lifetime and inherited-container differences. GREEN `test-settings-selector-green-2.log` passes the complete request corpus. Final independent/readability gates remain parent-owned.

## Old-only deletion

The common pin has no Mermaid/cache-miss settings. The authorized historical Mermaid row is restored from `532f7d12`, including its choices and callback; the focused selector test covers search, default display and selection. Cache-miss settings remain removed. [Cache removal](cache-stats.md).

## Exclusions

Closed enum inputs retain the approved typed-API exclusion for invalid JavaScript runtime types; no replacement text is permitted. Core HTTP parsing/runtime is not proved by selector display tests. Coupled [tool image work](interactive-tool-execution.md) is GREEN; tool's separate inherited-Container proof remains local to that module. Settings selector has no remaining local gap; workspace-level final gates do not keep its row partial. No whole interactive-mode or Q13 completion is claimed.
