# Theme JSON asset modules

Source paths are relative to `packages/coding-agent/src/modes/interactive/theme/` at the [original pin](../source-lock.json). Rust paths are relative to `crates/pi-coding-agent/theme/`. Full byte arrays compared equal on 2026-10-20 at the [proof-ledger revision](../proven-assets-proof.md).

| Source = Rust relative path | Bytes | SHA-256 | Status |
|---|---:|---|---|
| `dark.json` | 2189 | `113bc1fd556727b165804c18b5d377c14d421b3fdff3909a63736f421d0a3b00` | done |
| `light.json` | 2209 | `893f0d7e8513dbd5e5112982fd6eb65ceb03ae18ed04854ac57803b9c8908fa0` | done |
| `theme-schema.json` | 9528 | `afe8f61c9ef248a71d0abe0abe27df545d5cb49aef773bf7e1ec8a4f8936e477` | gap (waived) |

## Dark/light inclusion and behavior

`crates/pi-coding-agent/src/config.rs:35–36` compiles the two files with `include_str!`; public `embedded_builtin_theme` at lines 173–179 returns the selected constant. `modes/interactive/theme/mod.rs::get_builtin_themes` at lines 785–805 consumes those bytes by default; with `PI_PACKAGE_DIR`, it reads the corresponding external `theme/<name>.json`. Parsed values populate the built-in cache used by theme selection/rendering. This follows the existing embedded-assets approval, not a new distribution decision.

Complete byte identity proves the raw JSON data contract. Existing `loads_color_modes_and_global_theme_state` and `theme_loading_matches_typescript` exercise the theme-loading behavior through fixture packages, not an independent binary-embedding probe. Exact pass evidence and limits: [proof ledger](../proven-assets-proof.md). API/branch mapping: [theme.ts](theme.md).

## Schema waived

User-approved gap October 20, 2026 (“Waive as editor metadata”). The schema is editor-facing JSON Schema metadata; nothing at runtime reads it, and dark/light `$schema` values point to a remote upstream URL. The tracked Rust copy is byte-identical, but no packaging step ships it, and none is required. `validates_theme_json_and_names` tests Rust validation logic, not this schema; Rust does not run a JSON-schema engine.
