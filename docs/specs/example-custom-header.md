# Custom header example

Opt-in Rust-native equivalent of pinned `packages/coding-agent/examples/extensions/custom-header.ts`. [Traceability and embedding](../porting/traceability/example-custom-header.md).

## What it must do

- [x] Register a `session_start` callback; install a header factory only in TUI mode, never on registration alone. Text/print, RPC, and JSON starts leave the UI untouched.
- [x] Render eight mascot lines followed by the literal subtitle `   shitty coding agent` and ` v` plus the actual package version. Preserve every space, glyph, blank line, and foreground reset.
- [x] Keep eye blocks uncolored, pupils and version dim, roof and legs accent, subtitle muted. Preserve factory-supplied theme colors across narrow and wide render widths.
- [x] Ignore render width; invalidation has no effect on this stateless component's output.
- [x] Register `/builtin-header` with description `Restore built-in header with keybinding hints`; clear the header before notifying `Built-in header restored` at info level, regardless of mode or command arguments.

## How it works

- [Native example composition and source mapping](../porting/traceability/example-custom-header.md)

## Implementation inventory

- `crates/pi-coding-agent/examples/extensions/custom_header.rs`: embeddable registration function and private header component.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/example_custom_header.rs`: real API/registry/factory/context composition; exact differential output and UI-call sequences for four modes, two themes, five widths, and post-invalidation rendering.
- `oracle/example-custom-header-oracle.ts`: imports the actual pinned module with controlled public VERSION export and UI/theme fixtures; shared oracle process watchdog.

## Known gaps (current cycle)

- [ ] Parent final verification remains pending; coverage stays partial despite targeted GREEN.

## Out of scope

- Automatic loading, bundled registry entries, standalone executables, default CLI startup, and UI redesign: this is an explicitly embedded example.
- Invalid JavaScript runtime inputs: existing approved typed Rust boundary; native command arguments are `String`.
