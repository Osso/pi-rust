use pi_orchestrator::{RpcProcessInstance, Storage};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};
async fn oracle(mode: &str, input: Value) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lock: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("docs/porting/source-lock.json")).unwrap()).unwrap();
    let source = Path::new(lock["worktree"].as_str().unwrap());
    let output = tokio::process::Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(root.join("oracle/orchestrator-errors-oracle.mjs"))
        .arg(source)
        .arg(mode)
        .arg(input.to_string())
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test]
async fn filesystem_errors_match_frozen_node_messages() {
    let dir = tempfile::tempdir().unwrap();
    let expected = oracle("storage", json!({"dir":dir.path()})).await;
    let actual = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let storage = Storage(case["dir"].as_str().unwrap().into());
            let error = match case["operation"].as_str().unwrap() {
                "loadMachine" => storage.load_machine().unwrap_err(),
                "loadInstances" => storage.load_instances().unwrap_err(),
                "saveMachine" => storage.save_machine(&json!({"id":"m","createdAt":"time"})).unwrap_err(),
                "deleteMachine" => storage.delete_machine().unwrap_err(),
                _ => unreachable!(),
            };
            json!({"operation":case["operation"],"dir":case["dir"],"message":error})
        })
        .collect::<Vec<_>>();
    assert_eq!(json!(actual), expected);
}
#[tokio::test]
async fn spawn_failures_match_frozen_node_messages() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-executable");
    std::fs::write(&file, "text").unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    let node = String::from_utf8(node.stdout).unwrap().trim().to_owned();
    let cases = vec![
        json!({"program":dir.path().join("missing"),"cwd":dir.path()}),
        json!({"program":file,"cwd":dir.path()}),
        json!({"program":dir.path(),"cwd":dir.path()}),
        json!({"program":node,"cwd":dir.path().join("missing-cwd")}),
        json!({"program":node,"cwd":file}),
        json!({"program":node,"cwd":"bad\u{0}cwd"}),
    ];
    let mut expected = vec![];
    let mut actual = vec![];
    for case in cases {
        expected.push(oracle("spawn", case.clone()).await);
        let mut command = tokio::process::Command::new(case["program"].as_str().unwrap());
        let error = match RpcProcessInstance::spawn_command(&mut command, Path::new(case["cwd"].as_str().unwrap())) {
            Ok(process) => {
                process.dispose().await.unwrap();
                panic!("spawn unexpectedly succeeded")
            }
            Err(error) => error,
        };
        actual.push(json!({"message":error}));
    }
    assert_eq!(actual, expected);
}
#[tokio::test]
async fn closed_child_stdin_error_matches_frozen_write_callback() {
    let dir = tempfile::tempdir().unwrap();
    let script =
        "require('node:fs').closeSync(0);process.stdout.write('{\"type\":\"ready\"}\\n');setInterval(()=>{},1000)";
    let expected = oracle("stdin", json!({"cwd":dir.path(),"script":script})).await;
    let mut command = tokio::process::Command::new("node");
    command.args(["-e", script]);
    let process = RpcProcessInstance::spawn_command(&mut command, dir.path()).unwrap();
    let ready = Arc::new(tokio::sync::Notify::new());
    let notify = ready.clone();
    process.on_event(Arc::new(move |_| notify.notify_one()));
    ready.notified().await;
    let error = process
        .send(json!({"type":"get_state","id":"probe"}))
        .await
        .unwrap_err();
    process.dispose().await.unwrap();
    assert_eq!(json!({"message":error}), expected);
}

struct Handler;
impl pi_orchestrator::IpcRequestHandler for Handler {
    fn handle(&self, _request: Value) -> pi_ai::request::BoxFuture<'_, Result<Value, String>> {
        Box::pin(async { Ok(json!({"type":"list_result","ok":true,"instances":[]})) })
    }
    fn open_rpc_stream(
        &self,
        _id: &str,
        _callback: pi_orchestrator::OnMessage,
    ) -> Option<Arc<dyn pi_orchestrator::RpcStream>> {
        None
    }
}
#[tokio::test]
async fn unix_socket_errors_match_frozen_node_messages() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let regular = root.path().join("regular");
    std::fs::create_dir(&regular).unwrap();
    std::fs::write(regular.join("orchestrator.sock"), "not socket").unwrap();
    let blocked = root.path().join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o0)).unwrap();
    let readonly = root.path().join("readonly");
    std::fs::create_dir(&readonly).unwrap();
    std::fs::set_permissions(&readonly, std::fs::Permissions::from_mode(0o555)).unwrap();
    let stale = root.path().join("stale");
    std::fs::create_dir(&stale).unwrap();
    std::fs::write(stale.join("orchestrator.sock"), "stale").unwrap();
    std::fs::set_permissions(&stale, std::fs::Permissions::from_mode(0o555)).unwrap();
    let cases = [
        ("client", root.path().join("missing")),
        ("client", regular),
        ("client", blocked.clone()),
        ("server", root.path().join("missing")),
        ("server", readonly.clone()),
        ("server", stale.clone()),
    ];
    let mut actual = vec![];
    let mut expected = vec![];
    for (action, dir) in cases {
        expected.push(oracle("ipc", json!({"action":action,"dir":dir})).await);
        let error = if action == "client" {
            pi_orchestrator::send_ipc_request(&dir.join("orchestrator.sock"), &json!({"type":"list"}))
                .await
                .unwrap_err()
        } else {
            match pi_orchestrator::start_ipc_server(&dir.join("orchestrator.sock"), Arc::new(Handler)).await {
                Err(error) => error,
                Ok(server) => {
                    server.close().await;
                    panic!("server unexpectedly started")
                }
            }
        };
        actual.push(json!({"message":error}));
    }
    for dir in [blocked, readonly, stale] {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(actual, expected);
}
#[tokio::test]
async fn server_close_removes_its_socket_like_frozen_node_server() {
    let dir = tempfile::tempdir().unwrap();
    let expected = oracle("ipc", json!({"action":"server","dir":dir.path()})).await;
    let path = dir.path().join("orchestrator.sock");
    let server = pi_orchestrator::start_ipc_server(&path, Arc::new(Handler))
        .await
        .unwrap();
    server.close().await;
    assert_eq!(json!({"existsAfterClose":path.exists()}), expected);
}

#[tokio::test]
async fn server_close_keeps_already_open_streams_like_frozen_node() {
    use pi_orchestrator::{IpcRequestHandler, OnMessage, RpcStream};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    struct ActiveHandler;
    impl IpcRequestHandler for ActiveHandler {
        fn handle(&self, _request: Value) -> pi_ai::request::BoxFuture<'_, Result<Value, String>> {
            Box::pin(async { Ok(json!({"type":"rpc_ready","ok":true,"instance":{"id":"i"}})) })
        }
        fn open_rpc_stream(&self, _id: &str, on_message: OnMessage) -> Option<Arc<dyn RpcStream>> {
            Some(Arc::new(ActiveStream(on_message)))
        }
    }
    struct ActiveStream(OnMessage);
    impl RpcStream for ActiveStream {
        fn handle_request(&self, request: Value) -> pi_ai::request::BoxFuture<'_, Result<(), String>> {
            Box::pin(async move {
                (self.0)(json!({"type":"response","success":true,"id":request["id"]}));
                Ok(())
            })
        }
        fn close(&self) {}
    }
    let dir = tempfile::tempdir().unwrap();
    let expected = oracle("ipc", json!({"action":"stream-close","dir":dir.path()})).await;
    let path = dir.path().join("orchestrator.sock");
    let server = pi_orchestrator::start_ipc_server(&path, Arc::new(ActiveHandler))
        .await
        .unwrap();
    let socket = tokio::net::UnixStream::connect(&path).await.unwrap();
    let (read, mut write) = socket.into_split();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    write
        .write_all(b"{\"type\":\"rpc_stream\",\"instanceId\":\"i\"}\n")
        .await
        .unwrap();
    reader.read_line(&mut line).await.unwrap();
    server.close().await;
    write
        .write_all(b"{\"type\":\"get_state\",\"id\":\"after-close\"}\n")
        .await
        .unwrap();
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), expected);
}

#[tokio::test]
async fn socket_read_reset_matches_frozen_node_message() {
    let dir = tempfile::tempdir().unwrap();
    let expected = oracle("ipc-reset", json!({"dir":dir.path()})).await;
    let path = dir.path().join("orchestrator.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        drop(socket);
    });
    let error = pi_orchestrator::send_ipc_request(&path, &json!({"type":"list"}))
        .await
        .unwrap_err();
    task.await.unwrap();
    assert_eq!(json!({"message":error}), expected);
}
