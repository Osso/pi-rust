//! Port of `packages/session-backends/sqlite-node/src/sqlite/index.ts`.

pub mod branch_cache;
pub mod connection;
pub mod migrations;
pub mod repo;
pub mod search_backend;
pub mod sql;
pub mod storage;
pub mod time;
pub mod types;

pub use connection::{configure_sqlite_database, open_database, transaction};
pub use migrations::{SqliteMigration, apply_migrations, load_migrations};
pub use repo::{
    SqliteSessionRepository, SqliteSessionRepositoryOptions, SqliteSessionStorage, SqliteWriterLeaseOptions,
    SqliteWriterLeaseOptionsError,
};
pub use search_backend::{
    SqliteSessionSearch, SqliteSessionSearchHit, SqliteSessionSearchOptions, create_sqlite_session_search,
};
pub use sql::{SqlQuery, SqliteRunResult, join_sql_fragments};
pub use time::{iso_timestamp_from_millis, iso_timestamp_now};
pub use types::{SqliteSessionCreateOptions, SqliteSessionListOptions, SqliteSessionMetadata};
