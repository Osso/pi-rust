use pi_ai::request::BoxFuture;
use pi_orchestrator::{
    ipc::server::{IpcRequestHandler, OnMessage, RpcStream, start_ipc_server},
    rpc_process::RpcProcessInstance,
};
use serde_json::{Value, json};
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
    fn handle(&self, r: Value) -> BoxFuture<'_, Result<Value, String>> {
        Box::pin(async move {
            Ok(if r["type"] == "rpc_stream" {
                json!({"type":"rpc_ready","ok":true,"instance":{"id":"i"}})
            } else {
                json!({"type":"list_result","ok":true,"instances":[]})
            })
        })
    }
    fn open_rpc_stream(&self, _: &str, on: OnMessage) -> Option<Arc<dyn RpcStream>> {
        Some(Arc::new(Stream(on, self.0.clone())))
    }
}
impl RpcStream for Stream {
    fn handle_request(&self, _: Value) -> BoxFuture<'_, Result<(), String>> {
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
    let server = start_ipc_server(&path, Arc::new(Handler(Arc::new(AtomicUsize::new(0)))))
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
    std::fs::write(&script,format!("import {{writeFileSync}} from 'node:fs';import {{startIpcServer}} from '{SOURCE}/packages/orchestrator/src/ipc/server.ts';const h=async()=>({{type:'rpc_ready',ok:true,instance:{{id:'i'}}}});h.openRpcStream=(_id,response,event)=>({{async handleRequest(){{event({{type:'started'}});await new Promise(r=>setTimeout(r,150));writeFileSync({:?},'done');}},close(){{}}}});await startIpcServer(h);console.log('ready');",done.to_string_lossy())).unwrap();
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
    let server = start_ipc_server(&path, Arc::new(Handler(count.clone()))).await.unwrap();
    send_and_close(&path).await;
    server.close().await;
    let actual = count.load(Ordering::SeqCst);
    println!("disconnect completed work TS={expected} Rust={actual}");
    assert_eq!(actual, expected);
}
#[tokio::test]
async fn child_exit_is_not_delayed_until_inherited_pipes_close() {
    let d = dir("exit");
    let fake = d.join("fake.mjs");
    std::fs::write(&fake,"import {spawn} from 'node:child_process';spawn(process.execPath,['-e','setTimeout(()=>{},900)'],{stdio:['ignore',1,2]});process.exit(7);").unwrap();
    let oracle = d.join("oracle.mjs");
    std::fs::write(&oracle,format!("import {{RpcProcessInstance}} from '{SOURCE}/packages/orchestrator/src/rpc-process.ts';RpcProcessInstance.prototype.getSpawnCommand=()=>({{command:process.execPath,args:[{:?}]}});const p=new RpcProcessInstance({{cwd:{:?}}});const start=Date.now();p.onExit(()=>console.log(Date.now()-start));",fake.to_string_lossy(),d.to_string_lossy())).unwrap();
    let output = ts(&oracle).output().await.unwrap();
    assert!(output.status.success());
    let expected: u128 = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
    let mut command = tokio::process::Command::new("node");
    command.arg(&fake);
    let process = RpcProcessInstance::spawn_command(&mut command, &d).unwrap();
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
