use std::sync::{Arc, Mutex};

use crate::support::*;
use pi_client::{PiError, SessionLeaseMode};
use pi_protocol::{ClientMessage, CommandResult, ProtocolErrorCode, SessionPhase, ThinkingLevel};

fn respond_to_session_commands(server: &Arc<MemoryByteServer>) {
    let responder = server.clone();
    server.on_message(Arc::new(move |message| {
        let ClientMessage::Request(request) = message else {
            return;
        };
        match request.request() {
            pi_protocol::Command::Attach(command) => {
                responder.send(success_response(
                    request.id(),
                    CommandResult::Attach(session_snapshot(command.session_id())),
                ));
            }
            pi_protocol::Command::Detach(command) => {
                responder.send(success_response(request.id(), detach_result(command.session_id())));
            }
            _ => {}
        }
    }));
}

#[tokio::test]
async fn keeps_multiple_session_handles_independent_and_enforces_detach() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    respond_to_session_commands(&server);

    let first = client.attach_session("session-1".to_string()).unwrap().await.unwrap();
    let second = client.attach_session("session-2".to_string()).unwrap().await.unwrap();
    assert!(first.attached());
    assert!(second.attached());
    first.detach().unwrap().await.unwrap();
    assert!(!first.attached());
    assert!(second.attached());
    let error = match first.abort() {
        Err(error) => error,
        Ok(abort) => abort.await.unwrap_err(),
    };
    assert!(matches!(error, PiError::SessionDetached(_)));
}

#[tokio::test]
async fn detaches_shared_session_only_after_final_lease_release() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let requests = collect_requests(&server);
    respond_to_session_commands(&server);

    let first = client.attach_session("session-1".to_string()).unwrap().await.unwrap();
    let second = client.attach_session("session-1".to_string()).unwrap().await.unwrap();
    assert_eq!(requests.commands(), ["attach"]);

    first.detach().unwrap().await.unwrap();
    assert!(!first.attached());
    assert!(second.attached());
    assert_eq!(requests.commands(), ["attach"]);

    second.detach().unwrap().await.unwrap();
    assert!(!second.attached());
    assert_eq!(requests.commands(), ["attach", "detach"]);
}

#[tokio::test]
async fn enforces_exclusive_and_shared_lease_modes() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    respond_to_session_commands(&server);

    let shared = client
        .acquire_session("session-1".to_string(), SessionLeaseMode::Shared)
        .unwrap()
        .await
        .unwrap();
    let Err(error) = client.acquire_session("session-1".to_string(), SessionLeaseMode::Exclusive) else {
        panic!("exclusive acquisition must fail");
    };
    assert!(matches!(error, PiError::SessionOwnership(_)));
    shared.dispose().unwrap().await.unwrap();

    let exclusive = client
        .acquire_session("session-1".to_string(), SessionLeaseMode::Exclusive)
        .unwrap()
        .await
        .unwrap();
    let Err(error) = client.acquire_session("session-1".to_string(), SessionLeaseMode::Shared) else {
        panic!("shared acquisition must fail");
    };
    assert!(matches!(error, PiError::SessionOwnership(_)));
    exclusive.dispose().unwrap().await.unwrap();
}

#[tokio::test]
async fn invalidated_leases_dispose_without_protocol_cleanup() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let responder = server.clone();
    server.on_message(Arc::new(move |message| {
        let ClientMessage::Request(request) = message else {
            return;
        };
        if let pi_protocol::Command::Attach(command) = request.request() {
            responder.send(success_response(
                request.id(),
                CommandResult::Attach(session_snapshot(command.session_id())),
            ));
        }
    }));
    let requests = collect_requests(&server);
    let lease = client
        .acquire_session("session-1".to_string(), SessionLeaseMode::Exclusive)
        .unwrap()
        .await
        .unwrap();

    client.disconnect();

    lease.dispose().unwrap().await.unwrap();
    assert!(!lease.active());
    assert_eq!(requests.commands(), ["attach"]);
}

#[tokio::test]
async fn rejects_commands_while_releasing_and_restores_explicit_detach_after_failure() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let requests = collect_requests(&server);
    let acquiring = client
        .acquire_session("session-1".to_string(), SessionLeaseMode::Exclusive)
        .unwrap();
    let attach_request = requests.last().expect("missing attach request");
    server.send(success_response(
        attach_request.id(),
        CommandResult::Attach(session_snapshot("session-1")),
    ));
    let lease = acquiring.await.unwrap();

    let first_detach = lease.detach().unwrap();
    let failed_detach_request = requests.last().expect("missing detach request");
    let error = match lease.abort() {
        Err(error) => error,
        Ok(abort) => abort.await.unwrap_err(),
    };
    assert!(matches!(error, PiError::SessionDetached(_)));
    server.send(error_response(
        failed_detach_request.id(),
        ProtocolErrorCode::InvalidRequest,
        "retry",
    ));
    let error = first_detach.await.unwrap_err();
    assert_eq!(error.message(), "retry");
    assert!(lease.active());

    let second_detach = lease.detach().unwrap();
    let successful_detach_request = requests.last().expect("missing retry detach request");
    assert_ne!(successful_detach_request.id(), failed_detach_request.id());
    server.send(success_response(
        successful_detach_request.id(),
        detach_result("session-1"),
    ));
    second_detach.await.unwrap();
    assert!(!lease.active());
}

#[tokio::test]
async fn serializes_reacquisition_behind_final_lease_detachment() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let requests = collect_requests(&server);

    let first_attachment = client.attach_session("session-1".to_string()).unwrap();
    let first_attach_request = requests.last().expect("missing first attach request");
    server.send(success_response(
        first_attach_request.id(),
        CommandResult::Attach(session_snapshot("session-1")),
    ));
    let first = first_attachment.await.unwrap();
    let detaching = first.detach().unwrap();
    let detach_request = requests.last().expect("missing detach request");
    let reacquiring_client = client.clone();
    let reacquiring = tokio::spawn(async move {
        reacquiring_client
            .attach_session("session-1".to_string())
            .unwrap()
            .await
    });
    assert_eq!(requests.commands(), ["attach", "detach"]);

    server.send(success_response(detach_request.id(), detach_result("session-1")));
    detaching.await.unwrap();
    for _ in 0..20 {
        if requests
            .last()
            .is_some_and(|request| request.request().as_str() == "attach")
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    let second_attach_request = requests.last().expect("missing second attach request");
    assert_eq!(second_attach_request.request().as_str(), "attach");
    server.send(success_response(
        second_attach_request.id(),
        CommandResult::Attach(session_snapshot_with(
            "session-1",
            2,
            SessionPhase::Idle,
            ThinkingLevel::Off,
            true,
        )),
    ));

    let reacquired = reacquiring.await.unwrap().unwrap();
    assert!(reacquired.attached());
}

#[tokio::test]
async fn accepts_lower_revision_after_detaching_and_reacquiring_same_session() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let attach_count = Arc::new(Mutex::new(0u64));
    let responder = server.clone();
    server.on_message(Arc::new(move |message| {
        let ClientMessage::Request(request) = message else {
            return;
        };
        match request.request() {
            pi_protocol::Command::Attach(command) => {
                let mut count = attach_count.lock().unwrap();
                let revision = if *count == 0 { 10 } else { 0 };
                *count += 1;
                responder.send(success_response(
                    request.id(),
                    CommandResult::Attach(session_snapshot_with(
                        command.session_id(),
                        revision,
                        SessionPhase::Idle,
                        ThinkingLevel::Off,
                        true,
                    )),
                ));
            }
            pi_protocol::Command::Detach(command) => {
                responder.send(success_response(request.id(), detach_result(command.session_id())));
            }
            _ => {}
        }
    }));

    let first = client.attach_session("session-1".to_string()).unwrap().await.unwrap();
    assert_eq!(first.snapshot().unwrap().revision(), 10);
    first.detach().unwrap().await.unwrap();
    let reopened = client.attach_session("session-1".to_string()).unwrap().await.unwrap();
    assert_eq!(reopened.snapshot().unwrap().revision(), 0);
}
