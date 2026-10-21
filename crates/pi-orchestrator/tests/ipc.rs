use pi_ai::request::BoxFuture;
use pi_orchestrator::ipc::{
    client::send_ipc_request,
    server::{IpcRequestHandler, OnMessage, RpcStream, start_ipc_server},
};
use serde_json::{Value, json};
use std::sync::Arc;
struct Handler;
impl IpcRequestHandler for Handler {
    fn handle(&self, request: Value) -> BoxFuture<'_, Result<Value, String>> {
        Box::pin(async move {
            Ok(if request["type"] == "rpc_stream" {
                json!({"type":"rpc_ready","ok":true,"instance":{"id":"i"}})
            } else {
                json!({"type":"list_result","ok":true,"instances":[]})
            })
        })
    }
    fn open_rpc_stream(&self, _id: &str, on_message: OnMessage) -> Option<Arc<dyn RpcStream>> {
        Some(Arc::new(Stream(on_message)))
    }
}
struct Stream(OnMessage);
impl RpcStream for Stream {
    fn handle_request(&self, request: Value) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            (self.0)(json!({"type":"response","command":request["type"],"success":true,"id":request["id"]}));
            Ok(())
        })
    }
    fn close(&self) {}
}
#[tokio::test]
async fn fragmented_jsonl_and_live_socket_rejection() {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ipc.sock");
    let server = start_ipc_server(&path, Arc::new(Handler)).await.unwrap();
    assert!(
        start_ipc_server(&path, Arc::new(Handler))
            .await
            .err()
            .unwrap()
            .contains("already running")
    );
    let actual = send_ipc_request(&path, &json!({"type":"list"})).await.unwrap();
    assert_eq!(actual, json!({"type":"list_result","ok":true,"instances":[]}));
    let mut socket = tokio::net::UnixStream::connect(&path).await.unwrap();
    socket.write_all(b"{\"type\":").await.unwrap();
    tokio::task::yield_now().await;
    socket.write_all(b"\"list\"}\n").await.unwrap();
    let mut text = String::new();
    socket.read_to_string(&mut text).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), actual);
    let socket = tokio::net::UnixStream::connect(&path).await.unwrap();
    let (read, mut write) = socket.into_split();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    write
        .write_all(b"{\"type\":\"rpc_stream\",\"instanceId\":\"i\"}\n")
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["type"], "rpc_ready");
    write
        .write_all(b"{\"type\":\"get_state\",\"id\":\"1\"}\n{\"type\":\"abort\",\"id\":\"2\"}\n")
        .await
        .unwrap();
    for id in ["1", "2"] {
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["id"], id);
    }
    drop(write);
    drop(reader);
    server.close().await;
}

fn peer(dir: &std::path::Path, mode: &str) -> tokio::process::Command {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let mut command = tokio::process::Command::new(source.join("node_modules/.bin/tsx"));
    command
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-ipc-peer.mjs"))
        .arg(source)
        .arg(mode)
        .env("PI_ORCHESTRATOR_DIR", dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true);
    command
}
#[tokio::test]
async fn frozen_ts_client_to_rust_server() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("orchestrator.sock");
    let server = start_ipc_server(&path, Arc::new(Handler)).await.unwrap();
    for request in [json!({"type":"list"}), json!({"type":"status","instanceId":"中"})] {
        let output = peer(dir.path(), "client")
            .arg(request.to_string())
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"type":"list_result","ok":true,"instances":[]})
        );
    }
    server.close().await;
}
#[tokio::test]
async fn rust_client_to_frozen_ts_server_pins_approved_coalesced_discrepancy() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("orchestrator.sock");
    let mut child = peer(dir.path(), "server").spawn().unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .await
        .unwrap();
    assert_eq!(line, "ready\n");
    assert_eq!(
        send_ipc_request(&path, &json!({"type":"list"})).await.unwrap(),
        json!({"type":"list_result","ok":true,"instances":[]})
    );
    pin_coalesced_boundary(&path).await;
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    async fn pin_coalesced_boundary(path: &std::path::Path) {
        let socket = tokio::net::UnixStream::connect(path).await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        let mut line = String::new();
        write
            .write_all(b"{\"type\":\"rpc_stream\",\"instanceId\":\"i\"}\n{\"type\":\"get_state\",\"id\":\"one\"}\n")
            .await
            .unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["type"], "rpc_ready");
        line.clear();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(40), reader.read_line(&mut line))
                .await
                .is_err()
        );
        write.write_all(b"\n").await.unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["id"], "one");
    }
}

#[tokio::test]
async fn session_events_are_written_while_rpc_command_is_pending() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    struct Slow(OnMessage);
    impl RpcStream for Slow {
        fn handle_request(&self, _request: Value) -> BoxFuture<'_, Result<(), String>> {
            Box::pin(async move {
                (self.0)(json!({"type":"message_update"}));
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                Ok(())
            })
        }
        fn close(&self) {}
    }
    struct SlowHandler;
    impl IpcRequestHandler for SlowHandler {
        fn handle(&self, _request: Value) -> BoxFuture<'_, Result<Value, String>> {
            Box::pin(async { Ok(json!({"type":"rpc_ready","ok":true,"instance":{"id":"i"}})) })
        }
        fn open_rpc_stream(&self, _id: &str, on_message: OnMessage) -> Option<Arc<dyn RpcStream>> {
            Some(Arc::new(Slow(on_message)))
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("socket");
    let server = start_ipc_server(&path, Arc::new(SlowHandler)).await.unwrap();
    let socket = tokio::net::UnixStream::connect(&path).await.unwrap();
    let (read, mut write) = socket.into_split();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    write
        .write_all(b"{\"type\":\"rpc_stream\",\"instanceId\":\"i\"}\n")
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    write
        .write_all(b"{\"type\":\"prompt\",\"message\":\"hi\"}\n")
        .await
        .unwrap();
    line.clear();
    tokio::time::timeout(std::time::Duration::from_millis(200), reader.read_line(&mut line))
        .await
        .expect("events must not wait for command completion")
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["type"], "message_update");
    drop(write);
    drop(reader);
    server.close().await;
}

#[tokio::test]
async fn ipc_handler_errors_unknown_stream_and_client_eof() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    struct Failure;
    impl IpcRequestHandler for Failure {
        fn handle(&self, request: Value) -> BoxFuture<'_, Result<Value, String>> {
            Box::pin(async move {
                if request["type"] == "rpc_stream" {
                    Ok(json!({"type":"rpc_ready","ok":true,"instance":{"id":"gone"}}))
                } else {
                    Err("handler failed".into())
                }
            })
        }
        fn open_rpc_stream(&self, _id: &str, _callback: OnMessage) -> Option<Arc<dyn RpcStream>> {
            None
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("socket");
    let server = start_ipc_server(&path, Arc::new(Failure)).await.unwrap();
    assert_eq!(
        send_ipc_request(&path, &json!({"type":"list"})).await.unwrap(),
        json!({"type":"error","ok":false,"error":"handler failed"})
    );
    assert_eq!(
        send_ipc_request(&path, &json!({"type":"rpc_stream","instanceId":"gone"}))
            .await
            .unwrap(),
        json!({"type":"error","ok":false,"error":"Unknown instance: gone"})
    );
    let mut socket = tokio::net::UnixStream::connect(&path).await.unwrap();
    socket.write_all(b"{\n").await.unwrap();
    let mut text = String::new();
    socket.read_to_string(&mut text).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["error"],
        pi_orchestrator::ipc::protocol::parse_request_line("{").unwrap_err()
    );
    server.close().await;
    let eof = dir.path().join("eof");
    let listener = tokio::net::UnixListener::bind(&eof).unwrap();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 256];
        stream.read(&mut buffer).await.unwrap();
        stream.shutdown().await.unwrap();
    });
    assert_eq!(
        send_ipc_request(&eof, &json!({"type":"list"})).await.unwrap_err(),
        format!(
            "Orchestrator socket closed before a response was received: {}",
            eof.display()
        )
    );
    task.await.unwrap();
}

#[tokio::test]
async fn approved_delta_drains_coalesced_handshake_and_all_complete_stream_requests() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("socket");
    let server = start_ipc_server(&path, Arc::new(Handler)).await.unwrap();
    for prefix in ["", "\n  \n"] {
        let socket = tokio::net::UnixStream::connect(&path).await.unwrap();
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        let mut line = String::new();
        let payload = format!(
            "{prefix}{{\"type\":\"rpc_stream\",\"instanceId\":\"i\"}}\n{{\"type\":\"get_state\",\"id\":\"one\"}}\n\n{{\"type\":\"abort\",\"id\":\"two\"}}\n"
        );
        write.write_all(payload.as_bytes()).await.unwrap();
        for expected in ["rpc_ready", "one", "two"] {
            line.clear();
            tokio::time::timeout(std::time::Duration::from_millis(200), reader.read_line(&mut line))
                .await
                .expect("complete coalesced frames must drain without another data event")
                .unwrap();
            let response: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(
                if expected == "rpc_ready" {
                    &response["type"]
                } else {
                    &response["id"]
                },
                expected
            );
        }
    }
    server.close().await;
}
#[tokio::test]
async fn approved_delta_skips_blank_initial_lines_without_stalling_plain_request() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("socket");
    let server = start_ipc_server(&path, Arc::new(Handler)).await.unwrap();
    let mut socket = tokio::net::UnixStream::connect(&path).await.unwrap();
    socket.write_all(b"\n  \n{\"type\":\"list\"}\n").await.unwrap();
    let mut output = String::new();
    tokio::time::timeout(
        std::time::Duration::from_millis(200),
        socket.read_to_string(&mut output),
    )
    .await
    .expect("blank prefixes must not stall complete request")
    .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&output).unwrap()["type"], "list_result");
    server.close().await;
}
