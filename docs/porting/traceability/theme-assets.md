# Theme JSON asset modules

Frozen [source lock](../source-lock.json): `4f9e6f8a4`. Source directory `packages/coding-agent/src/modes/interactive/theme/`; Rust directory `crates/pi-coding-agent/theme/`.

| Asset | Bytes | SHA-256 | Proof |
|---|---:|---|---|
| `dark.json` | 2065 | `0eb4e621f5ad8fda68c759b8a8c4df58ff99fedeb507faebea943dbfe3e2a7fc` | `frozen_theme_assets` whole-byte comparison |
| `light.json` | 2085 | `73ff69318d8a677bcecba7c21e1f5da194623cef0ab9dbd934ecee3bc4b6aa29` | same |
| `theme-schema.json` | 8761 | `315e1600253a4a697ac6e688489510c0bf4847f5c58ece45178ae2f22a90d404` | same; restores frozen required-colors and xhigh/max/ultra descriptions |

Development ledger `/tmp/pi-rust-repin-lossless-interactive/test-theme-assets-red.log` reproduces schema-byte mismatch. `test-theme-assets-green.log`: 1/1, all three complete assets equal frozen bytes. Previous-pin hashes/line counts are superseded, not new-pin evidence.

## Inclusion and distribution

`config.rs` embeds dark/light using `include_str!`; `embedded_builtin_theme` returns selected constants and `theme/mod.rs::get_builtin_themes` consumes them unless `PI_PACKAGE_DIR` supplies external files. This is the existing embedded-assets approval, not a new distribution decision. Theme loading behavior is mapped in [theme.ts](theme.md).

Schema remains a packaging gap, not a missing runtime validator. Frozen package.json:48–49 copies all theme JSON (including schema) into npm `dist/modes/interactive/theme/` and `dist/runtime/src/modes/interactive/theme/`, and binary `dist/theme/`. Rust tracks the identical schema but config.rs:37–38,184–190 embeds dark/light only; there is no deploy/package sidecar copier or schema accessor. Thus installed binary `<package_dir>/theme/theme-schema.json` is unavailable. The previous editor-metadata waiver in [compatibility](../../specs/compatibility.md#out-of-scope) does not establish common-pin distribution parity. Byte identity is proved independently of this exact remaining gap.
