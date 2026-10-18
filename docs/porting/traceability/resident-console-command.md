# resident-console-command traceability

Source: `packages/coding-agent/src/cli/resident-console-command.ts` (200 lines, pinned revision
in `docs/porting/source-lock.json`). Rust:
`crates/pi-coding-agent/src/cli/resident_console_command.rs`.

Status: partial. `parseResidentConsoleArgs` is ported and tested; the runner and the UI class
are blocked.

## Exported symbols

| TS symbol | Rust | Tests |
| --- | --- | --- |
| `parseResidentConsoleArgs` | `parse_resident_console_args` | 4 tests in `tests/resident_console_command.rs` |
| `runResidentConsoleCommand` | blocked | — |
| `ResidentConsoleUi` | blocked | — |
| `ResidentConsoleCommand` | `ResidentConsoleCommand` | parser tests |

## Branches

| # | Branch | Test |
| --- | --- | --- |
| 1 | no `--supervisor`/`--architect` first arg → `undefined` | `non_console_commands_are_not_handled` |
| 2 | `--supervisor` → service | `service_flags_map_to_their_services` |
| 3 | `--architect` → service | same |
| 4 | any remaining part starting with `-` → throw `--<service> cannot be combined with normal Pi CLI flags` | `normal_cli_flags_are_rejected_with_the_service_name` |
| 5 | prompt parts joined with spaces and trimmed; empty → `undefined` | `prompt_parts_are_joined_and_trimmed` |

## Blocked parts

- **`runResidentConsoleCommand`** — needs `AgentSessionEvent` (`core/agent-session.ts`, not
  ported) for the typed client, plus `createStartupTui`/`startStartupTui` (`cli/startup-ui.ts`,
  blocked on the TUI detector adapter) and the TTY check.
- **`ResidentConsoleUi`** — needs `AssistantMessageComponent` and `UserMessageComponent`
  (`modes/interactive/components/`, not ported) plus `AgentSessionEvent`. The event handling
  itself (`entry_appended`, `agent_start`, `agent_end`, `message_start/update/end`), duplicate
  turn markers (`core::runtime_message_markers` is ported), the supervisor response tool name
  (`supervisor::response_tool` is ported), and `sessionEntryToContextMessages`
  (`core::session_manager` is ported) are ready to wire once the types exist.
- **`readTextContent`** — pure helper blocked with the UI class; it belongs to the same module
  surface and will be ported with it.

## Deviations

- **`parseResidentConsoleArgs` errors** — TypeScript throws; the Rust signature is
  `Result<Option<ResidentConsoleCommand>, String>` so the "not a console command" case is
  distinct from the flag-conflict error.

## Proof

`cargo test -p pi-coding-agent --test resident_console_command` (4 tests).
