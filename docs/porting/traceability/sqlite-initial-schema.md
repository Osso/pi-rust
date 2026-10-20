# Initial SQLite schema asset

Source: `packages/session-backends/sqlite-node/src/sqlite/migrations/001_initial.sql` at the [original pin](../source-lock.json). Rust artifact: `crates/pi-session-sqlite/src/sqlite/migrations/001_initial.sql`.

Complete byte arrays compared equal on 2026-10-20: 4,125 bytes, SHA-256 `ef14f89a243851b11010693a84c635a422d5e309fc8b475e84e28014322c8c53`. Revision and proof limits: [proof ledger](../proven-assets-proof.md).

`crates/pi-session-sqlite/src/sqlite/migrations.rs::load_migrations` embeds the entire SQL through `include_str!` at line 25, under ID `001_initial.sql`, order 1. `apply_migrations` executes that SQL transactionally when the ID is absent and records the migration; subsequent calls skip it. The asset is applied schema content, not an orphan copy. Complete byte identity maps every SQL statement as raw asset output; no independent Rust translation is required.

Existing behavioral proof: `crates/pi-session-sqlite/tests/migrations.rs::applies_the_current_schema_once_and_records_its_migration` invokes the public API twice, asserts one recorded migration, and checks resulting tables, columns and indexes. Exact current passing log location: [proof ledger](../proven-assets-proof.md). Status: `done`; not a claim that the full workspace gate passed. Related runtime mapping: [SQLite implementation](session-sqlite-sql.md).
