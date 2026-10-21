#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_manager::{SessionManager, load_entries_from_file, read_session_header};
use serde_json::{Value, json};
fn call(ts: &mut oracle::TsOracle, mut request: Value) -> Value {
    request["id"] = json!(1);
    let response = ts.request(&request);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}
#[test]
fn frozen_archive_stale_path_never_allocates_identity_or_masks_history() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","cwd":dir,"dir":dir}));
    let path = created["path"].as_str().unwrap();
    let before = call(&mut ts, json!({"op":"read","path":path}));
    let archived = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    assert!(!std::path::Path::new(path).exists());
    assert_eq!(
        read_session_header(path).map(|h| h.id),
        Some(created["id"].as_str().unwrap().to_owned())
    );
    assert!(!load_entries_from_file(path).is_empty());
    assert!(SessionManager::open(path, None, None).is_err());
    assert!(!std::path::Path::new(path).exists());
    assert_eq!(call(&mut ts, json!({"op":"read","path":archived})), before);
}
