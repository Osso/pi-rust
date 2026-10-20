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

The archive differential explicitly selects `session-names:` ([v15 reference](../session-name-oracle.md))
because `archiveSession` opens the approved v15 control DB. The command body is
unchanged from the general pin; that pin's v14 DB reader rejects v15 before the
notification. Every TS step must succeed. Both implementations use the manager's
actual session path; the fixture is unarchived between callers so each must
independently persist the archive transition and emit the full notification.

No gaps.
