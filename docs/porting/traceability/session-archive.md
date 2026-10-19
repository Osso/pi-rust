# session-archive traceability

Source: `packages/coding-agent/extensions/session-archive/src/index.ts` (26 lines).
Rust: `crates/pi-coding-agent/src/bundled_extensions/session_archive.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_commands.rs`.

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| argument usage warning | command handler | differential `now` step |
| missing control DB error | `ctx.control_db_path` branch | behavioral test |
| unpersisted session warning | `get_session_file` branch | branch implemented |
| `archiveSession(controlDbPath, sessionPath)` | `session_control_db::archive_session` | differential + metadata assertion |
| archived notification | command handler | differential |
| command metadata/description | `register_command` | registration differential |

No gaps.
