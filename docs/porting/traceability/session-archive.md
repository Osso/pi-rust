# session-archive traceability

Frozen source: `packages/coding-agent/extensions/session-archive/src/index.ts` at `4f9e6f8a4` ([lock](../source-lock.json)).
Rust: `crates/pi-coding-agent/src/bundled_extensions/session_archive.rs`.

| Source | Rust | Proof |
|---|---|---|
| command registrations, shutdown handler | `session_archive_extension` | registration differential RED; GREEN pending |
| command validation | `validate_current_session` | usage differential; missing DB/persistence expansion pending |
| unarchive and already-unarchived | `unarchive_current_session` | pending |
| active-child guard/confirmation | `confirm_delete` | pending |
| quit-only action, one-shot discard | `run_quit_action`, pending action | deferred archive/compressed file and relocated metadata test |
| deletion | [delete module](session-delete.md) | isolated fixtures |

Archive storage implementation: [storage trace](session-archive-storage.md). No old immediate metadata-only archival remains.
