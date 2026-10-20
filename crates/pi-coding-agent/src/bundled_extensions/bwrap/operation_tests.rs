use super::*;

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
    assert_eq!(operation.exists("/fixture").await.unwrap_err().to_string(), expected);
}

#[tokio::test]
async fn core_ops_bwrap_find_exists_preserves_runner_failure() {
    let directory = tempfile::tempdir().unwrap();
    let (params, expected) = unavailable_sandbox(directory.path());
    let operation = SandboxFindOperations { params };
    assert_eq!(operation.exists("/fixture").await.unwrap_err().to_string(), expected);
}

#[tokio::test]
async fn core_ops_bwrap_find_glob_preserves_runner_failure() {
    let directory = tempfile::tempdir().unwrap();
    let (params, expected) = unavailable_sandbox(directory.path());
    let operation = SandboxFindOperations { params };
    assert_eq!(
        operation
            .glob("*.txt", "/fixture", &[], 10)
            .await
            .unwrap_err()
            .to_string(),
        expected
    );
}
