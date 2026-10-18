# Traceability: `packages/session-backends/sqlite-node/src/sqlite/search-backend.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/search_backend.rs`. Tests: `search.rs`,
`differential_session_sqlite.rs`, `cross_implementation.rs`.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SqliteSessionSearchOptions` | same | search | |
| `SqliteSessionSearchHit` | same | search | Implements `SessionSearchHit`. |
| `getFileSystemResultOrThrow` | `get_file_system_result_or_throw` | search | Same message mapping. |
| `getParentPath` | `get_parent_path` | search | |
| `throwIfAborted` | `throw_if_aborted` | search | `CancellationToken` → `SearchError::Aborted`. |
| `configureSqliteDatabase` | shared helper | search | Same PRAGMAs. |
| `tableExists`/`rebuildSearchIndex`/`ensureSearchSchema` | same | search | FTS5 `content='entries'`, `content_rowid='rowid'`, `tokenize='trigram remove_diacritics 1'`, three triggers; rebuild when the FTS table is new but entries exist. |
| `SqliteSessionSearch.openDatabase` | `open_database` method | search | Opens a fresh connection per search, applies migrations + schema, closes on drop. |
| `search` | `SessionSearch::search` | search, differential | Trim/blank/limit/entryTypes guards, quoted MATCH text, `bm25` ordering, optional limit. |
| `createSqliteSessionSearch` | same | search | |
| `collect` (test helper) | `Vec` return | — | The Rust `SessionSearch` contract returns a `Vec`, so iteration is eager; observable results match. |

## Gaps and boundary deviations

- Cancellation is checked before opening and between rows, like the source; the Rust contract
  returns a vector instead of an async iterator.
- bm25 scores come from the bundled SQLite build; the differential test compares scores with a
  `1e-6` tolerance (ids, session ids, timestamps and ordering compare exactly).
