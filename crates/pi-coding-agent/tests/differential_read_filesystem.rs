use crate::shared_oracle as oracle;
use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::read::ReadTool;
use pi_js_compat::js_json as json;
use std::sync::Arc;

fn compare_path(cwd: &str, path: &str) {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-read-tool-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let params = json!({"path": path});
    let expected = oracle.request(&json!({"id": 31, "cwd": cwd, "cases": [params]}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let tool = ReadTool::new(cwd);
    let actual = runtime.block_on(tool.execute("call".into(), params, None, Arc::new(|_| Ok(())), None));
    let actual = match actual {
        Ok(result) => json!({"ok": true, "value": result}),
        Err(error) => json!({"ok": false, "error": error.message}),
    };
    assert_eq!(actual, expected["output"][0]);
}

#[test]
fn local_access_uses_node_read_permission_not_only_metadata() {
    use std::os::unix::fs::PermissionsExt;
    let cwd = tempfile::tempdir().unwrap();
    let path = cwd.path().join("write-only.txt");
    std::fs::write(&path, "private fixture").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();
    compare_path(cwd.path().to_str().unwrap(), path.to_str().unwrap());
}

#[test]
fn local_directory_read_preserves_the_failing_file_handle_syscall() {
    let cwd = tempfile::tempdir().unwrap();
    compare_path(cwd.path().to_str().unwrap(), cwd.path().to_str().unwrap());
}
