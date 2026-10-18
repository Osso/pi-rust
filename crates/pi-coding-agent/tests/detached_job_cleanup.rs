//! `core/detached-job-cleanup.ts` behavioral tests.

use std::fs;
use std::sync::Mutex;

use pi_coding_agent::core::detached_job_cleanup::{DetachedJobCleanupOptions, cleanup_detached_job_artifacts};
use pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent;
use serde_json::{Value, json};
use tempfile::TempDir;

const EXPIRED_AT: &str = "2026-07-19T18:00:00.000Z";
const RECENT_AT: &str = "2026-07-23T17:00:00.000Z";

fn now_millis() -> i64 {
    pi_js_compat::date::date_parse_millis("2026-07-23T18:00:00.000Z").expect("now")
}

struct Fixture {
    _directory: TempDir,
    root: String,
    control_db_path: String,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("temp dir");
    let root = directory.path().to_string_lossy().into_owned();
    let control_db_path = directory.path().join("control.sqlite").to_string_lossy().into_owned();
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
    let artifact_directory = std::path::Path::new(&fixture.root)
        .join("detached-jobs")
        .join(session_name)
        .join(job_id);
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
        &session_path.to_string_lossy(),
        job_id,
        &agent,
    )
    .expect("bootstrap agent");
    artifact_directory.to_string_lossy().into_owned()
}

fn cleanup(fixture: &Fixture) -> pi_coding_agent::core::detached_job_cleanup::DetachedJobCleanupResult {
    cleanup_detached_job_artifacts(
        &fixture.control_db_path,
        &DetachedJobCleanupOptions {
            artifact_root: fixture.root.clone(),
            now: Some(now_millis()),
            on_directory_quarantined: None,
        },
    )
    .expect("cleanup")
}

#[test]
fn deletes_expired_terminal_artifacts_and_preserves_nonterminal_directories() {
    let fixture = fixture();
    let expired = persist_artifact(&fixture, "session", "pyrun_expired", "completed", EXPIRED_AT, None);
    let running = persist_artifact(&fixture, "session", "pyrun_running", "running", EXPIRED_AT, None);
    let result = cleanup(&fixture);
    assert_eq!(result.deleted_directories, vec![expired.clone()]);
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
    assert_eq!(second.deleted_directories, vec![directory.clone()]);
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
        &session_path.to_string_lossy(),
        "outside",
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
        &session_path.to_string_lossy(),
        "..",
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
    assert_eq!(result.deleted_directories, vec![oldest.clone()]);
    assert!(!std::path::Path::new(&oldest).exists());
    assert!(std::path::Path::new(&newest).exists());
    assert!(result.retained_bytes >= 0);
}

#[test]
fn reports_quarantine_paths_and_removes_them_on_success() {
    let fixture = fixture();
    let directory = persist_artifact(&fixture, "session", "pyrun_quarantine", "completed", EXPIRED_AT, None);
    let quarantined: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let result = cleanup_detached_job_artifacts(
        &fixture.control_db_path,
        &DetachedJobCleanupOptions {
            artifact_root: fixture.root.clone(),
            now: Some(now_millis()),
            on_directory_quarantined: Some(&|path| {
                quarantined.lock().expect("quarantine").push(path.to_owned());
            }),
        },
    )
    .expect("cleanup");
    assert_eq!(result.deleted_directories, vec![directory.clone()]);
    let paths = quarantined.lock().expect("quarantine");
    assert_eq!(paths.len(), 1);
    assert!(paths[0].contains(".cleanup-pyrun_quarantine-"));
    assert!(!std::path::Path::new(&paths[0]).exists());
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
