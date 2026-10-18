# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/session-sequences.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/session_sequences.rs`. Tests: conformance,
differential.

| TypeScript | Rust | Tests |
|---|---|---|
| `createSequence` | `create_sequence` | conformance |
| `getNextSequence` | `get_next_sequence` | conformance |
| `setNextSequence` | `set_next_sequence` | conformance |
| `advanceSequence` | `advance_sequence` | conformance |
| `deleteSequence` | `delete_sequence` | conformance, repository |

Missing sequence rows produce the source message `Missing sequence row for session {id}`.
