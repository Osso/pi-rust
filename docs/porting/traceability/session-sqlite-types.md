# Traceability: `packages/session-backends/sqlite-node/src/sqlite/types.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/types.rs`, `src/error.rs`. Tests: conformance,
repository, differential, cross-implementation.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SqliteRunResult` | `SqliteRunResult` | adapter, sql | `changes` is `u64`; `last_insert_rowid` always set by rusqlite. |
| `SqliteStatement` | — | — | Gap: replaced by `SqlQuery` row callbacks. |
| `SqliteDatabase` | `Database` (crate-internal) | repository | |
| `SqliteDatabaseFactory` | `open_database` | adapter | |
| `SqliteSessionMetadata` | `SqliteSessionMetadata` | conformance, repository, differential | `serde(rename_all = "camelCase")`; `name`/`metadata` omitted when absent. |
| `SessionMetadataValue` impl | same | conformance | Uses `id()`. |
| `SqliteSessionCreateOptions` | `SqliteSessionCreateOptions` | conformance, differential | |
| `SqliteSessionListOptions` | `SqliteSessionListOptions` | conformance, repository | |
| `SqliteSessionRepositoryEnv` | `Arc<dyn FileSystem>` | all | Reuses the ported `FileSystem` contract; only `absolute_path`, `create_dir`, `exists` are called. |
