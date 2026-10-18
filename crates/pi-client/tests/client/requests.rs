use std::sync::Arc;

use crate::support::*;
use pi_client::{ConnectionState, PiError};
use pi_protocol::{CommandResult, ProtocolErrorCode};

#[tokio::test]
async fn correlates_coalesced_out_of_order_responses() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let requests = collect_requests(&server);
    let listed = client.list_sessions().unwrap();
    let attached = client.attach_session("session-1".to_string()).unwrap();
    assert_eq!(requests.len(), 2);

    let attach_request = requests.find_command("attach").expect("missing attach request");
    let list_request = requests.find_command("list").expect("missing list request");
    server.send_together(vec![
        success_response(
            attach_request.id(),
            CommandResult::Attach(session_snapshot("session-1")),
        ),
        success_response(
            list_request.id(),
            CommandResult::List(pi_protocol::ListResult::new(Vec::new())),
        ),
    ]);

    let sessions = listed.await.unwrap();
    assert!(matches!(sessions, CommandResult::List(ref result) if result.sessions().is_empty()));
    let handle = attached.await.unwrap();
    assert_eq!(handle.id(), "session-1");
    assert!(handle.attached());
}

#[tokio::test]
async fn rejects_mismatched_response_instead_of_leaving_request_pending() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let requests = collect_requests(&server);
    let listed = client.list_sessions().unwrap();
    assert_eq!(requests.commands(), ["list"]);
    server.send(success_response(
        requests.get(0).unwrap().id(),
        CommandResult::Attach(session_snapshot("session-1")),
    ));

    let error = listed.await.unwrap_err();
    match error {
        PiError::Protocol(inner) => {
            assert_eq!(inner.to_string(), "Response command attach does not match list");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn surfaces_typed_request_errors() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let requests = collect_requests(&server);
    let attaching = client.attach_session("locked".to_string()).unwrap();
    server.send(error_response(
        requests.get(0).unwrap().id(),
        ProtocolErrorCode::SessionLocked,
        "Already attached",
    ));

    let error = attaching.await.unwrap_err();
    match error {
        PiError::Server(inner) => {
            assert_eq!(inner.code(), ProtocolErrorCode::SessionLocked);
            assert_eq!(inner.message(), "Already attached");
        }
        other => panic!("unexpected error: {other:?}"),
    }
    let _ = Arc::new(());
}
