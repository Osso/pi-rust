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

## Q18 native message/input/render (2026-10-22)

Message input, color callbacks, component input and rendered lines are JsString.
Frozen bordered-loader.ts constructor ordering, empty indicator frames, optional
cancel hint, signal and dispose behavior are unchanged. raw_leaf_components
checks an unpaired message in rendered lines and Escape cancellation for both
cancellable settings. Written but unexecuted while coding compilation is blocked.
