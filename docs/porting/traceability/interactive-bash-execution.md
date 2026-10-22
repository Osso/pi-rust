# components/bash-execution.ts

Source: `coding-agent/src/modes/interactive/components/bash-execution.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/bash_execution.rs`

Tests: `interactive_components_c.rs::bash_execution_*`, differential `bashExecution`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `BashExecutionComponent` | `BashExecutionComponent` | behavioral + differential |
| `BashExecutionOptions.showElapsed` default true | `BashExecutionOptions` | timer test |
| border color `bashMode` / `dim` for excluded | constructor | differential exclude case |
| `setExpanded` | `set_expanded` | differential |
| `appendOutput` strips ANSI and normalizes `\r\n`/`\r` | `append_output` | behavioral + differential |
| incomplete-line continuation | last-line append | behavioral |
| `setComplete` status (`cancelled` > `error` > `complete`) | `set_complete` | behavioral + differential |
| context truncation (`DEFAULT_MAX_LINES`/`BYTES`) | `truncate_tail` | behavioral |
| preview slice of last 20 lines + hidden count | `PREVIEW_LINES` | behavioral |
| collapsed width-aware visual truncation | `VisualPreview` | behavioral |
| expanded output | expanded arm | differential |
| loader message with elapsed prefix | `format_timer` + `set_message` | behavioral |
| elapsed timer interval 1000ms | `poll_timer`/`next_timer_deadline` | timer test |
| status parts: elapsed, hidden lines, cancelled, exit, truncation warning | status arm | behavioral |
| `getOutput` / `getCommand` | `get_output` / `get_command` | differential |

## Gaps

- `setInterval` is exposed as `poll_timer(now)`/`next_timer_deadline`; the TUI drives it.
- `Date.now()` is replaced by `Instant` deltas, which is the same monotonic elapsed source.

## Discrepancies

None found.

## Q18 native UTF-16

Command/output, truncation preview, loader/status/path labels and cached visual lines use `JsString`. No encoding at component construction. `raw_error_consumer_components::bash_execution_keeps_raw_command_and_streamed_output` written **unexecuted**, coding-agent blocked.
