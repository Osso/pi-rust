//! Behavioral port of `test/client/remote-session.test.ts` over an in-memory protocol server.

#[path = "support/remote_session_memory.rs"]
mod memory;

use std::sync::{Arc, Mutex};

use memory::{MemoryServer, SnapshotOverrides, collect_requests, connect_client, session_snapshot, wait_for_request};
use pi_coding_agent::client::remote_session::{
    RemoteSession, RemoteSessionLifecycle, RemoteSessionOperation, RemoteSessionOptions,
};
use pi_protocol::{
    AssistantContent, AssistantTranscriptItem, AssistantTranscriptItemInput, AssistantTranscriptState, Command,
    CommandResult, ModelRef, ServerEvent, SessionPhase, SessionProgress, TextContent, ThinkingLevel,
    TranscriptDeltaKind, TranscriptItem, TranscriptProgress,
};

fn model() -> ModelRef {
    ModelRef::new("faux".to_owned(), "model".to_owned()).unwrap()
}

fn streaming_assistant(text: &str) -> TranscriptItem {
    TranscriptItem::Assistant(
        AssistantTranscriptItem::new(AssistantTranscriptItemInput {
            id: "assistant-1".to_owned(),
            content: vec![AssistantContent::Text(TextContent::new(text.to_owned()))],
            model: model(),
            response_model: None,
            usage: None,
            timestamp: 1,
            state: AssistantTranscriptState::Streaming,
        })
        .unwrap(),
    )
}

async fn open_session(
    server: &Arc<MemoryServer>,
    snapshot: pi_protocol::SessionSnapshot,
) -> (
    Arc<pi_client::PiClient>,
    Arc<RemoteSession>,
    Arc<Mutex<Vec<memory::CollectedRequest>>>,
) {
    let client = Arc::new(connect_client(server).await);
    let requests = collect_requests(server);
    let opening = {
        let client = client.clone();
        tokio::spawn(async move { RemoteSession::open(client, "session-1", RemoteSessionOptions::default()).await })
    };
    let request = wait_for_request(&requests, "attach").await;
    server.send_result(&request.id, CommandResult::Attach(snapshot));
    let session = Arc::new(opening.await.unwrap().unwrap());
    (client, session, requests)
}

#[tokio::test]
async fn projects_progress_for_subscribers_without_changing_the_authoritative_snapshot() {
    let server = MemoryServer::new();
    let snapshot = session_snapshot(
        "session-1",
        SnapshotOverrides {
            phase: Some(SessionPhase::Turn),
            transcript: Some(vec![streaming_assistant("hello")]),
            ..Default::default()
        },
    );
    let (_client, session, _requests) = open_session(&server, snapshot).await;
    let views = Arc::new(Mutex::new(Vec::new()));
    let recorded = views.clone();
    let _unsubscribe = session
        .subscribe(Arc::new(move |state| {
            if let Some(TranscriptItem::Assistant(item)) = state.transcript.first()
                && let Some(AssistantContent::Text(text)) = item.content().first()
            {
                recorded.lock().unwrap().push(text.text().to_owned());
            }
        }))
        .unwrap();

    server.send_event(ServerEvent::SessionProgress(
        SessionProgress::new(
            "session-1".to_owned(),
            TranscriptProgress::AssistantDelta {
                message_id: "assistant-1".to_owned(),
                content_index: 0,
                kind: TranscriptDeltaKind::Text,
                delta: " world".to_owned(),
            },
        )
        .unwrap(),
    ));

    assert_eq!(*views.lock().unwrap(), ["hello", "hello world"]);
    let snapshot = session.snapshot().unwrap();
    let TranscriptItem::Assistant(item) = &snapshot.transcript()[0] else {
        panic!("expected assistant item");
    };
    let AssistantContent::Text(text) = &item.content()[0] else {
        panic!("expected text");
    };
    assert_eq!(text.text(), "hello");
}

#[tokio::test]
async fn becomes_unbound_and_can_reopen_after_its_session_is_removed() {
    let server = MemoryServer::new();
    let (_client, session, _requests) =
        open_session(&server, session_snapshot("session-1", SnapshotOverrides::default())).await;

    server.send_event(ServerEvent::SessionRemoved(
        pi_protocol::SessionRemoved::new("session-1".to_owned()).unwrap(),
    ));

    assert_eq!(session.id(), None);
    assert!(session.snapshot().is_none());
    assert!(session.state().transcript.is_empty());
    assert_eq!(session.state().lifecycle, RemoteSessionLifecycle::Unbound);

    let requests = collect_requests(&server);
    let reopening = {
        let session = session.clone();
        tokio::spawn(async move { session.open_session("session-1").await })
    };
    let request = wait_for_request(&requests, "attach").await;
    server.send_result(
        &request.id,
        CommandResult::Attach(session_snapshot(
            "session-1",
            SnapshotOverrides {
                revision: Some(2),
                ..Default::default()
            },
        )),
    );
    reopening.await.unwrap().unwrap();
    assert_eq!(session.state().lifecycle, RemoteSessionLifecycle::Ready);
}

#[tokio::test]
async fn exposes_the_active_operation_while_prompting() {
    let server = MemoryServer::new();
    let (_client, session, requests) =
        open_session(&server, session_snapshot("session-1", SnapshotOverrides::default())).await;
    let lifecycles = Arc::new(Mutex::new(Vec::new()));
    let recorded = lifecycles.clone();
    let _unsubscribe = session
        .subscribe(Arc::new(move |state| {
            recorded.lock().unwrap().push(state.lifecycle);
        }))
        .unwrap();

    let prompting = {
        let session = session.clone();
        tokio::spawn(async move { session.submit("  first prompt  ").await })
    };
    let request = wait_for_request(&requests, "prompt").await;
    let Command::Prompt(prompt) = &request.command else {
        panic!("expected prompt command");
    };
    assert_eq!(prompt.text(), "first prompt");
    assert_eq!(session.operation(), Some(RemoteSessionOperation::Submit));
    server.send_result(
        &request.id,
        CommandResult::Prompt(session_snapshot(
            "session-1",
            SnapshotOverrides {
                revision: Some(2),
                phase: Some(SessionPhase::Turn),
                ..Default::default()
            },
        )),
    );
    prompting.await.unwrap().unwrap();

    assert_eq!(
        *lifecycles.lock().unwrap(),
        [
            RemoteSessionLifecycle::Ready,
            RemoteSessionLifecycle::Busy(RemoteSessionOperation::Submit),
            RemoteSessionLifecycle::Busy(RemoteSessionOperation::Submit),
            RemoteSessionLifecycle::Ready,
        ]
    );
    assert_eq!(session.state().lifecycle, RemoteSessionLifecycle::Ready);
}

#[tokio::test]
async fn steers_when_the_server_session_is_in_a_turn() {
    let server = MemoryServer::new();
    let (_client, session, requests) = open_session(
        &server,
        session_snapshot(
            "session-1",
            SnapshotOverrides {
                phase: Some(SessionPhase::Turn),
                ..Default::default()
            },
        ),
    )
    .await;

    let steering = {
        let session = session.clone();
        tokio::spawn(async move { session.submit("adjust").await })
    };
    let request = wait_for_request(&requests, "steer").await;
    let Command::Steer(steer) = &request.command else {
        panic!("expected steer command");
    };
    assert_eq!(steer.text(), "adjust");
    server.send_result(
        &request.id,
        CommandResult::Steer(session_snapshot(
            "session-1",
            SnapshotOverrides {
                revision: Some(2),
                phase: Some(SessionPhase::Turn),
                ..Default::default()
            },
        )),
    );
    steering.await.unwrap().unwrap();
}

#[tokio::test]
async fn aborts_while_a_prompt_response_is_pending() {
    let server = MemoryServer::new();
    let (_client, session, requests) =
        open_session(&server, session_snapshot("session-1", SnapshotOverrides::default())).await;

    let prompting = {
        let session = session.clone();
        tokio::spawn(async move { session.submit("hello").await })
    };
    let prompt_request = wait_for_request(&requests, "prompt").await;
    server.send_event(ServerEvent::SessionSnapshot(session_snapshot(
        "session-1",
        SnapshotOverrides {
            revision: Some(2),
            phase: Some(SessionPhase::Turn),
            ..Default::default()
        },
    )));

    let aborting = {
        let session = session.clone();
        tokio::spawn(async move { session.abort().await })
    };
    let abort_request = wait_for_request(&requests, "abort").await;
    assert_eq!(session.operation(), Some(RemoteSessionOperation::Abort));
    server.send_result(
        &prompt_request.id,
        CommandResult::Prompt(session_snapshot(
            "session-1",
            SnapshotOverrides {
                revision: Some(3),
                phase: Some(SessionPhase::Turn),
                ..Default::default()
            },
        )),
    );
    prompting.await.unwrap().unwrap();
    assert_eq!(session.operation(), Some(RemoteSessionOperation::Abort));
    server.send_result(
        &abort_request.id,
        CommandResult::Abort(session_snapshot(
            "session-1",
            SnapshotOverrides {
                revision: Some(4),
                ..Default::default()
            },
        )),
    );
    aborting.await.unwrap().unwrap();
    assert_eq!(session.state().lifecycle, RemoteSessionLifecycle::Ready);
}

#[tokio::test]
async fn rejects_conflicting_operations_while_locally_busy() {
    let server = MemoryServer::new();
    let (_client, session, requests) =
        open_session(&server, session_snapshot("session-1", SnapshotOverrides::default())).await;

    let prompting = {
        let session = session.clone();
        tokio::spawn(async move { session.submit("hello").await })
    };
    let request = wait_for_request(&requests, "prompt").await;
    assert_eq!(
        session.set_thinking(ThinkingLevel::High).await.unwrap_err().to_string(),
        "Remote session is busy with submit"
    );
    assert_eq!(
        session.open_session("session-2").await.unwrap_err().to_string(),
        "Remote session is busy with submit"
    );
    server.send_result(
        &request.id,
        CommandResult::Prompt(session_snapshot(
            "session-1",
            SnapshotOverrides {
                revision: Some(2),
                phase: Some(SessionPhase::Turn),
                ..Default::default()
            },
        )),
    );
    prompting.await.unwrap().unwrap();
}

#[tokio::test]
async fn reports_subscriber_failures_without_interrupting_other_subscribers() {
    let server = MemoryServer::new();
    let listener_errors = Arc::new(Mutex::new(Vec::new()));
    let recorded = listener_errors.clone();
    let client = Arc::new(connect_client(&server).await);
    let requests = collect_requests(&server);
    let opening = {
        let client = client.clone();
        tokio::spawn(async move {
            RemoteSession::open(
                client,
                "session-1",
                RemoteSessionOptions {
                    on_listener_error: Some(Arc::new(move |message| {
                        recorded.lock().unwrap().push(message);
                    })),
                },
            )
            .await
        })
    };
    let request = wait_for_request(&requests, "attach").await;
    server.send_result(
        &request.id,
        CommandResult::Attach(session_snapshot("session-1", SnapshotOverrides::default())),
    );
    let session = opening.await.unwrap().unwrap();
    let _unsubscribe = session.subscribe(Arc::new(|_| panic!("render failed"))).unwrap();
    let notified = Arc::new(Mutex::new(false));
    let recorded_notified = notified.clone();
    let _unsubscribe = session
        .subscribe(Arc::new(move |_| {
            *recorded_notified.lock().unwrap() = true;
        }))
        .unwrap();

    assert_eq!(*listener_errors.lock().unwrap(), ["render failed"]);
    assert!(*notified.lock().unwrap());
}

#[tokio::test]
async fn dispose_is_idempotent_and_marks_the_session_disposed() {
    let server = MemoryServer::new();
    let (_client, session, _requests) =
        open_session(&server, session_snapshot("session-1", SnapshotOverrides::default())).await;

    let disposing = {
        let session = session.clone();
        tokio::spawn(async move { session.dispose().await })
    };
    let detach = wait_for_request(&_requests, "detach").await;
    server.send_result(
        &detach.id,
        CommandResult::Detach(pi_protocol::DetachResult::new("session-1".to_owned()).unwrap()),
    );
    disposing.await.unwrap().unwrap();
    assert!(session.disposed());
    assert_eq!(session.state().lifecycle, RemoteSessionLifecycle::Disposed);
    assert_eq!(session.id(), None);
    assert_eq!(
        session.submit("hello").await.unwrap_err().to_string(),
        "Remote session is disposed"
    );
    session.dispose().await.unwrap();
}
