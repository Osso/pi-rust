//! `core/detached-job-cleanup.ts` behavioral tests.

use std::fs;
use std::sync::Mutex;

use crate::shared_oracle as oracle;
use pi_coding_agent::core::detached_job_cleanup::{
    DetachedJobCleanupOptions, cleanup_detached_job_artifacts, run_detached_job_artifact_cleanup,
};
use pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent;
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use tempfile::TempDir;

const EXPIRED_AT: &str = "2026-07-19T18:00:00.000Z";
const RECENT_AT: &str = "2026-07-23T17:00:00.000Z";

fn now_millis() -> f64 {
    pi_js_compat::date::date_parse("2026-07-23T18:00:00.000Z")
}

struct Fixture {
    _directory: TempDir,
    root: String,
    control_db_path: String,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().to_str().expect("utf-8 test path").to_owned();
    let control_db_path = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    Fixture {
        _directory: directory,
        root,
        control_db_path,
    }
}

fn persist_artifact(
    fixture: &Fixture,
    session_name: &str,
    job_id: &str,
    lifecycle: &str,
    updated_at: &str,
    size: Option<u64>,
) -> String {
    let session_path = std::path::Path::new(&fixture.root)
        .join("sessions")
        .join("project")
        .join(format!("{session_name}.jsonl"));
    persist_artifact_in_root(
        fixture,
        &session_path,
        std::path::Path::new(&fixture.root),
        job_id,
        lifecycle,
        updated_at,
        size,
    )
}

fn persist_artifact_in_root(
    fixture: &Fixture,
    session_path: &std::path::Path,
    artifact_root: &std::path::Path,
    job_id: &str,
    lifecycle: &str,
    updated_at: &str,
    size: Option<u64>,
) -> String {
    let session_name = session_path.file_stem().unwrap().to_str().expect("utf-8 test path");
    let artifact_directory = artifact_root.join("detached-jobs").join(session_name).join(job_id);
    fs::create_dir_all(&artifact_directory).expect("artifact directory");
    let output_path = artifact_directory.join("output.log");
    let file = fs::File::create(&output_path).expect("output");
    if let Some(size) = size {
        file.set_len(size).expect("sparse size");
    }
    let agent: Value = json!({
        "agentType": "background",
        "createdAt": updated_at,
        "cwd": fixture.root,
        "detached": true,
        "displayName": "Detached job",
        "id": job_id,
        "lifecycle": lifecycle,
        "parentId": "main",
        "permission": {"narrowed": true, "policy": "on-request"},
        "result": {"fileRefs": [{"label": "Pyrun output", "path": output_path}]},
        "revision": if lifecycle == "running" { 1 } else { 2 },
        "updatedAt": updated_at,
    });
    bootstrap_multi_agent_agent(
        &fixture.control_db_path,
        session_path.to_str().expect("utf-8 test path"),
        &job_id.into(),
        &agent,
    )
    .expect("bootstrap agent");
    artifact_directory.to_str().expect("utf-8 test path").to_owned()
}

fn cleanup(fixture: &Fixture) -> pi_coding_agent::core::detached_job_cleanup::DetachedJobCleanupResult {
    cleanup_detached_job_artifacts(
        &JsString::from(&fixture.control_db_path),
        &DetachedJobCleanupOptions {
            artifact_root: fixture.root.clone().into(),
            now: Some(now_millis()),
            on_directory_quarantined: None,
        },
    )
    .expect("cleanup")
}

#[test]
fn exported_process_references_keep_open_and_deleted_files_like_frozen() {
    use pi_coding_agent::core::detached_job_cleanup::read_linux_process_references;
    let fixture = fixture();
    let live = std::path::Path::new(&fixture.root).join("open-output");
    let deleted = std::path::Path::new(&fixture.root).join("deleted-output");
    let _live = fs::File::create(&live).unwrap();
    let _deleted = fs::File::create(&deleted).unwrap();
    fs::remove_file(&deleted).unwrap();
    let paths = vec![
        JsString::from(live.to_str().unwrap()),
        JsString::from(deleted.to_str().unwrap()),
    ];
    let mut source = oracle::TsOracle::spawn(
        "detached-job-cleanup-oracle.ts",
        "packages/coding-agent/src/core/detached-job-cleanup.ts",
    );
    let expected = source.request(&json!({"id":2,"mode":"references","paths":paths}));
    assert_eq!(expected["ok"], json!(true));
    assert_eq!(expected["output"], json!([true, true]));
    let references = read_linux_process_references().unwrap().unwrap();
    let actual = paths.iter().map(|path| references.contains(path)).collect::<Vec<_>>();
    assert_eq!(json!(actual), expected["output"]);
}

#[test]
fn cleanup_accepts_both_agent_and_session_artifact_roots_like_frozen_source() {
    let fixture = fixture();
    let session_path = std::path::Path::new(&fixture.root).join("session-side/project/beside.jsonl");
    let session_root = session_path.parent().unwrap();
    let bash_directory = persist_artifact(&fixture, "agent", "bash", "completed", EXPIRED_AT, Some(8));
    let pyrun_directory = persist_artifact_in_root(
        &fixture,
        &session_path,
        session_root,
        "pyrun",
        "completed",
        EXPIRED_AT,
        Some(8),
    );
    let mut source = oracle::TsOracle::spawn(
        "detached-job-cleanup-oracle.ts",
        "packages/coding-agent/src/core/detached-job-cleanup.ts",
    );
    let expected = source.request(&json!({
        "id": 1, "mode": "direct", "database": fixture.control_db_path,
        "artifactRoot": fixture.root, "now": now_millis(), "paths": [],
    }));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(expected["output"]["deletedDirectories"].as_array().unwrap().len(), 2);
    assert!(!std::path::Path::new(&bash_directory).exists());
    assert!(!std::path::Path::new(&pyrun_directory).exists());
    persist_artifact(&fixture, "agent", "bash", "completed", EXPIRED_AT, Some(8));
    persist_artifact_in_root(
        &fixture,
        &session_path,
        session_root,
        "pyrun",
        "completed",
        EXPIRED_AT,
        Some(8),
    );
    let actual = cleanup(&fixture);
    assert!(actual.skipped_reason.is_none());
    let actual = json!({
        "deletedBytes": actual.deleted_bytes,
        "deletedDirectories": actual.deleted_directories,
        "errors": actual.errors,
        "retainedBytes": actual.retained_bytes,
    });
    assert_eq!(actual, expected["output"]);
    assert!(!std::path::Path::new(&bash_directory).exists());
    assert!(!std::path::Path::new(&pyrun_directory).exists());
}

#[test]
fn wrapper_selects_recent_session_root_artifacts_over_the_cap_like_frozen_source() {
    let fixture = fixture();
    let session_path = std::path::Path::new(&fixture.root).join("session-side/project/recent.jsonl");
    let session_root = session_path.parent().unwrap();
    let over_cap_bytes = 2_u64 * 1024 * 1024 * 1024 + 1;
    let directory = persist_artifact_in_root(
        &fixture,
        &session_path,
        session_root,
        "pyrun",
        "completed",
        RECENT_AT,
        Some(over_cap_bytes),
    );
    let mut source = oracle::TsOracle::spawn(
        "detached-job-cleanup-oracle.ts",
        "packages/coding-agent/src/core/detached-job-cleanup.ts",
    );
    let expected = source.request(&json!({
        "id": 2, "mode": "wrapper", "database": fixture.control_db_path,
        "artifactRoot": fixture.root, "now": now_millis(), "paths": [directory],
    }));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(expected["output"], json!({"exists": [false]}));
    persist_artifact_in_root(
        &fixture,
        &session_path,
        session_root,
        "pyrun",
        "completed",
        RECENT_AT,
        Some(over_cap_bytes),
    );
    run_detached_job_artifact_cleanup(
        &JsString::from(&fixture.control_db_path),
        Some(&JsString::from(&fixture.root)),
        Some(now_millis()),
    )
    .unwrap();
    assert_eq!(
        json!({"exists": [std::path::Path::new(&directory).exists()]}),
        expected["output"]
    );
}

#[test]
fn deletes_expired_terminal_artifacts_and_preserves_nonterminal_directories() {
    let fixture = fixture();
    let expired = persist_artifact(&fixture, "session", "pyrun_expired", "completed", EXPIRED_AT, None);
    let running = persist_artifact(&fixture, "session", "pyrun_running", "running", EXPIRED_AT, None);
    let result = cleanup(&fixture);
    assert_eq!(result.deleted_directories, vec![JsString::from(expired.clone())]);
    assert!(!std::path::Path::new(&expired).exists());
    assert!(std::path::Path::new(&running).exists());
    assert!(result.errors.is_empty());
}

#[test]
fn preserves_a_terminal_directory_containing_an_open_file_descriptor() {
    let fixture = fixture();
    let directory = persist_artifact(&fixture, "session", "pyrun_open", "completed", EXPIRED_AT, None);
    let held = fs::File::open(std::path::Path::new(&directory).join("output.log")).expect("open output");
    let result = cleanup(&fixture);
    assert!(result.deleted_directories.is_empty());
    assert!(std::path::Path::new(&directory).exists());
    drop(held);
    let second = cleanup(&fixture);
    assert_eq!(second.deleted_directories, vec![JsString::from(directory.clone())]);
    assert!(!std::path::Path::new(&directory).exists());
}

#[test]
fn refuses_an_artifact_path_outside_the_agent_artifact_root() {
    let fixture = fixture();
    let outside = tempfile::tempdir().expect("outside");
    let output_path = outside.path().join("output.log");
    fs::write(&output_path, "output").expect("output");
    let session_path = std::path::Path::new(&fixture.root)
        .join("sessions")
        .join("project")
        .join("session.jsonl");
    bootstrap_multi_agent_agent(
        &fixture.control_db_path,
        session_path.to_str().expect("utf-8 test path"),
        &"outside".into(),
        &json!({
            "agentType": "background",
            "createdAt": EXPIRED_AT,
            "cwd": fixture.root,
            "detached": true,
            "displayName": "Detached job",
            "id": "outside",
            "lifecycle": "completed",
            "parentId": "main",
            "permission": {"narrowed": true, "policy": "on-request"},
            "result": {"fileRefs": [{"label": "Pyrun output", "path": output_path}]},
            "revision": 2,
            "updatedAt": EXPIRED_AT,
        }),
    )
    .expect("bootstrap");
    let result = cleanup(&fixture);
    assert!(result.deleted_directories.is_empty());
    assert!(output_path.exists());
}

#[test]
fn refuses_a_detached_job_id_containing_path_traversal() {
    let fixture = fixture();
    let escaped = std::path::Path::new(&fixture.root)
        .join("detached-jobs")
        .join("session")
        .join("escape");
    fs::create_dir_all(&escaped).expect("escape directory");
    let output_path = escaped.join("output.log");
    fs::write(&output_path, "output").expect("output");
    let session_path = std::path::Path::new(&fixture.root)
        .join("sessions")
        .join("project")
        .join("session.jsonl");
    bootstrap_multi_agent_agent(
        &fixture.control_db_path,
        session_path.to_str().expect("utf-8 test path"),
        &"..".into(),
        &json!({
            "agentType": "background",
            "createdAt": EXPIRED_AT,
            "cwd": fixture.root,
            "detached": true,
            "displayName": "Detached job",
            "id": "..",
            "lifecycle": "completed",
            "parentId": "main",
            "permission": {"narrowed": true, "policy": "on-request"},
            "result": {"fileRefs": [{"label": "Pyrun output", "path": output_path}]},
            "revision": 2,
            "updatedAt": EXPIRED_AT,
        }),
    )
    .expect("bootstrap");
    let result = cleanup(&fixture);
    assert!(result.deleted_directories.is_empty());
    assert!(escaped.exists());
}

#[test]
fn deletes_oldest_recent_terminal_directories_until_retained_bytes_fit_the_two_gib_cap() {
    let fixture = fixture();
    let one_point_five_gib = 1_610_612_736_u64;
    let oldest = persist_artifact(
        &fixture,
        "session",
        "pyrun_oldest",
        "completed",
        "2026-07-23T16:00:00.000Z",
        Some(one_point_five_gib),
    );
    let newest = persist_artifact(
        &fixture,
        "session",
        "pyrun_newest",
        "completed",
        "2026-07-23T17:00:00.000Z",
        Some(one_point_five_gib),
    );
    let result = cleanup(&fixture);
    assert_eq!(result.deleted_directories, vec![JsString::from(oldest.clone())]);
    assert!(!std::path::Path::new(&oldest).exists());
    assert!(std::path::Path::new(&newest).exists());
    assert!(result.retained_bytes >= 0);
}

#[test]
fn reports_quarantine_paths_and_removes_them_on_success() {
    let fixture = fixture();
    let directory = persist_artifact(&fixture, "session", "pyrun_quarantine", "completed", EXPIRED_AT, None);
    let quarantined: Mutex<Vec<JsString>> = Mutex::new(Vec::new());
    let result = cleanup_detached_job_artifacts(
        &JsString::from(&fixture.control_db_path),
        &DetachedJobCleanupOptions {
            artifact_root: fixture.root.clone().into(),
            now: Some(now_millis()),
            on_directory_quarantined: Some(&|path| {
                quarantined.lock().expect("quarantine").push(path.to_owned());
            }),
        },
    )
    .expect("cleanup");
    assert_eq!(result.deleted_directories, vec![JsString::from(directory.clone())]);
    let paths = quarantined.lock().expect("quarantine");
    assert_eq!(paths.len(), 1);
    assert!(paths[0].contains(".cleanup-pyrun_quarantine-"));
    assert!(!std::path::Path::new(&encode_node_utf8(&paths[0])).exists());
}

#[test]
fn reports_invalid_terminal_timestamps_without_deleting() {
    let fixture = fixture();
    let directory = persist_artifact(
        &fixture,
        "session",
        "pyrun_invalid",
        "completed",
        "0000-00-00T00:00:00.000Z",
        None,
    );
    let result = cleanup(&fixture);
    assert!(result.deleted_directories.is_empty());
    assert!(std::path::Path::new(&directory).exists());
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.starts_with("Invalid detached artifact terminal timestamp for")),
        "{result:?}"
    );
}

#[test]
fn keeps_recent_terminal_artifacts_within_the_age_limit() {
    let fixture = fixture();
    let recent = persist_artifact(&fixture, "session", "pyrun_recent", "completed", RECENT_AT, None);
    let result = cleanup(&fixture);
    assert!(result.deleted_directories.is_empty());
    assert!(std::path::Path::new(&recent).exists());
    assert_eq!(result.deleted_bytes, 0);
}
