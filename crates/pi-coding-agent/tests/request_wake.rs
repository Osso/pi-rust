use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use pi_coding_agent::supervisor::request_wake::{
    SupervisorRequestWakeServer, get_supervisor_request_socket_path, notify_supervisor_request,
};
use pi_js_compat::abort::AbortSignal;
use tokio::net::UnixStream;

fn socket_path() -> (tempfile::TempDir, JsString) {
    let directory = tempfile::tempdir().unwrap();
    let control_db = directory.path().join("control.sqlite").to_string_lossy().into_owned();
    (directory, control_db.into())
}

fn socket_file(control_db: &JsString) -> String {
    encode_node_utf8(&get_supervisor_request_socket_path(control_db))
}

#[test]
fn derives_the_socket_path_from_the_control_database() {
    assert_eq!(
        get_supervisor_request_socket_path(&"/home/osso/.pi/control.sqlite".into()),
        "/home/osso/.pi/control.sqlite.supervisor.sock"
    );
}

#[tokio::test]
async fn wakes_waiters_and_advances_the_generation_on_each_request() {
    let (_directory, path) = socket_path();
    let server = Arc::new(SupervisorRequestWakeServer::new(&path));
    server.start().await.expect("server must start");
    assert_eq!(server.current_generation(), 0);
    assert!(Path::new(&socket_file(&path)).exists());

    let observed = server.current_generation();
    let waiter = tokio::spawn({
        let server = Arc::clone(&server);
        async move {
            server.wait_for_wake_after(observed, &AbortSignal::default()).await;
            server.current_generation()
        }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    notify_supervisor_request(&path);

    let generation = tokio::time::timeout(Duration::from_secs(2), waiter)
        .await
        .expect("waiter must wake")
        .unwrap();
    assert_eq!(generation, 1);
    server.close().await.unwrap();
    assert!(!Path::new(&socket_file(&path)).exists());
}

#[tokio::test]
async fn returns_immediately_for_already_observed_generations_and_aborts() {
    let (_directory, path) = socket_path();
    let server = Arc::new(SupervisorRequestWakeServer::new(&path));
    server.start().await.unwrap();
    notify_supervisor_request(&path);
    tokio::time::sleep(Duration::from_millis(50)).await;

    let generation = server.current_generation();
    tokio::time::timeout(
        Duration::from_secs(1),
        server.wait_for_wake_after(generation - 1, &AbortSignal::default()),
    )
    .await
    .expect("an already-advanced generation must resolve immediately");

    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    controller_signal.abort(None);
    tokio::time::timeout(Duration::from_secs(1), server.wait_for_wake_after(generation, &signal))
        .await
        .expect("an aborted signal must resolve immediately");
    server.close().await.unwrap();
}

#[tokio::test]
async fn recovers_a_stale_socket_file() {
    let (_directory, control_db) = socket_path();
    let path = socket_file(&control_db);
    std::fs::create_dir_all(Path::new(&path).parent().unwrap()).unwrap();
    let stale = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(stale);

    let server = Arc::new(SupervisorRequestWakeServer::new(&control_db));
    server.start().await.expect("a stale socket must be replaced");
    assert!(UnixStream::connect(&path).await.is_ok());
    server.close().await.unwrap();
}

#[tokio::test]
async fn refuses_to_replace_an_active_socket() {
    let (_directory, path) = socket_path();
    let first = Arc::new(SupervisorRequestWakeServer::new(&path));
    first.start().await.unwrap();

    let second = Arc::new(SupervisorRequestWakeServer::new(&path));
    let error = second.start().await.expect_err("an active socket must be refused");
    assert_eq!(
        error,
        JsString::from(format!(
            "Supervisor request socket is already active: {}",
            socket_file(&path)
        ))
    );
    first.close().await.unwrap();
}

#[tokio::test]
async fn start_is_idempotent() {
    let (_directory, path) = socket_path();
    let server = Arc::new(SupervisorRequestWakeServer::new(&path));
    server.start().await.unwrap();
    server.start().await.unwrap();
    assert_eq!(server.current_generation(), 0);
    server.close().await.unwrap();
}

#[tokio::test]
async fn close_resolves_pending_waiters() {
    let (_directory, path) = socket_path();
    let server = Arc::new(SupervisorRequestWakeServer::new(&path));
    server.start().await.unwrap();
    let waiter = tokio::spawn({
        let server = Arc::clone(&server);
        async move { server.wait_for_wake_after(0, &AbortSignal::default()).await }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    server.close().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), waiter)
        .await
        .expect("close must resolve waiters")
        .unwrap();
}

async fn ping_and_wait_for_accept(server: &SupervisorRequestWakeServer) {
    use tokio::io::AsyncReadExt;

    let mut stream = UnixStream::connect(encode_node_utf8(server.socket_path()))
        .await
        .expect("listener must accept connections");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .expect("accepted connection must close")
        .unwrap();
    assert!(response.is_empty());
}

#[tokio::test]
async fn historical_wake_does_not_resolve_a_wait_for_the_current_generation() {
    let (_directory, path) = socket_path();
    let server = SupervisorRequestWakeServer::new(&path);
    server.start().await.unwrap();
    ping_and_wait_for_accept(&server).await;
    assert_eq!(server.current_generation(), 1);

    let signal = AbortSignal::default();
    let waiter = server.wait_for_wake_after(1, &signal);
    tokio::pin!(waiter);
    assert!(
        futures_util::poll!(&mut waiter).is_pending(),
        "a historical ping must not wake the waiter"
    );
    ping_and_wait_for_accept(&server).await;
    waiter.await;
    assert_eq!(server.current_generation(), 2);
    server.close().await.unwrap();
}

#[tokio::test]
async fn close_then_start_reopens_the_listener_without_resetting_generation() {
    let (_directory, path) = socket_path();
    let server = SupervisorRequestWakeServer::new(&path);
    server.start().await.unwrap();
    ping_and_wait_for_accept(&server).await;
    assert_eq!(server.current_generation(), 1);
    server.close().await.unwrap();
    server.start().await.unwrap();

    ping_and_wait_for_accept(&server).await;
    assert_eq!(server.current_generation(), 2);
    // The EOF handshake above lets the accept loop run before the next connection.
    ping_and_wait_for_accept(&server).await;
    assert_eq!(server.current_generation(), 3);
    server.close().await.unwrap();
}

#[tokio::test]
async fn dropping_a_rejected_candidate_preserves_the_active_owners_socket() {
    let (_directory, path) = socket_path();
    let owner = SupervisorRequestWakeServer::new(&path);
    owner.start().await.unwrap();
    {
        let candidate = SupervisorRequestWakeServer::new(&path);
        assert_eq!(
            candidate.start().await.unwrap_err(),
            JsString::from(format!(
                "Supervisor request socket is already active: {}",
                socket_file(&path)
            ))
        );
    }

    assert!(
        Path::new(&socket_file(&path)).exists(),
        "rejected candidate must not unlink the owner's socket"
    );
    ping_and_wait_for_accept(&owner).await;
    owner.close().await.unwrap();
}

#[test]
fn notifying_a_missing_socket_is_silent() {
    notify_supervisor_request(&"/tmp/pi-rust-missing-supervisor/control.sqlite".into());
}
