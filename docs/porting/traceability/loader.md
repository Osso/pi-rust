# components/loader.ts traceability

## Q2 frozen revalidation

Frozen components/loader.ts audited whole. Existing exported methods, constructor defaults, rendering/input branches and invalidation match the frozen implementation within typed numeric/String boundaries; native target plus differential_components re-run against 4f9e6f8a4. Loader frame intervals remain explicitly driver-owned poll/deadline adaptation; CancellableLoader maps AbortSignal to CancellationToken and preserves repeated cancel callbacks. See [Q2 proof](../q2-tui-components-proof.md).


Source: `packages/tui/src/components/loader.ts` (94 lines). Rust:
`crates/pi-tui/src/components/loader.rs`.

Tests: `crates/pi-tui/tests/loader.rs` (both listed TS tests ported plus
indicator/styling cases),
`crates/pi-tui/tests/differential_components.rs::loader_render_matches_typescript`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface LoaderIndicatorOptions` | `LoaderIndicatorOptions { frames, interval_ms }` | indicator tests, differential |
| `class Loader extends Text` | `struct Loader` (composes `Text`) | all |
| `constructor(ui, spinnerColorFn, messageColorFn, message = "Loading...", indicator?)` | `Loader::new(request_render, spinner_color_fn, message_color_fn, message, indicator)` | all |
| `render(width)` | `Component::render` (`["", ...text.render(width)]`) | all |
| `start` | `start` | timer test |
| `stop` | `stop` | timer test |
| `setMessage` | `set_message` | TUI fallback test, differential |
| `setIndicator` | `set_indicator` | indicator tests, differential |
| `DEFAULT_FRAMES`, `DEFAULT_INTERVAL_MS` (internal) | `DEFAULT_FRAMES`, `DEFAULT_INTERVAL_MS` (public) | timer test |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| construction renders the initial frame | `set_indicator` → `start` → `update_display` | timer test, differential `renderCount` |
| `renderIndicatorVerbatim = indicator !== undefined` | `render_indicator_verbatim` | styled/single-frame/empty-frame tests, differential |
| empty frames array hides the indicator | `frame.is_empty()` | `empty_frames_hide_the_indicator...` |
| `intervalMs > 0` else default | `filter(|interval| *interval > 0)` | differential (`intervalMs` 10) |
| `frames.length <= 1` disables the interval | `frames.len() <= 1` | single-frame test |
| interval tick advances `(current + 1) % frames.length` | `Loader::poll` | timer/styled tests |
| `updateDisplay` calls `requestComponentRender` | `request_render` callback | TUI fallback test, differential |

## Behavioral notes

- The `setInterval` timer is driver-owned, matching `stdin_buffer.rs`: the
  caller schedules `Loader::poll(now)` at `Loader::next_deadline()`. This keeps
  tests deterministic and creates no background thread. The TUI fallback test
  drives `set_message` and `flush_pending` directly.
- `Text` composition replaces TypeScript inheritance; all inherited padding and
  wrapping behavior is reused through `Text::render`.

## Gaps / exclusions

- Timer cadence is not differentially compared (Node `setInterval` is not
  controllable from the oracle); Rust timing is covered by the deterministic
  deadline test instead.
- JavaScript callers passing non-finite intervals are excluded by the typed API.

## Discrepancies

None found.
