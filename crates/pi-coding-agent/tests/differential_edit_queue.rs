use crate::shared_oracle as oracle;

use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::edit::{EditOperations, EditTool, EditToolOptions, LocalEditOperations};
use pi_coding_agent::core::tools::file_mutation_queue::with_file_mutation_queue;
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
    phase: JsString,
    reject: bool,
    gate: Arc<Gate>,
    events: Arc<Mutex<Vec<String>>>,
}
impl Operations {
    fn record(&self, phase: &str, state: &str) {
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:{phase}:{state}", self.label));
    }
    async fn enter(&self, phase: &str) -> io::Result<()> {
        self.record(phase, "start");
        if self.label == "a" && self.phase == phase {
            self.gate.started.notify_one();
            self.gate.release.acquire().await.unwrap().forget();
            if self.reject {
                self.record(phase, "reject");
                return Err(pi_coding_agent::core::tools::node_fs::operation_error(
                    "operation failed",
                ));
            }
        }
        Ok(())
    }
}
impl EditOperations for Operations {
    fn access<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.enter("access").await?;
            LocalEditOperations.access(path).await?;
            self.record("access", "end");
            Ok(())
        })
    }
    fn read_file<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async move {
            self.enter("read").await?;
            let bytes = LocalEditOperations.read_file(path).await?;
            self.record("read", "end");
            Ok(bytes)
        })
    }
    fn write_file<'a>(&'a self, path: &'a JsString, content: &'a JsString) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async move {
            self.enter("write").await?;
            LocalEditOperations.write_file(path, content).await?;
            self.record("write", "end");
            Ok(())
        })
    }
}
fn outcome(result: Result<AgentToolResult, ToolError>) -> JsValue {
    match result {
        Ok(result) => json!({"ok":true,"value":js_json::to_value(&result).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    }
}
async fn run_native(root: &std::path::Path, input: &JsValue) -> JsValue {
    for name in ["file.txt", "other.txt"] {
        tokio::fs::write(root.join(name), b"before\nkeep\n").await.unwrap();
    }
    let gate = Arc::new(Gate {
        started: Notify::new(),
        release: Semaphore::new(0),
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    let make_tool = |label| {
        EditTool::with_options(
            root.to_str().unwrap(),
            EditToolOptions {
                operations: Some(Arc::new(Operations {
                    label,
                    phase: input["phase"].as_string().unwrap().clone(),
                    reject: input["reject"] == JsValue::Bool(true),
                    gate: gate.clone(),
                    events: events.clone(),
                })),
            },
        )
    };
    let controller = AbortController::new();
    let first = tokio::spawn(make_tool("a").execute(
        "a".into(),
        json!({"path":"file.txt","edits":[{"oldText":"before","newText":"first"}]}),
        Some(controller.signal()),
        Arc::new(|_| Ok(())),
        None,
    ));
    tokio::time::timeout(Duration::from_secs(10), gate.started.notified())
        .await
        .expect("first operation entered gate");
    let different = input["different"] == JsValue::Bool(true);
    let mut second = make_tool("b").execute(
        "b".into(),
        json!({"path":if different {"other.txt"} else {"./file.txt"},"edits":[{"oldText":"keep","newText":"second"}]}),
        None,
        Arc::new(|_| Ok(())),
        None,
    );
    if input["abort"] == JsValue::Bool(true) {
        controller.abort(Some(json!("ignored reason").into()));
    }
    let independent = if different {
        Some(
            tokio::time::timeout(Duration::from_secs(10), &mut second)
                .await
                .expect("different file must proceed"),
        )
    } else {
        assert!(matches!(
            std::future::poll_fn(|cx| Poll::Ready(second.as_mut().poll(cx))).await,
            Poll::Pending
        ));
        let probe_path = JsString::from(root.to_str().unwrap()) + "/registration-probe";
        let probe = with_file_mutation_queue(&probe_path, || async {});
        tokio::select! {
            biased;
            result=&mut second=>panic!("queued edit bypassed active operation: {result:?}"),
            result=tokio::time::timeout(Duration::from_secs(10),probe)=>{result.expect("independent registration timeout").expect("probe registration");},
        }
        None
    };
    let before = json!({"settled":first.is_finished(),"events":events.lock().unwrap().clone()});
    gate.release.add_permits(1);
    let a = first.await.unwrap();
    let b = match independent {
        Some(result) => result,
        None => second.await,
    };
    let mut contents = Vec::new();
    for name in ["file.txt", "other.txt"] {
        contents.push(tokio::fs::read_to_string(root.join(name)).await.unwrap());
    }
    json!({"before":before,"events":events.lock().unwrap().clone(),"results":[outcome(a),outcome(b)],"contents":contents})
}

#[tokio::test]
async fn edit_queue_stays_locked_and_preserves_phase_specific_abort_precedence() {
    let mut source = oracle::TsOracle::spawn("edit-queue-oracle.ts", "packages/coding-agent/src/core/tools/edit.ts");
    let cases = [
        json!({"phase":"access","abort":true}),
        json!({"phase":"read","abort":true}),
        json!({"phase":"write","abort":true}),
        json!({"phase":"access","abort":true,"reject":true}),
        json!({"phase":"read","abort":true,"reject":true}),
        json!({"phase":"write","abort":true,"reject":true}),
        json!({"phase":"write","abort":false}),
        json!({"phase":"write","abort":true,"different":true}),
        json!({"phase":"access","abort":false,"reject":true}),
    ];
    for (index, input) in cases.into_iter().enumerate() {
        let source_root = tempfile::tempdir().unwrap();
        let native_root = tempfile::tempdir().unwrap();
        let mut request = input.clone();
        request
            .as_object_mut()
            .unwrap()
            .insert("root".into(), json!(source_root.path()));
        let expected: JsValue = source.request(&json!({"id":index,"cases":[request]}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let actual = run_native(native_root.path(), &input).await;
        assert_eq!(actual, expected["output"][0], "scenario {index}");
    }
}
