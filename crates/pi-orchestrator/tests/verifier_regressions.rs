use pi_ai::request::BoxFuture;
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue as Value},
};
use pi_orchestrator::{
    ipc::server::{IpcRequestHandler, OnMessage, RpcStream, start_ipc_server},
    rpc_process::RpcProcessInstance,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
const SOURCE: &str = "/home/osso/.worktrees/pi-rust-source-4f9e6f8a4";
fn dir(name: &str) -> PathBuf {
    tempfile::Builder::new().prefix(name).tempdir().unwrap().keep()
}
fn ts(script: &std::path::Path) -> tokio::process::Command {
    let mut c = tokio::process::Command::new("node");
    c.arg("--import")
        .arg(format!("{SOURCE}/node_modules/tsx/dist/loader.mjs"))
        .arg(script)
        .env("TSX_TSCONFIG_PATH", format!("{SOURCE}/tsconfig.json"))
        .kill_on_drop(true);
    c
}
struct Handler(Arc<AtomicUsize>);
struct Stream(OnMessage, Arc<AtomicUsize>);
impl IpcRequestHandler for Handler {
    fn handle(&self, r: Value) -> BoxFuture<'_, Result<Value, JsString>> {
        Box::pin(async move {
            Ok(if r["type"] == json!("rpc_stream") {
                json!({"type":"rpc_ready","ok":true,"instance":{"id":"i"}})
            } else {
                json!({"type":"list_result","ok":true,"instances":[]})
            })
        })
    }
    fn open_rpc_stream(&self, _: &JsString, on: OnMessage) -> Option<Arc<dyn RpcStream>> {
        Some(Arc::new(Stream(on, self.0.clone())))
    }
}
impl RpcStream for Stream {
    fn handle_request(&self, _: Value) -> BoxFuture<'_, Result<(), JsString>> {
        Box::pin(async move {
            (self.0)(json!({"type":"started"}));
            tokio::time::sleep(Duration::from_millis(150)).await;
            self.1.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
    fn close(&self) {}
}
#[tokio::test]
async fn bom_prefixed_ipc_matches_frozen() {
    let d = dir("bom");
    let path = d.join("orchestrator.sock");
    let script = d.join("peer.mjs");
    std::fs::write(&script,format!("import {{startIpcServer}} from '{SOURCE}/packages/orchestrator/src/ipc/server.ts';const h=async()=>({{type:'list_result',ok:true,instances:[]}});h.openRpcStream=()=>undefined;const s=await startIpcServer(h);console.log('ready');")).unwrap();
    let mut child = ts(&script)
        .env("PI_ORCHESTRATOR_DIR", &d)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .await
        .unwrap();
    async fn send(path: &std::path::Path) -> String {
        let mut s = tokio::net::UnixStream::connect(path).await.unwrap();
        s.write_all("\u{feff}{\"type\":\"list\"}\n".as_bytes()).await.unwrap();
        let mut text = String::new();
        s.read_to_string(&mut text).await.unwrap();
        text
    }
    let expected = send(&path).await;
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    let server = start_ipc_server(
        &JsString::from(path.to_str().expect("utf-8 test path")),
        Arc::new(Handler(Arc::new(AtomicUsize::new(0)))),
    )
    .await
    .unwrap();
    let actual = send(&path).await;
    server.close().await;
    println!("BOM TS={expected:?} Rust={actual:?}");
    assert_eq!(actual, expected);
}
#[tokio::test]
async fn queued_command_survives_disconnect_like_frozen() {
    let d = dir("close");
    let path = d.join("orchestrator.sock");
    let done = d.join("done");
    let script = d.join("peer.mjs");
    std::fs::write(&script,format!("import {{writeFileSync}} from 'node:fs';import {{startIpcServer}} from '{SOURCE}/packages/orchestrator/src/ipc/server.ts';const h=async()=>({{type:'rpc_ready',ok:true,instance:{{id:'i'}}}});h.openRpcStream=(_id,response,event)=>({{async handleRequest(){{event({{type:'started'}});await new Promise(r=>setTimeout(r,150));writeFileSync({:?},'done');}},close(){{}}}});await startIpcServer(h);console.log('ready');",done.to_str().expect("utf-8 test path"))).unwrap();
    let mut child = ts(&script)
        .env("PI_ORCHESTRATOR_DIR", &d)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .await
        .unwrap();
    async fn send_and_close(path: &std::path::Path) {
        let mut s = tokio::net::UnixStream::connect(path).await.unwrap();
        s.write_all(b"{\"type\":\"rpc_stream\",\"instanceId\":\"i\"}\n")
            .await
            .unwrap();
        let mut reader = BufReader::new(s);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        reader
            .get_mut()
            .write_all(b"{\"type\":\"abort\",\"id\":\"one\"}\n")
            .await
            .unwrap();
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.contains("started"));
        drop(reader);
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    send_and_close(&path).await;
    let expected = usize::from(done.exists());
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let server = start_ipc_server(
        &JsString::from(path.to_str().expect("utf-8 test path")),
        Arc::new(Handler(count.clone())),
    )
    .await
    .unwrap();
    send_and_close(&path).await;
    server.close().await;
    let actual = count.load(Ordering::SeqCst);
    println!("disconnect completed work TS={expected} Rust={actual}");
    assert_eq!(actual, expected);
}
#[tokio::test]
async fn all_queued_commands_finish_in_order_after_disconnect_like_frozen() {
    use std::sync::Mutex;
    struct OrderedHandler(Arc<Mutex<Vec<Value>>>);
    struct OrderedStream(OnMessage, Arc<Mutex<Vec<Value>>>);
    impl IpcRequestHandler for OrderedHandler {
        fn handle(&self, _: Value) -> BoxFuture<'_, Result<Value, JsString>> {
            Box::pin(async { Ok(json!({"type":"rpc_ready","ok":true,"instance":{"id":"i"}})) })
        }
        fn open_rpc_stream(&self, _: &JsString, message: OnMessage) -> Option<Arc<dyn RpcStream>> {
            Some(Arc::new(OrderedStream(message, self.0.clone())))
        }
    }
    impl RpcStream for OrderedStream {
        fn handle_request(&self, request: Value) -> BoxFuture<'_, Result<(), JsString>> {
            Box::pin(async move {
                (self.0)(json!({"type":"started"}));
                tokio::time::sleep(Duration::from_millis(50)).await;
                self.1.lock().unwrap().push(request["id"].clone());
                Ok(())
            })
        }
        fn close(&self) {}
    }
    async fn send_and_disconnect(path: &std::path::Path) {
        let socket = tokio::net::UnixStream::connect(path).await.unwrap();
        let mut reader = BufReader::new(socket);
        reader
            .get_mut()
            .write_all(b"{\"type\":\"rpc_stream\",\"instanceId\":\"i\"}\n")
            .await
            .unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        reader
            .get_mut()
            .write_all(b"{\"id\":\"one\"}\n{\"id\":\"two\"}\n{\"id\":\"three\"}\n")
            .await
            .unwrap();
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.contains("started"));
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("orchestrator.sock");
    let done = directory.path().join("done.json");
    let script = directory.path().join("queued.mjs");
    let source = r#"
import { writeFileSync } from 'node:fs';
import { startIpcServer } from 'SOURCE/packages/orchestrator/src/ipc/server.ts';
const completed = [];
const handler = async () => ({ type: 'rpc_ready', ok: true, instance: { id: 'i' } });
handler.openRpcStream = (_id, _response, event) => ({
    async handleRequest(request) {
        event({ type: 'started' });
        await new Promise(resolve => setTimeout(resolve, 50));
        completed.push(request.id);
        writeFileSync(DONE, JSON.stringify(completed));
    },
    close() {},
});
await startIpcServer(handler);
console.log('ready');
"#
    .replace("SOURCE", SOURCE)
    .replace("DONE", &serde_json::to_string(&done).unwrap());
    std::fs::write(&script, source).unwrap();
    let mut child = ts(&script)
        .env("PI_ORCHESTRATOR_DIR", directory.path())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut ready)
        .await
        .unwrap();
    assert_eq!(ready, "ready\n");
    send_and_disconnect(&path).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let expected = pi_js_compat::js_json::parse_lossless(&std::fs::read_to_string(&done).unwrap()).unwrap();
    assert_eq!(expected, json!(["one", "two", "three"]));
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    let completed = Arc::new(Mutex::new(Vec::new()));
    let server = start_ipc_server(
        &path.to_str().unwrap().into(),
        Arc::new(OrderedHandler(completed.clone())),
    )
    .await
    .unwrap();
    send_and_disconnect(&path).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(Value::Array(completed.lock().unwrap().clone()), expected);
    server.close().await;
}

#[tokio::test]
async fn inherited_pipes_do_not_delay_pending_rejection_or_disposal() {
    let directory = tempfile::tempdir().unwrap();
    let fake = directory.path().join("fake.mjs");
    std::fs::write(&fake, "import {spawn} from 'node:child_process'; spawn(process.execPath,['-e','setTimeout(()=>{},900)'],{stdio:['ignore',1,2]});process.exit(7);").unwrap();
    let script = directory.path().join("pending.mjs");
    let source = r#"
import { RpcProcessInstance } from 'SOURCE/packages/orchestrator/src/rpc-process.ts';
RpcProcessInstance.prototype.getSpawnCommand = () => ({ command: process.execPath, args: [FAKE] });
const processInstance = new RpcProcessInstance({ cwd: CWD });
const started = Date.now();
let pendingError;
try { await processInstance.send({type:'hang',id:'pending'}); }
catch (error) { pendingError = error.message; }
const rejectedBeforePipeEof = Date.now() - started < 500;
const disposeStarted = Date.now();
await processInstance.dispose();
const disposedBeforePipeEof = Date.now() - disposeStarted < 200;
let stoppedError;
try { await processInstance.send({type:'hang',id:'after'}); }
catch (error) { stoppedError = error.message; }
console.log(JSON.stringify({pendingError,stoppedError,rejectedBeforePipeEof,disposedBeforePipeEof}));
"#
    .replace("SOURCE", SOURCE)
    .replace("FAKE", &serde_json::to_string(&fake).unwrap())
    .replace("CWD", &serde_json::to_string(directory.path()).unwrap());
    std::fs::write(&script, source).unwrap();
    let output = ts(&script).output().await.unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected["rejectedBeforePipeEof"], json!(true));
    assert_eq!(expected["disposedBeforePipeEof"], json!(true));
    let mut command = tokio::process::Command::new("node");
    command.arg(&fake);
    let rpc = RpcProcessInstance::spawn_command(&mut command, &directory.path().to_str().unwrap().into()).unwrap();
    let started = std::time::Instant::now();
    let pending_error = rpc.send(json!({"type":"hang","id":"pending"})).await.unwrap_err();
    let rejected_before_pipe_eof = started.elapsed() < Duration::from_millis(500);
    let started = std::time::Instant::now();
    rpc.dispose().await.unwrap();
    let disposed_before_pipe_eof = started.elapsed() < Duration::from_millis(200);
    let stopped_error = rpc.send(json!({"type":"hang","id":"after"})).await.unwrap_err();
    assert_eq!(
        json!({"pendingError":pending_error,"stoppedError":stopped_error,"rejectedBeforePipeEof":rejected_before_pipe_eof,"disposedBeforePipeEof":disposed_before_pipe_eof}),
        expected
    );
}

#[tokio::test]
async fn child_exit_is_not_delayed_until_inherited_pipes_close() {
    let d = dir("exit");
    let fake = d.join("fake.mjs");
    std::fs::write(&fake,"import {spawn} from 'node:child_process';spawn(process.execPath,['-e','setTimeout(()=>{},900)'],{stdio:['ignore',1,2]});process.exit(7);").unwrap();
    let oracle = d.join("oracle.mjs");
    std::fs::write(&oracle,format!("import {{RpcProcessInstance}} from '{SOURCE}/packages/orchestrator/src/rpc-process.ts';RpcProcessInstance.prototype.getSpawnCommand=()=>({{command:process.execPath,args:[{:?}]}});const p=new RpcProcessInstance({{cwd:{:?}}});const start=Date.now();p.onExit(()=>console.log(Date.now()-start));",fake.to_str().expect("utf-8 test path"),d.to_str().expect("utf-8 test path"))).unwrap();
    let output = ts(&oracle).output().await.unwrap();
    assert!(output.status.success());
    let expected: u128 = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(&fake);
    let process =
        RpcProcessInstance::spawn_command(&mut command, &JsString::from(d.to_str().expect("utf-8 test path"))).unwrap();
    let (send, receive) = tokio::sync::oneshot::channel();
    let send = std::sync::Mutex::new(Some(send));
    let start = std::time::Instant::now();
    process.on_exit(Arc::new(move |_| {
        if let Some(s) = send.lock().unwrap().take() {
            let _ = s.send(());
        }
    }));
    receive.await.unwrap();
    let actual = start.elapsed().as_millis();
    println!("exit notification delay ms TS={expected} Rust={actual}");
    assert!(actual < 400, "child exit delayed: TS={expected}ms Rust={actual}ms");
}
