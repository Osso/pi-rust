//! Port of `packages/session-backends/sqlite-node/src/sqlite/migrations.ts`.

use rusqlite::Connection;

use crate::error::SqliteError;
use crate::sqlite::sql::{SqlQuery, text_param};
use crate::sqlite::time::iso_timestamp_now;

/// One embedded schema migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteMigration {
    pub id: String,
    pub order: u32,
    pub sql: String,
}

/// Returns the embedded migration list in application order.
///
/// The TypeScript loader reads the `.sql` files from disk; the Rust port embeds them so the
/// binary carries the schema.
pub fn load_migrations() -> Vec<SqliteMigration> {
    vec![SqliteMigration {
        id: "001_initial.sql".to_owned(),
        order: 1,
        sql: include_str!("migrations/001_initial.sql").to_owned(),
    }]
}

fn ensure_migrations_table(db: &Connection) -> Result<(), SqliteError> {
    SqlQuery::new(
        "CREATE TABLE IF NOT EXISTS migrations (
	id TEXT PRIMARY KEY,
	applied_at TEXT NOT NULL
);
",
        Vec::new(),
    )
    .exec(db)
}

/// Applies every migration that has not been recorded yet.
pub fn apply_migrations(db: &Connection) -> Result<(), SqliteError> {
    ensure_migrations_table(db)?;
    let applied_rows = SqlQuery::new("SELECT id FROM migrations ORDER BY applied_at, id", Vec::new())
        .all(db, |row| row.get::<_, String>(0))?;
    let mut applied: Vec<String> = applied_rows;

    for migration in load_migrations() {
        if applied.contains(&migration.id) {
            continue;
        }
        crate::sqlite::connection::transaction(db, |db| {
            SqlQuery::new(migration.sql.clone(), Vec::new()).exec(db)?;
            SqlQuery::new(
                "INSERT INTO migrations (id, applied_at) VALUES (?, ?)",
                vec![text_param(migration.id.clone()), text_param(iso_timestamp_now())],
            )
            .run(db)?;
            Ok(())
        })?;
        applied.push(migration.id);
    }
    Ok(())
}
