use crate::shared_oracle as oracle;

use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::node_fs::operation_error;
use pi_coding_agent::core::tools::write::{LocalWriteOperations, WriteOperations, WriteTool, WriteToolOptions};
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::io;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;
use tokio::sync::{Notify, Semaphore};

struct Gate {
    started: Notify,
    release: Semaphore,
}
struct Operations {
    label: &'static str,
    root: JsString,
    phase: JsString,
    reject: Option<JsString>,
    gate: Option<Arc<Gate>>,
    events: Arc<Mutex<Vec<JsValue>>>,
}
impl Operations {
    fn record(&self, operation: &str, state: &str, path: &JsString, content: Option<&JsString>) {
        let mut event = json!({"label":self.label,"operation":operation,"state":state,"path":path.slice(self.root.len(),path.len())});
        if let Some(content) = content {
            event.as_object_mut().unwrap().insert("content".into(), json!(content));
        }
        self.events.lock().unwrap().push(event);
    }
    async fn wait(&self, operation: &str, path: &JsString) -> io::Result<()> {
        self.record(operation, "start", path, None);
        if let Some(gate) = &self.gate
            && self.phase == operation
        {
            gate.started.notify_one();
            gate.release.acquire().await.unwrap().forget();
            if let Some(message) = &self.reject {
                self.record(operation, "reject", path, None);
                return Err(operation_error(message.clone()));
            }
        }
        Ok(())
    }
}
impl WriteOperations for Operations {
    fn mkdir<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.wait("mkdir", path).await?;
            LocalWriteOperations.mkdir(path).await?;
            self.record("mkdir", "end", path, None);
            Ok(())
        })
    }
    fn write_file<'a>(&'a self, path: &'a JsString, content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.wait("write", path).await?;
            LocalWriteOperations.write_file(path, content).await?;
            self.record("write", "end", path, Some(content));
            Ok(())
        })
    }
}
fn outcome(value: Result<AgentToolResult, ToolError>, root: &JsString) -> JsValue {
    match value {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message.replace(root, "ROOT")}),
    }
}
async fn run_native(input: &JsValue) -> JsValue {
    let root = input["root"].as_string().unwrap().clone();
    let phase = input["phase"].as_string().unwrap().clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::new(Gate {
        started: Notify::new(),
        release: Semaphore::new(0),
    });
    let make_tool = |label, pending| {
        WriteTool::with_options(
            root.clone(),
            WriteToolOptions {
                operations: Some(Arc::new(Operations {
                    label,
                    root: root.clone(),
                    phase: phase.clone(),
                    reject: input.get("reject").and_then(JsValue::as_string).cloned(),
                    gate: pending,
                    events: events.clone(),
                })),
            },
        )
    };
    let first_tool = make_tool("a", Some(gate.clone()));
    let second_tool = make_tool("b", None);
    let controller = AbortController::new();
    let path = input
        .get("path")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_else(|| "nested/file.txt".into());
    let other = if input["different"] == JsValue::Bool(true) {
        "nested/other.txt".into()
    } else {
        JsString::from("nested/../") + &path
    };
    let mut first = first_tool.execute(
        "a".into(),
        json!({"path":path,"content":"αA"}),
        Some(controller.signal()),
        Arc::new(|_| Ok(())),
        None,
    );
    if input["preabort"] == JsValue::Bool(true) {
        controller.abort(Some(json!({"ignored":"signal reason"}).into()));
        let result = outcome(first.await, &root);
        return json!({"events":events.lock().unwrap().clone(),"results":[result],"content":null});
    }
    tokio::select! {
        _ = gate.started.notified() => {},
        result = &mut first => panic!("first write settled before its operation gate: {result:?}"),
    }
    let mut second = second_tool.execute(
        "b".into(),
        json!({"path":other,"content":"B🐱"}),
        None,
        Arc::new(|_| Ok(())),
        None,
    );
    if input["abort"] == JsValue::Bool(true) {
        controller.abort(Some(json!({"ignored":"signal reason"}).into()));
    }
    assert!(
        matches!(
            std::future::poll_fn(|cx| Poll::Ready(first.as_mut().poll(cx))).await,
            Poll::Pending
        ),
        "abort must not settle the active operation"
    );
    let second_result = if input["different"] == JsValue::Bool(true) {
        Some(
            tokio::time::timeout(Duration::from_secs(10), &mut second)
                .await
                .expect("different file must progress independently"),
        )
    } else {
        assert!(
            matches!(
                std::future::poll_fn(|cx| Poll::Ready(second.as_mut().poll(cx))).await,
                Poll::Pending
            ),
            "same-file write must remain queued"
        );
        // A later independent registration cannot finish until the queued write's
        // asynchronous realpath and registration have completed. Keep polling the
        // queued write: merely observing its first Pending could stop at realpath.
        let probe_tool = WriteTool::new(&root);
        let probe = probe_tool.execute(
            "registration-probe".into(),
            json!({"path":"registration-probe.txt","content":"probe"}),
            None,
            Arc::new(|_| Ok(())),
            None,
        );
        tokio::select! {
            biased;
            result = &mut second => panic!("same-file write bypassed the active operation: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(10), probe) => {
                result.expect("independent registration must not wait for the active operation")
                    .expect("independent registration probe");
            },
        }
        None
    };
    let before = json!({"firstSettled":false,"events":events.lock().unwrap().clone()});
    gate.release.add_permits(1);
    let (a, b) = match second_result {
        Some(b) => (first.await, b),
        None => tokio::join!(first, second),
    };
    let absolute = pi_coding_agent::utils::paths::node_join(&root, &path);
    let content = match tokio::fs::read(js_json::encode_node_utf8(&absolute)).await {
        Ok(bytes) => Some(JsString::from(String::from_utf8(bytes).unwrap())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => panic!("{error}"),
    };
    json!({"before":before,"events":events.lock().unwrap().clone(),"results":[outcome(a, &root),outcome(b, &root)],"content":content})
}

#[tokio::test]
async fn write_queue_stays_locked_through_abort_and_operation_settlement() {
    let mut oracle = oracle::TsOracle::spawn("write-queue-oracle.ts", "packages/coding-agent/src/core/tools/write.ts");
    let cases = [
        json!({"phase":"mkdir","abort":true,"preabort":true}),
        json!({"phase":"mkdir","abort":true,"preabort":true,"path":"nested/null\0file.txt"}),
        json!({"phase":"mkdir","abort":true}),
        json!({"phase":"write","abort":true}),
        json!({"phase":"mkdir","abort":true,"reject":JsString::from_utf16(vec![0xd800])+" mkdir failed"}),
        json!({"phase":"write","abort":true,"reject":JsString::from_utf16(vec![0xdfff])+" write failed"}),
        json!({"phase":"write","abort":false}),
        json!({"phase":"write","abort":true,"different":true}),
        json!({"phase":"write","abort":false,"path":JsString::from("nested/raw-")+JsString::from_utf16(vec![0xd800])+".txt"}),
    ];
    for (index, case) in cases.into_iter().enumerate() {
        let source_root = tempfile::tempdir().unwrap();
        let native_root = tempfile::tempdir().unwrap();
        let mut source = case.clone();
        source
            .as_object_mut()
            .unwrap()
            .insert("root".into(), json!(source_root.path()));
        let mut native = case;
        native
            .as_object_mut()
            .unwrap()
            .insert("root".into(), json!(native_root.path()));
        let expected: JsValue = oracle.request(&json!({"id":index,"cases":[source]}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let actual = run_native(&native).await;
        assert!(
            actual == expected["output"][0],
            "case {index}: {} != {}",
            js_json::stringify_lossless(&actual),
            js_json::stringify_lossless(&expected["output"][0])
        );
    }
}
