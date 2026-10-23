# Theme language lookup and highlighting failures

Match frozen `modes/interactive/theme/theme.ts:1132–1221` at `4f9e6f8a4`, including inherited ordinary-object lookups. [Theme traceability](../porting/traceability/theme.md).

## What it must do

- [x] Extension lookup lowercases JS units and returns a real language, no value, or the inherited function/object value; use the shared prototype authority, not a second table.
- [x] An inherited non-string language rejects highlighting with the original TypeError `(name || "").toLowerCase is not a function`, before the grammar catch. Preserve constructor/message through Result carriers.
- [x] Preserve the existing named-language and grammar-catch behavior; never turn the inherited-value rejection into plain text or a panic.
- [x] Stateful write render callbacks and expanded/error read result callbacks propagate that failure.
- [x] Interactive tool rendering catches failed callbacks and emits frozen fallback content; HTML export returns no custom render so its structured fallback can run.

## How it works
- [Theme mapping](../porting/traceability/theme.md)
- [Shared prototype authority](../porting/traceability/object-prototype.md)

## Implementation inventory
- `crates/pi-coding-agent/src/modes/interactive/theme/mod.rs` — lookup value and fallible highlighter.
- `crates/pi-coding-agent/src/core/tools/{read,write}.rs` — callback propagation.
- `crates/pi-coding-agent/src/core/extensions/types.rs` — original boxed native HandlerError source.
- `crates/pi-coding-agent/src/bundled_extensions/pyrun/index.rs` — mechanical Result adapter.

## Tests asserting this spec
- `crates/pi-coding-agent/tests/differential_write_render_sequence.rs`
- `crates/pi-coding-agent/tests/differential_language_render_boundary.rs`
- `crates/pi-coding-agent/tests/{differential_theme,differential_theme_api,theme}.rs`

## Known gaps (current cycle)
No remaining gap in this bounded inherited-value correction.

## Out of scope
- Arbitrary non-string language inputs remain excluded by typed APIs; the inherited values produced internally from valid filename strings are in scope.
- Markdown fences and PiMarkdownTheme accept real language strings and retain their separate frozen grammar catch; no trait redesign or new fallback.
- Existing approved Mathematica/R grammar gaps remain unchanged.
