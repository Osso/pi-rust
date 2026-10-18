use std::sync::{Arc, Mutex};

use crate::support::*;
use pi_protocol::CommandResult;
use pi_protocol::{ClientMessage, SessionMetadata, SessionMetadataInput, SessionPhase, ThinkingLevel};

#[tokio::test]
async fn reduces_only_authoritative_snapshots_and_supports_unsubscribe() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let requests = collect_requests(&server);
    let initial = session_snapshot_with("session-1", 1, SessionPhase::Idle, ThinkingLevel::Off, true);
    let handle = attach_session(&client, &server, initial.clone()).await;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let progress_types = Arc::new(Mutex::new(Vec::new()));
    let observed_target = observed.clone();
    let unsubscribe = handle
        .subscribe(Arc::new(move |snapshot| {
            observed_target.lock().unwrap().push(snapshot.revision());
        }))
        .unwrap();
    let progress_target = progress_types.clone();
    let unsubscribe_events = handle
        .on_event(Arc::new(move |event| {
            progress_target.lock().unwrap().push(event.as_str().to_string());
        }))
        .unwrap();
    server.send(session_progress_event("session-1", "assistant-1", "hi"));
    assert_eq!(progress_types.lock().unwrap().as_slice(), ["session_progress"]);
    assert_eq!(handle.snapshot().unwrap(), initial);

    let prompting = handle.prompt("hello").unwrap();
    assert_eq!(handle.snapshot().unwrap(), initial);
    let prompt_request = requests.find_command("prompt").expect("missing prompt request");
    let updated = session_snapshot_with("session-1", 2, SessionPhase::Turn, ThinkingLevel::Off, true);
    server.send(success_response(
        prompt_request.id(),
        CommandResult::Prompt(updated.clone()),
    ));
    let prompted = prompting.await.unwrap();
    assert_eq!(prompted, updated);
    assert_eq!(handle.snapshot().unwrap(), updated);
    assert_eq!(observed.lock().unwrap().as_slice(), [2]);

    unsubscribe();
    unsubscribe_events();
    server.send(session_snapshot_event(session_snapshot_with(
        "session-1",
        3,
        SessionPhase::Idle,
        ThinkingLevel::Off,
        true,
    )));
    assert_eq!(observed.lock().unwrap().as_slice(), [2]);
}

#[tokio::test]
async fn keeps_session_leases_attached_across_server_metadata_snapshots() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let handle = attach_session(&client, &server, session_snapshot("session-1")).await;

    let metadata = SessionMetadata::new(SessionMetadataInput {
        id: "session-1".into(),
        created_at: 1,
        updated_at: None,
        parent_session_id: None,
        session_name: Some("Named session".into()),
        cwd: None,
    })
    .unwrap();
    server.send(server_snapshot_event(
        pi_protocol::ServerSnapshot::new("server-1".into(), 2, vec![metadata], Vec::new()).unwrap(),
    ));

    assert!(handle.attached());
}

#[tokio::test]
async fn delayed_command_response_does_not_replace_newer_event_snapshot() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    let initial = session_snapshot_with("session-1", 1, SessionPhase::Idle, ThinkingLevel::Off, true);
    let handle = attach_session(&client, &server, initial).await;
    let requests = collect_requests(&server);
    let changing = handle.set_thinking(ThinkingLevel::High).unwrap();
    let request = requests
        .find_command("set_thinking")
        .expect("missing set_thinking request");
    server.send(session_snapshot_event(session_snapshot_with(
        "session-1",
        3,
        SessionPhase::Idle,
        ThinkingLevel::High,
        true,
    )));
    server.send(success_response(
        request.id(),
        CommandResult::SetThinking(session_snapshot_with(
            "session-1",
            2,
            SessionPhase::Idle,
            ThinkingLevel::Medium,
            true,
        )),
    ));

    changing.await.unwrap();
    let snapshot = handle.snapshot().unwrap();
    assert_eq!(snapshot.revision(), 3);
    assert_eq!(snapshot.thinking_level(), ThinkingLevel::High);
}

#[tokio::test]
async fn attach_response_does_not_replace_newer_snapshot_from_reacquired_runtime() {
    let server = MemoryByteServer::new();
    let client = connect_client(&server).await;
    server.send(session_snapshot_event(session_snapshot_with(
        "session-1",
        10,
        SessionPhase::Idle,
        ThinkingLevel::Off,
        false,
    )));
    let responder = server.clone();
    server.on_message(Arc::new(move |message| {
        let ClientMessage::Request(request) = message else {
            return;
        };
        if let pi_protocol::Command::Attach(_) = request.request() {
            responder.send(session_snapshot_event(session_snapshot_with(
                "session-1",
                3,
                SessionPhase::Idle,
                ThinkingLevel::High,
                true,
            )));
            responder.send(success_response(
                request.id(),
                CommandResult::Attach(session_snapshot_with(
                    "session-1",
                    2,
                    SessionPhase::Idle,
                    ThinkingLevel::Medium,
                    true,
                )),
            ));
        }
    }));

    let requests = collect_requests(&server);
    let handle = client.attach_session("session-1".to_string()).unwrap().await.unwrap();
    assert_eq!(requests.commands(), ["attach"]);
    let snapshot = handle.snapshot().unwrap();
    assert_eq!(snapshot.revision(), 3);
    assert_eq!(snapshot.thinking_level(), ThinkingLevel::High);
}
