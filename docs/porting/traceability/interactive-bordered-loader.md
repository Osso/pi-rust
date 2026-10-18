# components/bordered-loader.ts

Source: `coding-agent/src/modes/interactive/components/bordered-loader.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/bordered_loader.rs`

Tests: `interactive_components_c.rs::bordered_loader_*`, differential `borderedLoader`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `BorderedLoader` | `BorderedLoader` | behavioral + differential |
| `constructor(tui, theme, message, { cancellable = true })` | `new(Rc<dyn Fn()>, message, BorderedLoaderOptions)` | differential both modes |
| `signal` (CancellableLoader token / fresh token) | `signal()` | `bordered_loader_cancels_on_cancel_key` |
| `onAbort` setter | `set_on_abort` | cancel test |
| `handleInput` only forwards when cancellable | `Component::handle_input` | cancel test |
| `dispose` stops loader | `dispose` | render test |
| empty frames (no animation) | `LoaderIndicatorOptions { frames: Some(vec![]) }` | render count test |
| border/cancel hint layout | children list | differential |

## Gaps

- The TypeScript constructor takes a `TUI`; the Rust port takes the request-render callback,
  matching the ported `Loader`/`CancellableLoader` boundary.

## Discrepancies

None found.
