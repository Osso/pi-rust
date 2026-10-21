#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_manager::{SessionManager, load_entries_from_file, read_session_header};
use serde_json::{Value, json};
fn configure_worker() {
    static PACKAGE: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let package = PACKAGE.get_or_init(|| {
        let package = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_pi"), package.path().join("pi")).unwrap();
        std::fs::create_dir(package.path().join("theme")).unwrap();
        for name in ["dark.json", "light.json"] {
            std::fs::copy(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("theme")
                    .join(name),
                package.path().join("theme").join(name),
            )
            .unwrap();
        }
        package
    });
    pi_js_compat::process_env::set("PI_PACKAGE_DIR", package.path().to_str().unwrap());
}

fn call(ts: &mut oracle::TsOracle, mut request: Value) -> Value {
    request["id"] = json!(1);
    let response = ts.request(&request);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}
#[test]
fn frozen_archive_stale_path_never_allocates_identity_or_masks_history() {
    configure_worker();
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
    configure_worker();
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
    configure_worker();
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

#[test]
fn frozen_compacted_archive_history_follows_only_active_ancestors() {
    configure_worker();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir,"compacted":true}));
    let path = created["path"].as_str().unwrap();
    let expected = call(&mut ts, json!({"op":"read","path":path}));
    let archive = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let bytes = std::fs::read(archive.as_str().unwrap()).unwrap();
    let manager = SessionManager::open_read_only(path, None).unwrap();
    let entries = pi_coding_agent::core::session_history::read_session_history_reverse(&manager)
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
    configure_worker();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let db = tmp.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(&mut ts, json!({"op":"create","dir":dir,"cwd":dir}));
    let path = created["path"].as_str().unwrap();
    let archived = call(&mut ts, json!({"op":"archive","db":db,"path":path}));
    let bytes = std::fs::read(archived.as_str().unwrap()).unwrap();
    let out = tmp.path().join("export.html").display().to_string();
    export_from_file(path, Some(ExportOptions::from_output_path(&out)))
        .await
        .unwrap();
    let html = std::fs::read_to_string(&out).unwrap();
    let marker = "<script id=\"session-data\" type=\"application/json\">";
    let start = html.find(marker).unwrap() + marker.len();
    let end = start + html[start..].find("</script>").unwrap();
    let data: Value = serde_json::from_slice(&pi_js_compat::base64::atob(&html[start..end]).unwrap()).unwrap();
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
    export_from_file(path, Some(ExportOptions::from_output_path(out)))
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), bytes);
}

#[path = "support/tool_context.rs"]
mod tool_context;
#[tokio::test]
async fn persisted_history_tool_recovers_source_compacted_ancestor() {
    use pi_agent_core::AgentHarnessTool;
    use pi_coding_agent::core::tools::search_current_session_history::SearchCurrentSessionHistoryTool;
    use std::sync::{Arc, Mutex};
    configure_worker();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut ts = oracle::TsOracle::spawn("session-storage-oracle.ts", "packages/coding-agent/src/core");
    let created = call(
        &mut ts,
        json!({"op":"create","dir":dir,"cwd":dir,"compacted":true,"text":"needle older"}),
    );
    let path = created["path"].as_str().unwrap();
    let manager = Arc::new(Mutex::new(SessionManager::open(path, None, None).unwrap()));
    let context = tool_context::TestContextBuilder::new(&dir, manager).build().await.1;
    let result = SearchCurrentSessionHistoryTool::new()
        .execute("history".into(), json!({"query":"needle"}), None, None, context, None)
        .await
        .unwrap();
    assert_eq!(result.details.unwrap()["totalMatches"], 1);
}

#[test]
fn source_receipts_survive_retained_rewrite_with_omission_null_empty_and_raw_values() {
    configure_worker();
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
        let mut manager = SessionManager::open(path, None, None).unwrap();
        let compaction = manager
            .get_entries()
            .into_iter()
            .find(|e| e.entry_type() == "compaction")
            .unwrap();
        manager
            .update_compaction_summary(compaction.id().unwrap(), "rewritten")
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
    configure_worker();
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().display().to_string();
    let mut manager = SessionManager::create(&dir, Some(&dir), None).unwrap();
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
