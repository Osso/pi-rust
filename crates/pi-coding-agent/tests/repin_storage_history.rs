#[path = "support/oracle.rs"]
pub(super) mod oracle;

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
fn corrupted_archive_history_preserves_frozen_decoder_error_and_cleanup() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create", "dir":dir, "cwd":dir, "compacted":true}));
    let archive = call(&mut ts, json!({"op":"fileArchive","path":created["path"]}));
    let path = archive.as_str().unwrap();
    let manager = SessionManager::open_read_only(&JsString::from(path), None).unwrap();
    let peer = ts.request(&json!({"id":1,"op":"historyAfterRewrite","path":path,"contents":"not a zstd archive"}));
    assert_eq!(peer["ok"], false, "{peer}");
    let mut history = read_session_history_reverse(manager.get_branch(None), manager.get_session_file().cloned());
    for _ in 0..3 {
        assert!(history.next().unwrap().is_ok());
    }
    let error = history.next().unwrap().unwrap_err();
    let expected = peer["error"].as_str().unwrap().strip_prefix("Error: ").unwrap();
    assert_eq!(error.message(), expected);
    let native = std::error::Error::source(&error)
        .expect("history must preserve the original decoder Error")
        .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
        .unwrap();
    let value = native.value().to_serde().unwrap();
    for property in ["name", "code", "errno"] {
        assert_eq!(value[property], peer["details"][property], "{property}");
    }
    assert!(history.next().is_none());
    assert_eq!(std::fs::read(path).unwrap(), b"not a zstd archive");
    assert!(!std::path::Path::new(created["path"].as_str().unwrap()).exists());
}

#[test]
fn reverse_history_ignores_malformed_inactive_and_missing_ancestors() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    for archived in [false, true] {
        for missing_ancestor in [false, true] {
            let created = call(&mut ts, json!({"op":"create", "dir":dir, "cwd":dir, "compacted":true}));
            let path = created["path"].as_str().unwrap();
            let mut lines = std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if missing_ancestor {
                let mut ancestor: Value = serde_json::from_str(&lines[1]).unwrap();
                ancestor["id"] = json!("not-the-retained-parent");
                lines[1] = serde_json::to_string(&ancestor).unwrap();
            }
            // A malformed complete line and an unterminated malformed tail must
            // not hide an older active ancestor or install inactive messages.
            let manager = SessionManager::open_read_only(&JsString::from(path), None).unwrap();
            let expected = call(
                &mut ts,
                json!({"op":"historyAfterRewrite","path":path,
                "contents":lines.join("\n") + "\n{broken\n\n{partial", "archive":archived}),
            );
            let actual = read_session_history_reverse(manager.get_branch(None), manager.get_session_file().cloned())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(serde_json::to_value(actual).unwrap(), expected);
            assert_eq!(expected.as_array().unwrap().len(), if missing_ancestor { 3 } else { 4 });
        }
    }
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
