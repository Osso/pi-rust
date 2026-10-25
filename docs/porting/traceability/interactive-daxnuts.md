# components/daxnuts.ts

Source: `coding-agent/src/modes/interactive/components/daxnuts.ts`
Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/daxnuts.rs`

Tests: `interactive_components_c.rs::daxnuts_reveals_image_and_text_over_time`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| 32x32 RGB hex decode | `parse_image` | rendered image |
| half-block image build (`▄`, fg/bg RGB) | `build_image` | rendered image |
| `DaxnutsComponent` | `DaxnutsComponent` | behavioral |
| 80ms ticks, 25 max | `poll`/`next_deadline` | tick assertion |
| scanline reveal progress | `revealed_rows` | initial/final render |
| scanline highlight row | render branch | render path |
| text phases (accent/success/muted/dim/mdLink) | render branches | final render assertions |
| centering by visible width | `center` + `strip_terminal_sequences` | final render |
| render cache keyed by width + tick | `cached_*` | render path |
| `dispose` stops the timer | `dispose` | behavioral |

## Gaps

- No differential: tick progression is driven by `setInterval` wall-clock time in the
  source; the port exposes `poll(now)` so tests step it deterministically.

## Discrepancies

None found.

## Q18 native UTF-16

Animation grids/images and render caches now use native `JsString`. Native composition retains units until terminal output. Cache/invalidation regression in `raw_error_consumer_components::animation_leaf_caches_native_render_output` written **unexecuted**, coding-agent compilation blocked.
