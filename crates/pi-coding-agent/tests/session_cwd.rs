use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::PathBuf;

use pi_coding_agent::core::session_cwd::{
    MissingSessionCwdError, assert_session_cwd_exists, format_missing_session_cwd_error,
    format_missing_session_cwd_prompt, get_missing_session_cwd_issue, reopen_session_with_cwd,
};
use pi_coding_agent::core::session_manager::SessionManager;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pi-rust-{name}-{}", pi_ai::utils::uuid::random_uuid()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_session_file(path: &std::path::Path, cwd: &str) {
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::json!({
                "type": "session",
                "version": 3,
                "id": "session-id",
                "timestamp": "2025-01-01T00:00:00.000Z",
                "cwd": cwd,
            })
        ),
    )
    .unwrap();
}

#[test]
fn detects_missing_session_cwd_from_persisted_sessions() {
    let fallback = temp_dir("cwd-fallback");
    let missing = fallback.join("does-not-exist");
    let session_dir = temp_dir("cwd-session-dir");
    let session_file = session_dir.join("session.jsonl");
    write_session_file(&session_file, &missing.display().to_string());

    let manager = SessionManager::open(&JsString::from(&session_file.display().to_string()), None, None).unwrap();
    let issue = get_missing_session_cwd_issue(&manager, &JsString::from(&fallback.display().to_string())).unwrap();
    assert_eq!(issue.session_file.as_ref(), manager.get_session_file());
    assert_eq!(issue.session_cwd, missing.display().to_string());
    assert_eq!(issue.fallback_cwd, fallback.display().to_string());
}

#[test]
fn uses_nearest_existing_parent_when_cwd_was_deleted() {
    let fallback_parent = temp_dir("cwd-deleted-fallback");
    let deleted = fallback_parent.join("deleted-worktree");
    fs::create_dir_all(&deleted).unwrap();
    let session_dir = temp_dir("cwd-deleted-session-dir");
    let session_file = session_dir.join("session.jsonl");
    write_session_file(&session_file, &deleted.display().to_string());
    fs::remove_dir_all(&deleted).unwrap();

    let manager = SessionManager::open(&JsString::from(&session_file.display().to_string()), None, None).unwrap();
    let issue = get_missing_session_cwd_issue(&manager, &JsString::from(&deleted.display().to_string())).unwrap();
    assert_eq!(issue.fallback_cwd, fallback_parent.display().to_string());
}

#[test]
fn supports_overriding_effective_cwd_when_opening() {
    let fallback = temp_dir("cwd-override");
    let missing = fallback.join("does-not-exist");
    let session_dir = temp_dir("cwd-override-session-dir");
    let session_file = session_dir.join("session.jsonl");
    write_session_file(&session_file, &missing.display().to_string());

    let manager = SessionManager::open(
        &JsString::from(&session_file.display().to_string()),
        None,
        Some(&JsString::from(&fallback.display().to_string())),
    )
    .unwrap();
    assert_eq!(manager.get_cwd(), fallback.display().to_string());
    assert!(get_missing_session_cwd_issue(&manager, &JsString::from(&fallback.display().to_string())).is_none());
}

#[test]
fn formats_errors_and_prompts() {
    let issue = pi_coding_agent::core::session_cwd::SessionCwdIssue {
        session_file: Some(JsString::from("/sessions/a.jsonl")),
        session_cwd: JsString::from("/missing"),
        fallback_cwd: JsString::from("/current"),
    };
    assert_eq!(
        format_missing_session_cwd_error(&issue),
        "Stored session working directory does not exist: /missing\nSession file: /sessions/a.jsonl\nCurrent working directory: /current"
    );
    assert_eq!(
        format_missing_session_cwd_prompt(&issue),
        "cwd from session file does not exist\n/missing\n\ncontinue in current cwd\n/current"
    );
    let error = MissingSessionCwdError::new(issue);
    assert_eq!(error.to_string(), format_missing_session_cwd_error(&error.issue));
    assert_eq!(MissingSessionCwdError::NAME, "MissingSessionCwdError");
}

#[test]
fn asserts_and_reopens_with_selected_cwd() {
    let fallback = temp_dir("cwd-assert");
    let missing = fallback.join("does-not-exist");
    let session_dir = temp_dir("cwd-assert-session-dir");
    let session_file = session_dir.join("session.jsonl");
    write_session_file(&session_file, &missing.display().to_string());
    let manager = SessionManager::open(&JsString::from(&session_file.display().to_string()), None, None).unwrap();

    let error = assert_session_cwd_exists(&manager, &JsString::from(&fallback.display().to_string())).unwrap_err();
    assert_eq!(error.issue.session_cwd, missing.display().to_string());

    let control_db_path = session_dir.join("control.sqlite").display().to_string();
    let reopened = reopen_session_with_cwd(
        &error.issue,
        Some(&JsString::from(&session_dir.display().to_string())),
        &JsString::from(&fallback.display().to_string()),
        &JsString::from(&control_db_path),
    )
    .unwrap();
    assert_eq!(reopened.get_cwd(), fallback.display().to_string());
    assert_eq!(
        reopened.get_metadata_control_db_path(),
        Some(control_db_path.as_str()).map(JsString::from).as_ref()
    );
    let metadata = pi_coding_agent::core::session_control_db::read_session_metadata(
        &control_db_path,
        &session_file.display().to_string(),
    )
    .unwrap()
    .expect("metadata snapshot written on reopen");
    assert_eq!(metadata.cwd, fallback.display().to_string());
}
