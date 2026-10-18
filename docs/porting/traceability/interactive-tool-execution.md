# interactive-tool-execution.ts traceability

Source: `packages/coding-agent/src/modes/interactive/components/tool-execution.ts`
Port: `crates/pi-coding-agent/src/modes/interactive/components/tool_execution.rs`

## Exported symbols

| TS export | Rust | Tests |
| --- | --- | --- |
| `ToolExecutionOptions` | `ToolExecutionOptions` | constructor cases |
| `ToolExecutionComponent` | `ToolExecutionComponent` | `toolExecution` differential, timer unit test |

## Branches

| Branch | Behavior | Coverage |
| --- | --- | --- |
| no definition | title + pretty args + output fallback | differential `mystery` |
| built-in definition, no call renderer | title fallback | differential `list_sessions` |
| built-in call renderer | renderer text | differential `list_sessions` |
| built-in result renderer | styled output | differential `list_sessions` |
| error result | error background + error color | differential `list_sessions` (empty error) |
| custom definition, `renderShell: "self"` | self shell: blank line, children, timer, images | differential `custom_tool` self |
| custom definition, default shell | box padding + pending/success background | differential `custom_tool` default |
| `updateArgs` | re-renders fallback args | differential events (args) |
| `setArgsComplete` | updates context `argsComplete` | differential events |
| `setExpanded` | result renderer receives `expanded` | differential `custom_tool` self |
| result content > 100 lines | head 50, hidden marker, tail 50 | `tool_execution_limits_long_text_blocks`, differential 150-line case |
| result with no text blocks | content unchanged | differential custom tool (image-free) |
| elapsed timer | `Elapsed: <duration>` while running | `tool_execution_timer_advances_and_stops_on_completion` |
| timer stops on completion | `finishedAt` freezes duration | same unit test |
| `poll` schedule | one-second ticks, `next_deadline` | same unit test |
| `hideComponent` | render returns `[]` | not reachable: source sets `hasContent = true` in every renderer branch (gap, dead branch) |
| image blocks | spacer + `Image` with width cap | not covered (requires terminal image capabilities) |
| Kitty conversion | non-PNG converted before render | not covered (requires kitty capabilities) |
| renderer throws | fallback component | not covered: Rust renderers are not panic-caught (deviation) |

## Deviations

- The TUI handle is replaced by `Arc<dyn Fn() + Send + Sync>` (`request_render`), matching other
  ported components; `ToolRenderContext.invalidate` is `Send + Sync` and cannot capture an `Rc`.
- `setInterval` is replaced by driver-polled `poll(now)`/`next_deadline`, matching the ported
  `Loader` and `CountdownTimer`.
- Renderer panics are not caught and converted to fallbacks; TS wraps both renderer calls in
  `try/catch`. Recorded as a gap.
