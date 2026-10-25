//! Behavioral tests for agents-core runtime, registration, and mailbox flows.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentMessage;
use pi_agent_core::agent::types::AgentToolResult;
use pi_ai::request::BoxFuture;
use pi_coding_agent::bundled_extensions::agents_core::control_runtime::{
    AgentSteeringRequest, AgentSteeringRuntimeBinding, CancelAgentParams, cancel_agent, request_agent_steering,
    steer_agent,
};
use pi_coding_agent::bundled_extensions::agents_core::dispatch_runtime::WaitingDesktopNotificationHandles;
use pi_coding_agent::bundled_extensions::agents_core::mailbox_runtime::{
    ContactParentParams, SendAgentMessageParams, contact_parent, send_agent_message,
};
use pi_coding_agent::bundled_extensions::agents_core::parent_agent_journal::JournalWriter;
use pi_coding_agent::bundled_extensions::agents_core::runtime::{
    MultiAgentExtensionOptions, create_multi_agent_pi_request_handler, create_multi_agent_runtime_handles,
    multi_agent_extension, wait_agents,
};
use pi_coding_agent::bundled_extensions::agents_core::shared::{
    ChildAgentDispatchInput, ChildAgentSession, ChildAgentSessionFactory,
};
use pi_coding_agent::bundled_extensions::agents_core::spawn_attach_runtime::{SpawnAgentRuntimeInput, spawn_agent};
use pi_coding_agent::core::desktop_notification::{DesktopNotification, DesktopNotificationHandle, DesktopNotifier};
use pi_coding_agent::core::extensions::types::{
    ExtensionActions, ExtensionContext, ExtensionContextActions, ExtensionMode, ExtensionUiContext, NotifyType,
};
use pi_coding_agent::core::extensions::{ExtensionRunner, create_extension_runtime, load_extension_from_factory};
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions, SessionPersistence,
};
use pi_coding_agent::core::session_control_db::{
    get_control_db_path, list_runtime_mailbox_messages, read_multi_agent_agent,
};
use pi_coding_agent::core::session_manager::{SessionManager, SharedSessionManager};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

struct TestPersistence {
    control_db_path: String,
    session_path: JsString,
}

impl SessionPersistence for TestPersistence {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.control_db_path.as_str().into())
    }

    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.session_path.clone())
    }
}

struct TestDir {
    _dir: tempfile::TempDir,
    control_db_path: String,
    session_path: JsString,
    cwd: String,
}

fn test_dir() -> TestDir {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path((dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let session_dir = dir.path().join("sessions");
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let session_manager = SessionManager::create(
        &JsString::from(&cwd),
        Some(&JsString::from(session_dir.display().to_string())),
        None,
    )
    .expect("session manager");
    let session_path = session_manager.get_session_file().expect("session file").to_owned();
    TestDir {
        _dir: dir,
        control_db_path,
        session_path,
        cwd,
    }
}

fn store_with_persistence(test: &TestDir) -> Arc<MultiAgentStore> {
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions {
        // Frozen multi-agent-store.ts:939-945 persists before enqueue; DB expiry uses Date.now().
        now: Some(Arc::new(pi_coding_agent::utils::iso_time::now_iso_string)),
    }));
    store.set_persistence_session_manager(Some(&TestPersistence {
        control_db_path: test.control_db_path.clone(),
        session_path: test.session_path.clone(),
    }));
    store
}

#[derive(Default)]
struct RecordingNotifier {
    notifications: Mutex<Vec<JsString>>,
}

impl DesktopNotifier for RecordingNotifier {
    fn notify(
        &self,
        notification: &DesktopNotification,
    ) -> Result<Option<Box<dyn DesktopNotificationHandle>>, pi_js_compat::callback_error::CallbackError> {
        self.notifications.lock().expect("notifications").push(JsString::join(
            [notification.title.clone(), notification.body.clone()],
            "|",
        ));
        Ok(None)
    }
}

#[derive(Default)]
struct RecordingUi {
    notifications: Mutex<Vec<(JsString, Option<NotifyType>)>>,
    editor_text: Mutex<JsString>,
}

impl ExtensionUiContext for RecordingUi {
    fn notify(
        &self,
        message: &JsString,
        notify_type: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.notifications
            .lock()
            .expect("notifications")
            .push((message.to_owned(), notify_type));
        Ok(())
    }

    fn set_editor_text(&self, text: &JsString) -> Result<(), pi_ai::request::CallbackError> {
        *self.editor_text.lock().expect("editor text") = text.to_owned();
        Ok(())
    }
}

fn actions() -> ExtensionActions {
    ExtensionActions {
        send_message: Arc::new(|_, _| Ok(())),
        send_user_message: Arc::new(|_, _| Ok(())),
        append_entry: Arc::new(|_, _| Ok(())),
        request_resume_continuation: Arc::new(|| {}),
        set_session_name: Arc::new(|_| Ok(())),
        get_session_name: Arc::new(|| Ok(None)),
        set_label: Arc::new(|_, _| Ok(())),
        get_active_tools: Arc::new(|| Ok(Vec::new())),
        get_all_tools: Arc::new(|| Ok(Vec::new())),
        set_active_tools: Arc::new(|_| Ok(())),
        call_tool: Arc::new(|_, _, _, _| {
            Box::pin(async {
                Ok(AgentToolResult {
                    content: Vec::new(),
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        call_command: Arc::new(|_, _| Box::pin(async { Ok(Value::Null) })),
        refresh_tools: Arc::new(|| Ok(())),
        get_commands: Arc::new(|| Ok(Vec::new())),
        set_model: Arc::new(|_| Box::pin(async { Ok(false) })),
        get_thinking_level: Arc::new(|| Ok(pi_agent_core::harness::ThinkingLevel::Off)),
        set_thinking_level: Arc::new(|_| Ok(())),
    }
}

fn context_actions(
    control_db_path: String,
    agent_id: Option<String>,
    requires_agent_id: bool,
) -> ExtensionContextActions {
    let mut actions = ExtensionContextActions {
        get_model: Arc::new(|| None),
        get_thinking_level: None,
        set_thinking_level: None,
        get_scoped_models: None,
        get_footer_data: None,
        is_idle: Arc::new(|| true),
        has_active_retry: Arc::new(|| false),
        is_project_trusted: Arc::new(|| true),
        get_signal: Arc::new(|| None),
        abort: Arc::new(|| {}),
        has_pending_messages: Arc::new(|| false),
        shutdown: Arc::new(|| {}),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
        get_control_db_path: Some(Arc::new(move || Some(control_db_path.clone().into()))),
        get_context_usage: Arc::new(|| None),
        get_multi_agent_agent_id: None,
        get_multi_agent_parent_session_id: None,
        register_supervisor_review_cancellation: None,
        get_no_supervisor: None,
        get_multi_agent_requires_agent_id: None,
        get_multi_agent_store: None,
        get_tool_detach_registry: None,
        compact: Arc::new(|_| {}),
        get_system_prompt: Arc::new(JsString::default),
        get_system_prompt_options: None,
    };
    if let Some(agent_id) = agent_id {
        actions.get_multi_agent_agent_id = Some(Arc::new(move || Some(agent_id.clone().into())));
    }
    if requires_agent_id {
        actions.get_multi_agent_requires_agent_id = Some(Arc::new(|| true));
    }
    actions
}

async fn test_context(
    test: &TestDir,
    ui: Option<Arc<dyn ExtensionUiContext>>,
    agent_id: Option<String>,
    requires_agent_id: bool,
) -> (ExtensionRunner, ExtensionContext) {
    let session_manager = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from(&test.cwd)), None).expect("session"),
    ));
    test_context_with(test, session_manager, ui, agent_id, requires_agent_id).await
}

async fn test_context_with(
    test: &TestDir,
    session_manager: SharedSessionManager,
    ui: Option<Arc<dyn ExtensionUiContext>>,
    agent_id: Option<String>,
    requires_agent_id: bool,
) -> (ExtensionRunner, ExtensionContext) {
    test_context_with_extensions(test, session_manager, ui, agent_id, requires_agent_id, Vec::new()).await
}

async fn test_context_with_extensions(
    test: &TestDir,
    session_manager: SharedSessionManager,
    ui: Option<Arc<dyn ExtensionUiContext>>,
    agent_id: Option<String>,
    requires_agent_id: bool,
    extensions: Vec<pi_coding_agent::core::extensions::Extension>,
) -> (ExtensionRunner, ExtensionContext) {
    let runtime = create_extension_runtime();
    let storage = pi_coding_agent::core::auth_storage::AuthStorage::in_memory(pi_js_compat::js_json::JsObject::new());
    let model_registry = pi_coding_agent::core::model_registry::ModelRegistry::in_memory(storage).unwrap();
    let registry = Arc::new(model_registry);
    let mut runner = ExtensionRunner::new(extensions, runtime, &test.cwd, session_manager, registry, None);
    runner
        .bind_core(
            actions(),
            context_actions(test.control_db_path.clone(), agent_id, requires_agent_id),
            None,
        )
        .unwrap();
    if let Some(ui) = ui {
        runner.set_ui_context(Some(ui), ExtensionMode::Rpc);
    }
    let ctx = runner.create_context();
    (runner, ctx)
}

fn seed_child(store: &Arc<MultiAgentStore>, test: &TestDir, id: &str, transcript_session: &str) -> AgentSnapshot {
    let coordinator =
        pi_coding_agent::bundled_extensions::agents_core::lifecycle_runtime::create_lifecycle_coordinator(store)
            .expect("coordinator");
    let prepared = coordinator
        .prepare_child(
            &pi_coding_agent::core::lifecycle_coordinator::PrepareChildCommandInput {
                spawn: pi_coding_agent::core::multi_agent_store::SpawnAgent {
                    parent_id: Some("main".to_owned().into()),
                    display_name: "Child".to_owned().into(),
                    agent_type: "explore".into(),
                    origin: None,
                    cwd: test.cwd.clone().into(),
                    permission: pi_coding_agent::core::multi_agent_store::AgentPermission {
                        policy: "on-request".into(),
                        inherited_from: None,
                        narrowed: true,
                    },
                    worktree: None,
                    model: None,
                    account: None,
                    slot: None,
                    transcript: Some(pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
                        session_id: transcript_session.to_owned().into(),
                        path: Some(format!("/tmp/{transcript_session}.jsonl").into()),
                    }),
                    event_stream: None,
                    worker: None,
                },
                agent_id: Some(id.to_owned().into()),
                detached: None,
                result: None,
            },
        )
        .unwrap();
    let created = coordinator
        .commit_running_child(
            &prepared,
            &JsString::from(&test.session_path),
            pi_coding_agent::bundled_extensions::agents_core::lifecycle_runtime::runtime_process_identity(),
            None,
        )
        .expect("commit child");
    let agent = match created {
        pi_coding_agent::core::session_control_db::CreateMultiAgentChildWithRuntimeOwnershipResult::Ok {
            agent,
            ..
        } => *agent,
        other => panic!("unexpected child creation: {other:?}"),
    };
    store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
    agent
}

struct FakeSession {
    messages: Arc<Mutex<Vec<AgentMessage>>>,
    transcript: Option<pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata>,
    summary: String,
    abort_calls: Arc<Mutex<usize>>,
}

impl FakeSession {
    fn new(summary: &str, transcript_session: Option<&str>) -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            transcript: transcript_session.map(|session_id| {
                pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
                    session_id: session_id.to_owned().into(),
                    path: Some(format!("/tmp/{session_id}.jsonl").into()),
                }
            }),
            summary: summary.to_owned(),
            abort_calls: Arc::new(Mutex::new(0)),
        }
    }
}

impl ChildAgentSession for FakeSession {
    fn abort(&self) {
        *self.abort_calls.lock().expect("abort") += 1;
    }

    fn messages(&self) -> Vec<AgentMessage> {
        self.messages.lock().expect("messages").clone()
    }

    fn prompt(&self, text: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        let summary = self.summary.clone();
        let messages = self.messages.lock().expect("messages").clone();
        let _ = messages;
        let user: AgentMessage = pi_js_compat::js_json::from_value(&json!({
            "role": "user",
            "content": text,
            "timestamp": 0
        }))
        .expect("user message");
        let mut assistant = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux", 1.0);
        assistant
            .content
            .push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
                summary,
            )));
        let session_messages = Arc::clone(&self.messages);
        Box::pin(async move {
            session_messages.lock().expect("messages").push(user);
            session_messages
                .lock()
                .expect("messages")
                .push(AgentMessage::Assistant(Box::new(assistant)));
            Ok(())
        })
    }

    fn transcript(&self) -> Option<pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata> {
        self.transcript.clone()
    }
}

fn fake_factory(summary: &'static str, transcript: bool) -> ChildAgentSessionFactory {
    ChildAgentSessionFactory::new(move |_input: ChildAgentDispatchInput| {
        Box::pin(async move {
            Ok(
                Arc::new(FakeSession::new(summary, transcript.then_some("child-session")))
                    as Arc<dyn ChildAgentSession>,
            )
        })
    })
}

type JournalEntries = Arc<Mutex<Vec<(JsString, Option<Value>)>>>;

fn recording_journal() -> (JournalWriter, JournalEntries) {
    let entries = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&entries);
    let writer = JournalWriter::from_handler(Arc::new(move |custom_type: &JsString, data: Option<Value>| {
        sink.lock().expect("journal").push((custom_type.to_owned(), data));
        Ok(())
    }));
    (writer, entries)
}

#[tokio::test]
async fn spawn_agent_persists_running_child_and_journals_start() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
    let (journal, entries) = recording_journal();
    let notifier: Arc<dyn DesktopNotifier> = Arc::new(RecordingNotifier::default());
    let waiting: WaitingDesktopNotificationHandles = Arc::new(Mutex::new(HashMap::new()));

    let result = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: Some("tool-1".to_owned()),
        create_child_session: Some(fake_factory("child done", true)),
        ctx,
        desktop_notifier: notifier,
        dispatches: Arc::clone(&handles.dispatches),
        handles: Some(Arc::clone(&handles.sessions)),
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "do work", "agentType": "explore"}),
        pi: Some(journal),
        store: Arc::clone(&store),
        waiting_desktop_notifications: waiting,
        _marker: std::marker::PhantomData,
    })
    .await
    .expect("spawn");

    let details = result.details.clone().expect("details");
    let agent_id = details["agent"]["id"].as_string().expect("agent id").to_owned();
    assert_eq!(
        text(&result),
        JsString::join(
            [
                JsString::from("Spawned explore ("),
                agent_id.clone(),
                JsString::from(") [type: explore, context: inherit]")
            ],
            ""
        )
    );
    assert_eq!(details["dispatched"], json!(true));
    assert_eq!(details["prompt"], json!("do work"));

    let journaled = entries.lock().expect("journal").clone();
    assert_eq!(journaled.len(), 1);
    assert_eq!(journaled[0].0, "agent_start");
    assert_eq!(
        journaled[0].1.as_ref().expect("data")["childSessionId"],
        json!("child-session")
    );

    let dispatch = handles
        .dispatches
        .lock()
        .expect("dispatches")
        .get(&agent_id)
        .cloned()
        .expect("dispatch");
    let completed = dispatch.await.expect("dispatch result");
    assert_eq!(completed.lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        completed.result.as_ref().and_then(|result| result.summary.as_ref()),
        Some(&JsString::from("child done"))
    );
    let persisted = read_multi_agent_agent(&test.control_db_path, &test.session_path, &agent_id)
        .expect("read agent")
        .expect("persisted agent");
    assert_eq!(persisted.lifecycle, AgentLifecycle::Completed);
    assert_eq!(persisted.transcript.expect("transcript").session_id, "child-session");
}

#[tokio::test]
async fn spawn_agent_rejects_missing_runtime_and_invalid_prompts() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
    let (journal, _entries) = recording_journal();
    let notifier: Arc<dyn DesktopNotifier> = Arc::new(RecordingNotifier::default());

    let missing_runtime = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: None,
        ctx: ctx.clone(),
        desktop_notifier: Arc::clone(&notifier),
        dispatches: Arc::clone(&handles.dispatches),
        handles: None,
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work"}),
        pi: Some(journal.clone()),
        store: Arc::clone(&store),
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        _marker: std::marker::PhantomData,
    })
    .await
    .expect("spawn");
    assert_eq!(
        text(&missing_runtime),
        "spawn_agent is unavailable: no child session runtime is configured."
    );

    let missing_journal = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(fake_factory("x", false)),
        ctx: ctx.clone(),
        desktop_notifier: Arc::clone(&notifier),
        dispatches: Arc::clone(&handles.dispatches),
        handles: None,
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work"}),
        pi: None,
        store: Arc::clone(&store),
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        _marker: std::marker::PhantomData,
    })
    .await
    .expect("spawn");
    assert_eq!(
        text(&missing_journal),
        "spawn_agent is unavailable: parent session journaling is not configured."
    );

    let invalid_context = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(fake_factory("x", false)),
        ctx,
        desktop_notifier: notifier,
        dispatches: Arc::clone(&handles.dispatches),
        handles: None,
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work", "context": "shared"}),
        pi: Some(journal),
        store,
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        _marker: std::marker::PhantomData,
    })
    .await;
    assert_eq!(
        invalid_context.expect_err("invalid context").message,
        "spawn_agent context must be \"fresh\" or \"inherit\""
    );
}

#[tokio::test]
async fn child_runtime_rejects_orchestration_tools() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, Some("child-agent".to_owned()), true).await;
    let handles = create_multi_agent_runtime_handles();
    let (journal, _entries) = recording_journal();

    let result = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(fake_factory("x", false)),
        ctx,
        desktop_notifier: Arc::new(RecordingNotifier::default()),
        dispatches: Arc::clone(&handles.dispatches),
        handles: None,
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work"}),
        pi: Some(journal),
        store,
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        _marker: std::marker::PhantomData,
    })
    .await
    .expect("spawn");
    assert_eq!(
        text(&result),
        "Agent orchestration is unavailable from child agent runtimes."
    );
}

#[tokio::test]
async fn cancel_agent_requests_cancellation_for_owned_runtime() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
    let (journal, _entries) = recording_journal();
    let result = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(ChildAgentSessionFactory::new(|_input| {
            Box::pin(async {
                Ok(Arc::new(FakeSession::new("running", Some("child-session"))) as Arc<dyn ChildAgentSession>)
            })
        })),
        ctx,
        desktop_notifier: Arc::new(RecordingNotifier::default()),
        dispatches: Arc::clone(&handles.dispatches),
        handles: Some(Arc::clone(&handles.sessions)),
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work"}),
        pi: Some(journal),
        store: Arc::clone(&store),
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        _marker: std::marker::PhantomData,
    })
    .await
    .expect("spawn");
    let agent_id = result.details.expect("details")["agent"]["id"]
        .as_string()
        .expect("id")
        .to_owned();

    let cancelled = cancel_agent(
        &store,
        &handles,
        &CancelAgentParams {
            agent_id: agent_id.clone(),
            reason: Some("done".to_owned().into()),
        },
    )
    .await
    .expect("cancel");
    assert!(
        text(&cancelled).starts_with("Cancellation requested for ") || text(&cancelled).starts_with("Cancelled "),
        "unexpected text: {:?}",
        text(&cancelled)
    );
    let agent = store.get_agent(&agent_id).expect("agent");
    assert!(matches!(
        agent.lifecycle,
        AgentLifecycle::Cancelling | AgentLifecycle::Aborted
    ));
}

#[tokio::test]
async fn steer_without_runtime_identity_reports_unavailable_identity() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, true).await;
    let handles = create_multi_agent_runtime_handles();
    let result = steer_agent(
        &store,
        &handles,
        &pi_coding_agent::bundled_extensions::agents_core::control_runtime::SteerAgentParams {
            agent_id: "missing".to_owned().into(),
            message: "hello".to_owned().into(),
            ..Default::default()
        },
        Some(&ctx),
    )
    .expect("steer");
    assert_eq!(
        text(&result),
        "Could not steer agent: subagent runtime identity is unavailable."
    );
    // Frozen runtime.ts:2801 uses errorResult (402-412), without an isError flag.
    assert_eq!(result.is_error, None);
}

#[tokio::test]
async fn steering_requests_reject_unknown_agents() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let result = request_agent_steering(
        &store,
        &AgentSteeringRequest {
            agent_id: "missing".to_owned().into(),
            message: "hello".to_owned().into(),
            file_refs: None,
            target_checkpoint: None,
        },
        &AgentSteeringRuntimeBinding {
            actor_agent_id: None,
            control_db_path: test.control_db_path.clone().into(),
            session_id: test.session_path.clone().into(),
        },
        None,
    )
    .expect("steering");
    match result {
        pi_coding_agent::bundled_extensions::agents_core::control_runtime::AgentSteeringRequestResult::Err {
            error,
            ..
        } => assert_eq!(error, "Could not steer missing: not_found"),
        other => panic!("unexpected steering result: {other:?}"),
    }
}

#[tokio::test]
async fn mailbox_delivers_to_child_session_and_records_outbound_message() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    seed_child(&store, &test, "child", "child-session");

    pi_coding_agent::core::session_control_db::write_session_metadata(&test.control_db_path, &pi_js_compat::js_json::from_value(&json!({"sessionPath":format!("{}/child.jsonl",test.cwd),"id":"child-session","cwd":test.cwd,"createdAt":pi_coding_agent::utils::iso_time::now_iso_string(),"modifiedAt":pi_coding_agent::utils::iso_time::now_iso_string(),"messageCount":0,"firstMessage":""})).unwrap()).unwrap();
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let result = send_agent_message(
        &store,
        &SendAgentMessageParams {
            file_refs: None,
            message: "hello".to_owned().into(),
            thread_id: None,
            to_agent_id: "child".to_owned().into(),
            to_session_id: Some("child-session".to_owned().into()),
            to_host: None,
        },
        Some(&ctx),
        None,
    )
    .expect("send");
    assert_eq!(text(&result), "Sent message to child in session child-session.");
    let details = result.details.expect("details");
    assert_eq!(details["message"]["fromAgentId"], json!("main"));
    assert_eq!(details["message"]["toAgentId"], json!("child"));

    let runtime_rows = list_runtime_mailbox_messages(&test.control_db_path).expect("runtime mailbox");
    assert_eq!(runtime_rows.len(), 1);
    assert_eq!(runtime_rows[0].recipient.session_id, "child-session");
    assert_eq!(
        runtime_rows[0].recipient.agent_id.clone(),
        Some(JsString::from("child"))
    );
    assert_eq!(runtime_rows[0].sender.agent_id, None);
}

#[tokio::test]
async fn contact_parent_delivers_to_parent_session() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    seed_child(&store, &test, "child", "child-session");

    let (_runner, ctx) = test_context(&test, None, Some("child".to_owned()), false).await;
    let result = contact_parent(
        &store,
        &ContactParentParams {
            file_refs: None,
            message: "need input".to_owned().into(),
            thread_id: None,
        },
        Some(&ctx),
    )
    .expect("contact");
    assert_eq!(text(&result), "Contacted parent for Child.");
    let runtime_rows = list_runtime_mailbox_messages(&test.control_db_path).expect("runtime mailbox");
    assert_eq!(runtime_rows.len(), 1);
    assert_eq!(runtime_rows[0].recipient.agent_id, None);
    assert_eq!(
        runtime_rows[0].recipient.session_id,
        ctx.session_manager().expect("session").get_session_id()
    );
    assert_eq!(runtime_rows[0].sender.agent_id.clone(), Some(JsString::from("child")));
}

#[tokio::test]
async fn wait_agent_returns_empty_when_no_agents_are_active() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
    let result = wait_agents(&store, &handles, None, Some(&ctx)).await.expect("wait");
    assert!(result.content.is_empty());
    assert_eq!(result.details, Some(json!({})));
}

#[tokio::test]
async fn request_handler_validates_wait_params_and_selects_main_thread() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let mut options = MultiAgentExtensionOptions {
        store: Some(Arc::clone(&store)),
        ..Default::default()
    };
    options.runtime_handles = Some(create_multi_agent_runtime_handles());
    let handler = create_multi_agent_pi_request_handler(&options, None);
    let (_runner, ctx) = test_context(&test, None, None, false).await;

    let error = handler
        .handle(
            &json!({"method": "agents.wait", "params": {"parentId": "main"}}),
            &ctx,
            None,
            None,
        )
        .await
        .expect_err("wait params");
    assert_eq!(error.message, "pi.agents.wait does not accept parameters");

    let selected = handler
        .handle(&json!({"method": "agents.current"}), &ctx, None, None)
        .await
        .expect("current");
    assert_eq!(selected["agent"]["id"], json!("main"));
    assert_eq!(selected["agent"]["displayName"], json!("Main thread"));
    assert_eq!(selected["agent"]["lifecycle"], json!("current"));
    assert_eq!(selected["agent"]["selected"], json!(true));

    let unknown = handler
        .handle(
            &json!({"method": "agents.select", "params": "missing"}),
            &ctx,
            None,
            None,
        )
        .await
        .expect_err("select");
    assert_eq!(unknown.message, "Agent not found: missing");
    handler.dispose();
}

#[tokio::test]
async fn request_handler_returns_last_session_message_summary() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let mut options = MultiAgentExtensionOptions {
        store: Some(Arc::clone(&store)),
        ..Default::default()
    };
    options.runtime_handles = Some(create_multi_agent_runtime_handles());
    let handler = create_multi_agent_pi_request_handler(&options, None);
    let mut session_manager = SessionManager::in_memory(Some(&JsString::from(&test.cwd)), None).expect("session");
    let mut assistant = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux", 2.0);
    assistant
        .content
        .push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
            "x".repeat(2500),
        )));
    session_manager
        .append_message(AgentMessage::Assistant(Box::new(assistant)))
        .expect("append message");
    let (_runner, ctx) = test_context_with(&test, Arc::new(Mutex::new(session_manager)), None, None, false).await;
    let details = handler
        .handle(&json!({"method": "messages.last"}), &ctx, None, None)
        .await
        .expect("last");
    assert_eq!(details["role"], json!("assistant"));
    assert_eq!(details["truncated"], json!(true));
    assert_eq!(details["text"].as_string().expect("text").len(), 2000);
    assert_eq!(details["content"], details["text"]);
}

#[tokio::test]
async fn mailbox_registration_includes_question_relay_tools() {
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let result = pi_coding_agent::bundled_extensions::agents_core::runtime::register_agents_mailbox_tools(
                pi,
                &MultiAgentExtensionOptions::default(),
            );
            Box::pin(async move { result })
        }),
        &JsString::from("/repo"),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        &JsString::from("<mailbox-registration>"),
    )
    .await
    .unwrap();
    let names = extension.read().tools.keys().cloned().collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "send_agent_message",
            "contact_parent",
            "ask_questions",
            "answer_question"
        ]
    );
}

#[tokio::test]
async fn registration_exposes_the_source_tools_and_commands() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let mut options = MultiAgentExtensionOptions {
        store: Some(Arc::clone(&store)),
        ..Default::default()
    };
    options.runtime_handles = Some(create_multi_agent_runtime_handles());
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let options = options.clone();
            let result = multi_agent_extension(pi, &options);
            Box::pin(async move { result })
        }),
        &JsString::from(&(test.cwd)),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        &JsString::from("<first-party:agents-core>"),
    )
    .await
    .expect("extension");

    let mut tool_names: Vec<String> = extension.read().tools.keys().cloned().collect();
    tool_names.sort_unstable();
    assert_eq!(
        tool_names,
        [
            "agent_viewer",
            "answer_question",
            "ask_questions",
            "attach_session_agent",
            "close_agent",
            "contact_parent",
            "list_agents",
            "send_agent_message",
            "set_agent_model",
            "spawn_agent",
            "steer_agent",
            "wait_agent",
        ]
    );
    let spawn = extension
        .read()
        .tools
        .get("spawn_agent")
        .expect("spawn tool")
        .definition
        .clone();
    assert_eq!(spawn.label, "Spawn Agent");
    assert_eq!(
        spawn.description,
        "Create a child agent record and optionally dispatch it through the multi-agent runtime."
    );
    assert_eq!(
        Value::from(spawn.parameters.clone()),
        json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "agentType": {"type": "string"},
                "context": {
                    "anyOf": [
                        {"type": "string", "const": "fresh"},
                        {"type": "string", "const": "inherit"}
                    ]
                },
                "displayName": {"type": "string", "description": "Short task slug shown in the agent status list, e.g. fix-footer-count (max ~24 chars)."},
                "parentId": {"type": "string"},
                "prompt": {"type": "string"}
            }
        })
    );
    let wait = extension
        .read()
        .tools
        .get("wait_agent")
        .expect("wait tool")
        .definition
        .clone();
    assert_eq!(
        Value::from(wait.parameters.clone()),
        json!({"type": "object", "properties": {}, "additionalProperties": false})
    );
    assert_eq!(
        extension
            .read()
            .commands
            .get(&pi_js_compat::js_json::JsString::from("bg"))
            .expect("bg")
            .description
            .clone(),
        Some(JsString::from("Run a prompt as a background agent job."))
    );
    assert_eq!(
        extension
            .read()
            .commands
            .get(&pi_js_compat::js_json::JsString::from("jobs"))
            .expect("jobs")
            .description
            .clone(),
        Some(JsString::from("List background agent jobs."))
    );
}

#[tokio::test]
async fn background_command_reports_unavailable_and_validation_errors() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let ui = Arc::new(RecordingUi::default());
    let mut options = MultiAgentExtensionOptions {
        store: Some(Arc::clone(&store)),
        ..Default::default()
    };
    options.runtime_handles = Some(create_multi_agent_runtime_handles());
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let options = options.clone();
            let result = multi_agent_extension(pi, &options);
            Box::pin(async move { result })
        }),
        &JsString::from(&(test.cwd)),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        &JsString::from("<first-party:agents-core>"),
    )
    .await
    .expect("extension");
    let session_manager = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from(&test.cwd)), None).expect("session"),
    ));
    let (runner, _ctx) = test_context_with_extensions(
        &test,
        session_manager,
        Some(ui.clone() as Arc<dyn ExtensionUiContext>),
        None,
        false,
        vec![extension],
    )
    .await;
    let command = runner.get_command("bg").expect("bg command").handler;
    command(String::new().into(), runner.create_command_context())
        .await
        .expect("bg command");
    let notifications = ui.notifications.lock().expect("notifications").clone();
    assert_eq!(
        notifications[0],
        (
            JsString::from("Background jobs are unavailable: no child session runtime is configured."),
            Some(NotifyType::Error)
        )
    );
}

#[tokio::test]
async fn jobs_command_lists_background_agents() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let ui = Arc::new(RecordingUi::default());
    let (_runner, ctx) = test_context(&test, Some(ui.clone() as Arc<dyn ExtensionUiContext>), None, false).await;
    let command_ctx = pi_coding_agent::core::extensions::types::ExtensionCommandContext { context: ctx.clone() };
    pi_coding_agent::bundled_extensions::agents_core::runtime::jobs_command(&store, &command_ctx).expect("jobs");
    assert_eq!(
        ui.notifications.lock().expect("notifications").clone(),
        vec![(JsString::from("No background jobs."), Some(NotifyType::Info))]
    );

    let mut agent = pi_js_compat::js_json::from_value::<AgentSnapshot>(&json!({
        "id": "job",
        "displayName": "Background Job",
        "agentType": "background",
        "lifecycle": "running",
        "revision": 1,
        "createdAt": pi_coding_agent::utils::iso_time::now_iso_string(),
        "updatedAt": pi_coding_agent::utils::iso_time::now_iso_string(),
        "cwd": "/repo",
        "permission": {"policy": "on-request", "narrowed": true}
    }))
    .expect("agent");
    agent.extra = Default::default();
    store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
    pi_coding_agent::bundled_extensions::agents_core::runtime::jobs_command(&store, &command_ctx).expect("jobs");
    let notifications = ui.notifications.lock().expect("notifications").clone();
    assert_eq!(notifications[1].0, "job Background Job is running.");
}

#[tokio::test]
async fn attach_session_agent_attaches_a_saved_session() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let session_path = test.session_path.to_utf8().expect("utf-8 test path");
    let session_dir = std::path::Path::new(&session_path).parent().expect("parent");
    let mut saved = SessionManager::create(
        &JsString::from(&test.cwd),
        Some(&JsString::from(session_dir.display().to_string())),
        None,
    )
    .expect("saved session");
    saved
        .set_session_name(Some(&JsString::from("Saved Session")))
        .expect("in-memory name");
    saved.persist_for_recovery().expect("persist header");
    let saved_id = saved.get_session_id().to_owned();
    let saved_path = saved.get_session_file().expect("saved path").to_owned();
    drop(saved);

    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
    let result = pi_coding_agent::bundled_extensions::agents_core::spawn_attach_runtime::attach_session_agent(
        &pi_coding_agent::bundled_extensions::agents_core::spawn_attach_runtime::AttachSessionAgentRuntimeInput {
            create_attached_session: None,
            ctx,
            desktop_notifier: Arc::new(RecordingNotifier::default()),
            dispatches: Arc::clone(&handles.dispatches),
            handles: Some(Arc::clone(&handles.sessions)),
            params: pi_coding_agent::bundled_extensions::agents_core::spawn_attach_runtime::AttachSessionAgentParams {
                agent_type: None,
                display_name: None,
                name: None,
                parent_id: None,
                path: Some(saved_path.clone()),
                prompt: None,
                session_id: None,
            },
            ownerships: Arc::clone(&handles.ownerships),
            store: Arc::clone(&store),
            waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        },
    )
    .await
    .expect("attach");
    let details = result.details.clone().expect("details");
    assert_eq!(details["dispatched"], json!(false));
    assert_eq!(details["agent"]["lifecycle"], json!("waiting_for_input"));
    assert_eq!(
        details["agent"]["transcript"]["sessionId"],
        json!(saved_id),
        "attach must preserve the freshly persisted session header"
    );
    assert_eq!(
        details["agent"]["displayName"],
        json!(JsString::join([JsString::from("Session "), saved_id.clone()], ""))
    );
    assert_eq!(details["agent"]["transcript"]["path"], json!(saved_path));
    assert!(text(&result).starts_with("Attached "));
}

#[tokio::test]
async fn wait_agent_consumes_pending_terminal_notification() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
    let (journal, _entries) = recording_journal();
    let result = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(fake_factory("finished", true)),
        ctx: ctx.clone(),
        desktop_notifier: Arc::new(RecordingNotifier::default()),
        dispatches: Arc::clone(&handles.dispatches),
        handles: Some(Arc::clone(&handles.sessions)),
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work"}),
        pi: Some(journal),
        store: Arc::clone(&store),
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        _marker: std::marker::PhantomData,
    })
    .await
    .expect("spawn");
    let agent_id = result.details.expect("details")["agent"]["id"]
        .as_string()
        .expect("id")
        .to_owned();
    let dispatch = handles
        .dispatches
        .lock()
        .expect("dispatches")
        .get(&agent_id)
        .cloned()
        .expect("dispatch");
    dispatch.await.expect("dispatch");

    let waited = wait_agents(&store, &handles, None, Some(&ctx)).await.expect("wait");
    let details = waited.details.clone().expect("details");
    assert_eq!(details["agent"]["id"], json!(agent_id));
    assert_eq!(details["agents"][0]["lifecycle"], json!("completed"));
    assert!(text(&waited).contains("finished") || text(&waited).contains("completed"));
}

#[tokio::test]
async fn request_handler_lists_agents_and_routes_select_callback() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    seed_child(&store, &test, "child", "child-session");
    let selected = Arc::new(Mutex::new(Vec::new()));
    let selected_clone = Arc::clone(&selected);
    let mut options = MultiAgentExtensionOptions {
        store: Some(Arc::clone(&store)),
        select_agent_view: Some(Arc::new(move |agent_id| {
            let selected_clone = Arc::clone(&selected_clone);
            let agent_id = agent_id.to_owned();
            Box::pin(async move {
                selected_clone.lock().expect("selected").push(agent_id);
                Some(true)
            })
        })),
        ..Default::default()
    };
    options.runtime_handles = Some(create_multi_agent_runtime_handles());
    let handler = create_multi_agent_pi_request_handler(&options, None);
    let (_runner, ctx) = test_context(&test, None, None, false).await;

    let list = handler
        .handle(&json!({"method": "agents.list"}), &ctx, None, None)
        .await
        .expect("list");
    assert_eq!(list["activeCount"], json!(1));
    assert_eq!(list["agents"][0]["id"], json!("child"));

    let selected_agent = handler
        .handle(
            &json!({"method": "agents.select", "params": {"agentId": "child"}}),
            &ctx,
            None,
            None,
        )
        .await
        .expect("select");
    assert_eq!(selected_agent["agent"]["id"], json!("child"));
    assert_eq!(selected.lock().expect("selected").as_slice(), ["child"]);
}

fn text(result: &AgentToolResult) -> JsString {
    match &result.content[0] {
        pi_ai::types::TextOrImageContent::Text(text) => text.text.clone(),
        pi_ai::types::TextOrImageContent::Image(_) => panic!("expected text content"),
    }
}

#[tokio::test(start_paused = true)]
async fn wait_slice_uses_latest_model_request_and_does_not_cancel_active_agents() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    seed_child(&store, &test, "child", "child-session");
    let handles = create_multi_agent_runtime_handles();
    let options = MultiAgentExtensionOptions {
        store: Some(store.clone()),
        runtime_handles: Some(handles.clone()),
        ..Default::default()
    };
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let result = multi_agent_extension(pi, &options);
            Box::pin(async move { result })
        }),
        &JsString::from(&(test.cwd)),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        &JsString::from("<wait-slice>"),
    )
    .await
    .unwrap();
    let session = Arc::new(Mutex::new(
        SessionManager::open(&JsString::from(&test.session_path), None, None).unwrap(),
    ));
    let (runner, ctx) = test_context_with_extensions(&test, session, None, None, false, vec![extension]).await;
    let mut event: pi_coding_agent::core::extensions::types::ExtensionEvent =
        pi_js_compat::js_json::from_value(&json!({"type":"model_request_start"}))
            .expect("model_request_start extension event");
    runner.emit(&mut event).await;
    *handles
        .latest_model_request_started_at
        .lock()
        .unwrap()
        .as_mut()
        .unwrap() -= 24 * 60_000;
    let waiting = tokio::spawn({
        let store = store.clone();
        let handles = handles.clone();
        let ctx = ctx.clone();
        async move { wait_agents(&store, &handles, None, Some(&ctx)).await.unwrap() }
    });
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(60_000 + 1)).await;
    tokio::task::yield_now().await;
    assert!(
        waiting.is_finished(),
        "deadline is measured from latest model request, not from wait start"
    );
    let result = waiting.await.unwrap();
    assert_eq!(result.details.unwrap(), json!({"timedOut":true}));
    assert_eq!(
        store.get_agent(&JsString::from("child")).unwrap().lifecycle,
        AgentLifecycle::Running
    );
    let mut end: pi_coding_agent::core::extensions::types::ExtensionEvent =
        pi_js_compat::js_json::from_value(&json!({"type":"agent_end","messages":[]})).unwrap();
    runner.emit(&mut end).await;
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let waiting = tokio::spawn({
        let store = store.clone();
        let handles = handles.clone();
        let ctx = ctx.clone();
        let signal = signal.clone();
        async move { wait_agents(&store, &handles, Some(signal), Some(&ctx)).await.unwrap() }
    });
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_secs(30 * 60)).await;
    assert!(!waiting.is_finished(), "idle wait has no foreground slice deadline");
    controller_signal.abort(None);
    waiting.await.unwrap();
}
