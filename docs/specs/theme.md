# Theme

Frozen `modes/interactive/theme/theme.ts` defines theme loading, styling, global switching and custom-file reload. [Authority and implementation map](../porting/traceability/theme.md).

## What it must do

- [x] Watch the custom directory, accepting only the watched filename (or no filename), and debounce reload by100ms.
- [x] Reload valid edits on the UI event loop and notify immediately, without requiring a subsequent theme getter.
- [x] Keep the last valid theme after invalid/missing files; observe atomic replacements and later recreation.
- [x] Cache a reload under the watched setting name even when the file's internal name differs.
- [x] Stopping or switching a watcher cancels its pending timer; sibling edits never reload it.
- [x] Direct constructor numeric indices preserve f64 and JS formatting; signed/partial parseInt hex channels retain source values.
- [x] Registration clears first and retains successful earlier inserts on a later name error; duplicates preserve insertion slots.
- [x] setTheme catches callback errors, activates dark, and returns the original UTF-16 message.
- [x] Highlighter formatter throws return raw lines in highlightCode and styled lines in the Markdown adapter.
- [x] OSC and direct RGB numbers retain fractions/nonfinite values; detector luminance and diagnostic text use JavaScript semantics.

## How it works

- [Theme traceability](../porting/traceability/theme.md)
- [Interactive controller](interactive-theme-controller.md)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/interactive/theme/mod.rs` — theme state, styling, loading and reload lifecycle.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_theme_lifecycle.rs` — real-file events against frozen source, ten complete snapshots.
- `crates/pi-coding-agent/tests/differential_theme_api.rs` — constructor, registration and catch boundaries against frozen source.
- `crates/pi-js-compat/tests/math_round.rs` — exact JS gray-rounding authority.

## Known gaps (current cycle)

- [ ] Remaining whole-module audit: existing source proofs and uncovered lifecycle branches.

## Out of scope

Whole interactive-mode integration and final workspace acceptance remain parent-owned.
