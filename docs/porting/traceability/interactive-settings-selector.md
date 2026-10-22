# settings-selector.ts

Source: `packages/coding-agent/src/modes/interactive/components/settings-selector.ts`.
Rust: `crates/pi-coding-agent/src/modes/interactive/components/settings_selector.rs`.

| Surface | Native contract | Proof level |
|---|---|---|
| SettingsConfig themes and ThemeSubmenu state | `JsString` names/settings; no scalar display authority | Implemented; current compilation pending parent coding gate |
| ThemePreviewFn/SelectionChangeFn/ThemeDoneFn/SettingsChangeHandler | Native values and callback references | Same; direct interactive theme callbacks adapted |
| SelectSubmenu, WarningSettingsSubmenu, ThemeSubmenu, SettingsSelectorComponent | Native titles/descriptions/options/render/input and TUI SettingsList/SelectList contracts | Raw title/option/selection test added to `raw_extension_dialogs`; execution blocked by coding library |
| Fixed setting/type tags | Strict scalar validation only for closed-tag matching; unmatched units are unknown tags, not replacement text | Parent diagnostics baseline; behavioral execution pending |
| Numeric setting values | Shared `parse_int` on native units; integer formatting through `js_format!` | Shared primitive has separate in-crate raw differential proof; callback integration pending |

RED baseline: provided full-library diagnostics show incompatible scalar TUI interfaces and native item/callback mismatches. No independent probe is added, per parent instruction. Existing legacy scalar settings tests are not current native proof. No new setting behavior, fallback, source fix, or whole-selector parity claim.
