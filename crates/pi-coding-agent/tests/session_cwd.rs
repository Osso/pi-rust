use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use std::fs;
use std::path::PathBuf;

use pi_coding_agent::core::session_cwd::{
    MissingSessionCwdError, assert_session_cwd_exists, format_missing_session_cwd_error,
    format_missing_session_cwd_prompt, get_missing_session_cwd_issue, reopen_session_with_cwd,
};
use pi_coding_agent::core::session_manager::SessionManager;

#[path = "support/oracle.rs"]
mod oracle;

fn issue_wire(issue: &pi_coding_agent::core::session_cwd::SessionCwdIssue) -> serde_json::Value {
    serde_json::json!({"sessionFile":issue.session_file.as_ref().map(JsString::units),"sessionCwd":issue.session_cwd.units(),"fallbackCwd":issue.fallback_cwd.units()})
}

fn issue_result_wire(
    result: Result<Option<pi_coding_agent::core::session_cwd::SessionCwdIssue>, pi_ai::request::CallbackError>,
) -> serde_json::Value {
    match result {
        Ok(issue) => serde_json::json!({"ok":true,"output":issue.as_ref().map(issue_wire)}),
        Err(error) => callback_error_wire(error),
    }
}

fn callback_error_wire(error: pi_ai::request::CallbackError) -> serde_json::Value {
    use pi_js_compat::abort::AbortReason;
    use pi_js_compat::node_error::NativeErrorClass;
    let native = match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => error,
        reason => panic!("session-cwd must reject with an Error: {reason:?}"),
    };
    let value = native.value();
    let issue = value.get("issue").map(|issue| serde_json::json!({
        "sessionFile":issue.get("sessionFile").map(|value|value.as_string().expect("sessionFile string").units()),
        "sessionCwd":issue.get("sessionCwd").expect("sessionCwd field").as_string().expect("sessionCwd string").units(),
        "fallbackCwd":issue.get("fallbackCwd").expect("fallbackCwd field").as_string().expect("fallbackCwd string").units(),
    }));
    serde_json::json!({"ok":false,"error":{"name":native.name,"message":native.message.units(),"code":value.get("code"),"errno":value.get("errno"),"syscall":value.get("syscall"),"path":value.get("path").map(|path|path.as_string().expect("stat path string").units()),"issue":issue,"typeError":native.is_instance_of(NativeErrorClass::TypeError)}})
}

struct CwdInput {
    file: Option<JsString>,
    cwd: JsString,
}

impl pi_coding_agent::core::session_cwd::SessionCwdSource for CwdInput {
    fn get_cwd(&self) -> &JsString {
        &self.cwd
    }
    fn get_session_file(&self) -> Option<&JsString> {
        self.file.as_ref()
    }
}

#[test]
fn empty_session_file_skips_recovery_like_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let input = CwdInput {
        file: Some(JsString::default()),
        cwd: JsString::from(temp.path().join("missing").to_str().unwrap()),
    };
    let fallback = JsString::from("invalid\0fallback");
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&serde_json::json!({"operation":"issue","file":input.file.as_ref().map(JsString::units),"cwd":input.cwd.units(),"fallback":fallback.units()}));
    assert_eq!(expected["ok"], true, "source: {expected}");
    assert_eq!(
        issue_result_wire(get_missing_session_cwd_issue(&input, &fallback)),
        expected
    );
}

#[test]
fn empty_session_file_is_omitted_from_diagnostics_like_frozen() {
    let issue = pi_coding_agent::core::session_cwd::SessionCwdIssue {
        session_file: Some(JsString::default()),
        session_cwd: JsString::from_utf16(vec![47, 0xd800, 10, 0xdc00]),
        fallback_cwd: JsString::from_utf16(vec![47, 0xdc00, 0xd83d, 0xde00]),
    };
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&serde_json::json!({"operation":"format","file":issue.session_file.as_ref().map(JsString::units),"cwd":issue.session_cwd.units(),"fallback":issue.fallback_cwd.units()}));
    assert_eq!(expected["ok"], true, "source: {expected}");
    let error = MissingSessionCwdError::new(issue.clone());
    assert_eq!(
        serde_json::json!({"error":format_missing_session_cwd_error(&issue).units(),"prompt":format_missing_session_cwd_prompt(&issue).units(),"name":MissingSessionCwdError::NAME,"message":error.message().units(),"issue":issue_wire(&error.issue)}),
        expected["output"]
    );
}

#[test]
fn empty_session_file_reopen_rejects_before_io_like_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let fallback = JsString::from(temp.path().to_str().unwrap());
    let database = temp.path().join("control.sqlite");
    let database = JsString::from(database.to_str().unwrap());
    let issue = pi_coding_agent::core::session_cwd::SessionCwdIssue {
        session_file: Some(JsString::default()),
        session_cwd: "/missing".into(),
        fallback_cwd: fallback.clone(),
    };
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&serde_json::json!({"operation":"reopen","file":issue.session_file.as_ref().map(JsString::units),"cwd":issue.session_cwd.units(),"fallback":fallback.units(),"sessionDir":fallback,"controlDbPath":database}));
    assert_eq!(expected["ok"], false, "source: {expected}");
    let actual = match reopen_session_with_cwd(&issue, Some(&fallback), &fallback, &database) {
        Ok(_) => serde_json::json!({"ok":true}),
        Err(error) => callback_error_wire(error.into()),
    };
    assert_eq!(actual, expected);
    assert!(
        !std::path::Path::new(&encode_node_utf8(&database)).exists(),
        "rejection must precede metadata writes"
    );
}

#[test]
fn reopen_cwd_and_metadata_match_frozen_for_empty_and_raw_arguments() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("session.jsonl");
    write_session_file(&file, "/missing");
    let file = JsString::from(file.to_str().unwrap());
    let directory = JsString::from(temp.path().to_str().unwrap());
    fs::create_dir(temp.path().join("\u{fffd}")).unwrap();
    let raw = directory.clone() + "/" + JsString::from_utf16(vec![0xd800]);
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    for (index, (selected, session_dir)) in [
        (directory.clone(), None),
        (JsString::default(), None),
        (raw, None),
        (directory.clone(), Some(JsString::default())),
        (directory.clone(), Some(directory.clone())),
    ]
    .into_iter()
    .enumerate()
    {
        let source_database = JsString::from(temp.path().join(format!("source-{index}.sqlite")).to_str().unwrap());
        let native_database = JsString::from(temp.path().join(format!("native-{index}.sqlite")).to_str().unwrap());
        let expected = source.request(&serde_json::json!({"operation":"reopen","file":file.units(),"cwd":JsString::from("/missing").units(),"fallback":selected.units(),"sessionDir":session_dir,"controlDbPath":source_database}));
        assert_eq!(expected["ok"], true, "case {index}, source: {expected}");
        assert_eq!(
            expected["output"]["controlDbPath"],
            serde_json::json!(source_database.units())
        );
        let issue = pi_coding_agent::core::session_cwd::SessionCwdIssue {
            session_file: Some(file.clone()),
            session_cwd: "/missing".into(),
            fallback_cwd: selected.clone(),
        };
        let actual = reopen_session_with_cwd(&issue, session_dir.as_ref(), &selected, &native_database).unwrap();
        assert_eq!(
            serde_json::json!(actual.get_cwd().units()),
            expected["output"]["cwd"],
            "case {index}, cwd"
        );
        assert_eq!(
            serde_json::json!(actual.get_session_dir().units()),
            expected["output"]["sessionDir"],
            "case {index}, session directory"
        );
        assert_eq!(actual.get_metadata_control_db_path(), Some(&native_database));
        let source_metadata = pi_coding_agent::core::session_control_db::read_session_metadata(&source_database, &file)
            .unwrap()
            .unwrap();
        let native_metadata = pi_coding_agent::core::session_control_db::read_session_metadata(&native_database, &file)
            .unwrap()
            .unwrap();
        assert_eq!(native_metadata.cwd, source_metadata.cwd, "case {index}, persisted cwd");
    }
}

#[test]
fn nearest_directory_matches_frozen_stat_branches_and_raw_candidate() {
    use pi_coding_agent::core::session_cwd::find_nearest_existing_cwd;
    let temp = tempfile::tempdir().unwrap();
    let directory = JsString::from(temp.path().to_str().unwrap());
    let file = temp.path().join("file");
    fs::write(&file, "regular file").unwrap();
    fs::create_dir(temp.path().join("\u{fffd}")).unwrap();
    let mut candidates = vec![
        directory.clone(),
        JsString::from(file.to_str().unwrap()),
        JsString::from(file.join("child").to_str().unwrap()),
        JsString::from(temp.path().join("missing/child").to_str().unwrap()),
        JsString::default(),
        JsString::from("/"),
        directory.clone() + "/" + JsString::from_utf16(vec![0xd800]),
    ];
    #[cfg(unix)]
    {
        for (name, target) in [
            ("directory-link", temp.path()),
            ("file-link", file.as_path()),
            ("broken-link", temp.path().join("absent").as_path()),
        ] {
            let link = temp.path().join(name);
            std::os::unix::fs::symlink(target, &link).unwrap();
            candidates.push(JsString::from(link.to_str().unwrap()));
        }
    }
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    for candidate in candidates {
        let expected = source
            .request(&serde_json::json!({"operation":"nearest","file":null,"cwd":[],"fallback":candidate.units()}));
        assert_eq!(expected["ok"], true, "source: {expected}");
        let actual = find_nearest_existing_cwd(&candidate).unwrap();
        assert_eq!(
            serde_json::json!(actual.units()),
            expected["output"],
            "candidate {candidate:?}"
        );
    }
}

#[test]
fn missing_cwd_guards_skip_invalid_fallback_like_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("file");
    fs::write(&file, "regular file").unwrap();
    let directory = JsString::from(temp.path().to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    for input in [
        CwdInput {
            file: None,
            cwd: directory.clone() + "/missing",
        },
        CwdInput {
            file: Some("/session.jsonl".into()),
            cwd: JsString::default(),
        },
        CwdInput {
            file: Some("/session.jsonl".into()),
            cwd: directory,
        },
        CwdInput {
            file: Some("/session.jsonl".into()),
            cwd: JsString::from(file.to_str().unwrap()),
        },
    ] {
        let fallback = JsString::from("invalid\0fallback");
        for operation in ["issue", "assert"] {
            let expected = source.request(&serde_json::json!({"operation":operation,"file":input.file.as_ref().map(JsString::units),"cwd":input.cwd.units(),"fallback":fallback.units()}));
            assert_eq!(expected, serde_json::json!({"ok":true,"output":null}));
            let actual = if operation == "issue" {
                issue_result_wire(get_missing_session_cwd_issue(&input, &fallback))
            } else {
                match assert_session_cwd_exists(&input, &fallback) {
                    Ok(()) => serde_json::json!({"ok":true,"output":null}),
                    Err(error) => callback_error_wire(error),
                }
            };
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn raw_missing_cwd_error_matches_frozen_rejection() {
    let temp = tempfile::tempdir().unwrap();
    let fallback = JsString::from(temp.path().to_str().unwrap());
    let input = CwdInput {
        file: Some(fallback.clone() + "/session" + JsString::from_utf16(vec![0xdc00]) + ".jsonl"),
        cwd: fallback.clone() + "/missing" + JsString::from_utf16(vec![0xd800]),
    };
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&serde_json::json!({"operation":"assert","file":input.file.as_ref().map(JsString::units),"cwd":input.cwd.units(),"fallback":fallback.units()}));
    assert_eq!(expected["ok"], false, "source: {expected}");
    assert_eq!(
        callback_error_wire(assert_session_cwd_exists(&input, &fallback).unwrap_err()),
        expected
    );
}

#[test]
fn missing_cwd_assertion_preserves_frozen_error_name_and_issue() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("session.jsonl");
    let missing = temp.path().join("deleted-cwd");
    write_session_file(&file, missing.to_str().unwrap());
    let manager = SessionManager::open(&JsString::from(file.to_str().unwrap()), None, None).unwrap();
    let fallback = JsString::from(temp.path().to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&serde_json::json!({"operation":"assert","file":manager.get_session_file().unwrap().units(),"cwd":manager.get_cwd().units(),"fallback":fallback.units()}));
    assert_eq!(expected["ok"], false, "source: {expected}");
    let error = assert_session_cwd_exists(&manager, &fallback).unwrap_err();
    assert_eq!(callback_error_wire(error), expected);
}

#[test]
fn invalid_fallback_stat_rejects_like_frozen_instead_of_climbing() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("session.jsonl");
    let missing = temp.path().join("deleted-cwd");
    write_session_file(&file, missing.to_str().unwrap());
    let manager = SessionManager::open(&JsString::from(file.to_str().unwrap()), None, None).unwrap();
    let fallback = JsString::from("invalid\0fallback");
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&serde_json::json!({"operation":"issue","file":manager.get_session_file().unwrap().units(),"cwd":manager.get_cwd().units(),"fallback":fallback.units()}));
    assert_eq!(expected["ok"], false, "source: {expected}");
    assert_eq!(
        issue_result_wire(get_missing_session_cwd_issue(&manager, &fallback)),
        expected
    );
}

#[cfg(unix)]
#[test]
fn inaccessible_fallback_stat_rejects_like_frozen_instead_of_climbing() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("session.jsonl");
    let missing = temp.path().join("deleted-cwd");
    let blocked = temp.path().join("blocked");
    let fallback = blocked.join("nested");
    fs::create_dir_all(&fallback).unwrap();
    write_session_file(&file, missing.to_str().unwrap());
    let manager = SessionManager::open(&JsString::from(file.to_str().unwrap()), None, None).unwrap();
    let fallback = JsString::from(fallback.to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
    let expected = source.request(&serde_json::json!({"operation":"issue","file":manager.get_session_file().unwrap().units(),"cwd":manager.get_cwd().units(),"fallback":fallback.units()}));
    let issue = get_missing_session_cwd_issue(&manager, &fallback);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(expected["ok"], false, "source: {expected}");
    assert_eq!(issue_result_wire(issue), expected);
}

#[test]
fn regular_file_fallback_uses_its_directory_like_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("session.jsonl");
    let missing = temp.path().join("deleted-cwd");
    let fallback = temp.path().join("fallback-file");
    fs::write(&fallback, "not a directory").unwrap();
    write_session_file(&file, missing.to_str().unwrap());
    let manager = SessionManager::open(&JsString::from(file.to_str().unwrap()), None, None).unwrap();
    let fallback = JsString::from(fallback.to_str().unwrap());
    let mut source = oracle::TsOracle::spawn("session-cwd-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&serde_json::json!({"operation":"issue","file":manager.get_session_file().unwrap().units(),"cwd":manager.get_cwd().units(),"fallback":fallback.units()}));
    assert_eq!(expected["ok"], true, "source: {expected}");
    let issue = get_missing_session_cwd_issue(&manager, &fallback).unwrap().unwrap();
    assert_eq!(issue_wire(&issue), expected["output"]);
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pi-rust-{name}-{}",
        pi_ai::utils::random_uuid::random_uuid().unwrap()
    ));
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
    let issue = get_missing_session_cwd_issue(&manager, &JsString::from(&fallback.display().to_string()))
        .unwrap()
        .unwrap();
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
    let issue = get_missing_session_cwd_issue(&manager, &JsString::from(&deleted.display().to_string()))
        .unwrap()
        .unwrap();
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
    assert_eq!(*manager.get_cwd(), fallback.to_str().expect("utf-8 path"));
    assert!(
        get_missing_session_cwd_issue(&manager, &JsString::from(&fallback.display().to_string()))
            .unwrap()
            .is_none()
    );
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
    assert_eq!(
        error.to_string(),
        encode_node_utf8(&format_missing_session_cwd_error(&error.issue))
    );
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
    let error = error
        .original
        .as_ref()
        .unwrap()
        .downcast_ref::<MissingSessionCwdError>()
        .unwrap();
    assert_eq!(error.issue.session_cwd, missing.display().to_string());

    let control_db_path = session_dir.join("control.sqlite").display().to_string();
    let reopened = reopen_session_with_cwd(
        &error.issue,
        Some(&JsString::from(&session_dir.display().to_string())),
        &JsString::from(&fallback.display().to_string()),
        &JsString::from(&control_db_path),
    )
    .unwrap();
    assert_eq!(*reopened.get_cwd(), fallback.to_str().expect("utf-8 path"));
    assert_eq!(
        reopened.get_metadata_control_db_path(),
        Some(JsString::from(control_db_path.as_str())).as_ref()
    );
    let metadata = pi_coding_agent::core::session_control_db::read_session_metadata(
        &control_db_path,
        session_file.display().to_string(),
    )
    .unwrap()
    .expect("metadata snapshot written on reopen");
    assert_eq!(metadata.cwd, fallback.display().to_string());
}
