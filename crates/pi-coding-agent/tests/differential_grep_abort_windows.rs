use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::grep::{GrepOperations, GrepTool, GrepToolOptions};
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};

struct Operations {
    phase: JsString,
    started: Notify,
    release: Semaphore,
    events: Mutex<Vec<String>>,
}
impl Operations {
    async fn operation(&self, phase: &str) {
        self.events.lock().unwrap().push(format!("{phase}:start"));
        if self.phase == phase {
            self.started.notify_one();
            self.release.acquire().await.unwrap().forget();
        }
        self.events.lock().unwrap().push(format!("{phase}:end"));
    }
}
impl GrepOperations for Operations {
    fn is_directory<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.operation("isDirectory").await;
            Ok(true)
        })
    }
    fn read_file<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<JsString>> {
        Box::pin(async move {
            self.operation("readFile").await;
            Ok("one\ntwo\nthree".into())
        })
    }
}
fn outcome(value: Result<AgentToolResult, ToolError>) -> JsValue {
    match value {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    }
}
fn fake_rg(root: &std::path::Path) -> String {
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap();
    let body = r#"if(process.argv[2]==='--version')process.exit(0);const fs=require('node:fs');
fs.writeSync(1,JSON.stringify({type:'match',data:{path:{text:'/repo/search/a.txt'},line_number:2,lines:{text:'inline\n'}}})+'\n');process.exit(0);
"#;
    executable_script::write_executable_script(&bin.join("rg"), &format!("#!{}\n{body}", node.trim()));
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}
#[tokio::test]
async fn grep_abort_before_registration_and_after_close_is_ignored_like_frozen() {
    let root = tempfile::tempdir().unwrap();
    let path = fake_rg(root.path());
    let cases = vec![
        json!({"phase":"isDirectory"}),
        json!({"phase":"readFile"}),
        json!({"phase":"isDirectory","preabort":true}),
    ];
    let mut source = oracle::TsOracle::spawn_with_environment(
        "grep-abort-window-oracle.ts",
        "packages/coding-agent/src/core/tools/grep.ts",
        &[("PATH", path.clone().into())],
    );
    let expected: JsValue = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"].as_array().unwrap().len(), cases.len());
    let old_path = pi_js_compat::process_env::get("PATH").unwrap();
    pi_js_compat::process_env::set("PATH", &path);
    let mut actual = Vec::new();
    for case in &cases {
        let operations = Arc::new(Operations {
            phase: case["phase"].as_string().unwrap().clone(),
            started: Notify::new(),
            release: Semaphore::new(0),
            events: Mutex::new(Vec::new()),
        });
        let controller = AbortController::new();
        let preabort = case["preabort"] == JsValue::Bool(true);
        if preabort {
            controller.abort(None);
        }
        let context = if operations.phase == "readFile" { 1 } else { 0 };
        let execution = GrepTool::with_options(
            "/repo",
            GrepToolOptions {
                operations: Some(operations.clone()),
            },
        )
        .execute(
            "grep-window".into(),
            json!({"pattern":"x","path":"search","context":context}),
            Some(controller.signal()),
            Arc::new(|_| Ok(())),
            None,
        );
        let mut task = tokio::spawn(execution);
        let value = if preabort {
            json!({"result":outcome(task.await.unwrap()),"events":operations.events.lock().unwrap().clone()})
        } else {
            tokio::select! {_=operations.started.notified()=>{},result=&mut task=>panic!("settled before gate: {result:?}")}
            controller.abort(Some(json!({"ignored":"reason"}).into()));
            tokio::task::yield_now().await;
            let settled_before_release = task.is_finished();
            operations.release.add_permits(1);
            let result = tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .expect("gate release must complete grep")
                .unwrap();
            json!({"result":outcome(result),"events":operations.events.lock().unwrap().clone(),"settledBeforeRelease":settled_before_release})
        };
        actual.push(value);
    }
    pi_js_compat::process_env::set("PATH", &old_path);
    assert_eq!(JsValue::Array(actual), expected["output"]);
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
