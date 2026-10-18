# coding-agent clipboard-native traceability

Pinned source: `packages/coding-agent/src/utils/clipboard-native.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: the source loads an npm native module through `createRequire`, which has no Rust equivalent.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `clipboard-native.ts:5-10` | Clipboard module shape: get/set text, image presence, image bytes | `utils::clipboard_native::ClipboardModule` trait; focused loader tests |
| `clipboard-native.ts:20-26` | Try each resolution root, return the first success, `null` when none | `load_clipboard_native`; focused fallback/all-none/first-success tests |
| `clipboard-native.ts:28` | Display required on Linux (`DISPLAY` or `WAYLAND_DISPLAY`) | `display_available` / `has_display`; focused `requires_a_display_on_linux` |
| `clipboard-native.ts:30` | Singleton is disabled under Termux or without a display | `clipboard`; see gap |

## Gaps and discrepancies

- **Gap:** the concrete `@mariozechner/clipboard` native binding is unavailable in Rust. `clipboard()` returns `None`; callers can supply a `ClipboardModule` through `load_clipboard_native`. The module is marked `partial` in the coverage matrix.
- Image bytes are `Vec<u8>` instead of `Array<number>`.
- Errors are a typed `ClipboardError` instead of thrown exceptions.

## Proof inventory

- Focused cases: 4.
