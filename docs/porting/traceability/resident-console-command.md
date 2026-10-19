# resident-console-command traceability

Source: `packages/coding-agent/src/cli/resident-console-command.ts` (200 lines, pinned revision
in `docs/porting/source-lock.json`). Rust:
`crates/pi-coding-agent/src/cli/resident_console_command.rs`.

Status: done. Argument parsing, the runner, and `ResidentConsoleUi` are ported and
tested. `main` dispatches `--supervisor`/`--architect` to the runner.

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `parseResidentConsoleArgs` | `parse_resident_console_args` | 4 tests in `tests/resident_console_command.rs` |
| `runResidentConsoleCommand` | `run_resident_console_command` | TTY check, socket path, settings, startup TUI, attach snapshot, event/submit pumping |
| `ResidentConsoleUi` | `ResidentConsoleUi` (`ResidentConsoleEvent` wire enum) | `console_renders_the_header_and_snapshot_branch`, `console_skips_duplicate_turn_guard_messages`, `console_removes_duplicate_turn_assistant_messages`, `console_renders_supervisor_response_tool_results_as_plain_text`, `console_wire_events_parse_the_session_stream` |
| `ResidentConsoleCommand` | `ResidentConsoleCommand` | parser tests |

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | no `--supervisor`/`--architect` first arg → `undefined` | `non_console_commands_are_not_handled` |
| 2 | `--supervisor` → service | `service_flags_map_to_their_services` |
| 3 | `--architect` → service | same |
| 4 | any remaining part starting with `-` → throw `--<service> cannot be combined with normal Pi CLI flags` | `normal_cli_flags_are_rejected_with_the_service_name` |
| 5 | prompt parts joined with spaces and trimmed; empty → `undefined` | `prompt_parts_are_joined_and_trimmed` |
| 6 | non-TTY stdin/stdout → throw `--<service> requires an interactive terminal` | runner guard (no TTY in tests) |
| 7 | socket path `<control.sqlite>.<service>-console.sock`, project-untrusted settings, startup TUI | runner wiring |
| 8 | snapshot branch entries render through `sessionEntryToContextMessages` | `console_renders_the_header_and_snapshot_branch` |
| 9 | `entry_appended`/`agent_start`/`agent_end` update transcript/status | `console_wire_events_parse_the_session_stream` + handler |
| 10 | `message_start` user/toolResult/assistant branches; guard marker skipped | `console_skips_duplicate_turn_guard_messages`, `console_renders_supervisor_response_tool_results_as_plain_text` |
| 11 | `message_update` updates the streaming assistant; duplicate `message_end` removes it | `console_removes_duplicate_turn_assistant_messages` |
| 12 | ctrl+c / ctrl+d close; disconnect sets the error status and closes | handler implementation |
| 13 | submit trims, clears the input, shows Queued…/Working… or the error | runner `submit` |
| 14 | `finally`: clear + stop the TUI, close the client | runner |

## Deviations

- The client deserializes the wire subset this UI renders (`ResidentConsoleEvent`:
  `entry_appended`, `agent_start`, `agent_end`, `message_start/update/end`, plus an
  `Other` catch-all). The source types the client with the full `AgentSessionEvent`
  union.
- The TypeScript TUI owns its render loop; the Rust runner pumps
  `process_events`/`flush_pending` every 5 ms (same as the startup selector) and
  awaits `client.prompt` inline, so a slow prompt response delays the next pump.
- `readTextContent` is split into `read_user_content`, `read_blocks`, and
  `read_value_content` because Rust content types are typed instead of `unknown`.

- **`parseResidentConsoleArgs` errors** — TypeScript throws; the Rust signature is
  `Result<Option<ResidentConsoleCommand>, String>` so the "not a console command" case is
  distinct from the flag-conflict error.

## Proof

`cargo test -p pi-coding-agent --test resident_console_command` (9 tests).
