//! Port of `packages/session-backends/sqlite-node/src/sqlite/sql.ts`.
//!
//! The TypeScript module builds queries from tagged templates. Rust call sites build the same
//! text and parameter list explicitly through [`SqlQuery::new`]; nested fragments keep their
//! parameter order through [`join_sql_fragments`], exactly like the source.

use rusqlite::types::Value;
use rusqlite::{Connection, Row};

use crate::error::SqliteError;

/// Result of a prepared statement execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqliteRunResult {
    pub changes: u64,
    pub last_insert_rowid: Option<i64>,
}

/// A parameterized SQLite query.
#[derive(Debug, Clone, PartialEq)]
pub struct SqlQuery {
    pub query_text: String,
    pub params: Vec<Value>,
}

impl SqlQuery {
    pub fn new(query_text: impl Into<String>, params: Vec<Value>) -> Self {
        Self {
            query_text: query_text.into(),
            params,
        }
    }

    /// Executes a multi-statement script. Parameters are rejected like the source `exec`.
    pub fn exec(&self, db: &Connection) -> Result<(), SqliteError> {
        if !self.params.is_empty() {
            return Err(SqliteError::type_error("SQLite exec queries cannot have parameters"));
        }
        db.execute_batch(&self.query_text)?;
        Ok(())
    }

    pub fn run(&self, db: &Connection) -> Result<SqliteRunResult, SqliteError> {
        let mut statement = db.prepare(&self.query_text)?;
        let changes = statement.execute(rusqlite::params_from_iter(self.params.iter()))?;
        Ok(SqliteRunResult {
            changes: changes as u64,
            last_insert_rowid: Some(db.last_insert_rowid()),
        })
    }

    pub fn get<T>(
        &self,
        db: &Connection,
        map: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Option<T>, SqliteError> {
        let mut statement = db.prepare(&self.query_text)?;
        let mut rows = statement.query(rusqlite::params_from_iter(self.params.iter()))?;
        match rows.next()? {
            Some(row) => Ok(Some(map(row)?)),
            None => Ok(None),
        }
    }

    pub fn all<T>(
        &self,
        db: &Connection,
        mut map: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, SqliteError> {
        let mut statement = db.prepare(&self.query_text)?;
        let mut rows = statement.query(rusqlite::params_from_iter(self.params.iter()))?;
        let mut values = Vec::new();
        while let Some(row) = rows.next()? {
            values.push(map(row)?);
        }
        Ok(values)
    }
}

/// Joins query fragments while preserving their parameter order.
pub fn join_sql_fragments(fragments: &[SqlQuery], separator: &str) -> SqlQuery {
    let mut query_text = String::new();
    let mut params = Vec::new();
    for (index, fragment) in fragments.iter().enumerate() {
        if index > 0 {
            query_text.push_str(separator);
        }
        query_text.push_str(&fragment.query_text);
        params.extend(fragment.params.iter().cloned());
    }
    SqlQuery { query_text, params }
}

pub(crate) fn text_param(value: impl Into<String>) -> Value {
    Value::Text(value.into())
}

pub(crate) fn int_param(value: i64) -> Value {
    Value::Integer(value)
}

pub(crate) fn real_param(value: f64) -> Value {
    Value::Real(value)
}

pub(crate) fn optional_text_param(value: Option<&str>) -> Value {
    match value {
        Some(value) => Value::Text(value.to_owned()),
        None => Value::Null,
    }
}
