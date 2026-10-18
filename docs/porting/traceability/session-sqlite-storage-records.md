# Traceability: `packages/session-backends/sqlite-node/src/sqlite/storage/records.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/storage/records.rs`. Tests: conformance,
repository, differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `RecordRow`/`NewRecordRow` | same | conformance | |
| `appendRecordRow` | `append_record_row` | conformance, differential | `run_id`/`op_kind` null when absent. |
| `idExistsInRecords` | `id_exists_in_records` | conformance | |
| `deleteRecordRows` | `delete_record_rows` | repository | |
| `readRecordRows` | `read_record_rows` | conformance | Same predicate order and `ORDER BY seq` direction. |
| `readOpenOperationRows` | `read_open_operation_rows` | conformance | Reads the lane pointer, then validates `lane`/`type`; errors match. |
| `_options.limit` ignored | `_limit` ignored | conformance | Matches source. |
