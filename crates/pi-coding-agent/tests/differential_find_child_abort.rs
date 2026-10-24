#[path = "support/executable_script.rs"]
mod executable_script;
#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_coding_agent::core::tools::find::FindTool;
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsValue};
use std::{path::Path, sync::Arc, time::Duration};

async fn wait_for_file(path: &Path, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::fs::metadata(path).await {
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

fn stop_fixture_child(directory: &Path) {
    let value = std::fs::read_to_string(directory.join("pid")).unwrap();
    let pid = rustix::process::Pid::from_raw(value.parse().unwrap()).unwrap();
    match rustix::process::kill_process(pid, rustix::process::Signal::KILL) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => {}
        Err(error) => panic!("fixture child cleanup: {error}"),
    }
}

fn fake_fd(root: &Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let script = format!(
        "#!{}\nif(process.argv[2]==='--version')process.exit(0);const fs=require('node:fs');const path=require('node:path');const p=JSON.parse(process.env.PI_FAKE_FD_ABORT);fs.mkdirSync(p.directory,{{recursive:true}});fs.writeFileSync(path.join(p.directory,'pid'),String(process.pid));process.on('SIGTERM',()=>{{fs.writeFileSync(path.join(p.directory,'sigterm'),'received');process.exit(0)}});if(p.closed){{fs.closeSync(1);fs.closeSync(2)}}fs.writeFileSync(path.join(p.directory,'ready'),'ready');setInterval(()=>{{}},1000);setTimeout(()=>process.exit(0),10000);\n",
        node.trim(),
    );
    executable_script::write_executable_script(&bin.join("fd"), &script);
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}

fn outcome(value: Result<AgentToolResult, ToolError>) -> JsValue {
    match value {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    }
}

async fn execute_abort(root: &Path, case: &JsValue) -> JsValue {
    pi_js_compat::process_env::set("PI_FAKE_FD_ABORT", &js_json::stringify_lossless(case));
    let directory = case["directory"].as_string().unwrap().to_utf8().unwrap();
    let directory = Path::new(&directory);
    let controller = AbortController::new();
    let mut execution = FindTool::new(root.to_str().unwrap()).execute(
        "fd-abort".into(),
        json!({"pattern":"*"}),
        Some(controller.signal()),
        Arc::new(|_| Ok(())),
        None,
    );
    let ready_file = directory.join("ready");
    let ready = tokio::select! {
        ready=wait_for_file(&ready_file,Duration::from_secs(5))=>ready,
        result=&mut execution=>panic!("settled before fd ready: {result:?}"),
    };
    if !ready {
        stop_fixture_child(directory);
        panic!("fd did not become ready");
    }
    controller.abort(Some(json!({"ignored":"reason"}).into()));
    let observed = tokio::time::timeout(Duration::from_secs(5), &mut execution).await;
    let sigterm = wait_for_file(&directory.join("sigterm"), Duration::from_millis(300)).await;
    stop_fixture_child(directory);
    tokio::task::yield_now().await;
    json!({"result":outcome(observed.expect("abort must settle promptly")),"sigterm":sigterm})
}

#[tokio::test]
async fn find_abort_sends_sigterm_even_after_child_stdio_has_closed() {
    let root = tempfile::tempdir().unwrap();
    let path = fake_fd(root.path());
    let cases: Vec<_> = [false, true]
        .into_iter()
        .enumerate()
        .map(|(index, closed)| json!({"directory":root.path().join(format!("source-{index}")),"closed":closed}))
        .collect();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "find-child-abort-oracle.ts",
        "packages/coding-agent/src/core/tools/find.ts",
        &[("PATH", path.clone().into())],
    );
    let expected: JsValue = source.request(&json!({"id":1,"cwd":root.path(),"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let old_path = pi_js_compat::process_env::get("PATH").unwrap();
    pi_js_compat::process_env::set("PATH", &path);
    let mut actual = Vec::new();
    for (index, closed) in [false, true].into_iter().enumerate() {
        actual.push(
            execute_abort(
                root.path(),
                &json!({"directory":root.path().join(format!("native-{index}")),"closed":closed}),
            )
            .await,
        );
    }
    pi_js_compat::process_env::set("PATH", &old_path);
    assert_eq!(JsValue::Array(actual), expected["output"]);
}
