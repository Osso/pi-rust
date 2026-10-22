use pi_coding_agent::supervisor::request_wake::{
    SupervisorRequestWakeServer, get_supervisor_request_socket_path, notify_supervisor_request,
};
use pi_js_compat::js_json::{JsString, encode_node_utf8};

#[tokio::test]
async fn wake_socket_retains_raw_path_until_bind_and_connect() {
    let temp = tempfile::tempdir().unwrap();
    let path = JsString::from(temp.path().to_str().unwrap()) + "/control-" + JsString::from_utf16(vec![0xd800]);
    let socket = get_supervisor_request_socket_path(&path);
    assert_eq!(socket, path.clone() + ".supervisor.sock");
    let server = SupervisorRequestWakeServer::new(&path);
    assert_eq!(server.socket_path(), &socket);
    server.start().await.unwrap();
    let observed = server.current_generation();
    notify_supervisor_request(&path);
    let signal = tokio_util::sync::CancellationToken::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        server.wait_for_wake_after(observed, &signal),
    )
    .await
    .unwrap();
    assert!(server.current_generation() > observed);
    server.close().await.unwrap();
    assert!(!std::path::Path::new(&encode_node_utf8(&socket)).exists());
}
