# session-archive-storage.ts

Frozen 4f9e6f8a4. Six exports ported to `session_archive_storage`: suffix/predicate/file archive/file restore/persisted archive/persisted restore. Uses foundation public atomic relocation, no SQL. Validation and mode/exclusive temporary/publication/rollback follow source. `repin_session_storage::rust_archive_restores_with_frozen_ts_and_exact_bytes` covers both producer directions. Pending: DB failure/rollback, both-file precedence, corrupt stream, modes and cleanup mutation proofs. Coverage remains todo.
