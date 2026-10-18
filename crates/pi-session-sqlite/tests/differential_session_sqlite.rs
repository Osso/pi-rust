//! Differential proof: identical operation scripts against the pinned TypeScript backend and the
//! Rust port.

mod support;

use pi_agent_core::harness::session::SessionError;
use serde_json::{Value, json};
use support::oracle::TsOracle;
use support::runner::{RustRunner, canonical_text, canonicalize, script};
use tempfile::TempDir;

const SOURCE_MODULE: &str = "packages/session-backends/sqlite-node/src/index.ts";

fn rust_response(result: Result<Value, SessionError>) -> Value {
    match result {
        Ok(output) => json!({"ok": true, "output": output}),
        Err(error) => json!({
            "ok": false,
            "error": {"code": error.code().as_str(), "message": error.message()},
        }),
    }
}

fn oracle_response(response: &Value) -> Value {
    if response.get("ok").and_then(Value::as_bool) == Some(true) {
        json!({"ok": true, "output": response.get("output").cloned().unwrap_or(Value::Null)})
    } else {
        let error = response.get("error").cloned().unwrap_or(Value::Null);
        json!({
            "ok": false,
            "error": {
                "code": error.get("code").cloned().unwrap_or(Value::Null),
                "message": error.get("message").cloned().unwrap_or(Value::Null),
            },
        })
    }
}

fn compare_search(ts: &Value, rust: &Value, op: &Value) {
    let ts_hits = ts.as_array().expect("ts hits");
    let rust_hits = rust.as_array().expect("rust hits");
    assert_eq!(ts_hits.len(), rust_hits.len(), "hit count for {op}");
    for (ts_hit, rust_hit) in ts_hits.iter().zip(rust_hits) {
        assert_eq!(
            ts_hit.get("sessionId"),
            rust_hit.get("sessionId"),
            "session id for {op}"
        );
        assert_eq!(ts_hit.get("entryId"), rust_hit.get("entryId"), "entry id for {op}");
        assert_eq!(ts_hit.get("timestamp"), rust_hit.get("timestamp"), "timestamp for {op}");
        let ts_score = ts_hit.get("score").and_then(Value::as_f64).expect("ts score");
        let rust_score = rust_hit.get("score").and_then(Value::as_f64).expect("rust score");
        assert!(
            (ts_score - rust_score).abs() < 1e-6,
            "bm25 score differs for {op}: {ts_score} vs {rust_score}"
        );
    }
}

#[tokio::test]
async fn differential_repository_and_search_operation_script() {
    let ts_root = TempDir::new().expect("ts root");
    let ts_database = ts_root.path().join("sessions.sqlite");
    let ts_root_text = ts_root.path().to_string_lossy().into_owned();
    let ts_database_text = ts_database.to_string_lossy().into_owned();
    let mut oracle = TsOracle::spawn("session-sqlite-oracle.ts", SOURCE_MODULE);
    let mut runner = RustRunner::new();

    let init = json!({
        "op": "init",
        "root": ts_root_text,
        "databasePath": ts_database_text,
    });
    let mut operations = vec![init];
    operations.extend(script());

    for (index, operation) in operations.iter().enumerate() {
        let mut request = operation.clone();
        request["requestId"] = json!(index);
        let ts = oracle.request(&request);
        assert_eq!(
            ts.get("requestId").and_then(Value::as_u64),
            Some(index as u64),
            "oracle id mismatch"
        );
        let ts = oracle_response(&ts);
        let rust = rust_response(runner.run(operation).await);
        let ts_canonical = canonicalize(&ts, &ts_root_text, &ts_database_text);
        let rust_canonical = canonicalize(&rust, &runner.cwd(), &runner.database_path);
        if operation.get("op").and_then(Value::as_str) == Some("search")
            && ts.get("ok").and_then(Value::as_bool) == Some(true)
        {
            assert_eq!(
                ts.get("ok"),
                rust.get("ok"),
                "search outcome differs for {operation}: {ts_canonical} vs {rust_canonical}"
            );
            compare_search(
                ts.get("output").expect("ts output"),
                rust.get("output").expect("rust output"),
                operation,
            );
            continue;
        }
        assert_eq!(
            canonical_text(&ts_canonical),
            canonical_text(&rust_canonical),
            "operation {index} differs: {operation}"
        );
    }
}
