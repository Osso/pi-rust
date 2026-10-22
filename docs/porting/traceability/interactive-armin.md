# components/armin.ts

Source: `coding-agent/src/modes/interactive/components/armin.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/armin.rs`

Tests: `interactive_components_c.rs::armin_animates_and_renders_message`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| XBM bit decode (31x36, LSB first) | `get_pixel`/`get_char`/`build_final_grid` | final-grid render assertion |
| `ArminComponent` | `ArminComponent` | behavioral |
| random effect selection | `random_index(EFFECTS)` | effect-name assertion |
| typewriter / scanline / fade / crt / glitch / dissolve | `EffectState` arms | convergence test |
| rain | `EffectState::Rain` | discrepancy test |
| 30 fps / 60 fps for glitch | `interval` | poll loop |
| render: 1-cell padding, clipped rows, accent color, `ARMIN SAYS HI` | `Component::render` | behavioral |
| cache keyed by width + grid version | `cached_width`/`cached_version` | render path |
| `dispose` stops the timer | `dispose` | behavioral |

## Gaps

- `setInterval` is exposed as `poll(now)`/`next_deadline`; no differential because the
  effect is random and time-driven.

## Discrepancies

- `rain` never settles columns 0 and 30 because the source only settles a column when it
  finds a non-space target row; those columns are all background, so `allSettled` stays
  false and the animation never stops. The port preserves this behaviour and the test
  asserts it instead of fixing it.

## Q18 native UTF-16

Animation grids/images and render caches now use native `JsString`. Native composition retains units until terminal output. Cache/invalidation regression in `raw_error_consumer_components::animation_leaf_caches_native_render_output` written **unexecuted**, coding-agent compilation blocked.
