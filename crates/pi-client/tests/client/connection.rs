use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::support::*;
use pi_client::{
    ByteTransport, ByteTransportHandlers, ConnectionState, PiClient, PiClientOptions, PiError, TransportFuture,
    TransportSendFuture,
};
use pi_protocol::{
    CborValue, ClientMessage, ProtocolError, ProtocolErrorCode, ServerHelloError, ServerMessage, encode_cbor,
    encode_frame, encode_server_message,
};

struct CountingTransport {
    sends: Arc<AtomicUsize>,
    closes: Arc<AtomicUsize>,
}

impl ByteTransport for CountingTransport {
    fn send(&self, _chunk: Vec<u8>) -> TransportSendFuture {
        self.sends.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }

    fn close(&self) {
        self.closes.fetch_add(1, Ordering::SeqCst);
    }
}

fn factory_from_closure<F>(factory: F) -> PiClientOptions
where
    F: Fn(ByteTransportHandlers) -> TransportFuture + Send + Sync + 'static,
{
    PiClientOptions::new(Arc::new(factory))
}

#[tokio::test]
async fn sends_framed_version_before_accepting_fragmented_hello() {
    let server = MemoryByteServer::new();
    let received = Arc::new(Mutex::new(Vec::new()));
    let target = received.clone();
    let responder = server.clone();
    server.on_message(Arc::new(move |message| {
        target.lock().unwrap().push(message.clone());
        if let ClientMessage::Hello(_) = message {
            responder.send_split(server_hello(base_server_snapshot()), 3);
        }
    }));
    let client = create_client(&server);

    let snapshot = client.connect().unwrap().await.unwrap();

    assert_eq!(snapshot, base_server_snapshot());
    let first = received.lock().unwrap()[0].clone();
    assert_eq!(
        first,
        ClientMessage::Hello(pi_protocol::ClientHello::new(PROTOCOL_VERSION))
    );
    assert_eq!(client.connection_state(), ConnectionState::Connected);
}

#[tokio::test]
async fn rejects_server_data_before_client_hello_is_sent() {
    let sends = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let hello = encode_server_message(&server_hello(base_server_snapshot()), None).unwrap();
    let options = factory_from_closure({
        let sends = sends.clone();
        let closes = closes.clone();
        move |handlers: ByteTransportHandlers| -> TransportFuture {
            (handlers.on_data)(hello.clone());
            let sends = sends.clone();
            let closes = closes.clone();
            Box::pin(async move { Ok(Arc::new(CountingTransport { sends, closes }) as Arc<dyn ByteTransport>) })
        }
    });
    let client = PiClient::new(options).unwrap();

    let error = client.connect().unwrap().await.unwrap_err();

    assert!(
        matches!(&error, PiError::Protocol(inner) if inner.to_string() == "Received server data before the client hello was sent")
    );
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
    assert_eq!(sends.load(Ordering::SeqCst), 0);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn isolates_subscriber_failures_from_handshake_and_transport_state() {
    let server = MemoryByteServer::new();
    install_hello_responder(&server);
    let client = create_client(&server);
    let _subscription = client.subscribe(Arc::new(|_| panic!("consumer failure"))).unwrap();

    let snapshot = client.connect().unwrap().await.unwrap();

    assert_eq!(snapshot, base_server_snapshot());
    assert_eq!(client.connection_state(), ConnectionState::Connected);
}

#[tokio::test]
async fn reports_subscriber_failures_without_changing_connection_state() {
    let server = MemoryByteServer::new();
    install_hello_responder(&server);
    let errors = Arc::new(Mutex::new(Vec::new()));
    let target = errors.clone();
    let options = PiClientOptions::new(transport_factory(&server)).with_listener_error(Arc::new(move |error| {
        target.lock().unwrap().push(error.message());
    }));
    let client = PiClient::new(options).unwrap();
    let _subscription = client.subscribe(Arc::new(|_| panic!("consumer failure"))).unwrap();

    client.connect().unwrap().await.unwrap();

    assert_eq!(errors.lock().unwrap().as_slice(), ["consumer failure"]);
    assert_eq!(client.connection_state(), ConnectionState::Connected);
}

#[tokio::test]
async fn does_not_restore_connection_when_snapshot_listener_disconnects_during_handshake() {
    let server = MemoryByteServer::new();
    install_hello_responder(&server);
    let client = create_client(&server);
    let listener_client = client.clone();
    let _subscription = client
        .subscribe(Arc::new(move |_| listener_client.disconnect()))
        .unwrap();

    let error = client.connect().unwrap().await.unwrap_err();

    assert!(matches!(error, PiError::Disconnected(_)));
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
    assert_eq!(server.client_close_count(), 1);
}

#[tokio::test]
async fn does_not_restore_stale_connection_when_listener_reconnects_during_handshake() {
    let first = MemoryByteServer::new();
    let second = MemoryByteServer::new();
    for (index, server) in [first.clone(), second.clone()].into_iter().enumerate() {
        let responder = server.clone();
        server.on_message(Arc::new(move |message| {
            if let ClientMessage::Hello(_) = message {
                responder.send(server_hello(server_snapshot_revision(index as u64 + 1)));
            }
        }));
    }
    let connection = Arc::new(AtomicUsize::new(0));
    let factory = factory_from_closure({
        let connection = connection.clone();
        let first = first.clone();
        let second = second.clone();
        move |handlers: ByteTransportHandlers| -> TransportFuture {
            let server = if connection.fetch_add(1, Ordering::SeqCst) == 0 {
                first.clone()
            } else {
                second.clone()
            };
            Box::pin(async move { Ok(server.connect(handlers)) })
        }
    });
    let client = PiClient::new(factory).unwrap();
    let slot = Arc::new(Mutex::new(None));
    let listener_client = client.clone();
    let listener_slot = slot.clone();
    let _subscription = client
        .subscribe(Arc::new(move |_| {
            if listener_slot.lock().unwrap().is_some() {
                return;
            }
            listener_client.disconnect();
            let reconnect = listener_client.reconnect().unwrap();
            *listener_slot.lock().unwrap() = Some(reconnect);
        }))
        .unwrap();

    let error = client.connect().unwrap().await.unwrap_err();
    assert!(matches!(error, PiError::Disconnected(_)));

    let reconnect = slot.lock().unwrap().take().expect("listener must reconnect");
    let snapshot = reconnect.await.unwrap();
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(client.connection_state(), ConnectionState::Connected);
    assert_eq!(first.client_close_count(), 1);
}

#[tokio::test]
async fn rejects_typed_handshake_version_error() {
    let server = MemoryByteServer::new();
    let responder = server.clone();
    server.on_message(Arc::new(move |_| {
        responder.send(ServerMessage::HelloError(ServerHelloError::new(ProtocolError::new(
            ProtocolErrorCode::Version,
            "Unsupported protocol version".into(),
            None,
        ))));
    }));
    let client = create_client(&server);

    let error = client.connect().unwrap().await.unwrap_err();

    match error {
        PiError::Server(inner) => {
            assert_eq!(inner.code(), ProtocolErrorCode::Version);
            assert_eq!(inner.message(), "Unsupported protocol version");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
    assert_eq!(server.client_close_count(), 1);
}

#[tokio::test]
async fn rejects_pending_requests_on_close_and_reconnects_through_fresh_factory_result() {
    let first = MemoryByteServer::new();
    let second = MemoryByteServer::new();
    for (index, server) in [first.clone(), second.clone()].into_iter().enumerate() {
        let responder = server.clone();
        server.on_message(Arc::new(move |message| {
            if let ClientMessage::Hello(_) = message {
                responder.send(server_hello(server_snapshot_revision(index as u64 + 1)));
            }
        }));
    }
    let connection = Arc::new(AtomicUsize::new(0));
    let factory = factory_from_closure({
        let connection = connection.clone();
        let first = first.clone();
        let second = second.clone();
        move |handlers: ByteTransportHandlers| -> TransportFuture {
            let server = if connection.fetch_add(1, Ordering::SeqCst) == 0 {
                first.clone()
            } else {
                second.clone()
            };
            Box::pin(async move { Ok(server.connect(handlers)) })
        }
    });
    let client = PiClient::new(factory).unwrap();
    let states = Arc::new(Mutex::new(Vec::new()));
    let target = states.clone();
    let _subscription = client
        .on_connection_state_change(Arc::new(move |change| {
            target.lock().unwrap().push(change.state.as_str().to_string());
        }))
        .unwrap();
    client.connect().unwrap().await.unwrap();
    let pending = client.list_sessions().unwrap();
    first.close();
    let error = pending.await.unwrap_err();
    assert!(matches!(error, PiError::Disconnected(_)));
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);

    let snapshot = client.reconnect().unwrap().await.unwrap();
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(client.connection_state(), ConnectionState::Connected);
    assert_eq!(
        states.lock().unwrap().as_slice(),
        ["connecting", "connected", "disconnected", "connecting", "connected"]
    );
}

#[tokio::test]
async fn supports_synchronous_reconnect_from_disconnection_listener() {
    let first = MemoryByteServer::new();
    let second = MemoryByteServer::new();
    for (index, server) in [first.clone(), second.clone()].into_iter().enumerate() {
        let responder = server.clone();
        server.on_message(Arc::new(move |message| {
            if let ClientMessage::Hello(_) = message {
                responder.send(server_hello(server_snapshot_revision(index as u64 + 1)));
            }
        }));
    }
    let connection = Arc::new(AtomicUsize::new(0));
    let factory = factory_from_closure({
        let connection = connection.clone();
        let first = first.clone();
        let second = second.clone();
        move |handlers: ByteTransportHandlers| -> TransportFuture {
            let server = if connection.fetch_add(1, Ordering::SeqCst) == 0 {
                first.clone()
            } else {
                second.clone()
            };
            Box::pin(async move { Ok(server.connect(handlers)) })
        }
    });
    let client = PiClient::new(factory).unwrap();
    client.connect().unwrap().await.unwrap();
    let slot = Arc::new(Mutex::new(None));
    let listener_client = client.clone();
    let listener_slot = slot.clone();
    let _subscription = client
        .on_connection_state_change(Arc::new(move |change| {
            if change.state == ConnectionState::Disconnected {
                *listener_slot.lock().unwrap() = Some(listener_client.reconnect().unwrap());
            }
        }))
        .unwrap();

    first.close();
    let reconnect = slot.lock().unwrap().take().expect("listener must reconnect");
    let snapshot = reconnect.await.unwrap();
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(client.connection_state(), ConnectionState::Connected);
}

#[tokio::test]
async fn rejects_pending_requests_on_transport_errors() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let pending = client.list_sessions().unwrap();
    server.error(PiError::Transport("read failed".into()));

    let error = pending.await.unwrap_err();
    match error {
        PiError::Disconnected(inner) => assert_eq!(inner.message(), "read failed"),
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn enforces_configured_frame_limit_for_outbound_and_inbound_messages() {
    let server = MemoryByteServer::new();
    install_hello_responder(&server);
    let options = PiClientOptions::new(transport_factory(&server)).with_max_frame_length(512);
    let client = PiClient::new(options).unwrap();
    client.connect().unwrap().await.unwrap();
    let handle = attach_session(&client, &server, session_snapshot("session-1")).await;
    let sent_before = server.sent_by_client().len();
    let error = match handle.prompt("x".repeat(1_000)) {
        Err(error) => error,
        Ok(prompt) => prompt.await.unwrap_err(),
    };
    assert!(matches!(error, PiError::Protocol(_)));
    assert_eq!(server.sent_by_client().len(), sent_before);

    server.send_raw(vec![0, 0, 2, 1]);
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn disconnects_on_invalid_protocol_data() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let payload = encode_cbor(
        &CborValue::Map(vec![
            ("type".into(), CborValue::Text("event".into())),
            (
                "event".into(),
                CborValue::Map(vec![
                    ("type".into(), CborValue::Text("session_removed".into())),
                    ("sessionId".into(), CborValue::Number(1.0)),
                ]),
            ),
        ]),
        None,
    )
    .unwrap();
    server.send_raw(encode_frame(&payload).unwrap());
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn reports_truncated_framing_when_transport_closes() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let pending = client.list_sessions().unwrap();
    server.send_raw(vec![0, 0, 0, 2, 1]);
    server.close();

    let error = pending.await.unwrap_err();
    match error {
        PiError::Protocol(inner) => assert!(inner.to_string().to_lowercase().contains("truncated")),
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
}

#[test]
fn rejects_frame_limits_outside_unsigned_32_bit_range() {
    let server = MemoryByteServer::new();
    let options = PiClientOptions::new(transport_factory(&server)).with_max_frame_length(0);
    let error = PiClient::new(options).unwrap_err();
    assert!(error.message().contains("maxFrameLength"));
}
