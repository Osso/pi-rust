//! Cross-implementation proof: the TypeScript backend and the Rust port read and write the same
//! SQLite database file.

mod support;

use pi_agent_core::harness::session::SessionErrorCode;
use serde_json::{Value, json};
use support::oracle::TsOracle;
use support::runner::{RustRunner, assistant_message, script_entries, user_message};
use tempfile::TempDir;

const SOURCE_MODULE: &str = "packages/session-backends/sqlite-node/src/index.ts";

fn init(root: &str, database_path: &str) -> Value {
    json!({"op": "init", "root": root, "databasePath": database_path})
}

fn assert_ok(response: &Value, op: &Value) {
    assert_eq!(
        response.get("ok").and_then(Value::as_bool),
        Some(true),
        "op {op} failed: {response}"
    );
}

fn entry_ids(response: &Value) -> Vec<String> {
    response
        .as_array()
        .expect("entry array")
        .iter()
        .map(|entry| entry.get("id").and_then(Value::as_str).expect("entry id").to_owned())
        .collect()
}

#[tokio::test]
async fn typescript_writes_then_rust_reads_appends_then_typescript_reads_back() {
    let root = TempDir::new().expect("root");
    let root_text = root.path().to_string_lossy().into_owned();
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();

    {
        let mut oracle = TsOracle::spawn("session-sqlite-oracle.ts", SOURCE_MODULE);
        for operation in [
            init(&root_text, &database_path),
            json!({"op": "create", "id": "ts-session", "metadata": {"origin": "typescript"}}),
            json!({"op": "append_entry", "session": "ts-session", "entry": {"type": "message", "id": "ts-m1", "message": user_message("root")}}),
            json!({"op": "append_entry", "session": "ts-session", "entry": {"type": "message", "id": "ts-m2", "message": assistant_message("answer")}}),
            json!({"op": "create_lane", "session": "ts-session", "lane": "thread", "at": "ts-m2"}),
            json!({"op": "append_record", "session": "ts-session", "record": {"type": "operation_started", "id": "ts-r1", "lane": "main", "sourceLeafId": null, "intent": {"kind": "run", "originalPrompt": [], "initialMessages": []}}}),
            json!({"op": "set_name", "session": "ts-session", "name": "From TypeScript"}),
            json!({"op": "close"}),
        ] {
            assert_ok(&oracle.request(&operation), &operation);
        }
    }

    let rust_entries;
    let rust_stats;
    {
        let mut runner = RustRunner::at(root.path());
        let metadata = runner
            .run(&json!({"op": "open_by_id", "session": "ts-session"}))
            .await
            .expect("rust opens the TypeScript database");
        assert_eq!(
            metadata.get("metadata").and_then(|value| value.get("origin")),
            Some(&Value::String("typescript".to_owned()))
        );
        let entries = runner
            .run(&json!({"op": "find_entries", "session": "ts-session", "query": {"order": "oldestFirst"}}))
            .await
            .expect("rust reads TypeScript entries");
        assert_eq!(entry_ids(&entries), vec!["ts-m1", "ts-m2"]);
        let lanes = runner
            .run(&json!({"op": "get_lanes", "session": "ts-session"}))
            .await
            .expect("rust reads lanes");
        assert_eq!(
            lanes,
            json!([
                {"lane": "main", "leafId": "ts-m2"},
                {"lane": "thread", "leafId": "ts-m2"}
            ])
        );
        let records = runner
            .run(&json!({"op": "find_records", "session": "ts-session", "query": {}}))
            .await
            .expect("rust reads records");
        assert_eq!(records.as_array().expect("records").len(), 1);
        runner
            .run(&json!({"op": "append_entry", "session": "ts-session", "entry": {"type": "message", "id": "rs-m3", "message": user_message("appended by rust")}}))
            .await
            .expect("rust appends");
        runner
            .run(&json!({"op": "set_name", "session": "ts-session", "name": "From Rust"}))
            .await
            .expect("rust renames");
        rust_entries = runner
            .run(&json!({"op": "find_entries", "session": "ts-session", "query": {"order": "oldestFirst"}}))
            .await
            .expect("rust re-reads");
        rust_stats = runner
            .run(&json!({"op": "get_stats", "session": "ts-session"}))
            .await
            .expect("rust stats");
        runner.repo.close().await.expect("rust closes");
    }
    assert_eq!(entry_ids(&rust_entries), vec!["ts-m1", "ts-m2", "rs-m3"]);
    assert_eq!(rust_stats.get("messageCount"), Some(&json!(3)));

    {
        let mut oracle = TsOracle::spawn("session-sqlite-oracle.ts", SOURCE_MODULE);
        for operation in [
            init(&root_text, &database_path),
            json!({"op": "open_by_id", "session": "ts-session"}),
            json!({"op": "find_entries", "session": "ts-session", "query": {"order": "oldestFirst"}}),
        ] {
            let response = oracle.request(&operation);
            assert_ok(&response, &operation);
            if operation.get("op").and_then(Value::as_str) == Some("find_entries") {
                assert_eq!(
                    entry_ids(response.get("output").expect("output")),
                    vec!["ts-m1", "ts-m2", "rs-m3"]
                );
            }
        }
        let name = oracle.request(&json!({"op": "get_name", "session": "ts-session"}));
        assert_eq!(name.get("output"), Some(&Value::String("From Rust".to_owned())));
        let stats = oracle.request(&json!({"op": "get_stats", "session": "ts-session"}));
        assert_eq!(
            stats.get("output").and_then(|value| value.get("messageCount")),
            Some(&json!(3))
        );
        let append = json!({"op": "append_entry", "session": "ts-session", "entry": {"type": "custom", "id": "ts-c4", "customType": "note", "data": {"value": 4}}});
        assert_ok(&oracle.request(&append), &append);
        assert_ok(&oracle.request(&json!({"op": "close"})), &json!({"op": "close"}));
    }

    {
        let mut runner = RustRunner::at(root.path());
        runner
            .run(&json!({"op": "open_by_id", "session": "ts-session"}))
            .await
            .expect("rust reopens");
        let entries = runner
            .run(&json!({"op": "find_entries", "session": "ts-session", "query": {"order": "oldestFirst"}}))
            .await
            .expect("rust reads back");
        assert_eq!(entry_ids(&entries), vec!["ts-m1", "ts-m2", "rs-m3", "ts-c4"]);
        runner.repo.close().await.expect("rust closes");
    }
}

#[tokio::test]
async fn rust_writes_then_typescript_reads_appends_then_rust_reads_back() {
    let root = TempDir::new().expect("root");
    let root_text = root.path().to_string_lossy().into_owned();
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();

    {
        let mut runner = RustRunner::at(root.path());
        runner
            .run(&json!({"op": "create", "id": "rs-session", "metadata": {"origin": "rust"}}))
            .await
            .expect("rust creates");
        runner
            .run(&json!({"op": "append_entry", "session": "rs-session", "entry": {"type": "message", "id": "rs-m1", "message": user_message("root")}}))
            .await
            .expect("rust appends");
        runner
            .run(&json!({"op": "append_entry", "session": "rs-session", "entry": {"type": "message", "id": "rs-m2", "message": assistant_message("answer")}}))
            .await
            .expect("rust appends");
        runner
            .run(&json!({"op": "create_lane", "session": "rs-session", "lane": "thread", "at": "rs-m2"}))
            .await
            .expect("rust lane");
        runner
            .run(&json!({"op": "append_record", "session": "rs-session", "record": {"type": "operation_finished", "id": "rs-r1", "lane": "main", "runId": "run", "outcome": "completed"}}))
            .await
            .expect("rust record");
        runner
            .run(&json!({"op": "set_name", "session": "rs-session", "name": "From Rust"}))
            .await
            .expect("rust name");
        runner.repo.close().await.expect("rust closes");
    }

    {
        let mut oracle = TsOracle::spawn("session-sqlite-oracle.ts", SOURCE_MODULE);
        for operation in [
            init(&root_text, &database_path),
            json!({"op": "open_by_id", "session": "rs-session"}),
            json!({"op": "find_entries", "session": "rs-session", "query": {"order": "oldestFirst"}}),
        ] {
            let response = oracle.request(&operation);
            assert_ok(&response, &operation);
            if operation.get("op").and_then(Value::as_str) == Some("find_entries") {
                assert_eq!(
                    entry_ids(response.get("output").expect("output")),
                    vec!["rs-m1", "rs-m2"]
                );
            }
        }
        let lanes = oracle.request(&json!({"op": "get_lanes", "session": "rs-session"}));
        assert_eq!(
            lanes.get("output"),
            Some(&json!([
                {"lane": "main", "leafId": "rs-m2"},
                {"lane": "thread", "leafId": "rs-m2"}
            ]))
        );
        let name = oracle.request(&json!({"op": "get_name", "session": "rs-session"}));
        assert_eq!(name.get("output"), Some(&Value::String("From Rust".to_owned())));
        let append = json!({"op": "append_entry", "session": "rs-session", "entry": {"type": "message", "id": "ts-m3", "message": user_message("appended by typescript")}});
        assert_ok(&oracle.request(&append), &append);
        assert_ok(&oracle.request(&json!({"op": "close"})), &json!({"op": "close"}));
    }

    {
        let mut runner = RustRunner::at(root.path());
        runner
            .run(&json!({"op": "open_by_id", "session": "rs-session"}))
            .await
            .expect("rust reopens");
        let entries = runner
            .run(&json!({"op": "find_entries", "session": "rs-session", "query": {"order": "oldestFirst"}}))
            .await
            .expect("rust reads back");
        assert_eq!(entry_ids(&entries), vec!["rs-m1", "rs-m2", "ts-m3"]);
        let stats = runner
            .run(&json!({"op": "get_stats", "session": "rs-session"}))
            .await
            .expect("rust stats");
        assert_eq!(stats.get("messageCount"), Some(&json!(3)));
        assert_eq!(stats.get("uncachedTokens"), Some(&json!(0)));
        runner.repo.close().await.expect("rust closes");
    }
}

#[tokio::test]
async fn entry_payloads_match_between_implementations() {
    let root = TempDir::new().expect("root");
    let root_text = root.path().to_string_lossy().into_owned();
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();
    let entries = script_entries();

    {
        let mut oracle = TsOracle::spawn("session-sqlite-oracle.ts", SOURCE_MODULE);
        assert_ok(
            &oracle.request(&init(&root_text, &database_path)),
            &json!({"op": "init"}),
        );
        assert_ok(
            &oracle.request(&json!({"op": "create", "id": "ts"})),
            &json!({"op": "create"}),
        );
        for entry in &entries {
            let operation = json!({"op": "append_entry", "session": "ts", "entry": entry});
            assert_ok(&oracle.request(&operation), &operation);
        }
        assert_ok(&oracle.request(&json!({"op": "close"})), &json!({"op": "close"}));
    }

    {
        let mut runner = RustRunner::at(root.path());
        runner
            .run(&json!({"op": "create", "id": "rs"}))
            .await
            .expect("rust creates");
        for entry in &entries {
            runner
                .run(&json!({"op": "append_entry", "session": "rs", "entry": entry}))
                .await
                .expect("rust appends");
        }
        runner.repo.close().await.expect("rust closes");
    }

    let db = rusqlite::Connection::open(&database_path).expect("inspection connection");
    let mut statement = db
        .prepare("SELECT id, type, payload FROM entries WHERE session_id = ? ORDER BY id")
        .expect("statement");
    let mut read = |session: &str| -> Vec<(String, String, String)> {
        statement
            .query_map((session,), |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("payloads")
    };
    let ts = read("ts");
    let rust = read("rs");
    assert_eq!(ts.len(), entries.len());
    assert_eq!(rust.len(), entries.len());
    for ((ts_id, ts_type, ts_payload), (rust_id, rust_type, rust_payload)) in ts.iter().zip(&rust) {
        assert_eq!(ts_id, rust_id);
        assert_eq!(ts_type, rust_type);
        let ts_value: Value = serde_json::from_str(ts_payload).expect("ts payload json");
        let rust_value: Value = serde_json::from_str(rust_payload).expect("rust payload json");
        assert_eq!(ts_value, rust_value, "payload mismatch for {ts_id}");
        if matches!(
            ts_type.as_str(),
            "message" | "model_change" | "thinking_level_change" | "active_tools_change" | "custom"
        ) {
            assert_eq!(ts_payload, rust_payload, "payload bytes mismatch for {ts_id}");
        }
    }
}

#[tokio::test]
async fn schema_dumps_match_between_implementations() {
    let ts_root = TempDir::new().expect("ts root");
    let ts_database = ts_root.path().join("sessions.sqlite").to_string_lossy().into_owned();
    let ts_root_text = ts_root.path().to_string_lossy().into_owned();
    {
        let mut oracle = TsOracle::spawn("session-sqlite-oracle.ts", SOURCE_MODULE);
        assert_ok(
            &oracle.request(&init(&ts_root_text, &ts_database)),
            &json!({"op": "init"}),
        );
        assert_ok(
            &oracle.request(&json!({"op": "create", "id": "ts"})),
            &json!({"op": "create"}),
        );
        assert_ok(&oracle.request(&json!({"op": "close"})), &json!({"op": "close"}));
    }
    let mut runner = RustRunner::new();
    runner
        .run(&json!({"op": "create", "id": "rs"}))
        .await
        .expect("rust creates");
    runner.repo.close().await.expect("rust closes");

    let dump = |path: &str| -> Vec<(String, String, String, Option<String>)> {
        let db = rusqlite::Connection::open(path).expect("inspection connection");
        db.prepare("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name")
            .expect("statement")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .expect("rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("schema")
    };
    let ts_schema = dump(&ts_database);
    let rust_schema = dump(&runner.database_path);
    assert_eq!(ts_schema, rust_schema);

    let migration_row = |path: &str| -> (String, String) {
        let db = rusqlite::Connection::open(path).expect("inspection connection");
        db.query_row("SELECT id, applied_at FROM migrations", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("migration row")
    };
    let (ts_id, ts_applied_at) = migration_row(&ts_database);
    let (rust_id, rust_applied_at) = migration_row(&runner.database_path);
    assert_eq!(ts_id, rust_id);
    assert_eq!(ts_applied_at.len(), rust_applied_at.len());
    assert!(rust_applied_at.ends_with('Z'));
}

#[tokio::test]
async fn cross_implementation_errors_match() {
    let root = TempDir::new().expect("root");
    let root_text = root.path().to_string_lossy().into_owned();
    let database_path = root.path().join("sessions.sqlite").to_string_lossy().into_owned();
    let mut runner = RustRunner::at(root.path());
    runner
        .run(&json!({"op": "create", "id": "shared"}))
        .await
        .expect("rust creates");
    runner.repo.close().await.expect("rust closes");

    let mut oracle = TsOracle::spawn("session-sqlite-oracle.ts", SOURCE_MODULE);
    assert_ok(
        &oracle.request(&init(&root_text, &database_path)),
        &json!({"op": "init"}),
    );
    let duplicate = oracle.request(&json!({"op": "create", "id": "shared"}));
    let duplicate_error = duplicate.get("error").expect("duplicate error");
    assert_eq!(
        duplicate_error.get("code"),
        Some(&Value::String(SessionErrorCode::AlreadyExists.as_str().to_owned()))
    );
    let rust_duplicate = runner
        .run(&json!({"op": "create", "id": "shared"}))
        .await
        .expect_err("rust duplicate");
    assert_eq!(rust_duplicate.code(), SessionErrorCode::AlreadyExists);
    assert_eq!(
        duplicate_error.get("message").and_then(Value::as_str),
        Some(rust_duplicate.message())
    );

    assert_ok(
        &oracle.request(&json!({"op": "open_by_id", "session": "shared"})),
        &json!({"op": "open_by_id"}),
    );
    let missing = oracle.request(&json!({"op": "get_entry", "session": "shared", "entryId": "missing"}));
    assert_ok(&missing, &json!({"op": "get_entry"}));
    assert_eq!(missing.get("output"), Some(&Value::Null));
    assert_ok(&oracle.request(&json!({"op": "close"})), &json!({"op": "close"}));

    runner
        .run(&json!({"op": "open_by_id", "session": "shared"}))
        .await
        .expect("rust opens");
    let rust_missing = runner
        .run(&json!({"op": "get_entry", "session": "shared", "entryId": "missing"}))
        .await
        .expect("missing entry is not an error");
    assert_eq!(rust_missing, Value::Null);
    runner.repo.close().await.expect("rust closes");
}
