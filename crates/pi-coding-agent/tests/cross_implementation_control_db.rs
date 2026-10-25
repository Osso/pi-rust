//! Cross-implementation proof for the shared control database.
//!
//! Common frozen TypeScript (docs/porting/source-lock.json; c248b0067 is historical) and Rust operate on the same temporary
//! `control.sqlite` file. Neither side may normalize the other's rows: reads are
//! compared as source-ordered JSON.

use crate::shared_oracle as oracle;
use pi_js_compat::js_json::JsString;

use std::thread;

use pi_coding_agent::core::session_control_db::{
    PostSharedChannelMessageInput, RuntimeMailboxAddress, WritableSessionMetadata, list_shared_channel_messages_after,
    read_prompt_history, read_session_health, read_session_metadata, read_session_name_state, write_session_metadata,
    write_session_name,
};
use pi_js_compat::js_json;
use serde_json::{Value, json};
use tempfile::TempDir;

fn temp_db() -> (TempDir, String) {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    (directory, path)
}

fn metadata(session_path: &str, id: &str) -> WritableSessionMetadata {
    WritableSessionMetadata {
        session_path: session_path.to_owned().into(),
        id: id.to_owned().into(),
        cwd: "/repo".to_owned().into(),
        name: Some("Cross".to_owned().into()),
        parent_session_path: None,
        archived_at: None,
        is_archived: None,
        is_subagent: Some(false),
        subagent_name: None,
        model_provider: Some("anthropic".to_owned()),
        model_id: Some("claude".to_owned()),
        thinking_level: Some("high".to_owned()),
        created_at: "2026-06-21T00:00:00.000Z".to_owned().into(),
        modified_at: "2026-06-21T00:00:00.000Z".to_owned().into(),
        message_count: 2,
        first_message: "hello".to_owned().into(),
    }
}

fn oracle_call(oracle: &mut oracle::TsOracle, id: u64, steps: Vec<Value>) -> Vec<Value> {
    let response = oracle.request(&json!({ "id": id, "steps": steps }));
    assert_eq!(response["ok"], Value::Bool(true), "oracle failed: {response}");
    response["output"]
        .as_array()
        .expect("oracle output")
        .iter()
        .map(|step| {
            assert_eq!(step["ok"], Value::Bool(true), "oracle step failed: {step}");
            step["value"].clone()
        })
        .collect()
}

#[test]
fn typescript_writes_are_readable_by_rust() {
    let (_directory, db) = temp_db();
    let mut oracle = oracle::TsOracle::spawn("control-db-oracle.mts", "session-names:packages/coding-agent/src");
    let metadata = json!({
        "sessionPath": "/sessions/ts.jsonl",
        "id": "ts-session",
        "cwd": "/repo",
        "name": "From TS",
        "isSubagent": false,
        "modelProvider": "anthropic",
        "modelId": "claude",
        "thinkingLevel": "high",
        "createdAt": "2026-06-21T00:00:00.000Z",
        "modifiedAt": "2026-06-21T00:00:00.000Z",
        "messageCount": 3,
        "firstMessage": "ts first",
        "allMessagesText": "ts first message",
    });
    oracle_call(
        &mut oracle,
        1,
        vec![
            json!({"op": "call", "args": ["writeSessionMetadata", [db.clone(), metadata]]}),
            json!({"op": "call", "args": ["writeSessionName", [db.clone(), "/sessions/ts.jsonl", "TS Name"]]}),
            json!({"op": "call", "args": ["postSharedChannelMessage", [db.clone(), {"sender": {"sessionId": "ts", "agentId": null}, "body": "from typescript"}]]}),
            json!({"op": "call", "args": ["recordPromptHistoryEntry", [db.clone(), "ts prompt"]]}),
            json!({"op": "call", "args": ["writeSessionHealth", [db.clone(), {
                "sessionId": "ts-session",
                "agentGeneration": 4,
                "pid": 123,
                "lastActiveAt": "2026-06-21T00:00:00.000Z",
                "lastCheckedAt": "2026-06-21T00:00:01.000Z",
                "checkStatus": "ok",
                "checkedGeneration": 4,
                "checkLatencyMs": 12,
                "updatedAt": "2026-06-21T00:00:02.000Z",
            }]]}),
        ],
    );

    let raw = rusqlite::Connection::open(&db).expect("TS-created database");
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 15, "Rust must accept an already-initialized TS v15 database");
    drop(raw);

    let read = read_session_metadata(&db, "/sessions/ts.jsonl")
        .expect("read metadata")
        .expect("metadata");
    assert_eq!(read.id, "ts-session");
    assert_eq!(read.message_count, 3);
    assert_eq!(read.first_message, "ts first");
    assert_eq!(read.model_provider.as_deref(), Some("anthropic"));
    assert_eq!(
        read_session_name_state(&db, "/sessions/ts.jsonl")
            .expect("name state")
            .name
            .as_ref(),
        Some(&JsString::from("TS Name"))
    );
    let messages = list_shared_channel_messages_after(&db, 0, 20, i64::MAX).expect("channel");
    assert_eq!(messages[0].body, "from typescript");
    assert_eq!(read_prompt_history(&db, 10).expect("history"), vec!["ts prompt"]);
    let health = read_session_health(&db, &JsString::from("ts-session"))
        .expect("health")
        .expect("record");
    assert_eq!(health.agent_generation, 4);
    assert_eq!(health.pid, Some(123));
    assert_eq!(health.check_latency_ms, Some(12));
    write_session_name(&db, "/sessions/ts.jsonl", Some(&JsString::from("Updated by Rust"))).expect("Rust update");
    let roundtrip = oracle_call(
        &mut oracle,
        11,
        vec![json!({"op":"call", "args":["readSessionNameState", [db.clone(), "/sessions/ts.jsonl"]]})],
    );
    assert_eq!(roundtrip[0], json!({"name":"Updated by Rust","hasStoredValue":true}));
}

#[test]
fn rust_writes_are_readable_by_typescript_in_source_order() {
    let (_directory, db) = temp_db();
    write_session_metadata(&db, &metadata("/sessions/rust.jsonl", "rust-session")).expect("metadata");
    write_session_name(&db, "/sessions/rust.jsonl", Some(&JsString::from("Rust Name"))).expect("name");
    pi_coding_agent::core::session_control_db::post_shared_channel_message(
        &db,
        &PostSharedChannelMessageInput {
            sender: RuntimeMailboxAddress {
                session_id: "rust".to_owned().into(),
                agent_id: None,
            },
            body: "from rust".to_owned().into(),
        },
    )
    .expect("channel");
    pi_coding_agent::core::session_control_db::record_prompt_history_entry(&db, "rust prompt").expect("history");

    let mut oracle = oracle::TsOracle::spawn("control-db-oracle.mts", "session-names:packages/coding-agent/src");
    let outputs = oracle_call(
        &mut oracle,
        2,
        vec![
            json!({"op": "call", "args": ["readSessionMetadata", [db.clone(), "/sessions/rust.jsonl"]]}),
            json!({"op": "call", "args": ["readSessionNameState", [db.clone(), "/sessions/rust.jsonl"]]}),
            json!({"op": "call", "args": ["listSharedChannelMessagesAfter", [db.clone(), 0, 20]]}),
            json!({"op": "call", "args": ["readPromptHistory", [db.clone(), 10]]}),
        ],
    );

    let rust_metadata = read_session_metadata(&db, "/sessions/rust.jsonl")
        .expect("read")
        .expect("metadata");
    assert_eq!(
        js_json::stringify(&outputs[0]),
        js_json::stringify(&serde_json::to_value(&rust_metadata).expect("metadata value"))
    );

    let rust_names = read_session_name_state(&db, "/sessions/rust.jsonl").expect("name state");
    assert_eq!(
        js_json::stringify(&outputs[1]),
        js_json::stringify(&serde_json::to_value(&rust_names).expect("names value"))
    );

    let rust_messages = list_shared_channel_messages_after(&db, 0, 20, i64::MAX).expect("channel");
    assert_eq!(
        js_json::stringify(&outputs[2]),
        js_json::stringify(&serde_json::to_value(&rust_messages).expect("messages value"))
    );
    assert_eq!(outputs[3], json!(["rust prompt"]));
}

#[test]
fn concurrent_typescript_and_rust_writers_keep_every_row() {
    let (_directory, db) = temp_db();
    let mut oracle = oracle::TsOracle::spawn("control-db-oracle.mts", "session-names:packages/coding-agent/src");
    let oracle_db = db.clone();
    let writer = thread::spawn(move || {
        oracle_call(
            &mut oracle,
            3,
            vec![json!({
                "op": "channelSpam",
                "args": [oracle_db, "typescript", 20, "ts-"],
            })],
        );
    });

    for index in 0..20 {
        pi_coding_agent::core::session_control_db::post_shared_channel_message(
            &db,
            &PostSharedChannelMessageInput {
                sender: RuntimeMailboxAddress {
                    session_id: "rust".to_owned().into(),
                    agent_id: None,
                },
                body: format!("rust-{index}").into(),
            },
        )
        .expect("rust write");
    }
    writer.join().expect("typescript writer");

    let messages = list_shared_channel_messages_after(&db, 0, 1000, i64::MAX).expect("messages");
    assert_eq!(messages.len(), 40);
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.body.starts_with("ts-"))
            .count(),
        20
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.body.starts_with("rust-"))
            .count(),
        20
    );
}

#[test]
fn schema_text_and_pragmas_match_the_typescript_module() {
    let (_directory, db) = temp_db();
    let _ = read_session_health(&db, &JsString::from("missing")).expect("initialize Rust schema");
    let (_oracle_directory, oracle_db) = temp_db();
    let mut oracle = oracle::TsOracle::spawn("control-db-oracle.mts", "session-names:packages/coding-agent/src");
    let outputs = oracle_call(
        &mut oracle,
        4,
        vec![
            json!({"op":"call", "args":["readSessionHealth",[oracle_db.clone(),"missing"]]}),
            json!({"op":"schema", "path":oracle_db}),
        ],
    );
    let oracle_schema = &outputs[1];

    let connection = pi_coding_agent::core::sqlite::create_sqlite_database(&db).expect("open");
    pi_coding_agent::core::sqlite::configure_shared_sqlite_database(&connection, 5000).expect("configure");
    let read_text = |sql: &str| -> String {
        connection
            .query_row(sql, [], |row| {
                Ok(match row.get_ref(0)? {
                    rusqlite::types::ValueRef::Text(value) => String::from_utf8_lossy(value).to_lowercase(),
                    rusqlite::types::ValueRef::Integer(value) => value.to_string(),
                    _ => String::new(),
                })
            })
            .expect("pragma")
    };
    let mut statement = connection
        .prepare(
            "SELECT type, name, sql FROM sqlite_master WHERE type IN ('table', 'index', 'trigger') ORDER BY type, name",
        )
        .expect("prepare");
    let rows: Vec<Value> = statement
        .query_map([], |row| {
            Ok(json!({
                "type": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "sql": row.get::<_, Option<String>>(2)?,
            }))
        })
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    let rust_schema = json!({
        "busyTimeout": read_text("PRAGMA busy_timeout"),
        "journalMode": read_text("PRAGMA journal_mode"),
        "rows": rows,
        "synchronous": read_text("PRAGMA synchronous"),
        "userVersion": read_text("PRAGMA user_version"),
    });
    assert_eq!(js_json::stringify(oracle_schema), js_json::stringify(&rust_schema));
}
