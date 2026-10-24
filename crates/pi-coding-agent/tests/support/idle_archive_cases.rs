use super::*;
use pi_coding_agent::core::session_archive_storage::{IdleSessionArchiveOptions, archive_idle_session_transcripts};
use pi_coding_agent::core::session_control_db::{WritableSessionMetadata, write_session_metadata};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

const NOW: f64 = 1_800_000_000_000.0;
const OLD: f64 = 1_600_000_000_000.0;
const AGE: f64 = 30.0 * 24.0 * 60.0 * 60.0 * 1000.0;

fn set_time(path: &std::path::Path, millis: f64) {
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs_f64(millis / 1000.0);
    std::fs::File::open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_accessed(time).set_modified(time))
        .unwrap();
}

fn seed(db: &JsString, file: &std::path::Path, id: &str, archived: bool) {
    let mut value = json!({"sessionPath":file,"id":id,"cwd":"/fixture-project","createdAt":"2020-01-01T00:00:00.000Z","modifiedAt":"2020-01-01T00:00:00.000Z","messageCount":1,"firstMessage":"archive fixture"});
    if archived {
        value["archivedAt"] = json!("2020-01-02T00:00:00.000Z");
    }
    let metadata: WritableSessionMetadata = serde_json::from_value(value).unwrap();
    write_session_metadata(db, &metadata).unwrap();
}

async fn run_native(
    root: &std::path::Path,
    kind: &str,
    content: &str,
    mtime: f64,
    input_archive: &serde_json::Value,
    guards: Option<&serde_json::Value>,
) -> serde_json::Value {
    let agent_dir = root.join("agent");
    let directory = agent_dir.join("sessions/project");
    let file = directory.join("case.jsonl");
    let archive = directory.join("case.jsonl.zst");
    for path in [&file, &archive] {
        if path.is_dir() {
            std::fs::remove_dir_all(path).unwrap();
        } else if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }
    let content = if kind == "header-resident" {
        content.replace("\"idle-old\"", "\"supervisor\"")
    } else {
        content.to_owned()
    };
    std::fs::write(&file, &content).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
    set_time(&file, mtime);
    let ignored_files = (kind == "layout").then(|| create_ignored_layout(root, &directory, &content, mtime));
    let db = JsString::from(root.join("native.db").to_str().unwrap());
    seed(&db, &root.join("schema-only.jsonl"), "schema-only", false);
    if kind == "resident" {
        seed(&db, &file, "supervisor", false);
    }
    if let Some(bytes) = input_archive.as_str() {
        std::fs::write(&archive, pi_js_compat::base64::buffer_from_base64(bytes)).unwrap();
        std::fs::set_permissions(&archive, std::fs::Permissions::from_mode(0o600)).unwrap();
        set_time(&archive, 1_650_000_000_000.0);
        if kind == "published-indexed" || kind == "changed" {
            seed(&db, &archive, "idle-old", true);
        }
    }
    if let Some(guards) = guards {
        if guards["seed"] == true {
            seed(&db, &file, "idle-old", false);
        }
        let connection = rusqlite::Connection::open(pi_js_compat::js_json::encode_node_utf8(&db)).unwrap();
        for row in guards["rows"].as_array().unwrap() {
            let parameters: Vec<rusqlite::types::Value> = row["params"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| match value {
                    serde_json::Value::String(text) => rusqlite::types::Value::Text(text.clone()),
                    serde_json::Value::Number(number) => rusqlite::types::Value::Integer(number.as_i64().unwrap()),
                    serde_json::Value::Null => rusqlite::types::Value::Null,
                    other => panic!("unsupported fixture SQL parameter {other}"),
                })
                .collect();
            connection
                .execute(row["sql"].as_str().unwrap(), rusqlite::params_from_iter(parameters))
                .unwrap();
        }
    }
    if kind == "archive-directory" {
        std::fs::create_dir(&archive).unwrap();
    }
    let descriptor = (kind == "open").then(|| std::fs::File::open(&file).unwrap());
    let options = IdleSessionArchiveOptions {
        agent_dir: Some(agent_dir.to_str().unwrap().into()),
        now: Some(NOW),
        current_session_path: (kind == "current").then(|| file.to_str().unwrap().into()),
    };
    let result = archive_idle_session_transcripts(&db, options).await.unwrap();
    drop(descriptor);
    let path = if archive.exists() { &archive } else { &file };
    let metadata = read_session_metadata(&db, JsString::from(path.to_str().unwrap())).unwrap().map(|metadata| {
        let mut value = json!({"id":metadata.id,"cwd":metadata.cwd,"messageCount":metadata.message_count,"firstMessage":metadata.first_message});
        if let Some(archived) = metadata.is_archived { value["isArchived"] = json!(archived); }
        value
    });
    let status = archive.is_file().then(|| std::fs::metadata(&archive).unwrap());
    let archive_bytes = archive.is_file().then(|| std::fs::read(&archive).unwrap());
    let decoded = archive_bytes
        .as_ref()
        .filter(|_| kind != "corrupt" && kind != "truncated")
        .map(|bytes| String::from_utf8(zstd::stream::decode_all(bytes.as_slice()).unwrap()).unwrap());
    let temps: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.contains(".archive-temp-"))
        .collect();
    let mut observation = json!({"ok":true,"result":{"archivedPaths":result.archived_paths,"errors":result.errors},"fileExists":file.exists(),"archiveExists":archive.exists(),"archiveBytes":archive_bytes.map(|bytes|pi_js_compat::base64::buffer_to_base64(&bytes)),"decoded":decoded,"metadata":metadata,"mode":status.as_ref().map(|metadata|metadata.mode() & 0o777),"mtime":status.as_ref().map(|metadata|pi_js_compat::number::js_number_value(metadata.mtime() as f64*1000.0+metadata.mtime_nsec() as f64/1_000_000.0)),"temps":temps});
    if let Some(paths) = ignored_files {
        observation["ignored"] = json!(
            paths
                .iter()
                .map(|path| json!([
                    std::fs::read_to_string(path).unwrap(),
                    path.with_file_name(format!("{}.zst", path.file_name().unwrap().to_str().unwrap()))
                        .exists()
                ]))
                .collect::<Vec<_>>()
        );
    }
    observation
}

fn create_ignored_layout(
    root: &std::path::Path,
    directory: &std::path::Path,
    content: &str,
    mtime: f64,
) -> Vec<std::path::PathBuf> {
    use std::os::unix::fs::symlink;
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(directory.join("nested")).unwrap();
    let files = vec![
        root.join("agent/sessions/root.jsonl"),
        directory.join("upper.JSONL"),
        directory.join("nested/deep.jsonl"),
        outside.join("external.jsonl"),
    ];
    for path in &files {
        std::fs::write(path, content).unwrap();
        set_time(path, mtime);
    }
    for (target, path) in [
        (outside.clone(), root.join("agent/sessions/linked-project")),
        (outside.join("external.jsonl"), directory.join("linked.jsonl")),
    ] {
        if path.symlink_metadata().is_ok() {
            std::fs::remove_file(&path).unwrap();
        }
        symlink(target, path).unwrap();
    }
    std::fs::create_dir_all(directory.join("directory.jsonl")).unwrap();
    files
}

#[tokio::test]
async fn archive_pass_matches_frozen_age_protection_publication_recovery_and_corrupt_frames() {
    let temp = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("idle-session-archive-pass-oracle.mts", "packages/coding-agent/src");
    let cases = [
        ("old", OLD),
        ("layout", OLD),
        ("fresh", NOW),
        ("cutoff", NOW - AGE),
        ("old", NOW - AGE - 1.0),
        ("old", OLD + 123.75),
        ("current", OLD),
        ("resident", OLD),
        ("header-resident", OLD),
        ("open", OLD),
        ("published", OLD),
        ("published-indexed", NOW),
        ("changed", OLD),
        ("corrupt", OLD),
    ];
    for (index, (kind, mtime)) in cases.into_iter().enumerate() {
        let root = temp.path().join(format!("{index}-{kind}"));
        std::fs::create_dir(&root).unwrap();
        let mut expected = source.request(&json!({"root":root,"kind":kind,"mtime":mtime,"now":NOW,"content":CONTENT}));
        assert_eq!(expected["ok"], true, "source {kind}: {expected}");
        let input_archive = expected.as_object_mut().unwrap().remove("inputArchive").unwrap();
        let actual = run_native(&root, kind, CONTENT, mtime, &input_archive, None).await;
        assert_eq!(actual, expected, "case {index} {kind}");
    }
}

// Frozen idle-session-archive.ts:149-165, 202-223: recovery compares exact
// bytes and original EOF, and reports read/decompression errors without deleting either input.
#[tokio::test]
async fn archive_pass_matches_frozen_comparison_and_archive_read_error_boundaries() {
    let temp = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("idle-session-archive-pass-oracle.mts", "packages/coding-agent/src");
    let large = CONTENT.replace("archive fixture", &"archive fixture ".repeat(20_000));
    for (index, (kind, content)) in [
        ("longer", CONTENT),
        ("same-length", CONTENT),
        ("archive-directory", CONTENT),
        ("published", large.as_str()),
        ("changed", large.as_str()),
        ("old", large.as_str()),
        ("old", "not a session\n"),
    ]
    .into_iter()
    .enumerate()
    {
        let root = temp.path().join(format!("{index}-{kind}"));
        std::fs::create_dir(&root).unwrap();
        let mut expected = source.request(&json!({"root":root,"kind":kind,"mtime":OLD,"now":NOW,"content":content}));
        assert_eq!(expected["ok"], true, "source {kind}: {expected}");
        let input_archive = expected.as_object_mut().unwrap().remove("inputArchive").unwrap();
        let actual = run_native(&root, kind, content, OLD, &input_archive, None).await;
        assert_eq!(actual, expected, "comparison {kind}");
    }
}

// Frozen :205 composes a decompressor without its own error listener. Its
// truncated-frame error escapes the pass catch and terminates the process.
#[tokio::test]
async fn truncated_archive_uses_the_frozen_uncaught_process_boundary() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    const CHILD: &str = "PI_IDLE_ARCHIVE_TRUNCATED_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        pi_coding_agent::utils::uncaught::install_uncaught_handler();
        let root = std::path::PathBuf::from(root);
        let error_record = root.join("uncaught.json");
        pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
            let error = payload
                .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
                .unwrap();
            let json = pi_js_compat::js_json::stringify_lossless(&error.value());
            std::fs::write(&error_record, json).unwrap();
        })));
        let bytes = std::fs::read(root.join("agent/sessions/project/case.jsonl.zst")).unwrap();
        let input = json!(pi_js_compat::base64::buffer_to_base64(&bytes));
        let content = std::fs::read_to_string(root.join("agent/sessions/project/case.jsonl")).unwrap();
        run_native(&root, "truncated", &content, OLD, &input, None).await;
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let mut child = oracle::spawn_oracle_process(
        &lock,
        "idle-session-archive-pass-oracle.mts",
        "packages/coding-agent/src",
        &[],
        Stdio::piped(),
    );
    let payload: String = (0..250_000).map(|index| format!("{index:08x}")).collect();
    let content = CONTENT.replace("archive fixture", &payload);
    let request = json!({"root":temp.path(),"kind":"truncated","mtime":OLD,"now":NOW,"content":content});
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let source = child.wait_with_output().unwrap();
    assert_eq!(source.status.code(), Some(1), "{source:?}");
    let stderr = String::from_utf8(source.stderr).unwrap();
    let error_line = stderr.lines().find(|line| line.starts_with("Error:")).unwrap();
    assert_eq!(error_line, "Error: unexpected end of file");
    assert!(stderr.contains("errno: -5"), "{stderr}");
    assert!(stderr.contains("code: 'Z_BUF_ERROR'"), "{stderr}");
    let native = Command::new(std::env::current_exe().unwrap())
        .current_dir(temp.path())
        .args([
            "--exact",
            "cases::truncated_archive_uses_the_frozen_uncaught_process_boundary",
            "--nocapture",
        ])
        .env(CHILD, temp.path())
        .output()
        .unwrap();
    assert_eq!(native.status.code(), source.status.code(), "{native:?}");
    assert_eq!(
        String::from_utf8(native.stderr).unwrap().lines().next(),
        Some(error_line)
    );
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("uncaught.json")).unwrap()).unwrap();
    assert_eq!(
        record,
        json!({"name":"Error","message":"unexpected end of file","code":"Z_BUF_ERROR","errno":-5})
    );
}

#[tokio::test]
async fn archive_pass_matches_frozen_live_database_guards_and_terminal_lifecycles() {
    let temp = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("idle-session-archive-pass-oracle.mts", "packages/coding-agent/src");
    let cases = [
        ("listener-live", "listener", "live", false),
        ("listener-dead", "listener", "dead", true),
        ("listener-empty-path", "listener", "empty", true),
        ("listener-unasserted-path", "listener", "unasserted", true),
        ("health-live", "health", "live", false),
        ("health-dead", "health", "dead", true),
        ("health-null", "health", "null", true),
        ("store-missing", "store", "{}", false),
        ("store-active", "store", r#"{"lifecycle":"running"}"#, false),
        ("store-completed", "store", r#"{"lifecycle":"completed"}"#, true),
        ("store-failed", "store", r#"{"lifecycle":"failed"}"#, true),
        ("store-aborted", "store", r#"{"lifecycle":"aborted"}"#, true),
        ("store-malformed", "store", "{", false),
        ("child-active", "child", "running", false),
        ("child-completed", "child", "completed", true),
        ("owner-identity", "owner", "identity", false),
        ("owner-empty-identity", "owner", "", false),
        ("owner-null", "owner", "null", true),
        ("relocation-rejected", "trigger", "", false),
    ];
    for (name, category, data, should_archive) in cases {
        let root = temp.path().join(name);
        std::fs::create_dir(&root).unwrap();
        let file = root.join("agent/sessions/project/case.jsonl");
        let pid = if data == "dead" {
            2_147_483_647u32
        } else {
            std::process::id()
        };
        let (sql, params) = match category {
            "listener" => (
                "INSERT INTO runtime_mailbox_listeners(recipient_session_id,recipient_agent_id_key,pid,session_path,session_path_asserted_at,updated_at) VALUES('fixture','',?,?,?,'2020-01-01')",
                json!([
                    pid,
                    if data == "empty" { json!("") } else { json!(file) },
                    if data == "unasserted" {
                        json!(null)
                    } else {
                        json!("2020-01-01")
                    }
                ]),
            ),
            "health" => (
                "INSERT INTO session_health(session_id,agent_generation,pid,check_status,updated_at) VALUES('idle-old',1,?,'ok','2020-01-01')",
                json!([if data == "null" { json!(null) } else { json!(pid) }]),
            ),
            "store" => (
                "INSERT INTO multi_agent_agents(session_path,agent_id,data,updated_at) VALUES(?,'child',?,'2020-01-01')",
                json!([file, data]),
            ),
            "child" => (
                "INSERT INTO multi_agent_agents(session_path,agent_id,data,updated_at) VALUES(?,'child',?,'2020-01-01')",
                json!([
                    root.join("store.jsonl"),
                    json!({"lifecycle":data,"transcript":{"path":file}}).to_string()
                ]),
            ),
            "owner" => (
                "INSERT INTO multi_agent_runtime_owners(session_path,agent_id,process_identity) VALUES(?,'child',?)",
                json!([file, if data == "null" { json!(null) } else { json!(data) }]),
            ),
            "trigger" => (
                "CREATE TRIGGER reject_archive BEFORE UPDATE OF archived_at ON session_metadata WHEN NEW.archived_at IS NOT NULL BEGIN SELECT RAISE(ABORT, 'fixture relocation rejected'); END",
                json!([]),
            ),
            _ => unreachable!(),
        };
        let guards = json!({"seed":true,"rows":[{"sql":sql,"params":params}]});
        let mut expected =
            source.request(&json!({"root":root,"kind":"old","mtime":OLD,"now":NOW,"content":CONTENT,"guards":guards}));
        assert_eq!(expected["ok"], true, "source {name}: {expected}");
        assert_eq!(
            expected["archiveExists"], should_archive,
            "source fixture {name}: {expected}"
        );
        let input_archive = expected.as_object_mut().unwrap().remove("inputArchive").unwrap();
        let actual = run_native(&root, "old", CONTENT, OLD, &input_archive, Some(&guards)).await;
        assert_eq!(actual, expected, "guard {name}");
    }
}
