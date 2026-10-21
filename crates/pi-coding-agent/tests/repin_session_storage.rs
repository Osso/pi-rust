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

#[test]
fn frozen_archived_fork_preserves_message_receipts() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let refs = json!([{"sessionPath":"/mail/canonical.jsonl","messageId":"mail-1"}]);
    let created = call(&mut ts, json!({"op":"create","cwd":dir,"dir":dir,"refs":refs}));
    let path = created["path"].as_str().unwrap();
    let original = call(&mut ts, json!({"op":"read","path":path}));
    call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let fork = SessionManager::fork_from(path, &dir, Some(&dir), None, None).expect("fork archived source");
    let read = call(&mut ts, json!({"op":"read","path":fork.get_session_file()}));
    assert_eq!(read["entries"][0]["mailboxStoreRefs"], refs);
    assert_eq!(read["entries"], original["entries"]);
    assert!(!std::path::Path::new(path).exists());
}

#[test]
fn rust_archive_restores_with_frozen_ts_and_exact_bytes() {
    use pi_coding_agent::core::session_archive_storage::{archive_persisted_session, restore_archived_session};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","cwd":dir,"dir":dir}));
    let path = created["path"].as_str().unwrap();
    let bytes = std::fs::read(path).unwrap();
    let archived = archive_persisted_session(&db, path).unwrap();
    assert_eq!(call(&mut ts, json!({"op":"read","path":path}))["id"], created["id"]);
    assert_eq!(call(&mut ts, json!({"op":"restore","db":db,"path":archived})), path);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    assert_eq!(restore_archived_session(&db, path).unwrap(), path);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}
