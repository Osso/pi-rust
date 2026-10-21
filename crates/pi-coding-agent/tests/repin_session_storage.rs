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
    configure_worker();
    let before = peak_rss_kib();
    let manager = SessionManager::open_read_only(path.to_str().unwrap(), None).unwrap();
    assert_eq!(manager.get_entries().len(), 3);
    let delta = peak_rss_kib().saturating_sub(before);
    assert!(
        delta < 64 * 1024,
        "compacted inactive payload retention exceeded64MiB: {delta}KiB"
    );
}
#[test]
fn compacted_reverse_loading_does_not_retain_large_inactive_payloads() {
    configure_worker();
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
    configure_worker();
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
        read_session_header(archive).is_none(),
        "header must validate full archived stream"
    );
    assert!(SessionManager::open_read_only(path, None).is_err());
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
    use pi_coding_agent::core::session_manager::SessionManagerError;
    use pi_coding_agent::core::session_transcript_spool::with_seekable_session_transcript;
    use std::os::unix::fs::PermissionsExt;
    configure_worker();
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
    let result: Result<(), _> = with_seekable_session_transcript(path, |plain| {
        transient = Some(std::path::PathBuf::from(plain));
        assert_eq!(std::fs::metadata(plain).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read(plain).unwrap(), bytes);
        Err(SessionManagerError::new("callback failed"))
    });
    assert_eq!(result.unwrap_err().to_string(), "callback failed");
    assert!(!transient.unwrap().exists());
    assert!(!std::path::Path::new(path).exists());
    let compressed = std::fs::read(archived).unwrap();
    for corrupt in [vec![0, 1, 2, 3], compressed[..compressed.len() / 2].to_vec()] {
        std::fs::write(archived, &corrupt).unwrap();
        let before = transcript_temporaries();
        assert!(with_seekable_session_transcript(archived, |_| Ok(())).is_err());
        assert!(transcript_temporaries().is_subset(&before));
        assert!(!std::path::Path::new(path).exists());
    }
}

#[test]
fn storage_worker_hard90second_timeout_kills_and_reaps_before_cleanup() {
    use pi_coding_agent::core::session_transcript_spool::with_seekable_session_transcript;
    configure_worker();
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
    let result = with_seekable_session_transcript(fifo.to_str().unwrap(), |_| Ok(()));
    assert!(
        result
            .unwrap_err()
            .to_string()
            .starts_with("Timed out decompressing session transcript:")
    );
    assert!(start.elapsed() >= std::time::Duration::from_secs(90));
    assert!(start.elapsed() < std::time::Duration::from_secs(100));
    assert!(transcript_temporaries().is_subset(&before));
    let children = std::fs::read_to_string(format!("/proc/self/task/{}/children", std::process::id())).unwrap();
    for pid in children.split_whitespace() {
        if let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) {
            assert!(!String::from_utf8_lossy(&cmdline).contains("--internal-session-transcript-spool"));
        }
    }
}
