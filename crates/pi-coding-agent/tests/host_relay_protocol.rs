//! Protocol differential: frozen runHostRelay, in-memory inputs only.
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::host_relay::{HostRelayOptions, run_host_relay};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Default)]
struct RecordedOutput(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl tokio::io::AsyncWrite for RecordedOutput {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        data: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.0.lock().unwrap().extend_from_slice(data);
        std::task::Poll::Ready(Ok(data.len()))
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
#[tokio::test]
async fn validation_handshake_routes_and_acks_match_frozen() {
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    let hello = json!({"type":"hello","host":"fixture-peer"}).to_string();
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let mailbox = json!({"type":"mailbox","seq":1,"sessionPath":"/s","messageId":"m","data":{"id":"m","kind":"message","status":"pending","fromAgentId":"main","toAgentId":"main","body":"payload","createdAt":now,"recipientHost":"fixture-local","relaySeq":1,"recipientSessionId":"r","recipientAgentId":null,"senderSessionId":"s","senderAgentId":null}});
    let channel = json!({"type":"channel","originHost":"fixture-peer","originId":2,"senderSessionId":"s","senderAgentId":null,"body":"channel","createdAt":now});
    let mut cases = vec![
        vec![],
        vec![hello.clone()],
        vec![hello.clone(), hello.clone()],
        vec![json!({"type":"hello","host":"fixture-local"}).to_string()],
        vec![mailbox.to_string()],
        vec![
            hello.clone(),
            mailbox.to_string(),
            mailbox.to_string(),
            channel.to_string(),
        ],
        vec![
            hello.clone(),
            json!({"type":"ack","kind":"mailbox","id":99}).to_string(),
        ],
        vec!["{".into()],
    ];
    cases.push(vec![format!(
        "{hello}\r{}",
        json!({"type":"ack","kind":"channel","id":1})
    )]);
    cases.push(vec![
        r#"{"type":"hello","host":"p\ud800"}"#.into(),
        channel.to_string().replace("fixture-peer", r"p\ud801"),
    ]);
    let malformed = vec![
        json!(null),
        json!([]),
        json!({}),
        json!({"type":"hello","host":" \t"}),
        json!({"type":"ack","kind":"wrong","id":1}),
        json!({"type":"ack","kind":"mailbox","id":0}),
        json!({"type":"ack","kind":"channel","id":1.25}),
        json!({"type":"ack","kind":"channel","id":9007199254740992u64}),
        json!({"type":"mailbox","seq":1,"data":[]}),
        json!({"type":"mailbox","seq":true,"data":{}}),
        json!({"type":"channel","originHost":"peer","originId":1,"senderSessionId":"s","body":"b","createdAt":now}),
    ];
    cases.extend(malformed.into_iter().map(|v| vec![hello.clone(), v.to_string()]));
    for field in ["recipientHost", "relaySeq"] {
        let mut frame = mailbox.clone();
        frame["data"][field] = json!("wrong");
        cases.push(vec![hello.clone(), frame.to_string()]);
    }
    let mut custom = mailbox.clone();
    custom["data"]["kind"] = json!("custom");
    cases.push(vec![hello.clone(), custom.to_string()]);
    let mut wrong = channel.clone();
    wrong["originHost"] = json!("wrong");
    cases.push(vec![hello.clone(), wrong.to_string()]);
    for frames in cases {
        let temp = tempfile::tempdir().unwrap();
        let rust = temp.path().join("rust.sqlite");
        let peer = temp.path().join("ts.sqlite");
        let expected = ts.request(&json!({"op":"protocol","path":peer,"frames":frames}));
        assert_eq!(expected["ok"], true, "{expected}");
        let input = if frames.is_empty() {
            String::new()
        } else {
            frames.join("\n") + "\n"
        };
        let output = RecordedOutput::default();
        let error = run_host_relay(
            input.as_bytes(),
            output.clone(),
            HostRelayOptions {
                control_db_path: rust.to_str().unwrap(),
                host: Some("fixture-local"),
                poll_ms: Some(20),
                signal: CancellationToken::new(),
            },
        )
        .await
        .err();
        let wire = String::from_utf8(output.0.lock().unwrap().clone()).unwrap();
        let actual = json!({"frames":wire.lines().map(|s|serde_json::from_str::<Value>(s).unwrap()).collect::<Vec<_>>(),"error":error});
        let expected: Value = serde_json::from_str(expected["wire"].as_str().unwrap()).unwrap();
        assert_eq!(actual, expected, "input {frames:?}");
    }
}
#[tokio::test]
async fn paired_streams_retry_lost_ack_without_resurrecting_deleted_mailbox() {
    use pi_coding_agent::core::host_relay_store::*;
    use pi_coding_agent::core::session_control_db::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a.sqlite").display().to_string();
    let b = temp.path().join("b.sqlite").display().to_string();
    let input:EnqueueStoredRuntimeMailboxMessageInput=serde_json::from_value(json!({"kind":"message","recipientHost":"peer","recipient":{"sessionId":"r","agentId":null},"sender":{"sessionId":"s","agentId":null},"storeRef":{"sessionPath":"/s","messageId":"m"},"message":{"id":"m","kind":"message","status":"pending","fromAgentId":"main","toAgentId":"main","body":"once"}})).unwrap();
    enqueue_stored_runtime_mailbox_message(&a, &input).unwrap();
    let (left, right) = tokio::io::duplex(65536);
    let (read, mut write) = tokio::io::split(right);
    let mut lines = BufReader::new(read).lines();
    let stop = CancellationToken::new();
    let relay = run_host_relay(
        left,
        tokio::io::sink(),
        HostRelayOptions {
            control_db_path: &b,
            host: Some("peer"),
            poll_ms: Some(10),
            signal: stop.clone(),
        },
    );
    // Direct receiver protocol drops output ACKs (sink) while sender retries its persisted sequence.
    let action = async {
        write
            .write_all(b"{\"type\":\"hello\",\"host\":\"origin\"}\n")
            .await
            .unwrap();
        let first = next_relay_mailbox(&a, "peer").unwrap().unwrap();
        let wire = pi_coding_agent::core::host_relay::RelayFrame::Mailbox(first.clone()).to_wire() + "\n";
        write.write_all(wire.as_bytes()).await.unwrap();
        for _ in 0..100 {
            if !list_runtime_mailbox_messages(&b).unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(list_runtime_mailbox_messages(&b).unwrap().len(), 1);
        mark_runtime_mailbox_message_delivered(&b, 1).unwrap();
        write.write_all(wire.as_bytes()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(list_runtime_mailbox_messages(&b).unwrap().is_empty());
        assert_eq!(list_runtime_mailbox_messages(&a).unwrap().len(), 1);
        stop.cancel();
        let _ = lines.next_line();
    };
    let (result, ()) = tokio::join!(relay, action);
    assert_eq!(result, Ok(()));
    // Reconnect still has the durable inbound high-water mark after recipient deletion.
    let first = next_relay_mailbox(&a, "peer").unwrap().unwrap();
    import_relay_mailbox(&b, "origin", &first).unwrap();
    assert!(list_runtime_mailbox_messages(&b).unwrap().is_empty());
    ack_relay_mailbox(&a, "peer", &first).unwrap();
    assert!(list_runtime_mailbox_messages(&a).unwrap().is_empty());
}

#[derive(Clone, Default)]
struct GateOutput {
    recorded: RecordedOutput,
    open: std::sync::Arc<std::sync::atomic::AtomicBool>,
    waker: std::sync::Arc<futures_util::task::AtomicWaker>,
}
impl tokio::io::AsyncWrite for GateOutput {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.waker.register(cx.waker());
        if !self.open.load(std::sync::atomic::Ordering::SeqCst) {
            return std::task::Poll::Pending;
        }
        std::pin::Pin::new(&mut self.recorded).poll_write(cx, bytes)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
#[tokio::test]
async fn incoming_mailbox_progresses_while_writer_is_blocked_and_queued_ack_drains() {
    use pi_coding_agent::core::session_control_db::list_runtime_mailbox_messages;
    use tokio::io::AsyncWriteExt;
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("rust.sqlite").display().to_string();
    let peer = temp.path().join("ts.sqlite");
    let frames=vec![json!({"type":"hello","host":"fixture-peer"}).to_string(),json!({"type":"mailbox","seq":1,"sessionPath":"/s","messageId":"m","data":{"id":"m","kind":"message","status":"pending","fromAgentId":"main","toAgentId":"main","body":"under backpressure","createdAt":pi_coding_agent::utils::iso_time::now_iso_string(),"recipientHost":"fixture-local","relaySeq":1,"recipientSessionId":"r","recipientAgentId":null,"senderSessionId":"s","senderAgentId":null}}).to_string()];
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    let response = ts.request(&json!({"op":"protocol","path":peer,"frames":frames,"blocked":true}));
    assert_eq!(response["ok"], true, "{response}");
    let expected: Value = serde_json::from_str(response["wire"].as_str().unwrap()).unwrap();
    assert_eq!(expected["importedWhileBlocked"], 1);
    let (input, mut sender) = tokio::io::duplex(65536);
    let output = GateOutput::default();
    let writer = output.clone();
    let path = db.clone();
    let signal = CancellationToken::new();
    let stop = signal.clone();
    let relay = tokio::spawn(async move {
        run_host_relay(
            input,
            writer,
            HostRelayOptions {
                control_db_path: &path,
                host: Some("fixture-local"),
                poll_ms: Some(5),
                signal,
            },
        )
        .await
    });
    sender.write_all((frames.join("\n") + "\n").as_bytes()).await.unwrap();
    let mut count = 0;
    for _ in 0..100 {
        count = list_runtime_mailbox_messages(&db).unwrap().len();
        if count == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    if count != 1 {
        stop.cancel();
        output.open.store(true, std::sync::atomic::Ordering::SeqCst);
        output.waker.wake();
        let _ = relay.await;
        panic!("blocked writer prevented inbound mailbox import");
    }
    assert!(output.recorded.0.lock().unwrap().is_empty());
    output.open.store(true, std::sync::atomic::Ordering::SeqCst);
    output.waker.wake();
    drop(sender);
    assert_eq!(relay.await.unwrap(), Ok(()));
    let text = String::from_utf8(output.recorded.0.lock().unwrap().clone()).unwrap();
    let actual: Vec<Value> = text.lines().map(|s| serde_json::from_str(s).unwrap()).collect();
    assert_eq!(json!(actual), expected["frames"]);
}
