# coding-agent clipboard-native traceability

Frozen source: `packages/coding-agent/src/utils/clipboard-native.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`; all 32 lines audited.

| Export / branch | Rust mapping and proof |
| --- | --- |
| `ClipboardModule`: `setText`, `hasImage`, `getImageBinary` | Native trait and `SystemClipboard`; same `clipboard-rs =0.3.4` backend as the frozen napi wrapper; `get_text` is a Rust extra |
| `loadClipboardNative`: ordered roots, first success, catch failed load, null if all fail | `load_clipboard_native`; four `suite_modes::clipboard_native` loader/display tests; failed native loaders return `None` |
| Linux `DISPLAY || WAYLAND_DISPLAY` truthiness, non-Linux enabled | `display_available` / `has_display`; native display table and private-process frozen differential |
| Empty/absent Termux enabled; nonempty Termux disabled; module export remains stable after env mutation | `clipboard` cached guard; `clipboard_native_process::clipboard_module_guard_matches_frozen_initialization` compares actual frozen module export in five fresh processes (no live display calls) |

RED `/tmp/settings-models-clipboard-red.log`: empty TERMUX_VERSION with DISPLAY=:1 returned false natively and true in frozen. Tests also mutate Termux/display after first lookup to prove export lifetime.

## Open items / approved-boundary review

- Rust caches the guard at first native access, not JavaScript import evaluation. Main startup must initialize it before extensions can change environment; that cross-lane integration is pending while main.rs is held.
- Static native binding replaces npm resolution in the production path; loader seam preserves ordered success/failure but native resolution/packaging still requires parent classification against this pin. No live X11/Wayland pasteboard or napi failure-message proof is claimed.
- Rust returns owned handles and `Vec<u8>` and uses `Result` instead of thrown/rejected errors. Object identity is excluded under the common owned-return approval.
- OS/runtime fault shapes are outside the current parity bar.
