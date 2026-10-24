# `loop` extension traceability

Source: `packages/coding-agent/extensions/loop/src/index.ts` (pinned in
`docs/porting/source-lock.json`). Rust:
`crates/pi-coding-agent-extensions/src/bundled_extensions/loop/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `default` factory registering `loop` command + tool | `loop_extension`, `register` | `bundled_commands.rs::loop_registers_command_and_tool`; differential `loop_registers_command_and_tool` (metadata) |
| `intervalPattern` `^(\d+(?:\.\d+)?)(ms|s|m|h)?$` | `parse_interval_ms` | differential cases `start-ms`, `start-hours`, `tool-start-invalid` |
| `Number(match[1])`, unit default `s`, `Math.floor`, minimum 1000 ms | `parse_interval_ms` | differential `start-ms`, `tool-start-invalid` |
| `formatInterval` (`h`/`m`/`s`/`ms`) | `format_interval` | differential notifications/tool text |
| `parseLoopCommand` (`status`, `stop`/`off`/`clear`, usage error, missing prompt) | `parse_loop_command` | differential `status-empty`, `stop-empty`, `off-empty`, `invalid-usage`, `missing-prompt` |
| `createLoopController.start/stop/status` with `setInterval` | `LoopController::{start,stop,status}` on `tokio::time::interval` | `bundled_commands.rs::loop_command_injects_prompt_at_interval`, `loop_tool_starts_and_stops_loop`, `loop_shutdown_clears_timer` |
| `pi.sendUserMessage(prompt, { deliverAs: "followUp" })` | `send_user_message` with `UserMessageDelivery::FollowUp` | `loop_command_injects_prompt_at_interval` |
| Tool `status` result + details | `execute_loop_tool` | differential `tool-status`; behavioral `loop_tool_starts_and_stops_loop` |
| Tool `stop` result + details | `execute_loop_tool` | differential `tool-stop` |
| Tool `start` validation (`intervalSeconds must be at least 1.`, `Prompt is required for action=start.`) | `execute_loop_tool` | differential `tool-start-invalid`, `tool-start-missing-prompt` |
| Tool `start` success text/details | `execute_loop_tool` | differential `tool-start`; behavioral timer test |
| Command handler notifications and editor clear | `handle_loop_command` | differential command cases |
| `session_shutdown` stops the active loop | `loop_extension` handler | `loop_shutdown_clears_timer` |
| Tool metadata (`label`, `description`, `promptSnippet`, `promptGuidelines`, `approvalRequired`, schema) | `loop_tool_definition` | differential metadata in every case |

## Discrepancies / gaps

- The timer is a Tokio task instead of Node `setInterval`; interval alignment and
  stop semantics match, but tests use paused Tokio time.
- `sendUserMessage` failures in the timer callback are reported to stderr and stop
  the loop; TypeScript has no observable handler for that rejection.
- `intervalMs` in tool details uses `pi_js_compat::number::js_number_value` so
  integral values serialize like `JSON.stringify` (`3000`, not `3000.0`).
