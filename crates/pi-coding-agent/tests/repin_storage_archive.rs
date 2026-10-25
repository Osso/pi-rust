use super::repin_storage_history as oracle;

use pi_coding_agent::core::session_archive_storage::{
    archive_persisted_session, archive_session_file, restore_archived_session, restore_archived_session_file,
};
use pi_coding_agent::core::session_manager::SessionManagerError;
use pi_js_compat::abort::AbortReason;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn native_result(result: Result<JsString, SessionManagerError>) -> Value {
    match result {
        Ok(path) => json!({"ok":true,"path":path}),
        Err(error) => {
            let AbortReason::Error(error) = CallbackError::from(error).thrown_value() else {
                panic!("archive failure must be an Error");
            };
            json!({"ok":false,"error":error.value().to_serde().unwrap(),"constructor":error.constructor_class().name()})
        }
    }
}

fn compare(ts: &mut oracle::TsOracle, operation: &str, path: &str, db: &str) -> Value {
    let peer = ts.request(&json!({"id":1,"op":operation,"path":path,"db":db}));
    assert_eq!(peer["ok"], true, "{peer}");
    let result = match operation {
        "archiveSessionFile" => archive_session_file(&path.into()),
        "restoreArchivedSessionFile" => restore_archived_session_file(&path.into()),
        "archivePersistedSession" => archive_persisted_session(&db.into(), &path.into()),
        "restoreArchivedSession" => restore_archived_session(&db.into(), &path.into()),
        _ => panic!("unknown fixture operation"),
    };
    let actual = native_result(result);
    assert_eq!(actual, peer["output"], "{operation} {path}");
    actual
}

#[test]
fn archive_directory_read_errors_preserve_frozen_records() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-archive-oracle.mts", "packages/coding-agent/src/core");
    for (name, operation) in [
        ("directory.jsonl", "archiveSessionFile"),
        ("archive-directory.jsonl.zst", "restoreArchivedSessionFile"),
    ] {
        let path = tmp.path().join(name);
        std::fs::create_dir(&path).unwrap();
        let result = compare(&mut ts, operation, path.to_str().unwrap(), "");
        assert_eq!(result["ok"], false);
        assert!(path.exists(), "failed archive operations must preserve source");
        let target = if operation == "archiveSessionFile" {
            path.with_extension("jsonl.zst")
        } else {
            path.with_extension("")
        };
        assert!(!target.exists());
    }
}

#[test]
fn archive_corrupt_frame_preserves_frozen_codec_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("corrupt.jsonl.zst");
    std::fs::write(&path, b"not a zstd archive").unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-archive-oracle.mts", "packages/coding-agent/src/core");
    let result = compare(&mut ts, "restoreArchivedSessionFile", path.to_str().unwrap(), "");
    assert_eq!(result["ok"], false);
    assert_eq!(std::fs::read(&path).unwrap(), b"not a zstd archive");
    assert!(!path.with_extension("").exists());
}

#[test]
fn archive_short_circuits_missing_files_and_existing_targets_match_frozen() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("control.sqlite");
    let plain = tmp.path().join("session.jsonl");
    let archived = tmp.path().join("session.jsonl.zst");
    let mut ts = oracle::TsOracle::spawn("storage-archive-oracle.mts", "packages/coding-agent/src/core");
    for (operation, path) in [
        ("archiveSessionFile", &archived),
        ("restoreArchivedSessionFile", &plain),
        ("archiveSessionFile", &plain),
        ("restoreArchivedSessionFile", &archived),
        ("restoreArchivedSession", &plain),
        ("restoreArchivedSession", &archived),
    ] {
        compare(&mut ts, operation, path.to_str().unwrap(), db.to_str().unwrap());
    }
    std::fs::write(&plain, b"surviving plain").unwrap();
    assert_eq!(
        compare(&mut ts, "restoreArchivedSessionFile", archived.to_str().unwrap(), "")["path"],
        plain.to_str().unwrap()
    );
    std::fs::write(&archived, b"older archive").unwrap();
    for operation in ["archiveSessionFile", "restoreArchivedSessionFile"] {
        let path = if operation == "archiveSessionFile" {
            &plain
        } else {
            &archived
        };
        assert_eq!(compare(&mut ts, operation, path.to_str().unwrap(), "")["ok"], false);
    }
    assert_eq!(std::fs::read(&plain).unwrap(), b"surviving plain");
    assert_eq!(std::fs::read(&archived).unwrap(), b"older archive");
}

#[test]
fn archive_metadata_failure_rolls_back_and_preserves_original_database_error() {
    let tmp = tempfile::tempdir().unwrap();
    let bad_parent = tmp.path().join("not-a-directory");
    std::fs::write(&bad_parent, b"block directory creation").unwrap();
    let db = bad_parent.join("control.sqlite");
    let mut producer = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = producer.request(&json!({"id":1,"op":"create","dir":tmp.path(),"cwd":tmp.path()}));
    assert_eq!(created["ok"], true, "{created}");
    let path = std::path::PathBuf::from(created["output"]["path"].as_str().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    let mut ts = oracle::TsOracle::spawn("storage-archive-oracle.mts", "packages/coding-agent/src/core");
    let result = compare(
        &mut ts,
        "archivePersistedSession",
        path.to_str().unwrap(),
        db.to_str().unwrap(),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(!path.with_extension("jsonl.zst").exists());
    let archived = archive_session_file(&path.to_str().unwrap().into()).unwrap();
    let result = compare(
        &mut ts,
        "restoreArchivedSession",
        &pi_js_compat::js_json::encode_node_utf8(&archived),
        db.to_str().unwrap(),
    );
    assert_eq!(result["ok"], false);
    assert!(!path.exists());
    assert_eq!(
        pi_coding_agent::core::session_transcript::read_session_transcript_bytes(path.to_str().unwrap()).unwrap(),
        bytes
    );
}
