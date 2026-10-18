use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::support::*;
use pi_client::unix::UnixTransportOptions;
use pi_client::{ByteTransportHandlers, PiClient, PiClientOptions, PiError, create_unix_transport_factory};
use pi_protocol::{
    ClientMessage, ClientMessageDecoder, CommandResult, ListResult, ServerMessage, encode_server_message,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio_util::sync::CancellationToken;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("pi-client-{}-{sequence}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn socket_path(&self) -> PathBuf {
        self.0.join("pi.sock")
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn write_server_message(stream: &mut UnixStream, message: ServerMessage) {
    let frame = encode_server_message(&message, None).unwrap();
    stream.write_all(&frame).await.unwrap();
}

#[test]
fn rejects_invalid_unix_transport_options() {
    let Err(empty) = create_unix_transport_factory(UnixTransportOptions {
        path: String::new(),
        max_pending_bytes: None,
    }) else {
        panic!("empty path must fail");
    };
    assert!(empty.message().contains("must not be empty"));

    let Err(long) = create_unix_transport_factory(UnixTransportOptions {
        path: format!("/tmp/{}", "x".repeat(512)),
        max_pending_bytes: None,
    }) else {
        panic!("long path must fail");
    };
    assert!(long.message().contains("too long"));

    let Err(zero) = create_unix_transport_factory(UnixTransportOptions {
        path: "/tmp/pi.sock".into(),
        max_pending_bytes: Some(0),
    }) else {
        panic!("zero pending bytes must fail");
    };
    assert!(zero.message().contains("positive"));
}

#[tokio::test]
async fn exchanges_fragmented_framed_messages_over_real_unix_socket() {
    let directory = TempDirectory::new();
    let socket_path = directory.socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let cancel = CancellationToken::new();
    let server_cancel = cancel.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut decoder = ClientMessageDecoder::new(None);
        let mut buffer = vec![0u8; 4096];
        loop {
            let read = tokio::select! {
                _ = server_cancel.cancelled() => return,
                read = stream.read(&mut buffer) => read.unwrap(),
            };
            if read == 0 {
                return;
            }
            for message in decoder.push(&buffer[..read]).unwrap() {
                match message {
                    ClientMessage::Hello(_) => {
                        let frame = encode_server_message(&server_hello(server_snapshot_revision(4)), None).unwrap();
                        for byte in frame {
                            stream.write_all(&[byte]).await.unwrap();
                        }
                    }
                    ClientMessage::Request(request) => {
                        let response = success_response(request.id(), CommandResult::List(ListResult::new(Vec::new())));
                        let frame = encode_server_message(&response, None).unwrap();
                        let split = frame.len() / 2;
                        stream.write_all(&frame[..split]).await.unwrap();
                        stream.write_all(&frame[split..]).await.unwrap();
                    }
                }
            }
        }
    });
    let factory = create_unix_transport_factory(UnixTransportOptions {
        path: socket_path.to_string_lossy().into_owned(),
        max_pending_bytes: None,
    })
    .unwrap();
    let client = PiClient::new(PiClientOptions::new(factory)).unwrap();

    let snapshot = client.connect().unwrap().await.unwrap();
    assert_eq!(snapshot.revision(), 4);
    let (first, second) = tokio::join!(client.list_sessions().unwrap(), client.list_sessions().unwrap());
    for result in [first, second] {
        match result.unwrap() {
            CommandResult::List(list) => assert!(list.sessions().is_empty()),
            other => panic!("unexpected result: {other:?}"),
        }
    }

    client.disconnect();
    cancel.cancel();
    let _ = server.await;
}

#[tokio::test]
async fn bounds_pending_writes_preserves_order_and_reports_remote_end_once() {
    let directory = TempDirectory::new();
    let socket_path = directory.socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let first = vec![1u8; 2 * 1024 * 1024];
    let second = vec![2u8; 2 * 1024 * 1024];
    let first_length = first.len();
    let expected_length = first_length + second.len();
    let start_reading = Arc::new(tokio::sync::Notify::new());
    let server_start = start_reading.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        server_start.notified().await;
        let mut received = Vec::new();
        let mut buffer = vec![0u8; 64 * 1024];
        while received.len() < expected_length {
            let read = stream.read(&mut buffer).await.unwrap();
            assert_ne!(read, 0);
            received.extend_from_slice(&buffer[..read]);
        }
        let invalid_order = received
            .iter()
            .enumerate()
            .any(|(index, byte)| *byte != if index < first_length { 1 } else { 2 });
        assert!(!invalid_order);
        stream.write_all(&[9]).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let inbound = Arc::new(Mutex::new(Vec::new()));
    let errors = Arc::new(Mutex::new(Vec::new()));
    let close_count = Arc::new(AtomicU64::new(0));
    let closed = Arc::new(tokio::sync::Notify::new());
    let handlers = ByteTransportHandlers {
        on_data: {
            let inbound = inbound.clone();
            Arc::new(move |chunk| inbound.lock().unwrap().extend(chunk))
        },
        on_close: {
            let close_count = close_count.clone();
            let closed = closed.clone();
            Arc::new(move || {
                close_count.fetch_add(1, Ordering::SeqCst);
                closed.notify_waiters();
            })
        },
        on_error: {
            let errors = errors.clone();
            Arc::new(move |error| errors.lock().unwrap().push(error))
        },
    };
    let factory = create_unix_transport_factory(UnixTransportOptions {
        path: socket_path.to_string_lossy().into_owned(),
        max_pending_bytes: Some(expected_length),
    })
    .unwrap();
    let transport = factory.create(handlers).await.unwrap();

    let first_write = transport.send(first);
    let second_write = transport.send(second);
    let error = transport.send(vec![3]).await.unwrap_err();
    assert!(error.message().contains("pending byte limit"));
    start_reading.notify_one();
    first_write.await.unwrap();
    second_write.await.unwrap();
    for _ in 0..100 {
        if close_count.load(Ordering::SeqCst) > 0 {
            break;
        }
        closed.notified().await;
    }
    assert_eq!(inbound.lock().unwrap().as_slice(), [9]);
    assert!(errors.lock().unwrap().is_empty());
    tokio::task::yield_now().await;
    assert_eq!(close_count.load(Ordering::SeqCst), 1);
    transport.close();
    let _ = server.await;
}

#[tokio::test]
async fn rejects_truncated_final_frame_from_real_unix_socket() {
    let directory = TempDirectory::new();
    let socket_path = directory.socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let cancel = CancellationToken::new();
    let server_cancel = cancel.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut decoder = ClientMessageDecoder::new(None);
        let mut buffer = vec![0u8; 4096];
        loop {
            let read = tokio::select! {
                _ = server_cancel.cancelled() => return,
                read = stream.read(&mut buffer) => read.unwrap(),
            };
            if read == 0 {
                return;
            }
            for message in decoder.push(&buffer[..read]).unwrap() {
                match message {
                    ClientMessage::Hello(_) => {
                        write_server_message(&mut stream, server_hello(server_snapshot_revision(4))).await;
                    }
                    ClientMessage::Request(_) => {
                        stream.write_all(&[0, 0, 0, 2, 1]).await.unwrap();
                        stream.shutdown().await.unwrap();
                        return;
                    }
                }
            }
        }
    });
    let factory = create_unix_transport_factory(UnixTransportOptions {
        path: socket_path.to_string_lossy().into_owned(),
        max_pending_bytes: None,
    })
    .unwrap();
    let client = PiClient::new(PiClientOptions::new(factory)).unwrap();

    client.connect().unwrap().await.unwrap();
    let error = client.list_sessions().unwrap().await.unwrap_err();
    assert!(matches!(error, PiError::Protocol(_)));
    assert_eq!(client.connection_state(), pi_client::ConnectionState::Disconnected);
    client.disconnect();
    cancel.cancel();
    let _ = server.await;
}

#[tokio::test]
async fn rejects_connection_errors() {
    let directory = TempDirectory::new();
    let missing = directory.socket_path();
    let factory = create_unix_transport_factory(UnixTransportOptions {
        path: missing.to_string_lossy().into_owned(),
        max_pending_bytes: None,
    })
    .unwrap();
    let handlers = ByteTransportHandlers {
        on_data: Arc::new(|_| {}),
        on_close: Arc::new(|| {}),
        on_error: Arc::new(|_| {}),
    };
    let result = factory.create(handlers).await;
    let Err(error) = result else {
        panic!("missing socket must fail");
    };
    match error {
        PiError::Io { raw_os_error, .. } => assert_eq!(raw_os_error, Some(2)),
        other => panic!("unexpected error: {other:?}"),
    }
}
