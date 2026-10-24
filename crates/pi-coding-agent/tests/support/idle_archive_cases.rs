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
    mtime: f64,
    input_archive: &serde_json::Value,
) -> serde_json::Value {
    let agent_dir = root.join("agent");
    let directory = agent_dir.join("sessions/project");
    let file = directory.join("case.jsonl");
    let archive = directory.join("case.jsonl.zst");
    for path in [&file, &archive] {
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
    }
    let content = if kind == "header-resident" {
        CONTENT.replace("\"idle-old\"", "\"supervisor\"")
    } else {
        CONTENT.to_owned()
    };
    std::fs::write(&file, content).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
    set_time(&file, mtime);
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
    let status = archive.exists().then(|| std::fs::metadata(&archive).unwrap());
    let archive_bytes = archive.exists().then(|| std::fs::read(&archive).unwrap());
    let decoded = archive_bytes
        .as_ref()
        .filter(|_| kind != "corrupt")
        .map(|bytes| String::from_utf8(zstd::stream::decode_all(bytes.as_slice()).unwrap()).unwrap());
    let temps: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.contains(".archive-temp-"))
        .collect();
    json!({"ok":true,"result":{"archivedPaths":result.archived_paths,"errors":result.errors},"fileExists":file.exists(),"archiveExists":archive.exists(),"archiveBytes":archive_bytes.map(|bytes|pi_js_compat::base64::buffer_to_base64(&bytes)),"decoded":decoded,"metadata":metadata,"mode":status.as_ref().map(|metadata|metadata.mode() & 0o777),"mtime":status.as_ref().map(|metadata|pi_js_compat::js_json::js_number_value(metadata.mtime() as f64*1000.0+metadata.mtime_nsec() as f64/1_000_000.0)),"temps":temps})
}

#[tokio::test]
async fn archive_pass_matches_frozen_age_protection_publication_recovery_and_corrupt_frames() {
    let temp = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("idle-session-archive-pass-oracle.mts", "packages/coding-agent/src");
    let cases = [
        ("old", OLD),
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
        let actual = run_native(&root, kind, mtime, &input_archive).await;
        assert_eq!(actual, expected, "case {index} {kind}");
    }
}
