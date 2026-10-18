# Traceability: `packages/session-backends/sqlite-node/src/sqlite/index.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/mod.rs`. Tests: every crate test imports through these
re-exports.

| TypeScript export | Rust | Notes |
|---|---|---|
| `./migrations.ts` | `migrations` module, `apply_migrations`, `load_migrations`, `SqliteMigration` | |
| `SqliteSessionRepository`, `SqliteSessionRepositoryOptions`, `SqliteWriterLeaseOptions` | same names in `repo` | |
| `./search-backend.ts` | `search_backend` module + `create_sqlite_session_search` | |
| `./sql.ts` | `sql` module + `SqlQuery`, `join_sql_fragments`, `SqliteRunResult` | |
| `SqliteDatabase`, `SqliteStatement`, `SqliteDatabaseFactory` | — | Gap: capability interfaces removed in favor of `rusqlite::Connection` (see `session-sqlite-node-index.md`). |
| `SqliteSessionCreateOptions`, `SqliteSessionListOptions`, `SqliteSessionMetadata`, `SqliteSessionRepositoryEnv` | `types` (env becomes `Arc<dyn FileSystem>`) | |
