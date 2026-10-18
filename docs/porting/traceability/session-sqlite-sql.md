# Traceability: `packages/session-backends/sqlite-node/src/sqlite/sql.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/sql.rs`. Tests: `sql.rs` (TS `sql.test.ts`),
differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SqlQuery` (`queryText`, `params`) | `SqlQuery { query_text, params }` | sql | Same text + ordered `rusqlite::types::Value` list. |
| `SqlQuery.exec` | `SqlQuery::exec` | sql | `TypeError` "SQLite exec queries cannot have parameters" maps to `SqliteError::TypeError`. |
| `SqlQuery.run` | `SqlQuery::run` | adapter, sql | |
| `SqlQuery.get`/`all` | `SqlQuery::get`/`all` | sql, facts | Row callbacks replace TS generics. |
| `SqlQuery.iterate` | — | — | Gap: no consumer; search reads all rows. |
| `sql` tagged template | explicit `SqlQuery::new` | all | Language boundary: Rust has no template literals; call sites build text and params explicitly with the same placeholder order. |
| `joinSqlFragments` | `join_sql_fragments` | sql, branch entries | Preserves parameter order. |

## Discrepancies

- None in SQL text or parameter ordering. `SqlQuery::run` always reports `last_insert_rowid`,
  matching `node:sqlite`; the source's optional field never observes `undefined` there.
