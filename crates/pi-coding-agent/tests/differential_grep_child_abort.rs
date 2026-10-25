use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_coding_agent::core::tools::grep::GrepTool;
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsValue};
use std::{path::Path, sync::Arc, time::Duration};

async fn wait_for_file(path: &Path, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match std::fs::metadata(path) {
            Ok(_) => return true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("fixture metadata: {error}"),
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
fn fake_rg(root: &Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let script = format!("#!{}\n{}", node.trim(), include_str!("support/grep_child_fixture.cjs"));
    executable_script::write_executable_script(&bin.join("rg"), &script);
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}
fn outcome(value: Result<AgentToolResult, ToolError>) -> JsValue {
    match value {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    }
}
async fn execute_abort(root: &Path, case: &JsValue) -> JsValue {
    pi_js_compat::process_env::set("PI_FAKE_RG_CHILD", &js_json::stringify_lossless(case));
    let directory = case["directory"].as_string().unwrap().to_utf8().unwrap();
    let directory = Path::new(&directory);
    let controller = AbortController::new();
    let args = if case["limit"] == JsValue::Bool(true) {
        json!({"pattern":"x","limit":1})
    } else {
        json!({"pattern":"x"})
    };
    let task = tokio::spawn(GrepTool::new(root.to_str().unwrap()).execute(
        "grep-child".into(),
        args,
        Some(controller.signal()),
        Arc::new(|_| Ok(())),
        None,
    ));
    assert!(
        wait_for_file(&directory.join("ready"), Duration::from_secs(5)).await,
        "rg did not become ready"
    );
    let limit_stopped = if case["limit"] == JsValue::Bool(true) {
        JsValue::Bool(wait_for_file(&directory.join("sigterm"), Duration::from_millis(300)).await)
    } else {
        JsValue::Null
    };
    controller.abort(Some(json!({"ignored":"reason"}).into()));
    let sigterm = wait_for_file(&directory.join("sigterm"), Duration::from_millis(300)).await;
    let settled_before_release = task.is_finished();
    std::fs::write(directory.join("finish"), "finish").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("stdio release must close grep")
        .unwrap();
    let released = wait_for_file(&directory.join("released"), Duration::from_millis(300)).await;
    let descendant_signalled = wait_for_file(&directory.join("descendant-sigterm"), Duration::from_millis(30)).await;
    json!({"result":outcome(result),"sigterm":sigterm,"settledBeforeRelease":settled_before_release,"released":released,"limitStopped":limit_stopped,"descendantSignalled":descendant_signalled})
}
#[tokio::test]
async fn grep_abort_waits_for_close_and_sends_only_parent_sigterm() {
    let root = tempfile::tempdir().unwrap();
    let path = fake_rg(root.path());
    let modes = vec![
        json!({}),
        json!({"closed":true}),
        json!({"afterExit":true}),
        json!({"limit":true}),
    ];
    let cases: Vec<_> = modes
        .iter()
        .enumerate()
        .map(|(index, mode)| {
            let mut case = mode.clone();
            case["directory"] = json!(root.path().join(format!("source-{index}")));
            case
        })
        .collect();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "grep-child-abort-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
        &[("PATH", path.clone().into())],
    );
    let expected: JsValue = source.request(&json!({"id":1,"cwd":root.path(),"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), modes.len());
    let old_path = pi_js_compat::process_env::get("PATH").unwrap();
    pi_js_compat::process_env::set("PATH", &path);
    let mut actual = Vec::new();
    for (index, mut mode) in modes.into_iter().enumerate() {
        mode["directory"] = json!(root.path().join(format!("native-{index}")));
        actual.push(execute_abort(root.path(), &mode).await);
    }
    pi_js_compat::process_env::set("PATH", &old_path);
    let actual = JsValue::Array(actual);
    assert!(
        actual == expected["output"],
        "actual={} expected={}",
        js_json::stringify_lossless(&actual),
        js_json::stringify_lossless(&expected["output"])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
