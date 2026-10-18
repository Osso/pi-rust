# keybinding-hints.ts

Source: `coding-agent/src/modes/interactive/components/keybinding-hints.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/keybinding_hints.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `KeyTextFormatOptions` | `KeyTextFormatOptions` | differential (`keyHints`) |
| darwin `alt` → `option` | `cfg!(target_os = "macos")` branch | Linux oracle run exercises the non-darwin branch |
| `capitalize` first code unit | `format_key_part` | differential `alt+left` capitalized |
| `/` and `+` splitting | `format_key_text` | differential key list |
| `keyText` | `key_text` (`get_keybindings().get_keys`) | differential bindings |
| `keyDisplayText` | `key_display_text` | differential bindings |
| `keyHint` | `key_hint` | differential hints |
| `rawKeyHint` | `raw_key_hint` | differential raw hints |

Gap: the darwin branch cannot be exercised on Linux; the code is a direct conditional on `process.platform === "darwin"`.
