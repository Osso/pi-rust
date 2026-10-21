# session-archive traceability

Frozen source: `packages/coding-agent/extensions/session-archive/src/index.ts` at `4f9e6f8a4` ([lock](../source-lock.json)). Rust: `crates/pi-coding-agent/src/bundled_extensions/session_archive.rs`. Contract: [spec](../../specs/session-archive.md).

| Source symbol/branch | Rust | Proof |
|---|---|---|
| registrations, descriptions/order, shutdown event | `session_archive_extension`, `register_quit_handler` | `bundled_extensions_commands::session_archive_matches_typescript`: frozen registration differential |
| usage / missing DB / unpersisted checks | `read_current_session_and_notify` | `session_archive_command_branches_match_frozen`: all three commands, all validation modes, exact calls |
| unarchive / already-unarchived | `unarchive_current_session` | same differential + actual metadata assertion |
| running transcript-writing children, singular/plural; nonwriting child allowed | `count_writing_children`, `confirm_delete` | same differential with native MultiAgentStore, one/two writers and pathless child |
| delete declined/confirmed, confirmation title/body, shutdown request | `execute_session_command`, `confirm_delete` | same differential, real deletion and metadata absence on quit |
| deferred archive after final write, compressed file and relocated metadata | `run_quit_action` | archive test appends after command, then verifies archived transcript contains final text |
| quit-only, consume once, nonquit clears pending action | `register_quit_handler` | repeated quit; reload then quit preserves transcript |
| wrapped errors emitted to stderr after stopped UI | `run_quit_action`, shutdown handler | source-faithful implementation inspected; direct stderr-capture branch proof not added |

Initial registration differential RED; eight command-target tests GREEN at `3d4e6a32`. Quit-gate inversion mutant killed by file-state assertion. No immediate metadata-only archival remains. [Deletion trace](session-delete.md), [storage trace](session-archive-storage.md), [proof limits](../session-archive-history-proof.md).
