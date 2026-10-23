# `components/settings-selector.ts`

Authority: common-pin complete851-line module. Rust: `crates/pi-coding-agent/src/modes/interactive/components/settings_selector.rs`.

## Current status: whole-module re-pin remains OPEN

Native text/theme/submenu carriers are JsString, but inherited Container/state-lifetime behavior, live submenu callbacks, numeric config and missing hide-tool/hide-coordination options still need full frozen differential acceptance. Existing native tests are not a whole-module source proof.

## Parent-authorized old-only deletion

Frozen has no Mermaid rendering or cache-miss notice settings. Removed their config fields, callbacks, rows, parsing helpers, settings-manager known APIs/types and interactive settings handlers; no inert alternative implementation is retained. Legacy JSON properties remain arbitrary unknown settings data, not migrated or scrubbed. [Whole cache removal](cache-stats.md).

`removed_mermaid_and_cache_notice_options_are_not_searchable` first rendered `Mermaid diagrams` after the legacy query (`test-old-settings-removal-red-1.log`), then matched no settings for both old option names after deletion. `test-old-settings-removal-green-1.log`: native settings tests6/6, retaining row-order, theme preview/single selection, warnings and thinking behavior. The adjacent output-pad-only test remains and passes in `test-old-settings-retained-sibling-green-1.log`; old Mermaid getter/setter assertions were deleted, not reinterpreted as current behavior.

## Native representation surfaces retained for the next slice

- SettingsConfig theme names and ThemeSubmenu strings are JsString; callbacks pass native text rather than scalar display authority.
- SelectSubmenu/WarningSettingsSubmenu/ThemeSubmenu/main selector use native TUI SettingsList/SelectList rendering and input. Their source-level whole-module lifecycle/alias proof is pending.
- Numeric parsing uses shared native parse_int. Config fields currently narrow f64 callers through integer UI types; replace those carriers with f64 where the real frozen settings getters can supply nonintegers.
- Closed enum tags remain exact literal validation; invalid JS runtime input types retain the explicit typed-API exclusion, not replacement-text permission.

No complete selector, interactive-mode or Q13 acceptance is claimed by this removal checkpoint.
