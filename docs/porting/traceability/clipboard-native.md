# coding-agent clipboard-native traceability

Pinned source: `packages/coding-agent/src/utils/clipboard-native.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: the source loads an npm napi module through `createRequire`. The port depends on the same Rust crate that the napi module vendors, so the platform code is the source's own.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `clipboard-native.ts:5-10` | Clipboard module shape: get/set text, image presence, image bytes | `utils::clipboard_native::ClipboardModule` trait; focused loader tests |
| `clipboard-native.ts:20-26` | Try each resolution root, return the first success, `null` when none | `load_clipboard_native`; focused fallback/all-none/first-success tests |
| `clipboard-native.ts:27` | `hasDisplay`: non-Linux or `DISPLAY`/`WAYLAND_DISPLAY` | `display_available` / `has_display`; focused `requires_a_display_on_linux` |
| `clipboard-native.ts:29-30` | Singleton disabled under Termux or without a display | `clipboard()`; returns `Some(SystemClipboard)` otherwise |
| `@mariozechner/clipboard` `getText`/`setText` | Text round-trip through `clipboard-rs` | `SystemClipboard::{get_text, set_text}` (fresh `ClipboardContext` per call, like the napi wrapper) |
| `@mariozechner/clipboard` `hasImage` | Image presence, throwing on context failure | `SystemClipboard::has_image` → `Result<bool, ClipboardError>` |
| `@mariozechner/clipboard` `getImageBinary` | `ctx.get_image()?.to_png()?.get_bytes()` | `SystemClipboard::get_image_binary` uses the same `clipboard-rs` calls |

## Gaps and discrepancies

- The concrete binding is now provided: `clipboard-rs =0.3.4` is the release that `@mariozechner/clipboard` 0.3.9 vendors (its Linux X11 and macOS backends are byte-identical modulo formatting; only the watch loop's interval plumbing differs, and the port does not use the watcher). **Not exercised:** opening a real X11/Wayland pasteboard requires a display, so `SystemClipboard` calls are untested here; the loader, display guard, and error propagation are.
- `has_image` returns `Result` instead of throwing; callers propagate the error like the source's rejected promise.
- Image bytes are `Vec<u8>` instead of `Array<number>`.
- Errors are a typed `ClipboardError` instead of thrown exceptions.
- `loadClipboardNative` keeps its loader-list signature; the process-wide `clipboard()` no longer needs resolution roots because the binding is statically linked.

## Proof inventory

- Focused cases: 4.
- Crate identity: `clipboard-rs 0.3.4` (see `docs/dependencies.md`).
