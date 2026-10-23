use pi_ai::request::BoxFuture;
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue as Value},
};
use pi_orchestrator::{
    ipc::{
        client::send_ipc_request,
        server::{IpcRequestHandler, OnMessage, RpcStream, start_ipc_server},
    },
    rpc_process::RpcProcessInstance,
};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Echo;
impl IpcRequestHandler for Echo {
    fn handle(&self, request: Value) -> BoxFuture<'_, Result<Value, JsString>> {
        Box::pin(async move {
            if request["type"] == json!("rpc_stream") {
                Ok(json!({"type":"rpc_ready","ok":true,"instance":{"id":"i"}}))
            } else {
                Ok(request)
            }
        })
    }
    fn open_rpc_stream(&self, _: &JsString, on: OnMessage) -> Option<Arc<dyn RpcStream>> {
        Some(Arc::new(EchoStream(on)))
    }
}
struct EchoStream(OnMessage);
impl RpcStream for EchoStream {
    fn handle_request(&self, request: Value) -> BoxFuture<'_, Result<(), JsString>> {
        Box::pin(async move {
            (self.0)(request);
            Ok(())
        })
    }
    fn close(&self) {}
}
fn frame(character: char) -> String {
    format!("{character}{{\"type\":\"response\",\"id\":\"w\",\"success\":true}}{character}\n")
}
async fn server_response(path: &Path, frame: &str, streaming: bool) -> Value {
    let socket = tokio::net::UnixStream::connect(path).await.unwrap();
    let mut reader = BufReader::new(socket);
    let mut line = String::new();
    if streaming {
        reader
            .get_mut()
            .write_all(b"{\"type\":\"rpc_stream\",\"instanceId\":\"i\"}\n")
            .await
            .unwrap();
        reader.read_line(&mut line).await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap()["type"],
            json!("rpc_ready")
        );
        line.clear();
    }
    reader.get_mut().write_all(frame.as_bytes()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn framing_whitespace_matches_node_trim_in_client_server_stream_and_child() {
    // LF itself is a JSONL delimiter, not part of a single frame's padding.
    let codes = [
        9, 11, 12, 13, 32, 0xa0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008,
        0x2009, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000, 0xfeff, 0x85, 0x1c, 0x1d, 0x1e, 0x1f, 0x180e, 0x200b,
    ];
    let frames: Vec<_> = codes.iter().map(|&code| frame(char::from_u32(code).unwrap())).collect();
    let oracle = tokio::process::Command::new("node").args(["-e", "const fs=require('fs');console.log(JSON.stringify(JSON.parse(fs.readFileSync(0,'utf8')).map(x=>{try{return {ok:true,value:JSON.parse(x.trim())}}catch(e){return {ok:false,error:e.message}}})))"])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
    let mut oracle = oracle;
    oracle
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&frames).unwrap().as_bytes())
        .await
        .unwrap();
    let output = oracle.wait_with_output().await.unwrap();
    assert!(output.status.success());
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let server_path = dir.path().join("server.sock");
    let server = start_ipc_server(
        &JsString::from(server_path.to_str().expect("utf-8 test path")),
        Arc::new(Echo),
    )
    .await
    .unwrap();
    for ((frame, expected), code) in frames.iter().zip(expected).zip(codes) {
        for streaming in [false, true] {
            let actual = server_response(&server_path, frame, streaming).await;
            if expected["ok"] == json!(true) {
                assert_eq!(actual, expected["value"], "U+{code:04X}");
            } else {
                assert_eq!(actual["error"], expected["error"], "U+{code:04X}");
            }
        }
        let path = dir.path().join(format!("client-{code}.sock"));
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let response = frame.clone();
        let writer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut request = String::new();
            socket.read_line(&mut request).await.unwrap();
            assert_eq!(serde_json::from_str::<Value>(&request).unwrap(), json!({"type":"list"}));
            socket.get_mut().write_all(response.as_bytes()).await.unwrap();
        });
        let actual = send_ipc_request(
            &JsString::from(path.to_str().expect("utf-8 test path")),
            &json!({"type":"list"}),
        )
        .await;
        writer.await.unwrap();
        if expected["ok"] == json!(true) {
            assert_eq!(actual.unwrap(), expected["value"], "U+{code:04X}");
        } else {
            assert_eq!(
                &actual.unwrap_err(),
                expected["error"].as_string().unwrap(),
                "U+{code:04X}"
            );
        }
        let fake = dir.path().join("child.mjs");
        std::fs::write(
            &fake,
            format!(
                "process.stdin.once('data',()=>{{process.stdout.write({});}});",
                serde_json::to_string(frame).unwrap()
            ),
        )
        .unwrap();
        let mut command = tokio::process::Command::new("node");
        command.arg(&fake);
        let rpc = RpcProcessInstance::spawn_command(
            &mut command,
            &JsString::from((dir.path()).to_str().expect("utf-8 test path")),
        )
        .unwrap();
        let mut fatal = rpc.subscribe_fatal_errors();
        if expected["ok"] == json!(true) {
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), rpc.send(json!({"type":"get_state","id":"w"})))
                    .await
                    .unwrap()
                    .unwrap(),
                expected["value"],
                "U+{code:04X}"
            );
        } else {
            let pending_rpc = rpc.clone();
            let pending = tokio::spawn(async move { pending_rpc.send(json!({"type":"get_state","id":"w"})).await });
            tokio::time::timeout(Duration::from_secs(2), fatal.changed())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(fatal.borrow().as_ref(), expected["error"].as_string(), "U+{code:04X}");
            rpc.dispose().await.unwrap();
            assert!(pending.await.unwrap().is_err());
        }
        rpc.dispose().await.unwrap();
    }
    server.close().await;
}
