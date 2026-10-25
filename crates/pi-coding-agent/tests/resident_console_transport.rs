use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_coding_agent::core::resident_console_transport::{
    RESIDENT_CONSOLE_PROTOCOL_VERSION, ResidentConsoleClient, ResidentConsoleEvent, ResidentConsoleHost,
    ResidentConsoleServer, ResidentConsoleServerOptions, ResidentConsoleService, ResidentConsoleSnapshot, Unsubscribe,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

type Snapshot = ResidentConsoleSnapshot<Value>;
type Listeners = Arc<Mutex<Vec<(u64, Arc<dyn Fn(Value) + Send + Sync>)>>>;

fn snapshot() -> Snapshot {
    ResidentConsoleSnapshot {
        service: ResidentConsoleService::Supervisor,
        session_id: "supervisor".into(),
        cwd: "/resident".into(),
        generation: 9,
        identity: None,
        branch: vec![json!({"value": "existing"})],
    }
}

struct TestHost {
    snapshot: Snapshot,
    prompts: Mutex<Vec<(JsString, JsString)>>,
    listeners: Listeners,
    next_listener: AtomicU64,
    rejection: Mutex<Option<JsString>>,
}

impl TestHost {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            snapshot: snapshot(),
            prompts: Mutex::new(Vec::new()),
            listeners: Arc::new(Mutex::new(Vec::new())),
            next_listener: AtomicU64::new(1),
            rejection: Mutex::new(None),
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

impl ResidentConsoleHost<Value, Value> for TestHost {
    fn get_snapshot(&self) -> Snapshot {
        self.snapshot.clone()
    }

    fn enqueue_prompt(
        &self,
        text: JsString,
        id: JsString,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), JsString>> + Send>> {
        self.prompts.lock().unwrap().push((id, text));
        let rejection = self.rejection.lock().unwrap().clone();
        Box::pin(async move { rejection.map_or(Ok(()), Err) })
    }

    fn subscribe(&self, listener: Arc<dyn Fn(Value) + Send + Sync>) -> Unsubscribe {
        let id = self.next_listener.fetch_add(1, Ordering::SeqCst);
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

fn options(socket_path: &str, host: Arc<TestHost>) -> ResidentConsoleServerOptions<Value, Value> {
    ResidentConsoleServerOptions {
        socket_path: socket_path.into(),
        service: ResidentConsoleService::Supervisor,
        host,
    }
}

struct ImmediateEventHost;

impl ResidentConsoleHost<Value, Value> for ImmediateEventHost {
    fn get_snapshot(&self) -> Snapshot {
        snapshot()
    }

    fn enqueue_prompt(
        &self,
        _text: JsString,
        _id: JsString,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), JsString>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn subscribe(&self, listener: Arc<dyn Fn(Value) + Send + Sync>) -> Unsubscribe {
        listener(json!({"type": "ready"}));
        Box::new(|| {})
    }
}

#[tokio::test]
async fn sends_snapshot_before_events_emitted_during_subscription() {
    let (_directory, path) = socket_path();
    let server = ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: path.clone().into(),
        service: ResidentConsoleService::Supervisor,
        host: Arc::new(ImmediateEventHost),
    });
    server.start().await.unwrap();
    let mut stream = UnixStream::connect(&path).await.unwrap();
    stream
        .write_all(b"{\"type\":\"attach\",\"version\":1,\"service\":\"supervisor\"}\n")
        .await
        .unwrap();
    let mut lines = BufReader::new(stream).lines();
    let first: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let second: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(first["type"], "attached");
    assert_eq!(second, json!({"type":"event", "sequence":1,"event":{"type":"ready"}}));
    server.close().await.unwrap();
}

#[tokio::test]
async fn rejects_new_prompts_after_remote_disconnect() {
    let (_directory, path) = socket_path();
    let host = TestHost::new();
    let server = ResidentConsoleServer::new(options(&path, host));
    server.start().await.unwrap();
    let client: ResidentConsoleClient<Value, Value> =
        ResidentConsoleClient::connect(&path, ResidentConsoleService::Supervisor)
            .await
            .unwrap();
    let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
    let _unsubscribe = client.on_disconnect(Arc::new(move |error| {
        sent.send(error).unwrap();
    }));
    server.close().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), received.recv())
            .await
            .unwrap(),
        Some(None)
    );
    let result = tokio::time::timeout(Duration::from_secs(2), client.prompt("late", "not queued")).await;
    assert_eq!(
        result.expect("disconnected prompt must not hang"),
        Err("Resident console is closed".into())
    );
    client.close().await;
}

#[tokio::test]
async fn reports_node_errors_for_existing_file_parent_and_connecting_to_a_directory() {
    let (directory, path) = socket_path();
    let blocked = directory.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let server = ResidentConsoleServer::new(options(blocked.join("console.sock").to_str().unwrap(), TestHost::new()));
    assert_eq!(
        server.start().await.unwrap_err(),
        format!("EEXIST: file already exists, mkdir '{}'", blocked.display())
    );
    let error = match ResidentConsoleClient::<Value, Value>::connect(
        directory.path().to_str().unwrap(),
        ResidentConsoleService::Supervisor,
    )
    .await
    {
        Ok(_) => panic!("directory cannot be a resident socket"),
        Err(error) => error,
    };
    // Linux connect to a directory is ECONNREFUSED, which maps to the public unavailable error.
    assert_eq!(
        error,
        format!("Resident console is unavailable: {}", directory.path().display())
    );
    assert!(!std::path::Path::new(&path).exists());
}

#[tokio::test]
async fn reports_disconnect_before_snapshot() {
    let (_directory, path) = socket_path();
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut lines = BufReader::new(stream).lines();
        assert!(lines.next_line().await.unwrap().is_some());
    });
    let error = match ResidentConsoleClient::<Value, Value>::connect(&path, ResidentConsoleService::Supervisor).await {
        Ok(_) => panic!("closed peer cannot attach"),
        Err(error) => error,
    };
    assert_eq!(error, "Resident console disconnected before attach");
    peer.await.unwrap();
}

#[tokio::test]
async fn attaches_to_the_existing_resident_and_receives_its_full_snapshot() {
    let (_directory, path) = socket_path();
    let host = TestHost::new();
    let server = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    server.start().await.unwrap();

    let client: ResidentConsoleClient<Value, Value> =
        ResidentConsoleClient::connect(&path, ResidentConsoleService::Supervisor)
            .await
            .expect("client must attach");
    assert_eq!(client.snapshot(), &snapshot());
    client.close().await;
    server.close().await.unwrap();
}

#[tokio::test]
async fn submits_prompts_and_receives_monotonic_events() {
    let (_directory, path) = socket_path();
    let host = TestHost::new();
    let server = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    server.start().await.unwrap();
    let client: ResidentConsoleClient<Value, Value> =
        ResidentConsoleClient::connect(&path, ResidentConsoleService::Supervisor)
            .await
            .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let _subscription = client.on_event(Arc::new(move |event: &ResidentConsoleEvent<Value>| {
        sink.lock().unwrap().push(event.clone());
    }));

    client
        .prompt("prompt-1", "inspect this")
        .await
        .expect("prompt accepted");
    host.publish(json!({"type": "agent_start"}));
    host.publish(json!({"type": "agent_end"}));
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(
        host.prompts(),
        vec![(JsString::from("prompt-1"), JsString::from("inspect this"))]
    );
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            ResidentConsoleEvent {
                sequence: 1,
                event: json!({"type": "agent_start"})
            },
            ResidentConsoleEvent {
                sequence: 2,
                event: json!({"type": "agent_end"})
            }
        ]
    );
    client.close().await;
    server.close().await.unwrap();
}

#[tokio::test]
async fn transfers_writable_ownership_to_the_newest_console_client() {
    let (_directory, path) = socket_path();
    let host = TestHost::new();
    let server = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    server.start().await.unwrap();
    let first: ResidentConsoleClient<Value, Value> =
        ResidentConsoleClient::connect(&path, ResidentConsoleService::Supervisor)
            .await
            .unwrap();
    let disconnected = Arc::new(tokio::sync::Notify::new());
    let notify = Arc::clone(&disconnected);
    let _subscription = first.on_disconnect(Arc::new(move |_| notify.notify_one()));

    let second: ResidentConsoleClient<Value, Value> =
        ResidentConsoleClient::connect(&path, ResidentConsoleService::Supervisor)
            .await
            .unwrap();
    tokio::time::timeout(Duration::from_secs(2), disconnected.notified())
        .await
        .expect("the first client must be disconnected");
    assert_eq!(second.snapshot(), &snapshot());
    second
        .prompt("prompt-2", "new owner")
        .await
        .expect("new owner may prompt");
    second.close().await;
    server.close().await.unwrap();
}

#[tokio::test]
async fn fails_explicitly_when_the_resident_socket_is_absent() {
    let (_directory, path) = socket_path();
    let error = match ResidentConsoleClient::<Value, Value>::connect(&path, ResidentConsoleService::Architect).await {
        Ok(_) => panic!("missing socket must fail"),
        Err(error) => error,
    };
    assert_eq!(error, format!("Resident console is unavailable: {path}"));
}

#[tokio::test]
async fn rejects_protocol_and_service_mismatches() {
    let (_directory, path) = socket_path();
    let host = TestHost::new();
    let server = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    server.start().await.unwrap();

    let mut stream = UnixStream::connect(&path).await.unwrap();
    stream
        .write_all(b"{\"type\":\"attach\",\"version\":2,\"service\":\"supervisor\"}\n")
        .await
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({"type": "error", "code": "version_mismatch", "message": "Resident console protocol version mismatch"})
    );

    let mut stream = UnixStream::connect(&path).await.unwrap();
    stream
        .write_all(b"{\"type\":\"attach\",\"version\":1,\"service\":\"architect\"}\n")
        .await
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        json!({"type": "error", "code": "service_mismatch", "message": "Resident console service mismatch"})
    );
    server.close().await.unwrap();
}

#[tokio::test]
async fn requires_attach_and_rejects_invalid_messages() {
    let (_directory, path) = socket_path();
    let host = TestHost::new();
    let server = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    server.start().await.unwrap();

    let mut stream = UnixStream::connect(&path).await.unwrap();
    stream
        .write_all(b"{\"type\":\"prompt\",\"id\":\"a\",\"text\":\"b\"}\nnot json\n{\"type\":\"attach\",\"version\":1,\"service\":\"supervisor\"}\n{\"type\":\"prompt\",\"id\":\"\",\"text\":\"b\"}\n")
        .await
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut lines = Vec::new();
    for _ in 0..4 {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        lines.push(serde_json::from_str::<Value>(&line).unwrap());
    }
    assert_eq!(lines[0]["code"], "missing_attach");
    assert_eq!(lines[0]["message"], "Resident console attach is required");
    assert_eq!(lines[1]["code"], "invalid_message");
    assert_eq!(lines[1]["message"], "Invalid resident console message");
    assert_eq!(lines[2]["type"], "attached");
    assert_eq!(lines[3]["code"], "invalid_message");
    server.close().await.unwrap();
}

#[tokio::test]
async fn reports_prompt_rejections_and_rejects_duplicate_ids() {
    let (_directory, path) = socket_path();
    let host = TestHost::new();
    *host.rejection.lock().unwrap() = Some("policy denied".into());
    let server = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    server.start().await.unwrap();
    let client: ResidentConsoleClient<Value, Value> =
        ResidentConsoleClient::connect(&path, ResidentConsoleService::Supervisor)
            .await
            .unwrap();

    let error = client.prompt("p1", "text").await.expect_err("rejection must surface");
    assert_eq!(error, "policy denied");
    assert_eq!(
        client.prompt("", "text").await.expect_err("empty id"),
        "Resident console prompt id and text are required"
    );
    assert_eq!(
        client.prompt("p2", "").await.expect_err("empty text"),
        "Resident console prompt id and text are required"
    );
    client.close().await;
    assert_eq!(
        client.prompt("p3", "text").await.expect_err("closed client"),
        "Resident console is closed"
    );
    server.close().await.unwrap();
}

#[tokio::test]
async fn rejects_an_active_socket_and_recovers_a_stale_one() {
    let (_directory, path) = socket_path();
    let host = TestHost::new();
    let first = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    first.start().await.unwrap();
    let second = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    assert_eq!(
        second.start().await.expect_err("active socket"),
        format!("Resident console socket is already active: {path}")
    );
    first.close().await.unwrap();

    std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
    let stale = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(stale);
    let third = ResidentConsoleServer::new(options(&path, Arc::clone(&host)));
    third.start().await.expect("stale socket must be replaced");
    third.close().await.unwrap();
}

#[test]
fn exposes_the_protocol_version_and_service_names() {
    assert_eq!(RESIDENT_CONSOLE_PROTOCOL_VERSION, 1);
    assert_eq!(
        serde_json::to_value([ResidentConsoleService::Architect, ResidentConsoleService::Supervisor]).unwrap(),
        json!(["architect", "supervisor"])
    );
}
