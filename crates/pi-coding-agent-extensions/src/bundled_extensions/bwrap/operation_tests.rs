use super::*;
use std::path::Path;

#[test]
fn worker_invocation_keeps_raw_workspace_and_payload_units_until_argv() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = JsString::from(directory.path().to_str().expect("utf-8 test path"))
        + "/cwd-"
        + JsString::from_utf16(vec![0xd800]);
    std::fs::create_dir(encode_node_utf8(&cwd)).unwrap();
    let path = cwd.clone() + "/file-" + JsString::from_utf16(vec![0xdfff]);
    let params = SandboxParams {
        bwrap_command: "bwrap".into(),
        cwd: cwd.clone(),
        profile: BwrapSandboxProfile(SandboxProfileName::ReadOnly),
    };
    let invocation = build_sandbox_worker_invocation(&params, "readFile", &path_payload(&path)).unwrap();
    let wire = invocation.argv.last().unwrap();
    let payload = js_json::parse_lossless(wire).unwrap();
    assert_eq!(payload["workspace"].as_string(), Some(&cwd));
    assert_eq!(payload["path"].as_string(), Some(&path));
    assert!(invocation.argv.contains(&cwd));
}

#[test]
fn operation_path_payload_keeps_distinct_surrogates() {
    let path =
        JsString::from("/cwd-") + JsString::from_utf16(vec![0xd800]) + "/file-" + JsString::from_utf16(vec![0xdfff]);
    let payload = Value::Object(path_payload(&path));
    let wire = js_json::stringify_lossless(&payload);
    assert_eq!(js_json::parse_lossless(&wire).unwrap()["path"], Value::String(path));
}

#[test]
fn operation_mime_extension_is_checked_without_projecting_filename() {
    let path = JsString::from("/file-") + JsString::from_utf16(vec![0xd800]) + ".PNG";
    assert_eq!(mime_type_for_path(&path), Some("image/png".to_owned()));
    let not_png = JsString::from("/file.png") + JsString::from_utf16(vec![0xd800]);
    assert_eq!(mime_type_for_path(&not_png), None);
}

fn unavailable_sandbox(directory: &Path) -> (SandboxParams, String) {
    let command = directory.join("missing-bwrap").display().to_string();
    let expected = format!("bubblewrap is required for sandbox profile but is unavailable: spawnSync {command} ENOENT");
    (
        SandboxParams {
            bwrap_command: command.into(),
            cwd: JsString::from(directory.to_str().expect("utf-8 test path")),
            profile: BwrapSandboxProfile(SandboxProfileName::ReadOnly),
        },
        expected,
    )
}

#[tokio::test]
async fn core_ops_bwrap_ls_exists_preserves_runner_failure() {
    let directory = tempfile::tempdir().unwrap();
    let (params, expected) = unavailable_sandbox(directory.path());
    let operation = SandboxLsOperations { params };
    assert_eq!(
        operation.exists(&"/fixture".into()).await.unwrap_err().to_string(),
        expected
    );
}

#[tokio::test]
async fn core_ops_bwrap_find_exists_preserves_runner_failure() {
    let directory = tempfile::tempdir().unwrap();
    let (params, expected) = unavailable_sandbox(directory.path());
    let operation = SandboxFindOperations { params };
    assert_eq!(
        operation.exists(&"/fixture".into()).await.unwrap_err().to_string(),
        expected
    );
}

#[tokio::test]
async fn core_ops_bwrap_find_glob_preserves_runner_failure() {
    let directory = tempfile::tempdir().unwrap();
    let (params, expected) = unavailable_sandbox(directory.path());
    let operation = SandboxFindOperations { params };
    assert_eq!(
        operation
            .glob(&"*.txt".into(), &"/fixture".into(), &[], 10.0)
            .await
            .unwrap_err()
            .to_string(),
        expected
    );
}

#[test]
fn zero_grep_limit_does_not_emit_a_notice() {
    let details = js_json::parse_lossless(r#"{"matchLimitReached":0}"#).unwrap();
    let truncation = crate::core::tools::truncate::truncate_head("none", Default::default());
    assert!(build_grep_notices(details.as_object().unwrap(), &truncation).is_empty());
}

#[tokio::test]
async fn signalled_worker_reports_null_exit_code() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let fake = directory.path().join("fake-bwrap");
    std::fs::write(
        &fake,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\nkill -TERM $$\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let params = SandboxParams {
        bwrap_command: fake.to_str().unwrap().into(),
        cwd: directory.path().to_str().unwrap().into(),
        profile: BwrapSandboxProfile(SandboxProfileName::ReadOnly),
    };
    let error = run_json_in_sandbox(&params, "stat", path_payload(&"/fixture".into()), None)
        .await
        .unwrap_err();
    assert_eq!(error, "sandbox operation failed with exit code null");
}
