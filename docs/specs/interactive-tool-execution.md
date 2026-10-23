# Interactive tool execution

Match the frozen `coding-agent/src/modes/interactive/components/tool-execution.ts`. [Traceability](../porting/traceability/interactive-tool-execution.md) separates proved behavior from open image work.

## What it must do

- [x] Match generic/default/self shells, call/result fallback behavior, renderer state/context and last-component reuse.
- [x] Keep call rows visible and run result renderers while output is hidden; unhide the latest result.
- [x] Limit aggregate text output to head50 + marker + tail50, retaining interleaved image blocks and raw UTF-16 text/JSON.
- [x] Omit elapsed labels before results unless showLiveElapsed is enabled; final results retain completion time even without a start timestamp.
- [x] Completion clocks cover the full JavaScript Date range through the [sole shared date authority](date-format.md).
- [x] Remove the old per-component interval/poll/deadline behavior; explicit updates drive the frozen component.
- [x] Preserve fractional/nonfinite image width options through Image rendering; [numeric Image proof](image-numeric-options.md).
- [x] Convert Kitty images asynchronously, followed by the later display update and requestRender. Pending conversions retain removed rows; preserve the frozen index-keyed replacement race and duplicate work.

## How it works

- [Module traceability](../porting/traceability/interactive-tool-execution.md)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/interactive/components/tool_execution.rs`: tool row state, shells and rendering.
- `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`: construction/settings and pending-tool timing integration.
- `crates/pi-tui/src/components/image.rs`: coupled f64 image numeric boundary.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_tool_execution.rs`:45 non-image plus20 asynchronous image source/native sequences.
- `crates/pi-coding-agent/tests/tool_execution_renderer_errors.rs`: typed renderer errors.
- `crates/pi-coding-agent/tests/interactive_components_c.rs`: focused elapsed/result regressions.

## Known gaps (current cycle)

- [ ] Parent final independent/readability gates.

## Out of scope

Tool result contents retain `TextOrImageContent`. Arbitrary `type` strings are invalid `TextContent | ImageContent` and fall under the approved August12 typed-API exclusion. Extra fields on valid blocks are excluded at this renderer API boundary, not claimed as proved: persistence retains raw session-entry JsValue, and this renderer consumes only text/image fields. Any frozen built-in reading extra block properties requires reassessment.
