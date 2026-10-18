//! Port of `packages/session-backends/sqlite-node/test/adapter.test.ts`.
//!
//! The source test that rejects asynchronous transaction callbacks is not representable: the Rust
//! transaction takes a synchronous closure, so a promise-returning callback cannot be expressed.

use pi_session_sqlite::{open_database, transaction};
use rusqlite::named_params;
use rusqlite::types::Value;

#[test]
fn commits_a_synchronous_transaction_and_returns_its_result() {
    let db = open_database(":memory:").expect("memory database");
    db.execute_batch("CREATE TABLE values_table (value INTEGER NOT NULL)")
        .expect("schema");
    let result = transaction(&db, |db| {
        db.prepare("INSERT INTO values_table (value) VALUES (?)")
            .expect("statement")
            .execute([Value::Integer(42)])
            .expect("insert");
        Ok("committed")
    })
    .expect("transaction");
    assert_eq!(result, "committed");
    let value: i64 = db
        .prepare("SELECT value FROM values_table")
        .expect("select")
        .query_row([], |row| row.get(0))
        .expect("row");
    assert_eq!(value, 42);
}

#[test]
fn rolls_back_a_failed_transaction() {
    let db = open_database(":memory:").expect("memory database");
    db.execute_batch("CREATE TABLE values_table (value INTEGER NOT NULL)")
        .expect("schema");
    let result: Result<(), pi_session_sqlite::SqliteError> = transaction(&db, |db| {
        db.execute("INSERT INTO values_table (value) VALUES (?)", [Value::Integer(42)])
            .expect("insert");
        Err(pi_session_sqlite::SqliteError::database("rollback"))
    });
    assert!(result.is_err());
    let count: i64 = db
        .prepare("SELECT COUNT(*) FROM values_table")
        .expect("count")
        .query_row([], |row| row.get(0))
        .expect("row");
    assert_eq!(count, 0);
}

#[test]
fn forwards_positional_and_named_statement_parameters() {
    let db = open_database(":memory:").expect("memory database");
    db.execute_batch("CREATE TABLE values_table (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .expect("schema");
    let positional = db
        .prepare("INSERT INTO values_table (value) VALUES (?)")
        .expect("statement")
        .execute(["positional"])
        .expect("insert");
    assert_eq!(positional, 1);
    let named = db
        .prepare("INSERT INTO values_table (value) VALUES (:value)")
        .expect("statement")
        .execute(named_params! { ":value": "named" })
        .expect("insert");
    assert_eq!(named, 1);
    let value: String = db
        .prepare("SELECT value FROM values_table WHERE id = ?")
        .expect("select")
        .query_row([1], |row| row.get(0))
        .expect("row");
    assert_eq!(value, "positional");
    let named_values: Vec<String> = db
        .prepare("SELECT value FROM values_table WHERE id >= :id ORDER BY id")
        .expect("select")
        .query_map(named_params! { ":id": 2 }, |row| row.get(0))
        .expect("rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("values");
    assert_eq!(named_values, vec!["named".to_owned()]);
}
