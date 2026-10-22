use super::*;

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
            bwrap_command: command,
            cwd: directory.display().to_string(),
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
            .glob(&"*.txt".into(), &"/fixture".into(), &[], 10)
            .await
            .unwrap_err()
            .to_string(),
        expected
    );
}
