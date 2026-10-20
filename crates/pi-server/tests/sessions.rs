mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use pi_protocol::{
    AbortCommand, Command, CommandResult, CreateCommand, CreateCommandInput, ModelMetadata, PromptCommand,
    ResponseEnvelope, ServerMessage, SessionMetadata, SessionSnapshot, SetModelCommand, SteerCommand,
    TranscriptProgress,
};
use pi_server::testing::{Deferred, ProtocolTestClient, TEST_MODEL, TestServerService};
use pi_server::types::BoxFuture;
use pi_server::{CreateSessionOptions, PiServerService, PiSessionRuntime};
use support::{ServerFixture, attach_command, detach_command, list_command, set_thinking_command, success_result};

fn create_command(cwd: Option<&str>, name: Option<&str>) -> Command {
    Command::Create(
        CreateCommand::new(CreateCommandInput {
            cwd: cwd.map(str::to_string),
            name: name.map(str::to_string),
            model: None,
            thinking_level: None,
        })
        .expect("create command must be valid"),
    )
}

fn prompt_command(session_id: &str, text: &str) -> Command {
    Command::Prompt(PromptCommand::new(session_id.to_string(), text.to_string()).expect("prompt command must be valid"))
}

fn steer_command(session_id: &str, text: &str) -> Command {
    Command::Steer(SteerCommand::new(session_id.to_string(), text.to_string()).expect("steer command must be valid"))
}

fn abort_command(session_id: &str) -> Command {
    Command::Abort(AbortCommand::new(session_id.to_string()).expect("abort command must be valid"))
}

fn set_model_command(session_id: &str, provider: &str, id: &str) -> Command {
    Command::SetModel(
        SetModelCommand::new(
            session_id.to_string(),
            pi_protocol::ModelRef::new(provider.to_string(), id.to_string()).expect("model reference must be valid"),
        )
        .expect("set model command must be valid"),
    )
}

fn error_code(response: &ResponseEnvelope) -> &str {
    match response {
        ResponseEnvelope::Error(response) => response.error().code().as_str(),
        ResponseEnvelope::Success(_) => panic!("expected error response"),
    }
}

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

struct OrderedSnapshotService {
    inner: TestServerService,
    first_started: Deferred<()>,
    second_started: Deferred<()>,
    first_release: Deferred<()>,
    second_release: Deferred<()>,
    controlled: AtomicBool,
    started_count: AtomicUsize,
}

impl OrderedSnapshotService {
    fn new() -> Self {
        Self {
            inner: TestServerService::new(),
            first_started: Deferred::new(),
            second_started: Deferred::new(),
            first_release: Deferred::new(),
            second_release: Deferred::new(),
            controlled: AtomicBool::new(false),
            started_count: AtomicUsize::new(0),
        }
    }
}

impl PiServerService for OrderedSnapshotService {
    fn list_sessions(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<SessionMetadata>>> {
        self.inner.list_sessions()
    }

    fn list_models(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<ModelMetadata>>> {
        Box::pin(async move {
            if !self.controlled.load(Ordering::SeqCst) {
                return self.inner.list_models().await;
            }
            let count = self.started_count.fetch_add(1, Ordering::SeqCst) + 1;
            match count {
                1 => {
                    self.first_started.resolve(());
                    self.first_release.wait().await;
                }
                2 => {
                    self.second_started.resolve(());
                    self.second_release.wait().await;
                }
                _ => {}
            }
            Ok(vec![TEST_MODEL.clone()])
        })
    }

    fn create_session(
        &self,
        options: CreateSessionOptions,
    ) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.create_session(options)
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.open_session(session_id)
    }
}

struct ExtendedMetadataService {
    inner: TestServerService,
}

impl PiServerService for ExtendedMetadataService {
    fn list_sessions(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<SessionMetadata>>> {
        Box::pin(async move {
            let sessions = self.inner.list_sessions().await?;
            Ok(sessions
                .into_iter()
                .map(|metadata| {
                    SessionMetadata::new(pi_protocol::SessionMetadataInput {
                        id: metadata.id().to_string(),
                        created_at: metadata.created_at(),
                        updated_at: metadata.updated_at(),
                        parent_session_id: Some("parent-1".into()),
                        session_name: Some("stale name".into()),
                        cwd: metadata.cwd().map(str::to_string),
                    })
                    .expect("metadata must remain valid")
                })
                .collect())
        })
    }

    fn list_models(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<ModelMetadata>>> {
        self.inner.list_models()
    }

    fn create_session(
        &self,
        options: CreateSessionOptions,
    ) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.create_session(options)
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.open_session(session_id)
    }
}

struct WrongIdService {
    inner: TestServerService,
}

impl PiServerService for WrongIdService {
    fn list_sessions(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<SessionMetadata>>> {
        self.inner.list_sessions()
    }

    fn list_models(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<ModelMetadata>>> {
        self.inner.list_models()
    }

    fn create_session(
        &self,
        options: CreateSessionOptions,
    ) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        Box::pin(async move {
            self.inner
                .create_session(CreateSessionOptions {
                    id: "wrong-id".into(),
                    ..options
                })
                .await
        })
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.open_session(session_id)
    }
}

struct GatedOpenService {
    inner: TestServerService,
    gated_id: &'static str,
    entered: Deferred<()>,
    release: Deferred<()>,
    gated_open_count: AtomicUsize,
}

impl GatedOpenService {
    fn new(gated_id: &'static str) -> Self {
        Self {
            inner: TestServerService::new(),
            gated_id,
            entered: Deferred::new(),
            release: Deferred::new(),
            gated_open_count: AtomicUsize::new(0),
        }
    }
}

impl PiServerService for GatedOpenService {
    fn list_sessions(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<SessionMetadata>>> {
        self.inner.list_sessions()
    }

    fn list_models(&self) -> BoxFuture<'_, pi_server::ServiceResult<Vec<ModelMetadata>>> {
        self.inner.list_models()
    }

    fn create_session(
        &self,
        options: CreateSessionOptions,
    ) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        self.inner.create_session(options)
    }

    fn open_session(&self, session_id: String) -> BoxFuture<'_, pi_server::ServiceResult<Arc<dyn PiSessionRuntime>>> {
        Box::pin(async move {
            if session_id == self.gated_id {
                self.gated_open_count.fetch_add(1, Ordering::SeqCst);
                self.entered.resolve(());
                self.release.wait().await;
            }
            self.inner.open_session(session_id).await
        })
    }
}

#[tokio::test]
async fn serializes_server_snapshot_revisions() {
    let service = Arc::new(OrderedSnapshotService::new());
    let fixture = ServerFixture::start(service.clone()).await;
    let client = Arc::new(fixture.connect().await);
    client.hello_default().await.expect("hello must succeed");
    service.controlled.store(true, Ordering::SeqCst);
    let message_index = client.message_count();

    let first_create = {
        let client = client.clone();
        tokio::spawn(async move { client.request(&create_command(None, Some("first")), "create-1").await })
    };
    service.first_started.wait().await;
    let second_create = {
        let client = client.clone();
        tokio::spawn(async move { client.request(&create_command(None, Some("second")), "create-2").await })
    };
    tokio::task::yield_now().await;
    assert_eq!(service.started_count.load(Ordering::SeqCst), 1);

    service.first_release.resolve(());
    service.second_started.wait().await;
    service.second_release.resolve(());
    first_create
        .await
        .expect("first create task must join")
        .expect("first create must respond");
    second_create
        .await
        .expect("second create task must join")
        .expect("second create must respond");
    client
        .next_from(message_index, |message| match message {
            ServerMessage::Event(envelope) => match envelope.event() {
                pi_protocol::ServerEvent::ServerSnapshot(snapshot) => snapshot.revision() == 2,
                _ => false,
            },
            _ => false,
        })
        .await
        .expect("revision 2 must be published");

    let revisions: Vec<u64> = client
        .messages()
        .into_iter()
        .skip(message_index)
        .filter_map(|message| match message {
            ServerMessage::Event(envelope) => match envelope.event() {
                pi_protocol::ServerEvent::ServerSnapshot(snapshot) => Some(snapshot.revision()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(revisions, vec![1, 2]);

    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn creates_server_assigned_durable_ids_and_supports_list_attach_and_detach() {
    let service = Arc::new(TestServerService::new());
    let fixture = ServerFixture::start(service.clone()).await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    let created = client
        .request(&create_command(Some("/work"), Some("Created")), "create")
        .await
        .expect("create must respond");
    let created_id = match success_result(&created) {
        CommandResult::Create(snapshot) => {
            assert_eq!(snapshot.cwd(), "/work");
            assert_eq!(snapshot.name(), Some("Created"));
            assert!(snapshot.attached());
            assert!(snapshot.locked());
            snapshot.id().to_string()
        }
        other => panic!("expected create result, got {other:?}"),
    };
    assert_eq!(service.last_created_id().as_deref(), Some(created_id.as_str()));

    let listed = client
        .request(&list_command(), "list")
        .await
        .expect("list must respond");
    match success_result(&listed) {
        CommandResult::List(result) => {
            let metadata = &result.sessions()[0];
            assert_eq!(metadata.id(), created_id);
            assert_eq!(metadata.created_at(), 1);
            assert_eq!(metadata.updated_at(), Some(1));
            assert_eq!(metadata.session_name(), Some("Created"));
            assert_eq!(metadata.cwd(), Some("/work"));
        }
        other => panic!("expected list result, got {other:?}"),
    }

    let detached = client
        .request(&detach_command(&created_id), "detach")
        .await
        .expect("detach must respond");
    assert!(matches!(success_result(&detached), CommandResult::Detach(_)));
    assert_eq!(service.latest_runtime(&created_id).dispose_count(), 1);
    let detached_again = client
        .request(&detach_command(&created_id), "detach-again")
        .await
        .expect("second detach must respond");
    assert!(matches!(success_result(&detached_again), CommandResult::Detach(_)));

    let attached = attach(&client, &created_id).await;
    assert_eq!(attached.id(), created_id);
    assert_eq!(service.runtime_count(&created_id), 2);

    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn preserves_backend_metadata_while_refreshing_live_session_metadata() {
    let service = Arc::new(ExtendedMetadataService {
        inner: TestServerService::new(),
    });
    service.inner.seed_named("session-1", "Live name");
    let fixture = ServerFixture::start(service).await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    attach(&client, "session-1").await;

    let listed = client
        .request(&list_command(), "list")
        .await
        .expect("list must respond");
    match success_result(&listed) {
        CommandResult::List(result) => {
            let metadata = &result.sessions()[0];
            assert_eq!(metadata.id(), "session-1");
            assert_eq!(metadata.created_at(), 1);
            assert_eq!(metadata.updated_at(), Some(1));
            assert_eq!(metadata.parent_session_id(), Some("parent-1"));
            assert_eq!(metadata.session_name(), Some("Live name"));
            assert_eq!(metadata.cwd(), Some("/tmp/pi-server-conformance"));
        }
        other => panic!("expected list result, got {other:?}"),
    }

    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn keeps_multiple_attachments_on_one_connection_independent() {
    let service = Arc::new(TestServerService::new());
    service.seed("first");
    service.seed("second");
    let fixture = ServerFixture::start(service.clone()).await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    attach(&client, "first").await;
    attach(&client, "second").await;

    client
        .request(&detach_command("first"), "detach-first")
        .await
        .expect("detach must respond");
    assert_eq!(service.latest_runtime("first").dispose_count(), 1);
    assert_eq!(service.latest_runtime("second").dispose_count(), 0);
    let response = client
        .request(
            &set_thinking_command("second", pi_protocol::ThinkingLevel::Medium),
            "thinking",
        )
        .await
        .expect("set thinking must respond");
    match success_result(&response) {
        CommandResult::SetThinking(snapshot) => {
            assert_eq!(snapshot.id(), "second");
            assert_eq!(snapshot.thinking_level(), pi_protocol::ThinkingLevel::Medium);
        }
        other => panic!("expected set_thinking result, got {other:?}"),
    }

    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn broadcasts_full_snapshots_and_progress_only_to_attached_clients() {
    let service = Arc::new(TestServerService::new());
    service.seed("session-1");
    let fixture = ServerFixture::start(service.clone()).await;
    let attached_client = fixture.connect().await;
    let unattached_client = fixture.connect().await;
    attached_client.hello_default().await.expect("hello must succeed");
    unattached_client.hello_default().await.expect("hello must succeed");
    attach(&attached_client, "session-1").await;
    let runtime = service.latest_runtime("session-1");

    let progress = TranscriptProgress::assistant_delta(
        "assistant-1".into(),
        0,
        pi_protocol::TranscriptDeltaKind::Text,
        "hello".into(),
    )
    .expect("progress must build");
    runtime.emit_progress(progress.clone());
    let progress_message = attached_client
        .next(|message| {
            matches!(
                message,
                ServerMessage::Event(envelope)
                    if matches!(envelope.event(), pi_protocol::ServerEvent::SessionProgress(_))
            )
        })
        .await
        .expect("progress must arrive");
    match progress_message {
        ServerMessage::Event(envelope) => match envelope.event() {
            pi_protocol::ServerEvent::SessionProgress(event) => {
                assert_eq!(event.session_id(), "session-1");
                assert_eq!(event.progress(), &progress);
            }
            other => panic!("expected session progress, got {other:?}"),
        },
        other => panic!("expected event, got {other:?}"),
    }
    assert!(!unattached_client.messages().iter().any(|message| matches!(
        message,
        ServerMessage::Event(envelope)
            if matches!(envelope.event(), pi_protocol::ServerEvent::SessionProgress(_))
    )));

    let message_count = attached_client.message_count();
    runtime.emit_snapshot();
    let revision = runtime
        .snapshot()
        .await
        .expect("runtime snapshot must succeed")
        .revision();
    let snapshot_message = attached_client
        .next_from(message_count, |message| match message {
            ServerMessage::Event(envelope) => match envelope.event() {
                pi_protocol::ServerEvent::SessionSnapshot(snapshot) => snapshot.revision() == revision,
                _ => false,
            },
            _ => false,
        })
        .await
        .expect("snapshot event must arrive");
    match snapshot_message {
        ServerMessage::Event(envelope) => match envelope.event() {
            pi_protocol::ServerEvent::SessionSnapshot(snapshot) => {
                assert_eq!(snapshot.id(), "session-1");
                assert!(snapshot.attached());
                assert!(snapshot.locked());
            }
            other => panic!("expected session snapshot, got {other:?}"),
        },
        other => panic!("expected event, got {other:?}"),
    }
    assert!(!unattached_client.messages().iter().any(|message| matches!(
        message,
        ServerMessage::Event(envelope)
            if matches!(envelope.event(), pi_protocol::ServerEvent::SessionSnapshot(_))
    )));

    attached_client.close().await;
    unattached_client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn allows_every_attached_client_to_control_a_singleton_live_runtime() {
    let service = Arc::new(TestServerService::new());
    service.seed("session-1");
    let fixture = ServerFixture::start(service.clone()).await;
    let first = fixture.connect().await;
    let second = fixture.connect().await;
    first.hello_default().await.expect("hello must succeed");
    second.hello_default().await.expect("hello must succeed");
    attach(&first, "session-1").await;
    let second_list = second
        .request(&list_command(), "list")
        .await
        .expect("list must respond");
    match success_result(&second_list) {
        CommandResult::List(result) => {
            assert_eq!(result.sessions()[0].id(), "session-1");
            assert_eq!(result.sessions()[0].session_name(), Some("Session session-1"));
        }
        other => panic!("expected list result, got {other:?}"),
    }
    attach(&second, "session-1").await;
    assert_eq!(service.runtime_count("session-1"), 1);

    let model_response = second
        .request(&set_model_command("session-1", "test", "large"), "model")
        .await
        .expect("set model must respond");
    match success_result(&model_response) {
        CommandResult::SetModel(snapshot) => assert_eq!(snapshot.model().id(), "large"),
        other => panic!("expected set_model result, got {other:?}"),
    }
    first
        .next(|message| match message {
            ServerMessage::Event(envelope) => match envelope.event() {
                pi_protocol::ServerEvent::SessionSnapshot(snapshot) => snapshot.model().id() == "large",
                _ => false,
            },
            _ => false,
        })
        .await
        .expect("model update must broadcast");

    let thinking_response = first
        .request(
            &set_thinking_command("session-1", pi_protocol::ThinkingLevel::High),
            "thinking",
        )
        .await
        .expect("set thinking must respond");
    match success_result(&thinking_response) {
        CommandResult::SetThinking(snapshot) => {
            assert_eq!(snapshot.thinking_level(), pi_protocol::ThinkingLevel::High)
        }
        other => panic!("expected set_thinking result, got {other:?}"),
    }

    first.close().await;
    second.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn does_not_queue_prompts_and_processes_steer_and_abort_while_a_prompt_is_pending() {
    let service = Arc::new(TestServerService::new());
    service.seed("session-1");
    let fixture = ServerFixture::start(service.clone()).await;
    let client = Arc::new(fixture.connect().await);
    client.hello_default().await.expect("hello must succeed");
    attach(&client, "session-1").await;

    let prompt = {
        let client = client.clone();
        tokio::spawn(async move { client.request(&prompt_command("session-1", "first"), "prompt").await })
    };
    client
        .next(|message| match message {
            ServerMessage::Event(envelope) => match envelope.event() {
                pi_protocol::ServerEvent::SessionSnapshot(snapshot) => {
                    snapshot.phase() == pi_protocol::SessionPhase::Turn
                }
                _ => false,
            },
            _ => false,
        })
        .await
        .expect("turn snapshot must arrive");

    let busy = client
        .request(&prompt_command("session-1", "second"), "busy")
        .await
        .expect("busy prompt must respond");
    assert_eq!(error_code(&busy), "busy");

    let steer = client
        .request(&steer_command("session-1", "adjust"), "steer")
        .await
        .expect("steer must respond");
    assert!(matches!(success_result(&steer), CommandResult::Steer(_)));
    assert_eq!(
        service.latest_runtime("session-1").steers(),
        vec![pi_server::PromptInput::new("adjust")]
    );
    let abort = client
        .request(&abort_command("session-1"), "abort")
        .await
        .expect("abort must respond");
    assert!(matches!(success_result(&abort), CommandResult::Abort(_)));
    let prompt = prompt
        .await
        .expect("prompt task must join")
        .expect("prompt must respond");
    match success_result(&prompt) {
        CommandResult::Prompt(snapshot) => {
            assert_eq!(snapshot.phase(), pi_protocol::SessionPhase::Idle)
        }
        other => panic!("expected prompt result, got {other:?}"),
    }

    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn returns_operation_attachment_state_relative_to_the_requesting_connection() {
    let service = Arc::new(TestServerService::new());
    service.seed("session-1");
    let fixture = ServerFixture::start(service.clone()).await;
    let first = Arc::new(fixture.connect().await);
    let second = fixture.connect().await;
    first.hello_default().await.expect("hello must succeed");
    second.hello_default().await.expect("hello must succeed");
    attach(&first, "session-1").await;
    attach(&second, "session-1").await;

    let prompt = {
        let first = first.clone();
        tokio::spawn(async move { first.request(&prompt_command("session-1", "hello"), "prompt").await })
    };
    first
        .next(|message| match message {
            ServerMessage::Event(envelope) => match envelope.event() {
                pi_protocol::ServerEvent::SessionSnapshot(snapshot) => {
                    snapshot.phase() == pi_protocol::SessionPhase::Turn
                }
                _ => false,
            },
            _ => false,
        })
        .await
        .expect("turn snapshot must arrive");
    first
        .request(&detach_command("session-1"), "detach")
        .await
        .expect("detach must respond");
    service.latest_runtime("session-1").finish_prompt();

    let prompt = prompt
        .await
        .expect("prompt task must join")
        .expect("prompt must respond");
    match success_result(&prompt) {
        CommandResult::Prompt(snapshot) => {
            assert_eq!(snapshot.id(), "session-1");
            assert!(!snapshot.attached());
        }
        other => panic!("expected prompt result, got {other:?}"),
    }

    first.close().await;
    second.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn keeps_busy_work_alive_after_disconnect_and_disposes_when_it_next_becomes_idle() {
    let service = Arc::new(TestServerService::new());
    service.seed("session-1");
    let fixture = ServerFixture::start(service.clone()).await;
    let client = Arc::new(fixture.connect().await);
    client.hello_default().await.expect("hello must succeed");
    attach(&client, "session-1").await;
    let prompt = {
        let client = client.clone();
        tokio::spawn(async move { client.request(&prompt_command("session-1", "survive"), "prompt").await })
    };
    client
        .next(|message| match message {
            ServerMessage::Event(envelope) => match envelope.event() {
                pi_protocol::ServerEvent::SessionSnapshot(snapshot) => {
                    snapshot.phase() == pi_protocol::SessionPhase::Turn
                }
                _ => false,
            },
            _ => false,
        })
        .await
        .expect("turn snapshot must arrive");
    let runtime = service.latest_runtime("session-1");
    client.close().await;
    assert!(prompt.await.expect("prompt task must join").is_err());
    assert_eq!(runtime.dispose_count(), 0);
    runtime.finish_prompt();
    runtime.disposed().await;
    assert_eq!(runtime.dispose_count(), 1);

    let reconnect = fixture.connect().await;
    reconnect.hello_default().await.expect("hello must succeed");
    let snapshot = attach(&reconnect, "session-1").await;
    assert_eq!(snapshot.transcript().len(), 2);
    match &snapshot.transcript()[1] {
        pi_protocol::TranscriptItem::Assistant(item) => {
            let pi_protocol::AssistantContent::Text(text) = &item.content()[0] else {
                panic!("expected assistant text content");
            };
            assert_eq!(text.text(), "reply:survive");
        }
        other => panic!("expected assistant transcript item, got {other:?}"),
    }

    reconnect.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn restores_persisted_sessions_lazily_after_a_server_restart() {
    let service = Arc::new(TestServerService::new());
    service.seed("session-1");
    let first_fixture = ServerFixture::start(service.clone()).await;
    let first_client = first_fixture.connect().await;
    first_client.hello_default().await.expect("hello must succeed");
    attach(&first_client, "session-1").await;
    first_client
        .request(
            &set_thinking_command("session-1", pi_protocol::ThinkingLevel::High),
            "thinking",
        )
        .await
        .expect("set thinking must respond");
    first_client.close().await;
    first_fixture.close().await;

    let second_fixture = ServerFixture::start(service.clone()).await;
    assert_eq!(service.runtime_count("session-1"), 1);
    let second_client = second_fixture.connect().await;
    second_client.hello_default().await.expect("hello must succeed");
    let restored = attach(&second_client, "session-1").await;
    assert_eq!(restored.thinking_level(), pi_protocol::ThinkingLevel::High);
    assert_eq!(service.runtime_count("session-1"), 2);

    second_client.close().await;
    second_fixture.close().await;
}

#[tokio::test]
async fn rejects_and_disposes_a_service_runtime_with_the_wrong_server_assigned_id() {
    let service = Arc::new(WrongIdService {
        inner: TestServerService::new(),
    });
    let fixture = ServerFixture::start(service.clone()).await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    let response = client
        .request(&create_command(None, None), "create")
        .await
        .expect("create must respond");
    assert_eq!(error_code(&response), "invalid_request");
    assert_eq!(service.inner.latest_runtime("wrong-id").dispose_count(), 1);

    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn maps_service_lock_errors_and_rejects_control_from_unattached_clients() {
    let service = Arc::new(TestServerService::new());
    service.seed("locked");
    service.lock("locked");
    let fixture = ServerFixture::start(service).await;
    let client = fixture.connect().await;
    client.hello_default().await.expect("hello must succeed");
    let locked = client
        .request(&attach_command("locked"), "attach")
        .await
        .expect("attach must respond");
    assert_eq!(error_code(&locked), "session_locked");
    let unattached = client
        .request(&abort_command("locked"), "abort")
        .await
        .expect("abort must respond");
    assert_eq!(error_code(&unattached), "invalid_request");

    client.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn attaches_an_unrelated_live_session_while_another_session_is_still_opening() {
    let service = Arc::new(GatedOpenService::new("slow-a"));
    service.inner.seed("slow-a");
    service.inner.seed("live-b");
    let fixture = ServerFixture::start(service.clone()).await;
    let owner_b = fixture.connect().await;
    let client_a = fixture.connect().await;
    let client_b = fixture.connect().await;
    for client in [&owner_b, &client_a, &client_b] {
        client.hello_default().await.expect("hello must succeed");
    }
    attach(&owner_b, "live-b").await;

    let attach_slow = attach_command("slow-a");
    let attach_live = attach_command("live-b");
    let slow = client_a.request(&attach_slow, "attach-a");
    let fast = async {
        service.entered.wait().await;
        let attached = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client_b.request(&attach_live, "attach-b"),
        )
        .await;
        service.release.resolve(());
        attached
    };
    let (slow, fast) = tokio::join!(slow, fast);

    let fast = fast.expect("attaching live-b must not wait for slow-a to open");
    assert!(matches!(
        success_result(&fast.expect("attach-b must respond")),
        CommandResult::Attach(_)
    ));
    assert!(matches!(
        success_result(&slow.expect("attach-a must respond")),
        CommandResult::Attach(_)
    ));

    for client in [&owner_b, &client_a, &client_b] {
        client.close().await;
    }
    fixture.close().await;
}

#[tokio::test]
async fn concurrent_opens_of_one_session_share_a_single_service_open() {
    let service = Arc::new(GatedOpenService::new("slow-a"));
    service.inner.seed("slow-a");
    let fixture = ServerFixture::start(service.clone()).await;
    let first = fixture.connect().await;
    let second = fixture.connect().await;
    first.hello_default().await.expect("hello must succeed");
    second.hello_default().await.expect("hello must succeed");

    let attach = attach_command("slow-a");
    let first_attach = first.request(&attach, "attach-1");
    let second_attach = async {
        service.entered.wait().await;
        let response = second.request(&attach, "attach-2");
        let release = async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            service.release.resolve(());
        };
        tokio::join!(response, release).0
    };
    let (first_response, second_response) = tokio::join!(first_attach, second_attach);

    for response in [first_response, second_response] {
        assert!(matches!(
            success_result(&response.expect("attach must respond")),
            CommandResult::Attach(_)
        ));
    }
    assert_eq!(service.gated_open_count.load(Ordering::SeqCst), 1);

    first.close().await;
    second.close().await;
    fixture.close().await;
}

#[tokio::test]
async fn concurrent_opens_share_the_failure_of_the_single_service_open() {
    let service = Arc::new(GatedOpenService::new("slow-a"));
    service.inner.seed("slow-a");
    service.inner.lock("slow-a");
    let fixture = ServerFixture::start(service.clone()).await;
    let first = fixture.connect().await;
    let second = fixture.connect().await;
    first.hello_default().await.expect("hello must succeed");
    second.hello_default().await.expect("hello must succeed");

    let attach = attach_command("slow-a");
    let first_attach = first.request(&attach, "attach-1");
    let second_attach = async {
        service.entered.wait().await;
        let response = second.request(&attach, "attach-2");
        let release = async {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            service.release.resolve(());
        };
        tokio::join!(response, release).0
    };
    let (first_response, second_response) = tokio::join!(first_attach, second_attach);

    for response in [first_response, second_response] {
        assert_eq!(error_code(&response.expect("attach must respond")), "session_locked");
    }
    assert_eq!(service.gated_open_count.load(Ordering::SeqCst), 1);

    first.close().await;
    second.close().await;
    fixture.close().await;
}
