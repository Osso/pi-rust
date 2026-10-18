# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/entries.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/entries.rs`. Tests: conformance,
branch-query, log-query, differential, cross-implementation.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `EntryRow`/`NewEntryRow` | same | conformance, differential | |
| `entryPayload` | `entry_payload` | cross, differential | Strips `type`/`id`; emits keys in source facade order so payload text is byte-identical for facade-produced entries. |
| `insertEntryRow` | `insert_entry_row` | conformance | |
| `readEntryRow` | `read_entry_row` | conformance | |
| `readEntryRows` | `read_entry_rows` | conformance, branch-query | Same dynamic predicates (`afterSeq`, cursor direction, type, order, limit). |
| `idExistsInEntries` | `id_exists_in_entries` | conformance | |
| `deleteEntryRows` | `delete_entry_rows` | repository, search | |

## Discrepancies

- Caller-provisioned entry objects keep their own key order in TypeScript; Rust emits facade
  order. JSON values are identical; raw payload text can differ only for that caller-defined
  case (proven in `entry_payloads_match_between_implementations`).
