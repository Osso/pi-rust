# interactive-tool-execution.ts traceability

Authority: common-pin496-line `packages/coding-agent/src/modes/interactive/components/tool-execution.ts`. Rust: `crates/pi-coding-agent/src/modes/interactive/components/tool_execution.rs`. [Contract](../../specs/interactive-tool-execution.md).

## Coverage matrix

| Exports / branches | Behavior | Proof |
| --- | --- | --- |
| ToolExecutionOptions / ToolExecutionStartOptions | defaults; hideOutput and showLiveElapsed | source/native sequence harness; option corpus expansion pending |
| ToolExecutionComponent construction/render/updateArgs | generic fallback, default Box/self Container shells; exact raw argument JSON | `differential_tool_execution`,21 request sequences |
| getCallRenderer/getResultRenderer/getRenderShell | built-in/custom precedence | existing interactive components differential; new harness covers custom/missing definitions |
| renderer context/state/lastComponent | stable shared state and retained call/result components | new harness records renderer-observable inputs and renders reused Text components |
| renderResultComponent | result callback runs when hidden, only mounting is suppressed; typed failures clear prior component and render fallback | new harness (three shell modes, call/result present/absent, call/result failures) |
| setExpanded/setShowImages/setHideOutput/invalidate | updated context/output; inherited child invalidation before display update | new state sequences |
| text limiting | aggregate text blocks,50head + marker +50tail; preserve interleaved images/raw UTF-16 | long mixed-content sequences; existing lossless/native tests |
| elapsed/completion | no automatic component interval; live elapsed only with partial result or showLiveElapsed; completion clock retained with or without start | source/native start/result sequences; completion timestamp includes fractional milliseconds |
| inherited Container | mounted shell remains live, no swallowed erased-container downcast | typed shell handles; further add/remove proof pending |
| image options | f64 constructor/setter through Image and terminal calculations; NaN propagates | [420 actual frozen Image sequences](image.md) GREEN; scalar narrowing removed |
| Kitty conversion | initial display stays synchronous; conversion completes later, mutates display, then requests render; duplicate pending work and removed-row lifetime retained | `differential_tool_execution::kitty_conversion_updates_later_before_request_render_and_retains_removed_rows`:20 actual frozen/native sequences, including failure/PNG/no-caps/cache/replacement paths |
| hideComponent | source sets hasContent true in every valid renderer branch | unreachable with typed Component-returning renderers; not a separate positive behavior proof |

RED `test-tool-execution-red-2.log`: all21 cases differed in hidden result state, timers and completion clocks. GREEN `test-tool-execution-green-2.log`:21 sequences match. Async follow-up GREEN `test-tool-image-async-green-2.log`:21 non-image plus20 image-lifecycle sequences pass. Wide valid Date completion values outside Jiff's calendar range remain open; no complete whole-module or Q13 claim. Parent owns final independent/readability gates.

## Corrections to stale claims

The frozen component does **not** call setInterval. Removed old Rust poll/next_deadline/dispose timer API and the old-only interactive tool_component_owns_elapsed working-status suppression. Rebuilt pending tools explicitly enable showLiveElapsed, matching frozen integration. Image conversion now runs in a local future, retaining the same Rc row until completion. The sole constructor returns Rc<RefCell<Self>> and initializes a Weak self identity; futures hold strong rows even after transcript removal. The later display update precedes requestRender and releases its mutable borrow before the callback. Linked Photon decoding begins after an explicit async yield; there is no synchronous compatibility path or render-time drain.

## Source discrepancy: index-keyed conversion cache

**DISCREPANCY, preserved:** conversion promises capture image index, not result identity. A conversion queued for an old result can later overwrite the cached image at the same index after a new result arrives; simultaneous uncached updates queue duplicate work. The replacement and duplicate cases compare actual emitted image payloads/rows and request-observed renderer events. No generation guard, deduplication or source-bug repair is introduced.

## Explicit exclusions

Arbitrary result-block type strings are invalid TextContent|ImageContent, covered by the approved August12 typed-API exclusion. Extra properties on valid blocks are excluded from this typed renderer boundary, not proved: session persistence retains raw JsValue and frozen tool rendering reads text/image fields. A built-in consumer reading such extras would reopen this classification. Renderer errors remain typed Result failures; Rust panics are not fabricated JavaScript exceptions. Opaque tool IDs/names retain the approved checked scalar boundary.
