# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/sessions.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/sessions.rs`. Tests: `repository.rs`,
`conformance.rs`, differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SessionRow`/`NewSessionRow` | same | repository | |
| `sessionExists` | `session_exists` | conformance, repository | |
| `insertSessionRow` | `insert_session_row` | repository, differential | Metadata serialized as JSON text or `NULL`. |
| `readSessionRow` | `read_session_row` | repository | Same name-fact `LEFT JOIN` subquery. |
| `readSessionRows` | `read_session_rows` | repository, differential | Optional `cwd` filter, `created_at DESC`. |
| `deleteSessionRow` | `delete_session_row` | conformance, repository | |
| `decodeSessionMetadata` | `decode_session_metadata` | repository, differential | |
| `parseMetadata` errors | same messages | repository | `metadata is not valid JSON`, `metadata must be an object`. |
| `parseSessionName` errors | same messages | repository | `name is not valid JSON`, `name must be a string`. |
| `serializeMetadata`/`assertJsonSerializable` | — | — | Gap: `JsonObject` is JSON by construction; non-JSON runtime values are unrepresentable. |
