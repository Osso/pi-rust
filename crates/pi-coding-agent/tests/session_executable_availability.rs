#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/agent_session_fixture.rs"]
mod fixture;
use crate::shared_oracle as oracle;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_js_compat::js_json::JsString;
use serde_json::json;

#[test]
fn session_filters_tools_by_executable_availability() {
    run_session_probe(false);
}

#[test]
fn session_propagates_deleted_cwd_error() {
    run_session_probe(true);
}

fn run_session_probe(deleted_only: bool) {
    let temp = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/usr/bin/git", temp.path().join("git")).unwrap();
    env_probe::run_probe(
        "session_availability_probe",
        &[
            ("PATH", temp.path().as_os_str().to_owned()),
            ("HOME", temp.path().as_os_str().to_owned()),
            ("PI_CODING_AGENT_STATE_DIR", temp.path().join("state").into_os_string()),
            ("PI_Q5_ORACLE_PATH", std::env::var_os("PATH").unwrap()),
            ("PI_OFFLINE", "1".into()),
            ("PI_Q5_DELETED_CWD_ONLY", if deleted_only { "1" } else { "0" }.into()),
        ],
    );
}

#[tokio::test]
#[ignore = "controlled cwd/PATH probe"]
async fn session_availability_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let session = fixture::fixture(root.path(), ThinkingLevel::Off).await.session;
    let oracle_path = std::env::var_os("PI_Q5_ORACLE_PATH").unwrap();
    let mut index = oracle::TsOracle::spawn_with_environment(
        "coding-agent-tools-index-oracle.ts",
        "packages/coding-agent/src/core/tools/index.ts",
        &[("PATH", oracle_path.clone())],
    );
    let expected = index.request(&json!({"id": 1, "operation": "availability", "cwd": root.path().to_str().unwrap(), "path": std::env::var("PATH").unwrap(), "injected": false}));
    assert_eq!(expected["ok"], json!(true), "{expected}");
    let mut actual: Vec<_> = session.get_all_tools().into_iter().map(|tool| tool.name).collect();
    let mut expected_names: Vec<String> = serde_json::from_value(expected["output"].clone()).unwrap();
    actual.sort();
    expected_names.sort();
    if std::env::var("PI_Q5_DELETED_CWD_ONLY").unwrap() == "0" {
        assert_eq!(actual, expected_names);
        return;
    }

    let native_deleted = root.path().join("native-deleted");
    let source_deleted = root.path().join("source-deleted");
    std::fs::create_dir(&native_deleted).unwrap();
    std::fs::create_dir(&source_deleted).unwrap();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "session-executable-availability-oracle.ts",
        "packages/coding-agent/src/core/agent-session.ts",
        &[("PATH", oracle_path)],
    );
    let expected = oracle.request(
        &json!({"id": 2, "cwd": root.path().to_str().unwrap(), "deletedCwd": source_deleted.to_str().unwrap()}),
    );
    assert_eq!(expected["ok"], json!(true), "{expected}");
    assert_eq!(
        expected["output"]["message"],
        json!(
            "ENOENT: process.cwd failed with error no such file or directory, the current working directory was likely removed without changing the working directory, uv_cwd"
        )
    );
    assert_eq!(expected["output"]["retained"], json!(true));
    let saved_cwd = std::env::current_dir().unwrap();
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(&native_deleted),
    )))
    .unwrap();
    std::fs::remove_dir(&native_deleted).unwrap();
    let helper_error =
        pi_coding_agent::utils::executable::is_executable_available(&JsString::from("code-index"), &Default::default())
            .expect_err("deleted cwd");
    let result = session.reload(None).await;
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(saved_cwd),
    )))
    .unwrap();
    assert_eq!(
        helper_error.message,
        JsString::from(expected["output"]["message"].as_str().unwrap())
    );
    let error = result.expect_err("runtime rebuild must propagate unavailable process cwd");
    assert_eq!(
        error.message(),
        &JsString::from(expected["output"]["message"].as_str().unwrap())
    );
    let mut after: Vec<_> = session.get_all_tools().into_iter().map(|tool| tool.name).collect();
    after.sort();
    assert_eq!(actual, after, "failed rebuild must preserve the previous registry");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
