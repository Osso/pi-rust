use crate::shared_ai_oracle as oracle;
use pi_coding_agent::core::resident_console_transport::{
    ResidentConsoleClient, ResidentConsoleHost, ResidentConsoleServer, ResidentConsoleServerOptions,
    ResidentConsoleService, ResidentConsoleSnapshot, Unsubscribe,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

type Listener = Arc<dyn Fn(JsValue) + Send + Sync>;
struct Host {
    prompts: Mutex<Vec<(JsString, JsString)>>,
    listener: Arc<Mutex<Option<Listener>>>,
    rejection: Option<JsString>,
}
impl Host {
    fn new(rejection: Option<JsString>) -> Arc<Self> {
        Arc::new(Self {
            prompts: Mutex::new(Vec::new()),
            listener: Arc::new(Mutex::new(None)),
            rejection,
        })
    }
}
impl ResidentConsoleHost<JsValue, JsValue> for Host {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<JsValue> {
        ResidentConsoleSnapshot {
            service: ResidentConsoleService::Supervisor,
            session_id: JsString::from("session-") + JsString::from_utf16(vec![0xd800]),
            cwd: JsString::from("/resident-") + JsString::from_utf16(vec![0xdfff]),
            generation: 1,
            identity: None,
            branch: vec![js_json::parse_lossless(r#"{"key-\ud800":"text-\udfff"}"#).unwrap()],
        }
    }
    fn enqueue_prompt(
        &self,
        text: JsString,
        id: JsString,
    ) -> Pin<Box<dyn Future<Output = Result<(), JsString>> + Send>> {
        self.prompts.lock().unwrap().push((id, text));
        let rejection = self.rejection.clone();
        Box::pin(async move { rejection.map_or(Ok(()), Err) })
    }
    fn subscribe(&self, listener: Listener) -> Unsubscribe {
        *self.listener.lock().unwrap() = Some(listener);
        let state = Arc::clone(&self.listener);
        Box::new(move || {
            state.lock().unwrap().take();
        })
    }
}

#[tokio::test]
async fn client_preserves_snapshot_events_prompt_errors_and_raw_socket_path() {
    let temp = tempfile::tempdir().unwrap();
    let raw = JsString::from_utf16(vec![0xd800]);
    let rejection = JsString::from("rejected-") + &raw;
    let host = Host::new(Some(rejection.clone()));
    let path = JsString::from(temp.path().to_str().expect("utf-8 test path")) + "/console-" + &raw;
    let server = ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: path.clone(),
        service: ResidentConsoleService::Supervisor,
        host: host.clone(),
    });
    server.start().await.unwrap();
    let client: ResidentConsoleClient<JsValue, JsValue> =
        ResidentConsoleClient::connect(&path, ResidentConsoleService::Supervisor)
            .await
            .unwrap();
    assert_eq!(client.snapshot(), &host.get_snapshot());
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let _subscription = client.on_event(Arc::new(move |event| {
        sender.send(event.clone()).unwrap();
    }));
    let event = js_json::parse_lossless(r#"{"arg-\ud801":"event-\ud802"}"#).unwrap();
    let listener = host.listener.lock().unwrap().clone().unwrap();
    listener(event.clone());
    let received = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.event, event);
    assert_eq!(received.sequence, 1);
    assert_eq!(
        client
            .prompt(JsString::from("request-") + &raw, JsString::from("text-") + &raw)
            .await
            .unwrap_err(),
        rejection
    );
    assert_eq!(
        *host.prompts.lock().unwrap(),
        vec![(JsString::from("request-") + &raw, JsString::from("text-") + &raw)]
    );
    client.close().await;
    server.close().await.unwrap();
    let actual = pi_js_compat::js_json!({"snapshot": client.snapshot(), "event": received, "error": rejection, "prompts": host.prompts.lock().unwrap().clone()});
    let mut source = oracle::TsOracle::spawn(
        "lossless-resident-console-oracle.ts",
        "packages/coding-agent/src/core/resident-console-transport.ts",
    );
    let expected = source.request(&pi_js_compat::js_json!({"socketPath":path,"snapshot":host.get_snapshot(),"event":event,"rejection":rejection,"id":JsString::from("request-")+&raw,"text":JsString::from("text-")+&raw}));
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn socket_prompt_preserves_unpaired_units_and_correlates_raw_id() {
    let temp = tempfile::tempdir().unwrap();
    let host = Host::new(None);
    let path = temp.path().join("resident.sock");
    let server = ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: path.to_str().expect("utf-8 test path").into(),
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
