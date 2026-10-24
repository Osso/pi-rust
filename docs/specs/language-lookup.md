# Theme language lookup

Frozen authority: `modes/interactive/theme/theme.ts:1132–1221` at `4f9e6f8a4`. [Theme traceability](../porting/traceability/theme.md).

- [x] Lowercase JS extension units; return a named language for explicit extension-map keys, otherwise no language.
- [x] Preserve named-language and grammar-catch behavior and Result/error carriers at render callbacks.
- [x] Unknown extensions render unhighlighted.

## Approved gap

2026-10-23 runtime-oddity exclusion: inherited Object.prototype extension lookups/native function projections are not reproduced. `HighlightLanguage` aliases `JsString`; no artificial non-string language value or TypeError remains. [Policy](object-prototype-lookups.md).

## Tests

- `tests/own_key_tables.rs`: unknown/prototype-like extensions stay absent.
- `tests/differential_write_render_sequence.rs`: ordinary streaming/history behavior.
- `tests/{differential_theme,differential_theme_api,theme}.rs`: named lookup/highlighting.
- `tests/differential_write_result_boundary.rs`: real interactive result catch/reset.

Arbitrary non-string API input types remain excluded. Approved Mathematica/R grammar gaps remain unchanged.
