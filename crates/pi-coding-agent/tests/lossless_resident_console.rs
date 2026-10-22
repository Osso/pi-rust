use pi_coding_agent::core::resident_console_transport::{
    ResidentConsoleHost, ResidentConsoleServer, ResidentConsoleServerOptions, ResidentConsoleService,
    ResidentConsoleSnapshot, Unsubscribe,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Host {
    prompts: Mutex<Vec<(JsString, JsString)>>,
}
impl ResidentConsoleHost<JsValue, JsValue> for Host {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<JsValue> {
        ResidentConsoleSnapshot {
            service: ResidentConsoleService::Supervisor,
            session_id: "supervisor".into(),
            cwd: "/resident".into(),
            generation: 1,
            branch: vec![],
        }
    }
    fn enqueue_prompt(
        &self,
        text: JsString,
        id: JsString,
    ) -> Pin<Box<dyn Future<Output = Result<(), JsString>> + Send>> {
        self.prompts.lock().unwrap().push((id, text));
        Box::pin(async { Ok(()) })
    }
    fn subscribe(&self, _listener: Arc<dyn Fn(JsValue) + Send + Sync>) -> Unsubscribe {
        Box::new(|| {})
    }
}

#[tokio::test]
async fn socket_prompt_preserves_unpaired_units_and_correlates_raw_id() {
    let temp = tempfile::tempdir().unwrap();
    let host = Arc::new(Host {
        prompts: Mutex::new(Vec::new()),
    });
    let path = temp.path().join("resident.sock");
    let server = ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: path.to_str().unwrap().into(),
        service: ResidentConsoleService::Supervisor,
        host: host.clone(),
    });
    server.start().await.unwrap();
    let stream = tokio::net::UnixStream::connect(path).await.unwrap();
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    write
        .write_all(b"{\"type\":\"attach\",\"version\":1,\"service\":\"supervisor\"}\n")
        .await
        .unwrap();
    let attached = lines.next_line().await.unwrap().unwrap();
    assert_eq!(
        js_json::parse_lossless(&attached).unwrap()["type"],
        JsValue::String("attached".into())
    );
    write
        .write_all(b"{\"type\":\"prompt\",\"id\":\"request-\\ud800\",\"text\":\"text-\\udfff\"}\n")
        .await
        .unwrap();
    let response = tokio::time::timeout(std::time::Duration::from_secs(2), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        js_json::parse_lossless(&response).unwrap(),
        js_json::parse_lossless(r#"{"type":"prompt_accepted","id":"request-\ud800"}"#).unwrap()
    );
    assert_eq!(
        *host.prompts.lock().unwrap(),
        vec![(
            JsString::from("request-") + JsString::from_utf16(vec![0xd800]),
            JsString::from("text-") + JsString::from_utf16(vec![0xdfff])
        )]
    );
    drop(write);
    server.close().await.unwrap();
}
