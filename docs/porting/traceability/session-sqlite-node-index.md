# Traceability: `packages/session-backends/sqlite-node/src/index.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/connection.rs` (+ `lib.rs` re-exports). Tests:
`adapter.rs`, `cross_implementation.rs`, `differential_session_sqlite.rs`.

`src/index.ts` is the `node:sqlite` adapter: `wrapNodeSqliteDatabase`, `createNodeSqliteFactory`,
plus the `sqlite/index.ts` re-exports. The Rust port has exactly one driver, so the
`SqliteDatabase`/`SqliteStatement`/`SqliteDatabaseFactory` capability interfaces collapse into
`rusqlite::Connection`, `Database`, and `open_database`.

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `wrapNodeSqliteDatabase` | `Database::new` (crate-internal) | adapter, repository | One shared connection; closes exactly once. |
| `createNodeSqliteFactory().open(path)` | `open_database(path)` | adapter, migrations, cross | `Connection::open`; `:memory:` accepted. |
| `exec(sql)` | `SqlQuery::exec` | sql | Multi-statement script; parameters rejected. |
| `prepare().run(...)` | `SqlQuery::run` | adapter, sql | Positional and named binding both work at the driver boundary. |
| `prepare().get/all/iterate` | `SqlQuery::get/all` | sql, facts | Callback row mapping; `iterate` has no consumer. |
| `transaction(fn)` | `transaction(conn, fn)` | adapter, repository, conformance | `BEGIN IMMEDIATE`, commit on success, rollback on failure. |
| `close()` | `Database::close` | repository | Drops the connection; later writes report `SQLite database is closed`. |
| named parameters | `rusqlite::named_params!` | adapter | Driver feature; the backend itself only emits positional `?`. |
| async transaction rejection | — | — | Gap: a Rust transaction closure is synchronous by type, so a promise-returning callback cannot be expressed. |
| `createNodeSqliteFactory` injection | — | — | Gap: the factory capability is removed. Setup-failure and connection-retention cases are proven with real files/triggers in `repository.rs`. |

## Branches and gaps

- `isNamedParameters`/`isAsyncResult` runtime detection has no Rust counterpart (typed API).
- Raw driver errors keep their SQLite message but are surfaced as `SessionError` with code
  `storage`, because the `SessionStorage`/`SessionSearch` contracts only carry `SessionError`.
