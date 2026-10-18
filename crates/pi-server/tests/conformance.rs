mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_protocol::{
    CommandResult, ModelMetadata, PROTOCOL_VERSION, ResponseEnvelope, ServerMessage, SessionMetadata, SessionSnapshot,
    TranscriptProgress, encode_client_message, encode_frame,
};
use pi_server::testing::{Deferred, ProtocolTestClient, TestServerService};
use pi_server::types::BoxFuture;
use pi_server::{InternalServerError, PiServerError, PiServerService, PiSessionRuntime, ServiceError};
use support::{ServerFixture, attach_command, detach_command, list_command, set_thinking_command, success_result};

async fn attach(client: &ProtocolTestClient, session_id: &str) -> SessionSnapshot {
    let response = client
        .request(&attach_command(session_id), &format!("attach-{session_id}"))
        .await
        .expect("attach response must arrive");
    match success_result(&response) {
        CommandResult::Attach(snapshot) => snapshot.clone(),
        other => panic!("expected attach result, got {other:?}"),
    }
}

fn error_messages(errors: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    errors.lock().expect("errors mutex poisoned").clone()
}

struct RacingService {
    inner: TestServerService,
    entered: Deferred<()>,
    release: Deferred<()>,
    race: Mutex<bool>,
}

impl RacingService {
    fn new() -> Self {
        Self {
            inner: TestServerService::new(),
            entered: Deferred::new(),
            release: Deferred::new(),
            race: Mutex::new(false),
        }
    }

    fn set_race(&self, race: bool) {
        *self.race.lock().expect("race mutex poisoned") = race;
    }
}

impl PiServerService for RacingService {
    fn list_sessions(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<SessionMetadata>>> {
        Box::pin(async move {
            let sessions = self.inner.list_sessions().await?;
            if !*self.race.lock().expect("race mutex poisoned") {
                return Ok(sessions);
            }
            self.entered.resolve(());
            self.release.wait().await;
            Ok(sessions)
        })
    }

    fn list_models(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<ModelMetadata>>> {
        self.inner.list_models()
    }

    fn create_session(
        &self,
        options: pi_server::CreateSessionOptions,
    ) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.create_session(options)
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.open_session(session_id)
    }
}

struct FailingService {
    inner: TestServerService,
    list_count: AtomicUsize,
}

impl PiServerService for FailingService {
    fn list_sessions(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<SessionMetadata>>> {
        Box::pin(async move {
            let count = self.list_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count > 1 {
                return Err(Box::new(std::io::Error::other("private service detail")) as ServiceError);
            }
            self.inner.list_sessions().await
        })
    }

    fn list_models(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<ModelMetadata>>> {
        self.inner.list_models()
    }

    fn create_session(
        &self,
        options: pi_server::CreateSessionOptions,
    ) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.create_session(options)
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.open_session(session_id)
    }
}

struct IncompleteService {
    inner: TestServerService,
    list_count: AtomicUsize,
}

impl PiServerService for IncompleteService {
    fn list_sessions(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<SessionMetadata>>> {
        Box::pin(async move {
            let count = self.list_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count > 1 {
                return Err(Box::new(PiServerError::not_implemented()) as ServiceError);
            }
            self.inner.list_sessions().await
        })
    }

    fn list_models(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<ModelMetadata>>> {
        self.inner.list_models()
    }

    fn create_session(
        &self,
        options: pi_server::CreateSessionOptions,
    ) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.create_session(options)
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.open_session(session_id)
    }
}

struct WrappedFailureService {
    inner: TestServerService,
    list_count: AtomicUsize,
}

impl PiServerService for WrappedFailureService {
    fn list_sessions(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<SessionMetadata>>> {
        Box::pin(async move {
            let count = self.list_count.fetch_add(1, Ordering::SeqCst) + 1;
            if count > 1 {
                return Err(Box::new(InternalServerError::new(Box::new(std::io::Error::other(
                    "private storage detail",
                )))) as ServiceError);
            }
            self.inner.list_sessions().await
        })
    }

    fn list_models(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<ModelMetadata>>> {
        self.inner.list_models()
    }

    fn create_session(
        &self,
        options: pi_server::CreateSessionOptions,
    ) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.create_session(options)
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.open_session(session_id)
    }
}

#[tokio::test]
async fn accepts_a_transport_fragmented_framed_cbor_hello() {
    let service = Arc::new(TestServerService::new());
    let fixture = ServerFixture::start(service).await;
    let client = fixture.connect().await;
    let response = client.next(|message| matches!(message, ServerMessage::Hello(_)));
    client
        .send_fragmented_message(
            &pi_protocol::ClientMessage::Hello(pi_protocol::ClientHello::new(PROTOCOL_VERSION)),
            2,
        )
        .await
        .expect("fragmented hello must send");
    match response.await.expect("hello must arrive") {
        ServerMessage::Hello(hello) => assert_eq!(hello.snapshot().protocol_version(), PROTOCOL_VERSION),
        other => panic!("expected hello, got {other:?}"),
    }
    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn enforces_version_and_exactly_one_first_message_hello() {
    let service = Arc::new(TestServerService::new());
    let fixture = ServerFixture::start(service).await;

    let bad_version = fixture.connect().await;
    match bad_version
        .hello(PROTOCOL_VERSION + 1)
        .await
        .expect("hello error must arrive")
    {
        ServerMessage::HelloError(error) => {
            assert_eq!(error.error().code(), pi_protocol::ProtocolErrorCode::Version);
        }
        other => panic!("expected hello_error, got {other:?}"),
    }
    bad_version.wait_for_close().await;

    let request_first = fixture.connect().await;
    let first_error = request_first.next(|message| matches!(message, ServerMessage::HelloError(_)));
    request_first
        .send_message(&pi_protocol::ClientMessage::Request(
            pi_protocol::RequestEnvelope::new("too-early".into(), list_command()).expect("request must build"),
        ))
        .await
        .expect("request must send");
    match first_error.await.expect("hello error must arrive") {
        ServerMessage::HelloError(error) => {
            assert_eq!(error.error().code(), pi_protocol::ProtocolErrorCode::InvalidRequest);
        }
        other => panic!("expected hello_error, got {other:?}"),
    }
    request_first.wait_for_close().await;

    let duplicate = fixture.connect().await;
    assert!(matches!(
        duplicate.hello_default().await.expect("hello must succeed"),
        ServerMessage::Hello(_)
    ));
    let duplicate_error = duplicate.next(|message| matches!(message, ServerMessage::HelloError(_)));
    duplicate
        .send_message(&pi_protocol::ClientMessage::Hello(pi_protocol::ClientHello::new(
            PROTOCOL_VERSION,
        )))
        .await
        .expect("duplicate hello must send");
    match duplicate_error.await.expect("hello error must arrive") {
        ServerMessage::HelloError(error) => {
            assert_eq!(error.error().code(), pi_protocol::ProtocolErrorCode::InvalidRequest);
        }
        other => panic!("expected hello_error, got {other:?}"),
    }
    duplicate.wait_for_close().await;

    fixture.close().await;
}

#[tokio::test]
async fn closes_connections_that_do_not_complete_hello_before_the_timeout() {
    let service = Arc::new(TestServerService::new());
    let fixture = ServerFixture::start_with(service, |options| {
        options.handshake_timeout_ms = Some(20);
    })
    .await;
    let client = fixture.connect().await;
    client.wait_for_close().await;
    assert!(client.messages().iter().any(|message| matches!(
        message,
        ServerMessage::HelloError(error)
            if error.error().code() == pi_protocol::ProtocolErrorCode::InvalidRequest
    )));
    fixture.close().await;
}

#[tokio::test]
async fn keeps_the_handshake_timeout_active_until_the_server_hello_is_sent() {
    let service = Arc::new(TestServerService::new());
    let delay = service.delay_next_list();
    let fixture = ServerFixture::start_with(service, |options| {
        options.handshake_timeout_ms = Some(20);
    })
    .await;
    let client = fixture.connect().await;
    client
        .send_message(&pi_protocol::ClientMessage::Hello(pi_protocol::ClientHello::new(
            PROTOCOL_VERSION,
        )))
        .await
        .expect("hello must send");
    delay.entered.wait().await;
    client.wait_for_close().await;
    delay.release.resolve(());
    assert!(client.messages().iter().any(|message| matches!(
        message,
        ServerMessage::HelloError(error)
            if error.error().code() == pi_protocol::ProtocolErrorCode::InvalidRequest
    )));
    fixture.close().await;
}

#[tokio::test]
async fn bounds_and_closes_malformed_or_oversized_frames() {
    let malformed_fixture = ServerFixture::start(Arc::new(TestServerService::new())).await;
    let malformed = malformed_fixture.connect().await;
    let malformed_error = malformed.next(|message| matches!(message, ServerMessage::HelloError(_)));
    malformed
        .send_bytes(&encode_frame(&[0xff]).expect("frame must encode"))
        .await
        .expect("bytes must send");
    match malformed_error.await.expect("hello error must arrive") {
        ServerMessage::HelloError(error) => {
            assert_eq!(error.error().code(), pi_protocol::ProtocolErrorCode::InvalidRequest);
        }
        other => panic!("expected hello_error, got {other:?}"),
    }
    malformed.wait_for_close().await;
    malformed_fixture.close().await;

    let oversized_fixture = ServerFixture::start_with(Arc::new(TestServerService::new()), |options| {
        options.max_frame_length = Some(128);
    })
    .await;
    let oversized = oversized_fixture.connect().await;
    let mut frame = vec![0_u8; 4 + 129];
    frame[3] = 129;
    oversized.send_bytes(&frame).await.expect("frame must send");
    oversized.wait_for_close().await;
    assert!(
        !oversized
            .messages()
            .iter()
            .any(|message| matches!(message, ServerMessage::Hello(_)))
    );
    oversized_fixture.close().await;

    let outbound_fixture = ServerFixture::start_with(Arc::new(TestServerService::new()), |options| {
        options.max_frame_length = Some(128);
    })
    .await;
    let outbound = outbound_fixture.connect().await;
    outbound
        .send_message(&pi_protocol::ClientMessage::Hello(pi_protocol::ClientHello::new(
            PROTOCOL_VERSION,
        )))
        .await
        .expect("hello must send");
    outbound.wait_for_close().await;
    assert!(outbound.messages().is_empty());
    outbound_fixture.close().await;
}

#[tokio::test]
async fn catches_up_a_handshaking_client_after_a_concurrent_server_change() {
    let service = Arc::new(RacingService::new());
    service.inner.seed("shared");
    let fixture = ServerFixture::start(service.clone()).await;
    let controller = fixture.connect().await;
    controller.hello_default().await.expect("hello must succeed");
    service.set_race(true);

    let joining = Arc::new(fixture.connect().await);
    let hello = {
        let joining = joining.clone();
        tokio::spawn(async move { joining.hello_default().await })
    };
    service.entered.wait().await;
    controller
        .request(&attach_command("shared"), "controller-attach")
        .await
        .expect("attach must respond");
    service.release.resolve(());
    let handshake = hello
        .await
        .expect("hello task must join")
        .expect("joining hello must arrive");
    let ServerMessage::Hello(handshake) = handshake else {
        panic!("expected server hello");
    };
    let revision = handshake.snapshot().revision();
    let catchup = joining
        .next(|message| match message {
            ServerMessage::Event(envelope) => match envelope.event() {
                pi_protocol::ServerEvent::ServerSnapshot(snapshot) => snapshot.revision() > revision,
                _ => false,
            },
            _ => false,
        })
        .await
        .expect("catch-up event must arrive");
    match catchup {
        ServerMessage::Event(envelope) => match envelope.event() {
            pi_protocol::ServerEvent::ServerSnapshot(snapshot) => {
                assert!(snapshot.sessions().iter().any(|session| session.id() == "shared"));
                assert_eq!(snapshot.sessions()[0].session_name(), Some("Session shared"));
            }
            other => panic!("expected server snapshot, got {other:?}"),
        },
        other => panic!("expected event, got {other:?}"),
    }

    controller.close().await;
    joining.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn shares_request_event_attachment_and_disconnect_behavior() {
    let service = Arc::new(TestServerService::new());
    service.seed("first");
    service.seed("second");
    let fixture = ServerFixture::start(service.clone()).await;
    let client = fixture.connect().await;
    let hello = client.hello_default().await.expect("hello must succeed");
    let ServerMessage::Hello(hello) = hello else {
        panic!("expected hello");
    };
    assert_eq!(
        hello
            .snapshot()
            .sessions()
            .iter()
            .map(|session| session.id())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );

    let listed = client
        .request(&list_command(), "list")
        .await
        .expect("list must respond");
    match success_result(&listed) {
        CommandResult::List(result) => {
            assert_eq!(
                result.sessions().iter().map(|session| session.id()).collect::<Vec<_>>(),
                vec!["first", "second"]
            );
        }
        other => panic!("expected list result, got {other:?}"),
    }

    assert_eq!(attach(&client, "first").await.id(), "first");
    assert_eq!(attach(&client, "second").await.id(), "second");

    let progress = TranscriptProgress::assistant_delta(
        "assistant-1".into(),
        0,
        pi_protocol::TranscriptDeltaKind::Text,
        "hello".into(),
    )
    .expect("progress must build");
    let progress_event = client.next(|message| {
        matches!(
            message,
            ServerMessage::Event(envelope)
                if matches!(envelope.event(), pi_protocol::ServerEvent::SessionProgress(_))
        )
    });
    service.latest_runtime("first").emit_progress(progress.clone());
    match progress_event.await.expect("progress must arrive") {
        ServerMessage::Event(envelope) => match envelope.event() {
            pi_protocol::ServerEvent::SessionProgress(event) => {
                assert_eq!(event.session_id(), "first");
                assert_eq!(event.progress(), &progress);
            }
            other => panic!("expected session progress, got {other:?}"),
        },
        other => panic!("expected event, got {other:?}"),
    }

    let detached = client
        .request(&detach_command("first"), "detach")
        .await
        .expect("detach must respond");
    assert!(matches!(success_result(&detached), CommandResult::Detach(_)));
    assert_eq!(service.latest_runtime("first").dispose_count(), 1);

    let thinking = client
        .request(
            &set_thinking_command("second", pi_protocol::ThinkingLevel::High),
            "thinking",
        )
        .await
        .expect("set thinking must respond");
    match success_result(&thinking) {
        CommandResult::SetThinking(snapshot) => {
            assert_eq!(snapshot.id(), "second");
            assert_eq!(snapshot.thinking_level(), pi_protocol::ThinkingLevel::High);
        }
        other => panic!("expected set_thinking result, got {other:?}"),
    }

    let second_runtime = service.latest_runtime("second");
    client.close().await;
    second_runtime.disposed().await;
    assert_eq!(second_runtime.dispose_count(), 1);
    fixture.close().await;
}

#[tokio::test]
async fn disconnects_attached_clients_when_a_runtime_reports_a_terminal_error() {
    let service = Arc::new(TestServerService::new());
    service.seed("terminal");
    let errors = Arc::new(Mutex::new(Vec::new()));
    let observer = {
        let errors = errors.clone();
        Arc::new(move |error: &(dyn std::error::Error + Send + Sync + 'static)| {
            errors.lock().expect("errors mutex poisoned").push(error.to_string());
        })
    };
    let fixture = ServerFixture::start_with(service.clone(), |options| {
        options.on_error = Some(observer);
    })
    .await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    attach(&client, "terminal").await;
    let runtime = service.latest_runtime("terminal");

    runtime.set_phase(pi_protocol::SessionPhase::Turn);
    runtime.emit_error(PiServerError::session_locked("lock ownership lost", None));
    client.wait_for_close().await;
    runtime.disposed().await;
    assert_eq!(runtime.dispose_count(), 1);
    assert!(!service.is_locked("terminal"));
    assert!(
        error_messages(&errors)
            .iter()
            .any(|message| message.contains("lock ownership lost"))
    );

    let next_client = fixture.connect().await;
    next_client.hello_default().await.expect("hello must succeed");
    let snapshot = attach(&next_client, "terminal").await;
    assert_eq!(snapshot.id(), "terminal");
    assert!(!Arc::ptr_eq(&service.latest_runtime("terminal"), &runtime));

    next_client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn does_not_expose_unexpected_service_errors_to_clients() {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let observer = {
        let errors = errors.clone();
        Arc::new(move |error: &(dyn std::error::Error + Send + Sync + 'static)| {
            errors.lock().expect("errors mutex poisoned").push(error.to_string());
            panic!("observer failure");
        })
    };
    let service = Arc::new(FailingService {
        inner: TestServerService::new(),
        list_count: AtomicUsize::new(0),
    });
    let fixture = ServerFixture::start_with(service, |options| {
        options.on_error = Some(observer);
    })
    .await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    let response = client
        .request(&list_command(), "list")
        .await
        .expect("list must respond");
    match &response {
        ResponseEnvelope::Error(error) => {
            assert_eq!(error.error().code(), pi_protocol::ProtocolErrorCode::InternalError);
            assert_eq!(error.error().message(), "Internal server error");
        }
        ResponseEnvelope::Success(_) => panic!("expected internal error"),
    }
    assert!(
        error_messages(&errors)
            .iter()
            .any(|message| message.contains("private service detail"))
    );
    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn keeps_not_implemented_stable() {
    let service = Arc::new(IncompleteService {
        inner: TestServerService::new(),
        list_count: AtomicUsize::new(0),
    });
    let fixture = ServerFixture::start(service).await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    let response = client
        .request(&list_command(), "list")
        .await
        .expect("list must respond");
    match &response {
        ResponseEnvelope::Error(error) => {
            assert_eq!(error.error().code(), pi_protocol::ProtocolErrorCode::NotImplemented);
            assert_eq!(error.error().message(), "Operation is not implemented");
        }
        ResponseEnvelope::Success(_) => panic!("expected not_implemented error"),
    }
    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn reports_wrapped_internal_causes_without_exposing_them() {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let observer = {
        let errors = errors.clone();
        Arc::new(move |error: &(dyn std::error::Error + Send + Sync + 'static)| {
            errors.lock().expect("errors mutex poisoned").push(error.to_string());
        })
    };
    let service = Arc::new(WrappedFailureService {
        inner: TestServerService::new(),
        list_count: AtomicUsize::new(0),
    });
    let fixture = ServerFixture::start_with(service, |options| {
        options.on_error = Some(observer);
    })
    .await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    let response = client
        .request(&list_command(), "list")
        .await
        .expect("list must respond");
    match &response {
        ResponseEnvelope::Error(error) => {
            assert_eq!(error.error().code(), pi_protocol::ProtocolErrorCode::InternalError);
            assert_eq!(error.error().message(), "Internal server error");
        }
        ResponseEnvelope::Success(_) => panic!("expected internal error"),
    }
    assert!(!format!("{response:?}").contains("private"));
    let messages = error_messages(&errors);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("private storage detail"))
    );
    assert!(!messages.iter().any(|message| message.contains("InternalServerError")));
    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn can_respond_out_of_request_order_after_the_handshake() {
    let service = Arc::new(TestServerService::new());
    service.seed("first");
    let fixture = ServerFixture::start(service.clone()).await;
    let client = Arc::new(fixture.connect().await);
    client.hello_default().await.expect("hello must succeed");

    let delay = service.delay_next_list();
    let slow = {
        let client = client.clone();
        tokio::spawn(async move { client.request(&list_command(), "slow").await })
    };
    delay.entered.wait().await;
    let fast = client
        .request(&attach_command("first"), "fast")
        .await
        .expect("fast request must respond");
    assert!(matches!(success_result(&fast), CommandResult::Attach(_)));
    assert!(!client
        .messages()
        .iter()
        .any(|message| matches!(message, ServerMessage::Response(ResponseEnvelope::Success(response)) if response.id() == "slow")));

    delay.release.resolve(());
    let slow = slow
        .await
        .expect("slow task must join")
        .expect("slow request must respond");
    assert!(matches!(success_result(&slow), CommandResult::List(_)));
    let response_ids: Vec<String> = client
        .messages()
        .iter()
        .filter_map(|message| match message {
            ServerMessage::Response(ResponseEnvelope::Success(response))
                if response.id() == "slow" || response.id() == "fast" =>
            {
                Some(response.id().to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(response_ids, vec!["fast".to_string(), "slow".to_string()]);
    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn gracefully_closes_connections_sessions_and_listener_resources() {
    let service = Arc::new(TestServerService::new());
    service.seed("first");
    let fixture = ServerFixture::start(service.clone()).await;
    let socket_path = fixture.server.addresses()[0].clone();
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    attach(&client, "first").await;
    let runtime = service.latest_runtime("first");

    fixture.server.close().await.expect("server must close");
    client.wait_for_close().await;
    assert_eq!(runtime.dispose_count(), 1);
    assert!(fixture.server.addresses().is_empty());
    assert!(std::fs::symlink_metadata(&socket_path).is_err());
    fixture.server.close().await.expect("second close must succeed");
}

#[tokio::test]
async fn decodes_multiple_framed_requests_from_one_raw_chunk() {
    let service = Arc::new(TestServerService::new());
    let fixture = ServerFixture::start(service).await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    let first = encode_client_message(
        &pi_protocol::ClientMessage::Request(
            pi_protocol::RequestEnvelope::new("first".into(), list_command()).expect("request must build"),
        ),
        None,
    )
    .expect("first request must encode");
    let second = encode_client_message(
        &pi_protocol::ClientMessage::Request(
            pi_protocol::RequestEnvelope::new("second".into(), list_command()).expect("request must build"),
        ),
        None,
    )
    .expect("second request must encode");
    let mut combined = first.clone();
    combined.extend_from_slice(&second);
    let first_response = client.next(|message| {
        matches!(message, ServerMessage::Response(ResponseEnvelope::Success(response)) if response.id() == "first")
    });
    let second_response = client.next(|message| {
        matches!(message, ServerMessage::Response(ResponseEnvelope::Success(response)) if response.id() == "second")
    });
    client.send_bytes(&combined).await.expect("combined frames must send");
    assert!(matches!(
        first_response.await.expect("first response must arrive"),
        ServerMessage::Response(ResponseEnvelope::Success(_))
    ));
    assert!(matches!(
        second_response.await.expect("second response must arrive"),
        ServerMessage::Response(ResponseEnvelope::Success(_))
    ));
    client.close().await;
    fixture.close().await;
}
