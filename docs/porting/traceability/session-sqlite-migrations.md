# Traceability: `packages/session-backends/sqlite-node/src/sqlite/migrations.ts`

Rust: `crates/pi-session-sqlite/src/sqlite/migrations.rs` + embedded `migrations/001_initial.sql`,
`src/sqlite/time.rs`. Tests: `migrations.rs`, `cross_implementation.rs` (schema dump),
differential.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `loadMigrations` | `load_migrations` | migrations, cross | `readFile` replaced by `include_str!`; returns the same single migration in the same order. |
| `SqliteMigration` | `SqliteMigration` | migrations | |
| `applyMigrations` | `apply_migrations` | migrations, repository, differential | Ensures `migrations`, reads applied ids ordered by `applied_at, id`, applies missing ones inside `BEGIN IMMEDIATE`. |
| `new Date().toISOString()` | `iso_timestamp_now` | cross | Same `YYYY-MM-DDTHH:MM:SS.sssZ` format (Hinnant civil-date conversion). |
| migration SQL | `migrations/001_initial.sql` | migrations, cross | Copied verbatim; `sqlite_master` dumps are identical between implementations. |

## Gaps

- The source loads `.sql` from disk with `fs/promises`; the Rust binary embeds it. No behavioral
  difference.
