use crate::support::*;
use pi_client::{ConnectionState, PiClient, PiClientOptions, PiError};

#[tokio::test]
async fn connects_through_its_ownership_factory() {
    let server = MemoryByteServer::new();
    install_hello_responder(&server);

    let client = PiClient::connect_new(PiClientOptions::new(transport_factory(&server)))
        .await
        .unwrap();

    assert!(client.connected());
    client.dispose().await;
}

#[tokio::test]
async fn disconnects_invalidates_child_handles_and_rejects_pending_requests() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let handle = attach_session(&client, &server, session_snapshot("session-1")).await;
    let pending = client.list_sessions().unwrap();

    let first_disposal = client.dispose();
    let second_disposal = client.dispose();

    assert!(client.disposed());
    assert!(!client.connected());
    assert!(!handle.attached());
    let error = pending.await.unwrap_err();
    assert!(matches!(error, PiError::Disposed(_)));
    let error = match handle.prompt("after disposal") {
        Err(error) => error,
        Ok(prompt) => prompt.await.unwrap_err(),
    };
    assert!(matches!(error, PiError::Disposed(_)));
    first_disposal.await;
    second_disposal.await;
}

#[tokio::test]
async fn supports_explicit_async_disposal() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;

    client.dispose().await;

    assert!(client.disposed());
    assert_eq!(client.connection_state(), ConnectionState::Disconnected);
}
