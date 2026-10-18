# visual-truncate.ts

Source: `coding-agent/src/modes/interactive/components/visual-truncate.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/visual_truncate.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `VisualTruncateResult` | `VisualTruncateResult` | differential (`visualTruncate`) |
| empty text → `{[], 0}` | `truncate_to_visual_lines` early return | differential empty case |
| `lines <= max` → all lines | early return | differential `one line`, max 5 |
| tail slice + skipped count | `all_visual_lines[len - max..]` | differential 3-line cases |
| `Text(text, paddingX, 0)` wrapping | `pi_tui::Text` | differential long-line cases, padding 0/1 |

Discrepancy (preserved source quirk): JS `slice(-0)` returns the whole array, so `maxVisualLines = 0` keeps every line and reports all of them as skipped. The Rust port reproduces this explicitly instead of treating 0 as an empty result.
