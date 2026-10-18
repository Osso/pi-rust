use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pi_protocol::{ProtocolErrorCode, ServerMessage, ServerMessageDecoder};
use pi_server::connection::{ByteConnection, ConnectionResult};
use pi_server::testing::TestServerService;
use pi_server::transports::unix::{UnixServerOptions, create_unix_server};
use pi_server::types::BoxFuture;
use pi_server::{PiServer, PiServerOptions, PiServerService};

fn service() -> Arc<dyn PiServerService> {
    Arc::new(TestServerService::new())
}

fn expect_error<T, E>(result: Result<T, E>, message: &str) -> E {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

struct BlockedConnection {
    closed: AtomicBool,
    final_chunk: Mutex<Option<Vec<u8>>>,
}

impl BlockedConnection {
    fn new() -> Self {
        Self {
            closed: AtomicBool::new(false),
            final_chunk: Mutex::new(None),
        }
    }
}

impl ByteConnection for BlockedConnection {
    fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn send(&self, _chunk: Vec<u8>) -> BoxFuture<'_, ConnectionResult> {
        Box::pin(std::future::pending())
    }

    fn close(&self, final_chunk: Option<Vec<u8>>) -> BoxFuture<'_, ConnectionResult> {
        Box::pin(async move {
            *self.final_chunk.lock().expect("final chunk mutex poisoned") = final_chunk;
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        })
    }
}

#[tokio::test]
async fn empty_listener_list_is_valid_and_has_no_addresses() {
    let server = PiServer::new(service(), PiServerOptions::default()).expect("options must be valid");
    server.start().await.expect("server must start");
    assert!(server.addresses().is_empty());
    server.close().await.expect("server must close");
}

#[tokio::test]
async fn rejects_unix_socket_paths_that_cannot_fit_in_sockaddr_un() {
    let path = format!("/tmp/{}", "x".repeat(512));
    let error = expect_error(
        create_unix_server(
            service(),
            UnixServerOptions {
                path,
                ..UnixServerOptions::default()
            },
        ),
        "overlong path must fail",
    );
    assert!(error.to_string().contains("too long"));
}

#[tokio::test]
async fn rejects_an_overlong_derived_private_unix_bind_path() {
    let max_length = 107;
    let suffix_length = "/tmp//s".len();
    let path = format!("/tmp/{}/s", "x".repeat(max_length - suffix_length));
    let server = create_unix_server(
        service(),
        UnixServerOptions {
            path,
            ..UnixServerOptions::default()
        },
    )
    .expect("configured path must validate");

    let error = expect_error(server.start().await, "private bind path must fail");
    assert!(error.to_string().contains("private Unix bind path"));
    assert!(error.to_string().contains("too long"));
}

#[tokio::test]
async fn rejects_concurrent_start_calls_without_leaking_the_unix_listener() {
    let directory = tempfile::tempdir().expect("temporary directory must be created");
    let path = directory.path().join("server.sock").to_string_lossy().into_owned();
    let server = Arc::new(
        create_unix_server(
            service(),
            UnixServerOptions {
                path: path.clone(),
                ..UnixServerOptions::default()
            },
        )
        .expect("server options must be valid"),
    );

    let starting = server.start();
    let error = server.start().await.expect_err("second start must fail");
    assert!(error.to_string().contains("starting"));
    starting.await.expect("first start must succeed");
    server.close().await.expect("server must close");
    assert!(server.addresses().is_empty());
    assert!(std::fs::symlink_metadata(&path).is_err());
}

#[tokio::test]
async fn handshake_timeout_cleanup_does_not_wait_for_a_blocked_output_queue() {
    let core = PiServer::new(
        service(),
        PiServerOptions {
            max_frame_length: Some(1024),
            handshake_timeout_ms: Some(10),
            ..PiServerOptions::default()
        },
    )
    .expect("server options must be valid");
    let connection = Arc::new(BlockedConnection::new());
    let _handler = core.accept(connection.clone());

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while !connection.closed.load(Ordering::SeqCst) {
        assert!(tokio::time::Instant::now() < deadline, "handshake timeout must close");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let final_chunk = connection
        .final_chunk
        .lock()
        .expect("final chunk mutex poisoned")
        .clone()
        .expect("timeout must write a final frame");
    let messages = ServerMessageDecoder::new(None)
        .push(&final_chunk)
        .expect("final frame must decode");
    match &messages[0] {
        ServerMessage::HelloError(error) => {
            assert_eq!(error.error().code(), ProtocolErrorCode::InvalidRequest);
        }
        other => panic!("expected hello_error, got {other:?}"),
    }
    core.close().await.expect("server must close");
}

#[tokio::test]
async fn rejects_timeout_values_above_the_maximum_timer_delay() {
    let path = "/tmp/pi-server-timeout-test.sock".to_string();
    let error = expect_error(
        create_unix_server(
            service(),
            UnixServerOptions {
                path: path.clone(),
                handshake_timeout_ms: Some(2_147_483_648),
                ..UnixServerOptions::default()
            },
        ),
        "handshake timeout must fail",
    );
    assert!(error.to_string().contains("handshakeTimeoutMs"));

    let error = expect_error(
        create_unix_server(
            service(),
            UnixServerOptions {
                path,
                graceful_close_timeout_ms: Some(2_147_483_648),
                ..UnixServerOptions::default()
            },
        ),
        "graceful close timeout must fail",
    );
    assert!(error.to_string().contains("gracefulCloseTimeoutMs"));
}

#[tokio::test]
async fn rejects_pending_byte_limits_smaller_than_one_maximum_frame() {
    let error = expect_error(
        create_unix_server(
            service(),
            UnixServerOptions {
                path: "/tmp/pi-server-pending-bytes-test.sock".to_string(),
                max_frame_length: Some(128),
                max_pending_bytes: Some(131),
                ..UnixServerOptions::default()
            },
        ),
        "pending byte limit must fail",
    );
    assert!(error.to_string().contains("maxPendingBytes"));
}
