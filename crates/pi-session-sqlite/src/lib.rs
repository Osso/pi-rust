//! Port of `packages/session-backends/sqlite-node/src/index.ts` and `src/sqlite/**`.
//!
//! `src/index.ts` is the `node:sqlite` adapter plus the package re-exports; in Rust the adapter
//! collapses into [`sqlite::connection`] and the re-exports into this module and [`sqlite`].

pub mod error;
pub mod sqlite;

pub use error::SqliteError;
pub use sqlite::*;
