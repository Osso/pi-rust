# Interactive hotkeys

Frozen `interactive-mode.ts:7540–7661` at the [common pin](../porting/source-lock.json) defines `/hotkeys`.

## Contract

- Render the complete Navigation, Editing and Other Markdown tables using effective keybinding displays, followed by registered extension shortcuts in resolved order.
- Extension descriptions preserve arbitrary UTF-16. Missing descriptions use the extension path; empty descriptions remain empty.
- Append a spacer, border, accented bold heading, spacer, Markdown with horizontal/vertical padding 1, and closing border; request rendering.
- Preserve the frozen platform-specific newline hint; Windows execution remains outside the POSIX proof.

## Implementation and tests

- `interactive_mode.rs`: hotkey tables and command rendering.
- `ExtensionShortcut.description`, `ExtensionApi::register_shortcut` and `loader::extension_shortcut`: `Option<JsString>`.
- `rank5_hotkeys_command_renders_the_complete_frozen_page`: actual frozen/native rendered UTF-16 frames.
- `rank5_hotkeys_preserve_lone_surrogate_empty_and_absent_descriptions`: registered shortcuts through the actual native mode versus frozen rendering.
- `oracle/interactive-hotkeys-oracle.mts`: invokes the actual frozen handler and real Markdown/container renderers.

[Branch/proof map](../porting/traceability/interactive-mode.md#hotkeys-page).
