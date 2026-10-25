//! Pending common-pin source-drift REDs. Run in-crate before implementation.
use crate::shared_ai_oracle as oracle;
use pi_coding_agent::core::resident_console_transport::{
    ResidentConsoleClient, ResidentConsoleHost, ResidentConsoleServer, ResidentConsoleServerOptions,
    ResidentConsoleService, ResidentConsoleSnapshot, Unsubscribe,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn snapshot_value() -> JsValue {
    js_json::parse_lossless(r#"{"service":"supervisor","sessionId":"session-\ud800","cwd":"/cwd-\udfff","generation":1,"identity":{"version":"v-\ud800","pid":123,"executable":"/exe-\udfff","entrypoint":"/entry-\ud801","instanceId":"instance-\ud802","managedBy":"pi","ready":true},"branch":[]}"#).unwrap()
}

#[tokio::test]
async fn client_retains_frozen_identity_metadata_in_the_attached_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("identity.sock");
    let snapshot = snapshot_value();
    let mut frame = snapshot.clone();
    frame
        .as_object_mut()
        .unwrap()
        .insert("type".into(), JsValue::String("attached".into()));
    frame
        .as_object_mut()
        .unwrap()
        .insert("version".into(), JsValue::Number(1.0));
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let send_frame = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let request = lines.next_line().await.unwrap().unwrap();
        assert_eq!(
            js_json::parse_lossless(&request).unwrap()["type"].as_string(),
            Some(&JsString::from("attach"))
        );
        write
            .write_all(format!("{}\n", js_json::stringify_lossless(&frame)).as_bytes())
            .await
            .unwrap();
    });
    let client: ResidentConsoleClient<JsValue, JsValue> = ResidentConsoleClient::connect(
        path.to_str().expect("utf-8 test path"),
        ResidentConsoleService::Supervisor,
    )
    .await
    .unwrap();
    let actual = js_json::to_value(client.snapshot()).unwrap();
    client.close().await;
    send_frame.await.unwrap();
    std::fs::remove_file(&path).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-resident-console-oracle.ts",
        "packages/coding-agent/src/core/resident-console-transport.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"kind":"identity","socketPath":path,"snapshot":snapshot}));
    assert_eq!(actual, expected["snapshot"]);
}

struct Host;
impl ResidentConsoleHost<JsValue, JsValue> for Host {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<JsValue> {
        ResidentConsoleSnapshot {
            service: ResidentConsoleService::Supervisor,
            session_id: "s".into(),
            cwd: "/cwd".into(),
            generation: 1,
            identity: None,
            branch: vec![],
        }
    }
    fn enqueue_prompt(
        &self,
        _text: JsString,
        _id: JsString,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), JsString>> + Send>> {
        Box::pin(async { Ok(()) })
    }
    fn subscribe(&self, _listener: Arc<dyn Fn(JsValue) + Send + Sync>) -> Unsubscribe {
        Box::new(|| {})
    }
}

#[tokio::test]
async fn probe_returns_a_snapshot_without_replacing_the_attached_owner() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("probe.sock");
    let server = ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: path.to_str().expect("utf-8 test path").into(),
        service: ResidentConsoleService::Supervisor,
        host: Arc::new(Host),
    });
    server.start().await.unwrap();
    let owner: ResidentConsoleClient<JsValue, JsValue> = ResidentConsoleClient::connect(
        path.to_str().expect("utf-8 test path"),
        ResidentConsoleService::Supervisor,
    )
    .await
    .unwrap();
    let stream = tokio::net::UnixStream::connect(&path).await.unwrap();
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    write
        .write_all(b"{\"type\":\"probe\",\"version\":1,\"service\":\"supervisor\"}\n")
        .await
        .unwrap();
    let response = tokio::time::timeout(std::time::Duration::from_secs(2), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    owner.prompt("still-owner", "accepted").await.unwrap();
    drop(write);
    owner.close().await;
    server.close().await.unwrap();
    let mut source = oracle::TsOracle::spawn(
        "lossless-resident-console-oracle.ts",
        "packages/coding-agent/src/core/resident-console-transport.ts",
    );
    let expected =
        source.request(&pi_js_compat::js_json!({"kind":"probe","socketPath":path,"snapshot":Host.get_snapshot()}));
    assert_eq!(js_json::parse_lossless(&response).unwrap(), expected["probe"]);
}
