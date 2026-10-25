//! Cross-language wire compatibility for the resident console transport:
//! the pinned TypeScript client against the Rust server and vice versa.

use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_coding_agent::core::resident_console_transport::{
    ResidentConsoleClient, ResidentConsoleHost, ResidentConsoleServer, ResidentConsoleServerOptions,
    ResidentConsoleService, ResidentConsoleSnapshot, Unsubscribe,
};
use serde_json::{Value, json};

type Listeners = Arc<Mutex<Vec<(u64, Arc<dyn Fn(Value) + Send + Sync>)>>>;

fn snapshot() -> ResidentConsoleSnapshot<Value> {
    ResidentConsoleSnapshot {
        service: ResidentConsoleService::Supervisor,
        session_id: "supervisor".into(),
        cwd: "/resident".into(),
        generation: 9,
        identity: None,
        branch: vec![json!({"value": "existing"})],
    }
}

struct CrossHost {
    snapshot: ResidentConsoleSnapshot<Value>,
    prompts: Mutex<Vec<(JsString, JsString)>>,
    listeners: Listeners,
    next_listener: Mutex<u64>,
}

impl CrossHost {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            snapshot: snapshot(),
            prompts: Mutex::new(Vec::new()),
            listeners: Arc::new(Mutex::new(Vec::new())),
            next_listener: Mutex::new(1),
        })
    }

    fn publish(&self, event: Value) {
        for (_, listener) in self.listeners.lock().unwrap().iter() {
            listener(event.clone());
        }
    }

    fn prompts(&self) -> Vec<(JsString, JsString)> {
        self.prompts.lock().unwrap().clone()
    }
}

impl ResidentConsoleHost<Value, Value> for CrossHost {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<Value> {
        self.snapshot.clone()
    }

    fn enqueue_prompt(
        &self,
        text: JsString,
        id: JsString,
    ) -> Pin<Box<dyn Future<Output = Result<(), JsString>> + Send>> {
        self.prompts.lock().unwrap().push((id, text));
        self.publish(json!({"type": "agent_start"}));
        self.publish(json!({"type": "agent_end"}));
        Box::pin(async { Ok(()) })
    }

    fn subscribe(&self, listener: Arc<dyn Fn(Value) + Send + Sync>) -> Unsubscribe {
        let id = *self.next_listener.lock().unwrap();
        *self.next_listener.lock().unwrap() += 1;
        self.listeners.lock().unwrap().push((id, listener));
        let listeners = Arc::clone(&self.listeners);
        Box::new(move || {
            listeners.lock().unwrap().retain(|(listener_id, _)| *listener_id != id);
        })
    }
}

fn socket_path() -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("console.sock");
    (directory, path.to_str().expect("utf-8 test path").to_owned())
}

#[tokio::test(flavor = "multi_thread")]
async fn typescript_client_attaches_prompts_and_receives_rust_server_events() {
    let (_directory, path) = socket_path();
    let host = CrossHost::new();
    let server = ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: path.clone().into(),
        service: ResidentConsoleService::Supervisor,
        host: Arc::clone(&host) as Arc<dyn ResidentConsoleHost<Value, Value>>,
    });
    server.start().await.unwrap();

    let mut oracle = oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src");
    let response = oracle.request(&json!({
        "id": 27_020,
        "module": "core/resident-console-transport.ts",
        "operation": "residentConsoleClient",
        "cases": [],
        "socketPath": path,
        "prompt": {"id": "prompt-1", "text": "inspect this"},
        "eventCount": 2,
    }));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let output = &response["output"];
    assert_eq!(
        output["snapshot"],
        json!({
            "service": "supervisor",
            "sessionId": "supervisor",
            "cwd": "/resident",
            "generation": 9,
            "branch": [{"value": "existing"}],
        })
    );
    assert_eq!(
        output["events"],
        json!([
            {"sequence": 1, "event": {"type": "agent_start"}},
            {"sequence": 2, "event": {"type": "agent_end"}},
        ])
    );
    assert_eq!(output["promptError"], Value::Null);
    assert_eq!(
        host.prompts(),
        vec![(JsString::from("prompt-1"), JsString::from("inspect this"))]
    );
    server.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn rust_client_attaches_prompts_and_receives_typescript_server_events() {
    let (_directory, path) = socket_path();
    let mut oracle = oracle::TsOracle::spawn("coding-agent-core-leaves-oracle.ts", "packages/coding-agent/src");
    let started = oracle.request(&json!({
        "id": 27_021,
        "module": "core/resident-console-transport.ts",
        "operation": "residentConsoleServer",
        "cases": [],
        "socketPath": path,
        "snapshot": {
            "service": "supervisor",
            "sessionId": "typescript",
            "cwd": "/ts",
            "generation": 3,
            "branch": [{"value": "ts"}],
        },
    }));
    assert_eq!(started["ok"], json!(true), "oracle failed: {started}");

    let client: ResidentConsoleClient<Value, Value> =
        ResidentConsoleClient::connect(&path, ResidentConsoleService::Supervisor)
            .await
            .expect("Rust client must attach to the TypeScript server");
    assert_eq!(client.snapshot().session_id, "typescript");
    assert_eq!(client.snapshot().generation, 3);

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let _subscription = client.on_event(Arc::new(move |event| sink.lock().unwrap().push(event.clone())));
    client
        .prompt("rust-1", "hello from rust")
        .await
        .expect("prompt accepted");

    let received = oracle.request(&json!({
        "id": 27_022,
        "module": "core/resident-console-transport.ts",
        "operation": "residentConsoleReceived",
        "cases": [],
    }));
    assert_eq!(
        received["output"]["prompts"],
        json!([{"id": "rust-1", "text": "hello from rust"}])
    );

    let published = oracle.request(&json!({
        "id": 27_023,
        "module": "core/resident-console-transport.ts",
        "operation": "residentConsolePublish",
        "cases": [],
        "event": {"type": "advisory"},
    }));
    assert_eq!(published["output"]["published"], json!(1));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            pi_coding_agent::core::resident_console_transport::ResidentConsoleEvent {
                sequence: 1,
                event: json!({"type": "advisory"}),
            }
        ]
    );

    client.close().await;
    let stopped = oracle.request(&json!({
        "id": 27_024,
        "module": "core/resident-console-transport.ts",
        "operation": "residentConsoleStop",
        "cases": [],
    }));
    assert_eq!(stopped["ok"], json!(true), "oracle failed: {stopped}");
}
