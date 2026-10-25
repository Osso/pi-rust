#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_manager::{SessionManager, load_entries_from_file, read_session_header};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
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
        read_session_header(&JsString::from(path)).map(|h| h.id().clone()),
        Some(JsString::from(created["id"].as_str().unwrap()))
    );
    assert!(!load_entries_from_file(&JsString::from(path)).unwrap().is_empty());
    assert!(SessionManager::open(&JsString::from(path), None, None).is_err());
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
    let fork = SessionManager::fork_from(
        &JsString::from(path),
        &JsString::from(&dir),
        Some(&JsString::from(&dir)),
        None,
        None,
    )
    .expect("fork archived source");
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
    let archived = archive_persisted_session(&JsString::from(&db), &JsString::from(path)).unwrap();
    assert_eq!(call(&mut ts, json!({"op":"read","path":path}))["id"], created["id"]);
    assert_eq!(call(&mut ts, json!({"op":"restore","db":db,"path":archived})), path);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    assert_eq!(
        restore_archived_session(&JsString::from(&db), &JsString::from(path)).unwrap(),
        path
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn frozen_compacted_archive_history_follows_only_active_ancestors() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir,"compacted":true}));
    let path = created["path"].as_str().unwrap();
    let expected = call(&mut ts, json!({"op":"read","path":path}));
    let archive = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let bytes = std::fs::read(archive.as_str().unwrap()).unwrap();
    let manager = SessionManager::open_read_only(&JsString::from(path), None).unwrap();
    let entries = pi_coding_agent::core::session_history::read_session_history_reverse(
        manager.get_branch(None),
        manager.get_session_file().cloned(),
    )
    .collect::<Result<Vec<_>, _>>()
    .unwrap();
    assert_eq!(serde_json::to_value(entries).unwrap(), expected["history"]);
    assert_eq!(
        serde_json::to_value(manager.get_entries()).unwrap(),
        expected["entries"]
    );
    assert_eq!(std::fs::read(archive.as_str().unwrap()).unwrap(), bytes);
    assert!(!std::path::Path::new(path).exists());
}

#[tokio::test]
async fn export_uses_read_only_stale_archive_and_legacy_source() {
    use pi_coding_agent::core::export_html::{ExportOptions, export_from_file};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir}));
    let path = created["path"].as_str().unwrap();
    let archived = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let bytes = std::fs::read(archived.as_str().unwrap()).unwrap();
    let out = tmp.path().join("export.html").display().to_string();
    export_from_file(&JsString::from(path), Some(ExportOptions::from_output_path(&out)))
        .await
        .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    let marker = "<script id=\"session-data\" type=\"application/json\">";
    let start = html.find(marker).unwrap() + marker.len();
    let end = start + html[start..].find("</script>").unwrap();
    let data: Value = serde_json::from_slice(&pi_js_compat::base64::atob(&html[start..end].into()).unwrap()).unwrap();
    assert_eq!(data["header"]["id"], created["id"]);
    assert_eq!(data["entries"][0]["message"]["content"], "hello 😀漢字");
    assert!(!std::path::Path::new(path).exists());
    assert_eq!(std::fs::read(archived.as_str().unwrap()).unwrap(), bytes);
    call(&mut ts, json!({"op":"restore","db":db,"path":path}));
    // Mutate only the version of a real source-created transcript to exercise migration.
    let bytes = std::fs::read_to_string(path)
        .unwrap()
        .replacen("\"version\":3", "\"version\":2", 1);
    std::fs::write(path, &bytes).unwrap();
    export_from_file(&JsString::from(path), Some(ExportOptions::from_output_path(out)))
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), bytes);
}

#[path = "support/tool_context.rs"]
mod tool_context;
#[tokio::test]
async fn persisted_history_tool_recovers_source_compacted_ancestor() {
    use pi_coding_agent::core::extensions::types::ContextTool;
    use pi_coding_agent::core::tools::search_current_session_history::SearchCurrentSessionHistoryTool;
    use std::sync::{Arc, Mutex};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"compacted":true,"text":"needle older"}),
    );
    let path = created["path"].as_str().unwrap();
    let manager = Arc::new(Mutex::new(
        SessionManager::open(&JsString::from(path), None, None).unwrap(),
    ));
    let context = tool_context::TestContextBuilder::new(&dir, manager).build().await.1;
    let result = SearchCurrentSessionHistoryTool::new()
        .execute(
            "history".into(),
            JsValue::from(json!({"query":"needle"})),
            None,
            None,
            Some(context),
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.details.unwrap()["totalMatches"], JsValue::Number(1.0));
}

#[test]
fn source_receipts_survive_retained_rewrite_with_omission_null_empty_and_raw_values() {
    for raw in [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!("malformed receipt")),
        Some(json!({"invalid":true})),
        Some(json!([{"sessionPath":"/mail","messageId":"accepted"}])),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().display().to_string();
        let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
        let mut request = json!({"op":"create","dir":dir,"cwd":dir,"compacted":true});
        if let Some(raw) = &raw {
            request["rawRefs"] = raw.clone();
        }
        let created = call(&mut ts, request);
        let path = created["path"].as_str().unwrap();
        let mut manager = SessionManager::open(&JsString::from(path), None, None).unwrap();
        let compaction = manager
            .get_entries()
            .into_iter()
            .find(|e| e.entry_type() == "compaction")
            .unwrap();
        manager
            .update_compaction_summary(compaction.id().unwrap(), &JsString::from("rewritten"))
            .unwrap();
        let read = call(&mut ts, json!({"op":"read","path":path}));
        for message in read["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["type"] == "message")
        {
            assert_eq!(message.get("mailboxStoreRefs"), raw.as_ref());
        }
    }
}

#[test]
fn rust_append_receipt_is_read_by_frozen_complete_record_acceptance_scan() {
    use pi_coding_agent::core::session_control_db::RuntimeMailboxStoreRef;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut manager = SessionManager::create(&JsString::from(&dir), Some(&JsString::from(&dir)), None).unwrap();
    let message = serde_json::from_value(json!({"role":"user","content":"mail payload","timestamp":1})).unwrap();
    let refs = [RuntimeMailboxStoreRef {
        session_path: "/canonical/mail.jsonl".into(),
        message_id: "mail-id".into(),
    }];
    manager
        .append_message_with_mailbox_store_refs(message, Some(&refs))
        .unwrap();
    manager.persist_for_recovery().unwrap();
    manager.fsync().unwrap();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    assert_eq!(
        call(&mut ts, json!({"op":"scan","path":manager.get_session_file()})),
        serde_json::to_value(refs).unwrap()
    );
}

fn peak_rss_kib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}
#[test]
fn storage_memory_probe_child() {
    let Some(path) = std::env::var_os("PI_STORAGE_MEMORY_PROBE") else {
        return;
    };
    let before = peak_rss_kib();
    let manager = SessionManager::open_read_only(&JsString::from(path.to_str().unwrap()), None).unwrap();
    assert_eq!(manager.get_entries().len(), 3);
    let delta = peak_rss_kib().saturating_sub(before);
    assert!(
        delta < 64 * 1024,
        "compacted inactive payload retention exceeded64MiB: {delta}KiB"
    );
}
#[test]
fn compacted_reverse_loading_does_not_retain_large_inactive_payloads() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"compacted":true,"inactiveCount":24,"largeInactive":true}),
    );
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "storage_memory_probe_child", "--nocapture"])
        .env("PI_STORAGE_MEMORY_PROBE", created["path"].as_str().unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn late_truncated_archive_header_does_not_hide_decode_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"text":"😀漢".repeat(30000)}),
    );
    let path = created["path"].as_str().unwrap();
    let archive = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let archive = archive.as_str().unwrap();
    let mut bytes = std::fs::read(archive).unwrap();
    bytes.truncate(bytes.len() - 2);
    std::fs::write(archive, &bytes).unwrap();
    assert_eq!(call(&mut ts, json!({"op":"header","path":archive})), Value::Null);
    assert!(
        read_session_header(&JsString::from(archive)).is_none(),
        "header must validate full archived stream"
    );
    assert!(SessionManager::open_read_only(&JsString::from(path), None).is_err());
    assert!(!std::path::Path::new(path).exists());
}

fn transcript_temporaries() -> std::collections::BTreeSet<std::path::PathBuf> {
    std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("pi-transcript-"))
        .map(|e| e.path())
        .collect()
}
#[test]
fn spool_callback_and_corruption_failures_cleanup_private_files() {
    // Observe only this probe's temporary root, not concurrent readers' spools.
    if std::env::var_os("PI_STORAGE_CLEANUP_CHILD").is_none() {
        let private = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "spool_callback_and_corruption_failures_cleanup_private_files",
                "--nocapture",
            ])
            .env("PI_STORAGE_CLEANUP_CHILD", "1")
            .env("TMPDIR", private.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use pi_coding_agent::core::session_transcript_spool::with_seekable_session_transcript;
    use pi_js_compat::callback_error::CallbackError;
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"text":"😀漢".repeat(50000)}),
    );
    let path = created["path"].as_str().unwrap();
    let bytes = std::fs::read(path).unwrap();
    let archived = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let archived = archived.as_str().unwrap();
    let mut transient = None;
    let result: Result<(), _> = with_seekable_session_transcript(&path.into(), |plain| {
        let plain = plain.to_utf8_lossy();
        transient = Some(std::path::PathBuf::from(&plain));
        assert_eq!(std::fs::metadata(&plain).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read(&plain).unwrap(), bytes);
        Err(CallbackError::new("callback failed"))
    });
    assert_eq!(result.unwrap_err().to_string(), "callback failed");
    assert!(!transient.unwrap().exists());
    assert!(!std::path::Path::new(path).exists());
    let compressed = std::fs::read(archived).unwrap();
    for corrupt in [vec![0, 1, 2, 3], compressed[..compressed.len() / 2].to_vec()] {
        std::fs::write(archived, &corrupt).unwrap();
        let before = transcript_temporaries();
        assert!(with_seekable_session_transcript(&archived.into(), |_| Ok(())).is_err());
        assert!(transcript_temporaries().is_subset(&before));
        assert!(!std::path::Path::new(path).exists());
    }
}

#[test]
fn storage_thread_90second_timeout_cleans_spool() {
    use pi_coding_agent::core::session_transcript_spool::with_seekable_session_transcript;
    let tmp = tempfile::tempdir().unwrap();
    let fifo = tmp.path().join("blocked.jsonl.zst");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let before = transcript_temporaries();
    let start = std::time::Instant::now();
    let result = with_seekable_session_transcript(&fifo.to_str().unwrap().into(), |_| Ok(()));
    assert_eq!(
        result.unwrap_err().to_string(),
        format!("Timed out decompressing session transcript: {}", fifo.display())
    );
    assert!(start.elapsed() >= std::time::Duration::from_secs(90));
    assert!(start.elapsed() < std::time::Duration::from_secs(100));
    assert!(transcript_temporaries().is_subset(&before));
    // Release the injected blocked open; no test-owned syscall remains blocked.
    drop(std::fs::OpenOptions::new().write(true).open(&fifo).unwrap());
}

#[test]
fn fallible_full_loader_and_context_match_frozen_archive_preconditions() {
    use pi_coding_agent::core::session_manager::load_session_context_entries;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir,"compacted":true}));
    let path = created["path"].as_str().unwrap();
    let archive = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let expected = call(&mut ts, json!({"op":"context","path":archive}));
    let loaded = load_session_context_entries(&JsString::from(archive.as_str().unwrap())).unwrap();
    assert_eq!(serde_json::to_value(loaded.header).unwrap(), expected["header"]);
    assert_eq!(serde_json::to_value(loaded.entries).unwrap(), expected["entries"]);
    assert_eq!(ts.request(&json!({"id":1,"op":"context","path":path}))["ok"], false);
    assert!(load_session_context_entries(&JsString::from(path)).is_err());
    assert_eq!(
        serde_json::to_value(load_entries_from_file(&JsString::from(path)).unwrap()).unwrap(),
        call(&mut ts, json!({"op":"load","path":path}))
    );
    std::fs::write(archive.as_str().unwrap(), [0, 1, 2, 3]).unwrap();
    assert_eq!(ts.request(&json!({"id":1,"op":"load","path":path}))["ok"], false);
    assert!(load_entries_from_file(&JsString::from(path)).is_err());
}

#[test]
fn archive_restore_modes_metadata_rollback_and_surviving_plain_precedence() {
    use pi_coding_agent::core::session_archive_storage::{
        archive_persisted_session, archive_session_file, restore_archived_session, restore_archived_session_file,
    };
    use pi_coding_agent::core::session_control_db::read_session_metadata;
    use pi_coding_agent::core::session_transcript::read_session_transcript_bytes;
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let bad_db = tmp.path().join("db-is-directory");
    std::fs::create_dir(&bad_db).unwrap();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir,"db":db}));
    let path = created["path"].as_str().unwrap();
    let bytes = std::fs::read(path).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(archive_persisted_session(&JsString::from(bad_db.to_str().unwrap()), &JsString::from(path)).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert!(!std::path::Path::new(&format!("{path}.zst")).exists());
    let archive = archive_persisted_session(&JsString::from(&db), &JsString::from(path)).unwrap();
    assert_eq!(
        std::fs::metadata(encode_node_utf8(&archive))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert_eq!(read_session_transcript_bytes(path).unwrap(), bytes);
    assert!(read_session_metadata(&db, path).unwrap().is_none());
    let metadata = call(&mut ts, json!({"op":"metadata","db":db,"path":archive}));
    assert_eq!(metadata["id"], created["id"]);
    assert!(metadata["archivedAt"].is_string());
    assert!(restore_archived_session(&JsString::from(bad_db.to_str().unwrap()), &archive).is_err());
    assert!(!std::path::Path::new(path).exists());
    assert_eq!(read_session_transcript_bytes(&archive).unwrap(), bytes);
    let archived_bytes = std::fs::read(encode_node_utf8(&archive)).unwrap();
    restore_archived_session(&JsString::from(&db), &archive).unwrap();
    assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o640);
    assert!(call(&mut ts, json!({"op":"metadata","db":db,"path":path}))["archivedAt"].is_null());
    let mut manager = SessionManager::open(&JsString::from(path), None, None).unwrap();
    manager
        .append_message(
            serde_json::from_value(json!({"role":"user","content":"newer than archive","timestamp":7})).unwrap(),
        )
        .unwrap();
    let newer = std::fs::read(path).unwrap();
    std::fs::write(encode_node_utf8(&archive), &archived_bytes).unwrap();
    assert_eq!(read_session_transcript_bytes(path).unwrap(), newer);
    assert!(archive_session_file(&JsString::from(path)).is_err());
    assert!(restore_archived_session_file(&archive).is_err());
    assert_eq!(restore_archived_session(&JsString::from(&db), &archive).unwrap(), path);
    assert_eq!(std::fs::read(path).unwrap(), newer);
    assert!(!std::path::Path::new(&encode_node_utf8(&archive)).exists());
    assert!(
        std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(Result::ok)
            .all(|e| !e.file_name().to_string_lossy().contains(".archive-temp-"))
    );
}

#[test]
fn unknown_legacy_summary_usage_survives_without_a_typed_accounting_contract() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"compacted":true,"legacyUsage":"legacy raw payload"}),
    );
    let path = created["path"].as_str().unwrap();
    let mut manager = SessionManager::open(&JsString::from(path), None, None).unwrap();
    let compaction = manager
        .get_entries()
        .into_iter()
        .find(|e| e.entry_type() == "compaction")
        .unwrap();
    manager
        .update_compaction_summary(compaction.id().unwrap(), &JsString::from("new summary"))
        .unwrap();
    let read = call(&mut ts, json!({"op":"read","path":path}));
    let entry = read["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "compaction")
        .unwrap();
    assert_eq!(entry["usage"], "legacy raw payload");
    assert_eq!(entry["summary"], "new summary");
}

#[test]
fn source_escaped_surrogate_receipt_remains_a_persisted_message() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"splitSurrogate":true}),
    );
    let path = created["path"].as_str().unwrap();
    assert_eq!(
        call(&mut ts, json!({"op":"receiptTokens","path":path})),
        json!(r#"[{"sessionPath":"\ud800","messageId":"surrogate"}]"#)
    );
    assert_eq!(
        load_entries_from_file(&JsString::from(path)).unwrap().len(),
        2,
        "source-valid escaped receipt must not drop the message"
    );
    assert_eq!(
        pi_coding_agent::core::session_manager::parse_session_entries(std::fs::read_to_string(path).unwrap()).len(),
        2
    );
    let mut manager = SessionManager::open(&JsString::from(path), None, None).unwrap();
    let first = manager.get_entries()[0].id().unwrap().to_owned();
    let id = manager
        .append_compaction(&JsString::from("summary"), &first, 1.0, Default::default())
        .unwrap();
    manager
        .update_compaction_summary(&id, &JsString::from("rewritten"))
        .unwrap();
    let expected = call(&mut ts, json!({"op":"receiptTokens","path":path}));
    let fork = SessionManager::fork_from(
        &JsString::from(path),
        &JsString::from(&dir),
        Some(&JsString::from(&dir)),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        call(&mut ts, json!({"op":"receiptTokens","path":fork.get_session_file()})),
        expected
    );
    assert_eq!(expected, json!(r#"[{"sessionPath":"\ud800","messageId":"surrogate"}]"#));
}

#[test]
fn mutable_open_rejects_all_archived_suffixes_before_allocating_missing_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("arbitrary.zst");
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let source = ts.request(&json!({"id":1,"op":"mutable","path":path}));
    assert_eq!(source["ok"], false);
    assert!(SessionManager::open(&JsString::from(path.to_str().unwrap()), None, None).is_err());
    assert!(!path.exists());
}

#[test]
fn uncompressed_inactive_tree_and_archived_list_context_are_source_equivalent() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir,"branched":true}));
    let path = created["path"].as_str().unwrap();
    let expected = call(&mut ts, json!({"op":"read","path":path}));
    for archived in [false, true] {
        if archived {
            call(&mut ts, json!({"op":"archive","db":db,"path":path}));
        }
        let manager = SessionManager::open_read_only(&JsString::from(path), None).unwrap();
        assert_eq!(
            serde_json::to_value(manager.get_entries()).unwrap(),
            expected["entries"]
        );
        assert_eq!(serde_json::to_value(manager.get_tree()).unwrap(), expected["tree"]);
        assert_eq!(
            serde_json::to_value(manager.build_session_context().messages).unwrap(),
            expected["context"]
        );
        let resolved = pi_coding_agent::core::session_transcript::read_session_transcript_path(path);
        let info = pi_coding_agent::core::session_manager::build_session_info(&resolved).unwrap();
        assert_eq!(
            json!({"path":info.path,"id":info.id,"cwd":info.cwd,"messageCount":info.message_count,"firstMessage":info.first_message}),
            call(&mut ts, json!({"op":"info","path":resolved}))
        );
        let actual=SessionManager::list(&JsString::from(&dir),Some(&JsString::from(&dir)),None,None).unwrap().into_iter().map(|s|json!({"path":s.path,"id":s.id,"cwd":s.cwd,"messageCount":s.message_count,"firstMessage":s.first_message})).collect::<Vec<_>>();
        assert_eq!(json!(actual), call(&mut ts, json!({"op":"list","cwd":dir,"dir":dir})));
    }
    assert!(
        pi_coding_agent::core::session_manager::find_most_recent_session(&JsString::from(&dir), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn unknown_summary_unpaired_keys_and_values_survive_rewrite_and_fork() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"compacted":true,"splitExtra":true}),
    );
    let path = created["path"].as_str().unwrap();
    let expected = call(&mut ts, json!({"op":"extraTokens","path":path}));
    let mut manager = SessionManager::open(&JsString::from(path), None, None).unwrap();
    let entry = manager
        .get_entries()
        .into_iter()
        .find(|e| e.entry_type() == "compaction")
        .unwrap();
    manager
        .update_compaction_summary(entry.id().unwrap(), &JsString::from("rewritten"))
        .unwrap();
    assert_eq!(call(&mut ts, json!({"op":"extraTokens","path":path})), expected);
    let fork = SessionManager::fork_from(
        &JsString::from(path),
        &JsString::from(&dir),
        Some(&JsString::from(&dir)),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        call(&mut ts, json!({"op":"extraTokens","path":fork.get_session_file()})),
        expected
    );
}
#[tokio::test]
async fn escaped_receipt_html_export_retains_source_raw_utf16_tokens() {
    use pi_coding_agent::core::export_html::{ExportOptions, export_from_file};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"splitSurrogate":true}),
    );
    let path = created["path"].as_str().unwrap();
    let expected = call(&mut ts, json!({"op":"receiptTokens","path":path}));
    call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let output = tmp.path().join("raw.html").display().to_string();
    export_from_file(&JsString::from(path), Some(ExportOptions::from_output_path(&output)))
        .await
        .unwrap();
    assert_eq!(
        call(&mut ts, json!({"op":"exportReceiptTokens","path":output})),
        expected
    );
    assert!(!std::path::Path::new(path).exists());
}

#[test]
fn large_unicode_compacted_history_streaming_and_early_drop_cleanup() {
    use pi_coding_agent::core::session_history::read_session_history_reverse;
    use pi_coding_agent::core::session_transcript::{
        create_session_transcript_read_stream, read_session_transcript_text,
    };
    use std::io::{Read, Write};
    if std::env::var_os("PI_STORAGE_HISTORY_CHILD").is_none() {
        let private = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "large_unicode_compacted_history_streaming_and_early_drop_cleanup",
                "--nocapture",
            ])
            .env("PI_STORAGE_HISTORY_CHILD", "1")
            .env("TMPDIR", private.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"compacted":true,"text":"漢😀é".repeat(150000)}),
    );
    let path = created["path"].as_str().unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(b"{\"type\":\"message\"")
        .unwrap();
    let expected = call(&mut ts, json!({"op":"read","path":path}));
    let original = std::fs::read(path).unwrap();
    let archive = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let archive = archive.as_str().unwrap();
    let compressed = std::fs::read(archive).unwrap();
    let mut stream = create_session_transcript_read_stream(path).unwrap();
    let mut actual = Vec::new();
    let mut buffer = [0; 13];
    loop {
        let n = stream.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        actual.extend_from_slice(&buffer[..n]);
    }
    assert_eq!(actual, original);
    assert_eq!(read_session_transcript_text(path).unwrap().as_bytes(), original);
    let manager = SessionManager::open_read_only(&JsString::from(path), None).unwrap();
    let history = read_session_history_reverse(manager.get_branch(None), manager.get_session_file().cloned())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(serde_json::to_value(history).unwrap(), expected["history"]);
    let mut history = read_session_history_reverse(manager.get_branch(None), manager.get_session_file().cloned());
    for _ in 0..4 {
        assert!(history.next().unwrap().is_ok());
    }
    drop(history);
    assert!(
        std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(Result::ok)
            .all(|e| !e.file_name().to_string_lossy().starts_with("pi-history-"))
    );
    assert_eq!(std::fs::read(archive).unwrap(), compressed);
    assert!(!std::path::Path::new(path).exists());
}

#[test]
fn frozen_transcript_exports_match_stream_frame_and_utf8_error_boundaries() {
    use pi_coding_agent::core::session_transcript::*;
    use std::io::Read;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"text":"😀漢é".repeat(10000)}),
    );
    let path = created["path"].as_str().unwrap();
    let original = std::fs::read(path).unwrap();
    let archive = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let archive = archive.as_str().unwrap();
    let compressed = std::fs::read(archive).unwrap();
    let mut mismatches = Vec::new();
    for (case, bytes) in [
        ("valid", compressed.clone()),
        ("concatenated", [compressed.clone(), compressed.clone()].concat()),
        ("trailing-garbage", [compressed.clone(), b"garbage".to_vec()].concat()),
        ("truncated-tail", compressed[..compressed.len() - 1].to_vec()),
        ("bad-magic", vec![0, 1, 2, 3]),
    ] {
        std::fs::write(archive, &bytes).unwrap();
        let source = ts.request(&json!({"id":1,"op":"transcript","path":path,"stream":false}));
        let rust = read_session_transcript_bytes(path);
        if rust.is_ok() != (source["ok"] == true) {
            mismatches.push(format!(
                "case={case}, Rust error={:?}, TS ok={}, TS error={}",
                rust.as_ref().err(),
                source["ok"],
                source["error"]
            ));
            continue;
        }
        if let Ok(bytes) = rust {
            let source = &source["output"];
            assert_eq!(
                JsValue::String(read_session_transcript_path(path)),
                JsValue::from(source["path"].clone())
            );
            assert_eq!(is_session_transcript_file(path), source["isTranscript"]);
            assert_eq!(pi_js_compat::base64::btoa(&bytes), source["bytes"]);
            assert_eq!(read_session_transcript_text(path).unwrap(), source["text"]);
            let mut streamed = Vec::new();
            create_session_transcript_read_stream(path)
                .unwrap()
                .read_to_end(&mut streamed)
                .unwrap();
            assert_eq!(
                String::from_utf8_lossy(&streamed).as_ref(),
                source["text"].as_str().unwrap()
            );
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    // Existing plain wins even over the last corrupt compressed sibling; replacement UTF8 follows Node.
    let mut plain = original;
    plain.extend_from_slice(&[0xf0, 0x9f, 0xff, 0x80]);
    std::fs::write(path, &plain).unwrap();
    let source = call(&mut ts, json!({"op":"transcript","path":path}));
    assert_eq!(read_session_transcript_text(path).unwrap(), source["streamed"]);
    for name in ["plain.jsonl", "archive.jsonl.zst", "ordinary.zst", "other.json"] {
        let fixture = tmp.path().join(name);
        std::fs::write(&fixture, b"plain text").unwrap();
        let result = ts.request(&json!({"id":1,"op":"transcript","path":fixture}));
        if result["ok"] == true {
            assert_eq!(
                is_session_transcript_file(fixture.to_str().unwrap()),
                result["output"]["isTranscript"]
            );
        } else {
            assert!(read_session_transcript_bytes(fixture.to_str().unwrap()).is_err());
        }
    }
}

#[test]
fn frozen_storage_suffix_noops_missing_and_metadata_surviving_plain_branches() {
    use pi_coding_agent::core::session_archive_storage::*;
    use pi_coding_agent::core::session_control_db::read_session_metadata;
    use pi_coding_agent::core::session_transcript::*;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let paths = ["a.jsonl", "a.jsonl.zst", "a.zst", "file.jsonl.zst.tmp", "a.JSONL", ""];
    let expected = call(&mut ts, json!({"op":"predicates","paths":paths}));
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(
            json!({"transcript":is_session_transcript_file(*path),"archive":is_archived_session_file(&JsString::from(*path)),"resolved":read_session_transcript_path(*path)}),
            expected[index]
        );
    }
    let missing = tmp.path().join("missing.jsonl").display().to_string();
    let archived = format!("{missing}.zst");
    assert_eq!(
        JsValue::String(archive_session_file(&JsString::from(&archived)).unwrap()),
        JsValue::from(call(&mut ts, json!({"op":"fileArchive","path":archived})))
    );
    assert_eq!(
        JsValue::String(restore_archived_session_file(&JsString::from(&missing)).unwrap()),
        JsValue::from(call(&mut ts, json!({"op":"fileRestore","path":missing})))
    );
    assert!(restore_archived_session_file(&JsString::from(&archived)).is_err());
    assert_eq!(
        ts.request(&json!({"id":1,"op":"fileRestore","path":archived}))["ok"],
        false
    );
    assert_eq!(
        restore_archived_session(&JsString::from(&db), &JsString::from(&archived)).unwrap(),
        missing
    );
    assert_eq!(call(&mut ts, json!({"op":"restore","db":db,"path":archived})), missing);
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir,"db":db}));
    let path = created["path"].as_str().unwrap();
    let archive = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let archive = archive.as_str().unwrap();
    let compressed = std::fs::read(archive).unwrap();
    restore_archived_session_file(&JsString::from(archive)).unwrap();
    let mut manager = SessionManager::open(&JsString::from(path), None, None).unwrap();
    manager
        .append_message(
            serde_json::from_value(json!({"role":"user","content":"surviving publication","timestamp":3})).unwrap(),
        )
        .unwrap();
    let newer = std::fs::read(path).unwrap();
    std::fs::write(archive, compressed).unwrap();
    assert!(read_session_metadata(&db, archive).unwrap().is_some());
    assert_eq!(
        restore_archived_session(&JsString::from(&db), &JsString::from(archive)).unwrap(),
        path
    );
    assert!(read_session_metadata(&db, archive).unwrap().is_none());
    let metadata = call(&mut ts, json!({"op":"metadata","db":db,"path":path}));
    assert_eq!(metadata["id"], created["id"]);
    assert!(metadata["archivedAt"].is_null());
    assert_eq!(std::fs::read(path).unwrap(), newer);
    assert!(!std::path::Path::new(archive).exists());
    assert_eq!(restore_archived_session_file(&JsString::from(archive)).unwrap(), path);
}

#[test]
fn metadata_filtered_empty_does_not_fall_back_or_cache_unindexed_files() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let known = tmp.path().join("known").display().to_string();
    let other = tmp.path().join("other").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    call(&mut ts, json!({"op":"create","dir":known,"cwd":known,"db":db}));
    let unindexed = call(&mut ts, json!({"op":"create","dir":other,"cwd":other}));
    assert_eq!(
        call(&mut ts, json!({"op":"list","dir":other,"cwd":other,"db":db})),
        json!([])
    );
    let listed = SessionManager::list(
        &JsString::from(&other),
        Some(&JsString::from(&other)),
        None,
        Some(&JsString::from(&db)),
    )
    .unwrap();
    assert!(
        listed.is_empty(),
        "nonempty metadata table with empty scoped result must suppress filesystem discovery"
    );
    assert_eq!(
        call(&mut ts, json!({"op":"metadata","db":db,"path":unindexed["path"]})),
        Value::Null
    );
}
#[test]
fn resident_sessions_are_excluded_from_ordinary_filesystem_listing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    for (index, (id, segment, resident)) in [
        ("supervisor", "plain", true),
        ("architect", "plain", true),
        ("ordinary", "supervisor-sessions", true),
        ("ordinary", "architect-sessions", true),
        ("ordinary", "root\\architect-sessions\\nested", true),
        ("ordinary", "root\\supervisor-sessions\\nested", true),
        ("ordinary", "not-supervisor-sessions", false),
        ("ordinary", "architect-sessions-suffix", false),
        ("Supervisor", "plain", false),
    ]
    .into_iter()
    .enumerate()
    {
        let dir = tmp.path().join(index.to_string()).join(segment).display().to_string();
        let db = tmp.path().join(format!("{index}.sqlite")).display().to_string();
        call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir,"options":{"id":id}}));
        let expected = call(&mut ts, json!({"op":"list","dir":dir,"cwd":dir}));
        assert_eq!(expected.as_array().unwrap().is_empty(), resident, "{segment} {id}");
        for control in [None, Some(db.as_str())] {
            let actual = SessionManager::list(
                &JsString::from(&dir),
                Some(&JsString::from(&dir)),
                None,
                control.map(JsString::from).as_ref(),
            )
            .unwrap();
            assert_eq!(actual.is_empty(), resident, "{segment} {id} {control:?}");
            // Second read exercises populated metadata, not filesystem fallback.
            let again = SessionManager::list(
                &JsString::from(&dir),
                Some(&JsString::from(&dir)),
                None,
                control.map(JsString::from).as_ref(),
            )
            .unwrap();
            assert_eq!(again.is_empty(), resident, "cached {segment} {id}");
        }
    }
}

#[test]
fn source_tool_result_raw_receipt_survives_storage_truncation_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"toolResult":true,"splitSurrogate":true}),
    );
    let path = created["path"].as_str().unwrap();
    let expected = call(&mut ts, json!({"op":"receiptTokens","path":path}));
    let mut manager = SessionManager::open(&JsString::from(path), None, None).unwrap();
    let first = manager.get_entries()[0].id().unwrap().to_owned();
    let id = manager
        .append_compaction(&JsString::from("summary"), &first, 1.0, Default::default())
        .unwrap();
    manager
        .update_compaction_summary(&id, &JsString::from("rewritten"))
        .unwrap();
    assert_eq!(call(&mut ts, json!({"op":"receiptTokens","path":path})), expected);
    let entries = load_entries_from_file(&JsString::from(path)).unwrap();
    assert_eq!(
        entries[1].entry().unwrap().as_message().unwrap().message.role(),
        "toolResult"
    );
    let fork = SessionManager::fork_from(
        &JsString::from(path),
        &JsString::from(&dir),
        Some(&JsString::from(&dir)),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        call(&mut ts, json!({"op":"receiptTokens","path":fork.get_session_file()})),
        expected
    );
}

#[test]
fn source_legacy_v1_read_only_migrates_context_without_mutating_original() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir,"legacy1":true}));
    let path = created["path"].as_str().unwrap();
    let bytes = std::fs::read(path).unwrap();
    let expected = call(&mut ts, json!({"op":"read","path":path}));
    let manager = SessionManager::open_read_only(&JsString::from(path), None).unwrap();
    assert_eq!(
        serde_json::to_value(manager.build_session_context().messages).unwrap(),
        expected["context"]
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(manager.get_header().unwrap().version(), Some(3.0));
}

#[tokio::test]
async fn legacy_v1_raw_receipt_survives_in_memory_migration_and_export() {
    use pi_coding_agent::core::export_html::{ExportOptions, export_from_file};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"legacy1":true,"splitSurrogate":true}),
    );
    let path = created["path"].as_str().unwrap();
    let before = std::fs::read(path).unwrap();
    let expected = call(&mut ts, json!({"op":"receiptTokens","path":path}));
    let output = tmp.path().join("legacy-raw.html").display().to_string();
    export_from_file(&JsString::from(path), Some(ExportOptions::from_output_path(&output)))
        .await
        .unwrap();
    assert_eq!(
        call(&mut ts, json!({"op":"exportReceiptTokens","path":output})),
        expected
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn source_lossless_tool_migration_preserves_receipt_summary_and_classifications() {
    use pi_coding_agent::core::session_tool_output::migrate_tool_result_session_files;
    let tmp = tempfile::tempdir().unwrap();
    let rust_dir = tmp.path().join("rust");
    let source_dir = tmp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":rust_dir,"cwd":"/fixture","toolResult":true,"compacted":true,"splitSurrogate":true,"splitExtra":true,"oversizedRaw":true}),
    );
    let path = created["path"].as_str().unwrap();
    let before = std::fs::read(path).unwrap();
    let source_path = source_dir.join("fixture.jsonl");
    std::fs::write(&source_path, &before).unwrap();
    for dir in [&rust_dir, &source_dir] {
        std::fs::write(dir.join("malformed.jsonl"), "{broken\n").unwrap();
        std::fs::write(dir.join("non-session.jsonl"), "{}\n").unwrap();
    }
    let expected = call(&mut ts, json!({"op":"migrateTools","dir":source_dir}));
    assert_eq!(expected["changedFiles"], 1);
    assert_eq!(expected["skippedMalformedFiles"], 1);
    assert_eq!(expected["skippedNonSessionFiles"], 1);
    let report = migrate_tool_result_session_files(&rust_dir.to_str().unwrap().into(), || Ok(1792584000000.0)).unwrap();
    assert_eq!(report.changed_files, 1, "{report:?}");
    assert_eq!(report.truncated_messages, 1);
    assert_eq!(report.scanned_files, 3);
    assert_eq!(report.skipped_malformed_files, 1);
    assert_eq!(report.skipped_non_session_files, 1);
    assert_eq!(report.skipped_error_files, 0);
    assert_eq!(
        std::fs::read(pi_js_compat::js_json::encode_node_utf8(&report.backup_paths[0])).unwrap(),
        before
    );
    for op in ["receiptTokens", "extraTokens"] {
        assert_eq!(
            call(&mut ts, json!({"op":op,"path":path})),
            call(&mut ts, json!({"op":op,"path":source_path}))
        );
    }
    assert_eq!(
        call(&mut ts, json!({"op":"entriesEqual","left":path,"right":source_path})),
        true,
        "frozen source must read semantically identical entries including raw UTF16 fields"
    );
    assert_eq!(
        migrate_tool_result_session_files(&rust_dir.to_str().unwrap().into(), || Ok(1792584000000.0))
            .unwrap()
            .changed_files,
        0
    );
}

#[test]
fn tool_migration_classifies_empty_ids_js_blank_lines_and_read_errors_like_source() {
    use pi_coding_agent::core::session_tool_output::migrate_tool_result_session_files;
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let rust_dir = tmp.path().join("rust");
    let source_dir = tmp.path().join("source");
    for dir in [&rust_dir, &source_dir] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("empty-id.jsonl"), "{\"type\":\"session\",\"id\":\"\"}\n").unwrap();
        std::fs::write(
            dir.join("bom-blank.jsonl"),
            "{\"type\":\"session\",\"id\":\"s\"}\n\u{feff}\n",
        )
        .unwrap();
        let unreadable = dir.join("unreadable.jsonl");
        std::fs::write(&unreadable, "{}\n").unwrap();
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(std::fs::read(&unreadable).is_err(), "fixture must be unreadable");
    }
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let expected = call(&mut ts, json!({"op":"migrateTools","dir":source_dir}));
    let report = migrate_tool_result_session_files(&rust_dir.to_str().unwrap().into(), || Ok(1792584000000.0)).unwrap();
    let source_prefix = source_dir.to_str().unwrap();
    let rust_prefix = rust_dir.to_str().unwrap();
    let rust_errors: Vec<String> = report
        .errors
        .iter()
        .map(|error| error.to_utf8().unwrap().replace(rust_prefix, source_prefix))
        .collect();
    assert_eq!(
        json!({
            "scannedFiles": report.scanned_files,
            "changedFiles": report.changed_files,
            "truncatedMessages": report.truncated_messages,
            "skippedMalformedFiles": report.skipped_malformed_files,
            "skippedNonSessionFiles": report.skipped_non_session_files,
            "skippedErrorFiles": report.skipped_error_files,
            "backupPaths": report.backup_paths.len(),
            "errors": rust_errors,
        }),
        json!({
            "scannedFiles": expected["scannedFiles"],
            "changedFiles": expected["changedFiles"],
            "truncatedMessages": expected["truncatedMessages"],
            "skippedMalformedFiles": expected["skippedMalformedFiles"],
            "skippedNonSessionFiles": expected["skippedNonSessionFiles"],
            "skippedErrorFiles": expected["skippedErrorFiles"],
            "backupPaths": expected["backupPaths"].as_array().unwrap().len(),
            "errors": expected["errors"],
        })
    );
    assert_eq!(report.skipped_error_files, 1, "{report:?}");
}

#[tokio::test]
async fn legacy_v1_raw_summary_extras_survive_migration_and_export() {
    use pi_coding_agent::core::export_html::{ExportOptions, export_from_file};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"legacy1":true,"compacted":true,"splitExtra":true}),
    );
    let path = created["path"].as_str().unwrap();
    let before = std::fs::read(path).unwrap();
    let expected = call(&mut ts, json!({"op":"extraTokens","path":path}));
    let output = tmp.path().join("legacy-summary.html").display().to_string();
    export_from_file(&JsString::from(path), Some(ExportOptions::from_output_path(&output)))
        .await
        .unwrap();
    assert_eq!(call(&mut ts, json!({"op":"exportExtraTokens","path":output})), expected);
    assert_eq!(std::fs::read(path).unwrap(), before);
}
