#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_history::read_session_history_reverse;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn call(ts: &mut oracle::TsOracle, mut request: Value) -> Value {
    request["id"] = json!(1);
    let response = ts.request(&request);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}

#[test]
fn missing_compacted_transcript_preserves_frozen_open_error() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create", "dir":dir, "cwd":dir, "compacted":true}));
    let path = created["path"].as_str().unwrap();
    let manager = SessionManager::open_read_only(&JsString::from(path), None).unwrap();
    let peer = ts.request(&json!({"id":1,"op":"historyAfterRemove","path":path}));
    assert_eq!(peer["ok"], false, "{peer}");
    let mut history = read_session_history_reverse(manager.get_branch(None), manager.get_session_file().cloned());
    for _ in 0..3 {
        assert!(history.next().unwrap().is_ok());
    }
    let error = history.next().unwrap().unwrap_err();
    let expected = peer["error"].as_str().unwrap().strip_prefix("Error: ").unwrap();
    assert_eq!(error.message(), expected);
    assert!(history.next().is_none(), "a failed history iterator is exhausted");
}

#[test]
fn empty_parent_does_not_open_removed_transcript() {
    // Frozen session-history.ts:75-76 stops on !parentId before opening a file.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create", "dir":dir, "cwd":dir, "compacted":true}));
    let path = created["path"].as_str().unwrap();
    let mut records: Vec<Value> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    // The retained compaction accepts a missing ancestor; the first kept parent
    // remains an empty string, which the history generator treats as a stop.
    records[2]["parentId"] = json!("");
    let bytes = records
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(path, &bytes).unwrap();
    let native = SessionManager::open_read_only(&JsString::from(path), None).unwrap();
    let expected = call(&mut ts, json!({"op":"historyAfterRemove", "path":path}));
    assert!(!std::path::Path::new(path).exists());
    let actual = read_session_history_reverse(native.get_branch(None), native.get_session_file().cloned())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), expected);
    assert_eq!(expected.as_array().unwrap().len(), 3);
}
