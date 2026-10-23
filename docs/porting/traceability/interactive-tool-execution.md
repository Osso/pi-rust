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
| image options / Kitty conversion | fractional/NaN width propagation; later asynchronous conversion update | OPEN: ImageOptions still narrows numeric carriers and conversion is still synchronous at this checkpoint |
| hideComponent | source sets hasContent true in every valid renderer branch | unreachable with typed Component-returning renderers; not a separate positive behavior proof |

RED `test-tool-execution-red-2.log`: all21 cases differed in hidden result state, timers and completion clocks. GREEN `test-tool-execution-green-2.log`:21 sequences match. No complete whole-module or Q13 claim until open image/lifecycle branches close. Parent owns final independent/readability gates.

## Corrections to stale claims

The frozen component does **not** call setInterval. Removed old Rust poll/next_deadline/dispose timer API and the old-only interactive tool_component_owns_elapsed working-status suppression. Rebuilt pending tools explicitly enable showLiveElapsed, matching frozen integration. Image conversion still needs the source `.then` ordering; the former claim that synchronous conversion was equivalent is not accepted proof.

## Explicit exclusions

Arbitrary result-block type strings are invalid TextContent|ImageContent, covered by the approved August12 typed-API exclusion. Extra properties on valid blocks are excluded from this typed renderer boundary, not proved: session persistence retains raw JsValue and frozen tool rendering reads text/image fields. A built-in consumer reading such extras would reopen this classification. Renderer errors remain typed Result failures; Rust panics are not fabricated JavaScript exceptions. Opaque tool IDs/names retain the approved checked scalar boundary.
