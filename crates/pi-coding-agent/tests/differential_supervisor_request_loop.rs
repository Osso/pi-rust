//! Differential: `supervisor/main.ts` request loop against the pinned TypeScript module.

#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::session_control_db::{
    SupervisorRequestKind, post_supervisor_request, read_supervisor_request,
};
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionEntry, SessionManager};
use pi_coding_agent::supervisor::main::{
    RunSupervisorRequestLoopInput, SupervisorConsolePromptQueue, SupervisorSession, process_supervisor_request,
    run_supervisor_request_loop,
};
use pi_coding_agent::supervisor::request_wake::SupervisorRequestWakeServer;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

const SOURCE_MODULE: &str = "packages/coding-agent/src/supervisor/main.ts";

struct FakeSession {
    manager: Arc<Mutex<SessionManager>>,
}

impl SupervisorSession for FakeSession {
    fn abort(&self) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }

    fn prompt(&self, _content: String) -> BoxFuture<'static, Result<(), String>> {
        let manager = Arc::clone(&self.manager);
        Box::pin(async move {
            let message: AgentMessage = serde_json::from_value(json!({
                "role": "assistant",
                "content": [{"type": "text", "text": "{\"kind\":\"approve\",\"reason\":\"ok\"}"}],
                "api": "faux",
                "provider": "faux",
                "model": "faux",
                "usage": {
                    "input": 0,
                    "output": 0,
                    "cacheRead": 0,
                    "cacheWrite": 0,
                    "totalTokens": 0,
                    "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}
                },
                "stopReason": "stop",
                "timestamp": 0
            }))
            .expect("assistant message");
            manager.lock().unwrap().append_message(message).expect("append");
            Ok(())
        })
    }

    fn get_branch(&self) -> Vec<SessionEntry> {
        self.manager.lock().unwrap().get_branch(None)
    }

    fn get_leaf_id(&self) -> Option<String> {
        self.manager.lock().unwrap().get_leaf_id().map(str::to_owned)
    }
}

fn normalize_session_file(path: &str) -> Vec<Value> {
    let content = std::fs::read_to_string(path).expect("session file");
    content
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).expect("session line");
            normalize_entry(&mut value);
            value
        })
        .collect()
}

fn normalize_entry(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_entry(item);
            }
        }
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if key == "id" || key == "parentId" {
                    *child = Value::String("<id>".to_owned());
                } else if key == "timestamp" {
                    *child = Value::String("<timestamp>".to_owned());
                } else if key == "cwd" {
                    *child = Value::String("<cwd>".to_owned());
                } else {
                    normalize_entry(child);
                }
            }
        }
        _ => {}
    }
}

async fn run_rust(dir: &str) -> Value {
    let control_db_path = format!("{dir}/control.sqlite");
    let session_dir = format!("{dir}/sessions");
    let manager = SessionManager::create(
        dir,
        Some(&session_dir),
        Some(NewSessionOptions {
            id: Some("supervisor".to_owned()),
            ..Default::default()
        }),
    )
    .expect("session manager");
    post_supervisor_request(
        &control_db_path,
        &pi_coding_agent::core::session_control_db::PostSupervisorRequestInput {
            sender_session_id: "requester".to_owned(),
            project_id: "pi".to_owned(),
            kind: SupervisorRequestKind::ApprovalReview,
            payload: Map::from_iter([("toolName".to_owned(), Value::String("read".to_owned()))]),
            deadline_at: "2099-01-01T00:00:00.000Z".to_owned(),
        },
    )
    .expect("post request");
    let session_file = manager.get_session_file().expect("session file").to_owned();
    let manager = Arc::new(Mutex::new(manager));
    let signal = CancellationToken::new();
    let session: Arc<dyn SupervisorSession> = Arc::new(FakeSession {
        manager: Arc::clone(&manager),
    });
    let wake_server = Arc::new(SupervisorRequestWakeServer::new(&control_db_path));
    wake_server.start().await.expect("wake server");
    let process_signal = signal.clone();
    run_supervisor_request_loop(RunSupervisorRequestLoopInput {
        claim_token: "claim-token".to_owned(),
        console_prompts: Arc::new(Mutex::new(SupervisorConsolePromptQueue::new())),
        control_db_path: control_db_path.clone(),
        session: Arc::clone(&session),
        signal: signal.clone(),
        wake_server: Arc::clone(&wake_server),
        claim_next_request: None,
        process_request: Some(Arc::new(move |db_path, request, session| {
            let process_signal = process_signal.clone();
            Box::pin(async move {
                process_supervisor_request(&db_path, request, session).await;
                process_signal.cancel();
            })
        })),
    })
    .await
    .expect("request loop");
    wake_server.close().await.expect("close wake");

    let connection = rusqlite::Connection::open(&control_db_path).expect("control db");
    let mut statement = connection
        .prepare("SELECT id, kind, status, response_json FROM supervisor_requests ORDER BY id")
        .expect("select");
    let rows: Vec<Value> = statement
        .query_map([], |row| {
            let response: Option<String> = row.get(3)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "kind": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "response_json": response
            }))
        })
        .expect("rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("row values");
    assert!(read_supervisor_request(&control_db_path, 1).unwrap().is_some());
    json!({
        "controlRows": rows,
        "session": normalize_session_file(&session_file)
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn supervisor_request_loop_matches_typescript() {
    let rust_dir = tempfile::tempdir().expect("rust temp");
    let ts_dir = tempfile::tempdir().expect("ts temp");
    let rust = run_rust(&rust_dir.path().display().to_string()).await;
    let mut oracle = oracle::TsOracle::spawn("coding-agent-supervisor-request-loop-oracle.ts", SOURCE_MODULE);
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "supervisor-request-loop",
        "dir": ts_dir.path().display().to_string()
    }));
    assert_eq!(response["ok"], true, "{response}");
    let ts = &response["output"];
    assert_eq!(rust["controlRows"], ts["controlRows"]);
    assert_eq!(rust["session"], ts["session"]);
}
