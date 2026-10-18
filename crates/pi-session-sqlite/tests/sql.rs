//! Port of `packages/session-backends/sqlite-node/test/sql.test.ts`.

mod support;

use pi_session_sqlite::{SqlQuery, join_sql_fragments, open_database};
use rusqlite::types::Value;
use support::Fixture;

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

#[test]
fn composes_sqlite_queries_without_renumbering_parameters() {
    let db = open_database(":memory:").expect("memory database");
    SqlQuery::new(
        "CREATE TABLE entries (id TEXT PRIMARY KEY, kind TEXT NOT NULL, active INTEGER NOT NULL)",
        Vec::new(),
    )
    .exec(&db)
    .expect("schema");
    SqlQuery::new(
        "INSERT INTO entries (id, kind, active) VALUES (?, ?, ?)",
        vec![text("one"), text("message"), Value::Integer(1)],
    )
    .run(&db)
    .expect("first insert");
    SqlQuery::new(
        "INSERT INTO entries (id, kind, active) VALUES (?, ?, ?)",
        vec![text("two"), text("message"), Value::Integer(0)],
    )
    .run(&db)
    .expect("second insert");
    let filters = join_sql_fragments(
        &[
            SqlQuery::new("kind = ?", vec![text("message")]),
            SqlQuery::new("active = ?", vec![Value::Integer(1)]),
        ],
        " AND ",
    );
    let rows = SqlQuery::new(
        format!("SELECT id FROM entries WHERE {} LIMIT ?", filters.query_text),
        {
            let mut params = filters.params.clone();
            params.push(Value::Integer(10));
            params
        },
    )
    .all(&db, |row| row.get::<_, String>(0))
    .expect("filtered rows");
    assert_eq!(rows, vec!["one".to_owned()]);
}

#[test]
fn executes_parameterized_queries() {
    let db = open_database(":memory:").expect("memory database");
    SqlQuery::new(
        "CREATE TABLE values_table (id INTEGER PRIMARY KEY, value TEXT NOT NULL)",
        Vec::new(),
    )
    .exec(&db)
    .expect("schema");
    SqlQuery::new(
        "INSERT INTO values_table (id, value) VALUES (?, ?)",
        vec![Value::Integer(1), text("one")],
    )
    .run(&db)
    .expect("first insert");
    SqlQuery::new(
        "INSERT INTO values_table (id, value) VALUES (?, ?)",
        vec![Value::Integer(2), text("two")],
    )
    .run(&db)
    .expect("second insert");

    let single = SqlQuery::new("SELECT value FROM values_table WHERE id = ?", vec![Value::Integer(1)])
        .get(&db, |row| row.get::<_, String>(0))
        .expect("single row");
    assert_eq!(single.as_deref(), Some("one"));
    let all = SqlQuery::new("SELECT value FROM values_table ORDER BY id", Vec::new())
        .all(&db, |row| row.get::<_, String>(0))
        .expect("all rows");
    assert_eq!(all, vec!["one".to_owned(), "two".to_owned()]);
}

#[test]
fn exec_rejects_parameters() {
    let db = open_database(":memory:").expect("memory database");
    let error = SqlQuery::new("SELECT ?", vec![Value::Integer(1)])
        .exec(&db)
        .expect_err("exec must reject parameters");
    assert_eq!(error.to_string(), "SQLite exec queries cannot have parameters");
}

#[test]
fn fixture_repository_reports_a_database_path() {
    let fixture = Fixture::new();
    assert!(fixture.database_path.ends_with("sessions.sqlite"));
}
