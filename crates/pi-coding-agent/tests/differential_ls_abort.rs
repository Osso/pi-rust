use crate::shared_oracle as oracle;

use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::ls::{LsOperations, LsTool, LsToolOptions};
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
    reject: bool,
    started: Notify,
    release: Semaphore,
    done: Notify,
    events: Mutex<Vec<String>>,
}
impl Operations {
    async fn operation(&self, phase: &str) -> io::Result<()> {
        self.events.lock().unwrap().push(format!("{phase}:start"));
        if self.phase == phase {
            self.started.notify_one();
            self.release.acquire().await.unwrap().forget();
            if self.reject {
                self.events.lock().unwrap().push(format!("{phase}:reject"));
                if phase != "stat-a" {
                    self.done.notify_one();
                }
                return Err(io::Error::other("operation rejected"));
            }
        }
        self.events.lock().unwrap().push(format!("{phase}:end"));
        if phase == "stat-b" {
            self.done.notify_one();
        }
        Ok(())
    }
}
impl LsOperations for Operations {
    fn exists<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            self.operation("exists").await?;
            Ok(true)
        })
    }
    fn is_directory<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async move {
            let phase = if path == "/repo/list" {
                "stat-root"
            } else if path.ends_with("/a") {
                "stat-a"
            } else {
                "stat-b"
            };
            self.operation(phase).await?;
            Ok(phase == "stat-root")
        })
    }
    fn readdir<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, io::Result<Vec<JsString>>> {
        Box::pin(async move {
            self.operation("readdir").await?;
            Ok(vec!["a".into(), "b".into()])
        })
    }
}
fn outcome(result: Result<AgentToolResult, ToolError>) -> JsValue {
    match result {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    }
}
#[tokio::test]
async fn ls_abort_rejects_observer_without_dropping_the_running_producer() {
    let cases: Vec<_> = ["exists", "stat-root", "readdir", "stat-a"]
        .into_iter()
        .flat_map(|phase| [json!({"phase":phase}), json!({"phase":phase,"reject":true})])
        .chain([json!({"phase":"exists","preabort":true})])
        .collect();
    let mut source = oracle::TsOracle::spawn("ls-abort-oracle.ts", "packages/coding-agent/src/core/tools/ls.ts");
    let expected: JsValue = source.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut failures = Vec::new();
    for (index, input) in cases.iter().enumerate() {
        let operations = Arc::new(Operations {
            phase: input["phase"].as_string().unwrap().clone(),
            reject: input["reject"] == JsValue::Bool(true),
            started: Notify::new(),
            release: Semaphore::new(0),
            done: Notify::new(),
            events: Mutex::new(Vec::new()),
        });
        let tool = LsTool::with_options(
            "/repo",
            LsToolOptions {
                operations: Some(operations.clone()),
            },
        );
        let controller = AbortController::new();
        let preabort = input["preabort"] == JsValue::Bool(true);
        if preabort {
            controller.abort(Some(json!({"ignored":"reason"}).into()));
        }
        let mut execution = tool.execute(
            "ls-abort".into(),
            json!({"path":"list"}),
            Some(controller.signal()),
            Arc::new(|_| Ok(())),
            None,
        );
        let actual = if preabort {
            json!({"result":outcome(execution.await),"events":operations.events.lock().unwrap().clone(),"continued":true})
        } else {
            tokio::select! {_=operations.started.notified()=>{},result=&mut execution=>panic!("settled before gate: {result:?}")}
            controller.abort(Some(json!({"ignored":"reason"}).into()));
            let result = tokio::time::timeout(Duration::from_secs(5), &mut execution)
                .await
                .expect("abort must reject while operation remains pending");
            let before = operations.events.lock().unwrap().clone();
            operations.release.add_permits(1);
            let continued = tokio::time::timeout(Duration::from_millis(300), operations.done.notified())
                .await
                .is_ok();
            tokio::task::yield_now().await;
            json!({"result":outcome(result),"before":before,"events":operations.events.lock().unwrap().clone(),"continued":continued})
        };
        if actual != expected["output"][index] {
            failures.push(format!(
                "case {index}: {} != {}",
                js_json::stringify_lossless(&actual),
                js_json::stringify_lossless(&expected["output"][index])
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
