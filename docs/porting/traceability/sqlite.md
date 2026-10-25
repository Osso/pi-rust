# `coding-agent/src/core/sqlite.ts`

Source: `packages/coding-agent/src/core/sqlite.ts` (145 lines) at the pinned revision.
Rust: `crates/pi-coding-agent-core/src/core/sqlite.rs`.

## Exported symbols

| TypeScript | Rust | Proof |
|---|---|---|
| `DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS` (5000) | `DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS: u64` | `cross_implementation_control_db::schema_text_and_pragmas_match_the_typescript_module` |
| `createSqliteDatabase(path)` | `create_sqlite_database(&str) -> Result<Connection, ControlDbError>` | `session_control_db::schema_and_pragmas_match_the_shared_contract`, all DB tests |
| `createReadOnlySqliteDatabase(path)` | `create_read_only_sqlite_database(&str)` | `session_control_db::schema_and_pragmas_match_the_shared_contract` |
| `configureReadOnlySqliteDatabase(db, busyTimeoutMs)` | `configure_read_only_sqlite_database(&Connection, u64)` | same PRAGMA assertion |
| `configureSharedSqliteDatabase(db, options)` | `configure_shared_sqlite_database(&Connection, u64)` | same PRAGMA assertion |
| `isSqliteContentionError(error)` | `is_sqlite_contention_error(&ControlDbError)` + `ControlDbError::is_contention` | contention retry paths in `session_control_db` and `lifecycle_coordinator` tests |
| `SqliteValue`, `SqliteStatement`, `SqliteDatabase` | — | Explicit typed-core exclusion: these describe the dual-driver (`bun:sqlite`/`node:sqlite`) runtime statement API. Rust exposes `rusqlite::Connection` directly; invalid JavaScript value types cannot be constructed. |
| `ConfigureSharedSqliteDatabaseOptions` | — | Explicit typed-core exclusion: the only option is `busyTimeoutMs`; the Rust function takes the value directly. |

## Branches

- Writable open: `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=<ms>`; read-only open:
  `journal_mode=WAL`, `busy_timeout` only. Both asserted through `PRAGMA` reads in
  `session_control_db::schema_and_pragmas_match_the_shared_contract` and cross-language
  `schema_text_and_pragmas_match_the_typescript_module`.
- Contention detection: SQLite primary code `SQLITE_BUSY` (5) and `SQLITE_LOCKED` (6), including
  extended codes; `ControlDbError::contention` is set from `rusqlite::Error::sqlite_error_code`.
- Non-contention errors are returned unchanged; no retry loop lives in this module (matching TS).

## Common-pin open-error prerequisite (2026-10-24)

Main approved the root open-boundary correction after the question-delivery differential exposed rusqlite's path suffix. `differential_sqlite_open_errors` calls the unmodified frozen module's actual `node:sqlite` driver and both native open functions: regular-file parent, directory, missing parent; writable/read-only6cases. Actual RED: `sqlite-open-errors-red.log`. Node exposes Error/message plus `code="ERR_SQLITE_ERROR"`, numeric extended `errcode` (14; read-only directory266), and `errstr`. Native carries them in its existing NativeNodeError/ControlDbError channel; only CannotOpen's library-added path text is normalized. Other open-driver messages retain their actual SQLite text; non-open mappings are unchanged. No tool-layer string rewrite. [Contract](../../specs/sqlite-open-errors.md).

Current focused GREEN/retained-test/checkpoint proof pending in `/tmp/pi-rust-repin-core-tools/ledger.md`; this does not revalidate unrelated old-pin module claims.

## Discrepancies / gaps

- TypeScript can pass arbitrary runtime values to `run`/`get`/`all`; Rust cannot express those
  calls (typed-core exclusion, decision 2026-08-12). Observable SQL behavior for valid inputs is
  unchanged.
