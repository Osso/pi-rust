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
        now: Some(Arc::new(|| {
            Ok(pi_coding_agent::utils::iso_time::now_iso_string().into())
        })),
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
    messages: Arc<Mutex<Vec<Arc<AgentMessage>>>>,
    transcript: Option<pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata>,
    summary: String,
    abort_calls: Arc<Mutex<usize>>,
    on_prompt: Option<Arc<dyn Fn() + Send + Sync>>,
    response_error: Option<JsString>,
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
            on_prompt: None,
            response_error: None,
        }
    }
}

impl ChildAgentSession for FakeSession {
    fn abort(&self) {
        *self.abort_calls.lock().expect("abort") += 1;
    }

    fn messages(&self) -> Vec<Arc<AgentMessage>> {
        self.messages.lock().expect("messages").clone()
    }

    fn prompt(&self, text: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        if let Some(on_prompt) = &self.on_prompt {
            on_prompt();
        }
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
        if let Some(error) = &self.response_error {
            assistant.stop_reason = pi_ai::types::StopReason::Error;
            assistant.error_message = Some(error.clone());
        }
        let session_messages = Arc::clone(&self.messages);
        Box::pin(async move {
            session_messages.lock().expect("messages").push(Arc::new(user));
            session_messages
                .lock()
                .expect("messages")
                .push(Arc::new(AgentMessage::Assistant(Box::new(assistant))));
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

#[tokio::test]
async fn spawn_agent_persists_child_without_legacy_journal_or_transcript_requirement() {
    let mut ts = repin_oracle::TsOracle::spawn(
        "agents-core-drift-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src/runtime.ts",
    );
    let expected = ts.request(&json!({"kind":"spawnWithoutJournal"}));
    assert_eq!(
        expected,
        json!({"dispatched":true,"lifecycle":"completed","hasTranscript":false,"customEntries":0})
    );
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
    let notifier: Arc<dyn DesktopNotifier> = Arc::new(RecordingNotifier::default());
    let waiting: WaitingDesktopNotificationHandles = Arc::new(Mutex::new(HashMap::new()));

    let result = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: Some("tool-1".to_owned()),
        create_child_session: Some(fake_factory("child done", false)),
        ctx: ctx.clone(),
        desktop_notifier: notifier,
        dispatches: Arc::clone(&handles.dispatches),
        handles: Some(Arc::clone(&handles.sessions)),
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "do work", "agentType": "explore"}),
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

    assert!(
        ctx.session_manager()
            .unwrap()
            .get_branch(None)
            .iter()
            .all(|entry| entry.entry_type() != "custom")
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
    assert!(persisted.transcript.is_none());
}

#[tokio::test]
async fn spawn_agent_rejects_missing_runtime_and_invalid_prompts() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
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

    let without_transcript = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(fake_factory("x", false)),
        ctx: ctx.clone(),
        desktop_notifier: Arc::clone(&notifier),
        dispatches: Arc::clone(&handles.dispatches),
        handles: None,
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work"}),
        store: Arc::clone(&store),
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        _marker: std::marker::PhantomData,
    })
    .await
    .expect("spawn");
    assert_eq!(without_transcript.details.unwrap()["dispatched"], json!(true));

    let invalid_context = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(fake_factory("x", false)),
        ctx,
        desktop_notifier: notifier,
        dispatches: Arc::clone(&handles.dispatches),
        handles: None,
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work", "context": "shared"}),
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

    let result = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(fake_factory("x", false)),
        ctx,
        desktop_notifier: Arc::new(RecordingNotifier::default()),
        dispatches: Arc::clone(&handles.dispatches),
        handles: None,
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work"}),
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

#[path = "support/oracle.rs"]
mod repin_oracle;

#[tokio::test]
async fn child_response_ignores_inherited_text_and_fails_terminal_model_error() {
    for response_error in [None, Some(JsString::from("model rejected")), Some(JsString::default())] {
        let test = test_dir();
        let store = store_with_persistence(&test);
        let (_runner, ctx) = test_context(&test, None, None, false).await;
        let handles = create_multi_agent_runtime_handles();
        let expected_error = response_error.clone();
        let factory = ChildAgentSessionFactory::new(move |_| {
            let response_error = response_error.clone();
            Box::pin(async move {
                let mut session = FakeSession::new("", Some("child-session"));
                let mut old = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux", 0.0);
                old.content
                    .push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
                        "inherited answer",
                    )));
                session
                    .messages
                    .lock()
                    .unwrap()
                    .push(Arc::new(AgentMessage::Assistant(Box::new(old))));
                session.response_error = response_error;
                Ok(Arc::new(session) as Arc<dyn ChildAgentSession>)
            })
        });
        let result = spawn_agent(SpawnAgentRuntimeInput {
            active_tool_call_id: None,
            create_child_session: Some(factory),
            ctx,
            desktop_notifier: Arc::new(RecordingNotifier::default()),
            dispatches: Arc::clone(&handles.dispatches),
            handles: Some(Arc::clone(&handles.sessions)),
            ownerships: Arc::clone(&handles.ownerships),
            params: json!({"prompt":"work"}),
            store: Arc::clone(&store),
            waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
            _marker: std::marker::PhantomData,
        })
        .await
        .unwrap();
        let details = result.details.unwrap();
        assert_eq!(details["dispatched"], json!(true));
        let id = details["agent"]["id"].as_string().unwrap();
        let terminal = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let current = store.get_agent(id).unwrap();
                if !pi_coding_agent::core::multi_agent_store::is_active_lifecycle(current.lifecycle) {
                    break current;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        if let Some(error) = expected_error {
            assert_eq!(terminal.lifecycle, AgentLifecycle::Failed);
            let expected = if error.is_empty() {
                JsString::from("Child model request ended with error")
            } else {
                error
            };
            assert_eq!(terminal.error.unwrap().message, expected);
        } else {
            assert_eq!(terminal.lifecycle, AgentLifecycle::Completed);
            assert!(
                terminal.result.is_none(),
                "Inherited answer must not become this turn's result: {:?}",
                terminal.result
            );
        }
    }
}

#[test]
fn frozen_child_response_matrix_preserves_identity_errors_and_utf16() {
    use pi_coding_agent::bundled_extensions::agents_core::child_response::read_current_child_assistant_text;
    let mut ts = repin_oracle::TsOracle::spawn(
        "agents-core-repin-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    let assistant = |text: JsString, error: Option<JsString>| {
        let mut message = pi_ai::types::AssistantMessage::empty("faux", "faux", "faux", 1.0);
        message
            .content
            .push(pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
                text,
            )));
        if error.is_some() {
            message.stop_reason = pi_ai::types::StopReason::Error;
            message.error_message = error;
        }
        json!(AgentMessage::Assistant(Box::new(message)))
    };
    let old = assistant("old".into(), None);
    let empty = assistant("".into(), None);
    let raw = assistant(JsString::from_utf16(vec![0xD800, 0x78, 0xDC00]), None);
    let failed = assistant("do not use".into(), Some("raw error".into()));
    let fallback = assistant("".into(), Some("".into()));
    let cases = vec![
        (vec![], 0),
        (vec![old.clone()], 1),
        (vec![old.clone(), empty.clone()], 1),
        (vec![old.clone(), old.clone()], 1),
        (vec![old.clone(), raw.clone(), empty.clone()], 1),
        (vec![old.clone(), failed.clone()], 1),
        (vec![fallback], 0),
        (vec![failed, raw.clone()], 1),
        (vec![raw], 0),
    ];
    for (messages, previous_count) in cases {
        let source =
            ts.request(&json!({"id":3,"op":"childResponse","messages":messages,"previousCount":previous_count}));
        let native: Vec<Arc<AgentMessage>> = messages
            .iter()
            .map(|value| Arc::new(pi_js_compat::js_json::from_value(value).unwrap()))
            .collect();
        let actual = read_current_child_assistant_text(&native, &native[..previous_count]);
        let response = match actual {
            Ok(output) => json!({"id":3,"ok":true,"output":output}),
            Err(message) => json!({"id":3,"ok":false,"error":{"name":"Error","message":message}}),
        };
        assert_eq!(source, response, "messages={messages:?}; previous={previous_count}");
    }
}

#[tokio::test]
async fn child_shutdown_does_not_invalidate_shared_parent_dispatches() {
    use pi_coding_agent::core::extensions::types::{SessionShutdownEvent, SessionShutdownReason};
    let mut ts = repin_oracle::TsOracle::spawn(
        "agents-core-repin-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    for (child, reason, label) in [
        (true, SessionShutdownReason::Quit, "quit"),
        (false, SessionShutdownReason::Reload, "reload"),
        (false, SessionShutdownReason::Quit, "quit"),
    ] {
        let test = test_dir();
        let store = store_with_persistence(&test);
        let options = MultiAgentExtensionOptions {
            store: Some(Arc::clone(&store)),
            runtime_handles: Some(create_multi_agent_runtime_handles()),
            ..Default::default()
        };
        let extension = load_extension_from_factory(
            Arc::new(move |pi| {
                let result = multi_agent_extension(pi, &options);
                Box::pin(async move { result })
            }),
            &test.cwd.clone().into(),
            Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
            create_extension_runtime(),
            &"<core>".into(),
        )
        .await
        .unwrap();
        let hooks = extension.read().handlers.session_shutdown.clone();
        let (_runner, ctx) = test_context(&test, None, child.then(|| "child".to_owned()), child).await;
        let before = store.get_restore_generation();
        for hook in hooks {
            hook.call(
                SessionShutdownEvent {
                    reason,
                    target_session_file: None,
                },
                ctx.clone(),
            )
            .await
            .unwrap();
        }
        let source = ts.request(&json!({"id":7,"op":"shutdownScope","child":child,"reason":label}));
        assert_eq!(source["ok"], json!(true), "{source:?}");
        assert_eq!(
            source["output"],
            json!({"invalidations":store.get_restore_generation()-before}),
            "child={child}, reason={label}"
        );
    }
}

#[test]
fn detached_memory_selection_and_cleanup_failures_match_frozen() {
    use pi_coding_agent::bundled_extensions::agents_core::control_runtime::{
        read_detached_runtime_agent_ids, request_direct_detached_runtime_cancellations,
    };
    use pi_coding_agent::bundled_extensions::agents_core::shared::empty_agent;
    let mut ts = repin_oracle::TsOracle::spawn(
        "agents-core-repin-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    let mut active = json!(empty_agent("detached"));
    active["agentType"] = json!("background");
    active["parentId"] = json!("parent");
    active["lifecycle"] = json!("running");
    active["detached"] = json!(true);
    active["worker"] = json!({"adapter":"runtime","handleId":"job"});
    active["result"] = json!({"fileRefs":[{"path":"output","label":"Pyrun output"}]});
    let mut terminal = active.clone();
    terminal["lifecycle"] = json!("completed");
    let mut other_parent = active.clone();
    other_parent["parentId"] = json!("other");
    let mut other_worker = active.clone();
    other_worker["worker"] = json!({"adapter":"terminal","handleId":"job"});
    for agents in [
        vec![],
        vec![active],
        vec![terminal],
        vec![other_parent],
        vec![other_worker],
    ] {
        let source = ts.request(
            &json!({"id":4,"op":"detachedMemory","agents":agents,"ids":["detached","unknown"],"parent":"parent"}),
        );
        assert_eq!(source["ok"], json!(true), "{source:?}");
        let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
        for value in &agents {
            store
                .publish_lifecycle_coordinator_snapshot(&pi_js_compat::js_json::from_value(value).unwrap())
                .unwrap();
        }
        let ids = read_detached_runtime_agent_ids(&store, &["detached".into(), "unknown".into()]).unwrap();
        let direct_error = request_direct_detached_runtime_cancellations(&store, &"parent".into(), &"cleanup".into())
            .err()
            .map(|error| error.message);
        assert_eq!(
            source["output"],
            json!({"ids":ids.into_iter().collect::<Vec<_>>(),"directError":direct_error})
        );
    }
}

#[tokio::test]
async fn frozen_settlement_abort_and_detached_predicate_match() {
    use pi_coding_agent::bundled_extensions::agents_core::control_runtime::is_detached_runtime_agent;
    use pi_coding_agent::bundled_extensions::agents_core::shared::empty_agent;
    let mut ts = repin_oracle::TsOracle::spawn(
        "agents-core-repin-oracle.ts",
        "packages/coding-agent/extensions/agents-core/src",
    );
    for cancelling in [false, true] {
        let source = ts.request(&json!({"id":1,"op":"settlementAbort","cancelling":cancelling}));
        assert_eq!(source["ok"], json!(true), "{source:?}");
        assert_eq!(
            source["output"],
            json!({"settledBeforeUpdate":!cancelling,"listenersAfterFinish":0})
        );
    }
    let mut agent = json!(empty_agent("runtime"));
    agent["agentType"] = json!("background");
    agent["lifecycle"] = json!("running");
    agent["detached"] = json!(true);
    agent["worker"] = json!({"adapter":"runtime","handleId":"job"});
    agent["result"] = json!({"fileRefs":[{"path":"output","label":"Bash output"}]});
    let mut cases = vec![agent.clone()];
    for (key, value) in [
        ("detached", json!(false)),
        ("worker", json!({"adapter":"terminal","handleId":"job"})),
        ("agentType", json!("worker")),
        ("result", json!({"fileRefs":[{"path":"output","label":"Other output"}]})),
        ("result", Value::Null),
    ] {
        let mut other = agent.clone();
        other[key] = value;
        cases.push(other);
    }
    agent["result"] = json!({"fileRefs":[{"path":"first"},{"path":"output","label":"Pyrun output"}]});
    cases.push(agent);
    for agent in cases {
        let source = ts.request(&json!({"id":2,"op":"isDetached","agent":agent}));
        assert_eq!(source["ok"], json!(true), "{source:?}");
        let native: AgentSnapshot = pi_js_compat::js_json::from_value(&agent).unwrap();
        assert_eq!(source["output"], json!(is_detached_runtime_agent(&native)), "{agent:?}");
    }
}

#[tokio::test]
async fn child_terminal_path_cancels_detached_runtime_before_settling() {
    use pi_coding_agent::bundled_extensions::agents_core::lifecycle_runtime::create_lifecycle_coordinator;
    use pi_coding_agent::core::lifecycle_coordinator::{
        LifecycleCommandResult, OwnedLifecycleCommandInput, PrepareChildCommandInput,
    };
    use pi_coding_agent::core::multi_agent_store::SpawnAgent;
    use pi_coding_agent::core::session_control_db::{
        CreateMultiAgentChildWithRuntimeOwnershipResult, RuntimeMailboxAddress, RuntimeMailboxRegistrationOptions,
        register_runtime_mailbox_listener,
    };
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
    let identity = pi_coding_agent::core::runtime_process::read_process_identity(std::process::id()).unwrap();
    let owner_id = ctx.session_manager().unwrap().get_session_id().to_owned();
    register_runtime_mailbox_listener(
        &test.control_db_path,
        &RuntimeMailboxAddress {
            session_id: owner_id.clone(),
            agent_id: None,
        },
        identity.pid,
        Some(&test.session_path),
        RuntimeMailboxRegistrationOptions::default(),
    )
    .unwrap();
    let detached = Arc::new(Mutex::new(None));
    let captured = Arc::clone(&detached);
    let factory_store = Arc::clone(&store);
    let factory = ChildAgentSessionFactory::new(move |input| {
        let store = Arc::clone(&factory_store);
        let captured = Arc::clone(&captured);
        let identity = identity.clone();
        let owner_id = owner_id.clone();
        Box::pin(async move {
            let mut session = FakeSession::new("parent done", Some("parent-session"));
            session.on_prompt = Some(Arc::new(move || {
                let coordinator = create_lifecycle_coordinator(&store).unwrap();
                let spawn: SpawnAgent = pi_js_compat::js_json::from_value(&json!({
                    "parentId":input.agent.id,"displayName":"background-job","agentType":"background","cwd":"/tmp",
                    "permission":{"policy":"on-request","narrowed":true},"worker":{"adapter":"runtime","handleId":"job"}
                }))
                .unwrap();
                let prepared = coordinator
                    .prepare_child(&PrepareChildCommandInput {
                        spawn,
                        agent_id: None,
                        detached: Some(true),
                        result: Some(
                            pi_js_compat::js_json::from_value(
                                &json!({"fileRefs":[{"path":"/tmp/job","label":"Bash output"}]}),
                            )
                            .unwrap(),
                        ),
                    })
                    .unwrap();
                let result = coordinator
                    .commit_running_child(&prepared, &owner_id, &identity, None)
                    .unwrap();
                let CreateMultiAgentChildWithRuntimeOwnershipResult::Ok { agent, ownership } = result else {
                    panic!("child commit {result:?}")
                };
                store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
                *captured.lock().unwrap() = Some((coordinator, *agent, *ownership));
            }));
            Ok(Arc::new(session) as Arc<dyn ChildAgentSession>)
        })
    });
    let result = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(factory),
        ctx,
        desktop_notifier: Arc::new(RecordingNotifier::default()),
        dispatches: Arc::clone(&handles.dispatches),
        handles: Some(Arc::clone(&handles.sessions)),
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt":"work"}),
        store: Arc::clone(&store),
        waiting_desktop_notifications: Arc::new(Mutex::new(HashMap::new())),
        _marker: std::marker::PhantomData,
    })
    .await
    .unwrap();
    assert_eq!(
        result.details.as_ref().unwrap()["dispatched"],
        json!(true),
        "{result:?}"
    );
    let parent_id = result.details.unwrap()["agent"]["id"].as_string().unwrap().clone();
    let (coordinator, child, ownership) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(detached) = detached.lock().unwrap().take() {
                break detached;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let cancelling = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let current = read_multi_agent_agent(&test.control_db_path, &test.session_path, &child.id)
                .unwrap()
                .unwrap();
            if current.lifecycle == AgentLifecycle::Cancelling {
                break current;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("terminal cleanup must cancel direct detached child");
    assert_eq!(cancelling.extra.get("suppressTerminalNotification"), Some(&json!(true)));
    assert_eq!(store.get_agent(&parent_id).unwrap().lifecycle, AgentLifecycle::Running);
    let LifecycleCommandResult::Ok { agent: aborted } = coordinator
        .acknowledge_cancellation(&OwnedLifecycleCommandInput {
            agent: cancelling,
            ownership,
        })
        .unwrap()
    else {
        panic!("acknowledge")
    };
    store.publish_lifecycle_coordinator_snapshot(&aborted).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while store.get_agent(&parent_id).unwrap().lifecycle == AgentLifecycle::Running {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        store.get_agent(&parent_id).unwrap().lifecycle,
        AgentLifecycle::Completed
    );
}

#[tokio::test]
async fn cancelling_parent_waits_for_descendants_even_after_abort() {
    use pi_coding_agent::bundled_extensions::agents_core::dispatch_runtime::wait_for_active_descendants;
    use pi_coding_agent::bundled_extensions::agents_core::shared::empty_agent;
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let mut parent = empty_agent("parent");
    parent.lifecycle = AgentLifecycle::Cancelling;
    let mut child = empty_agent("child");
    child.parent_id = Some(parent.id.clone());
    child.lifecycle = AgentLifecycle::Running;
    store.publish_lifecycle_coordinator_snapshot(&parent).unwrap();
    store.publish_lifecycle_coordinator_snapshot(&child).unwrap();
    let controller = pi_js_compat::abort::AbortController::new();
    controller.abort(None);
    let signal = controller.signal();
    let waiting = wait_for_active_descendants(&store, &parent.id, Some(&signal));
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut waiting)
            .await
            .is_err()
    );
    child.lifecycle = AgentLifecycle::Completed;
    child.revision += 1;
    store.publish_lifecycle_coordinator_snapshot(&child).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn descendant_settlement_reconciles_persisted_newer_revision_without_local_event() {
    use pi_coding_agent::bundled_extensions::agents_core::dispatch_runtime::wait_for_active_descendants;
    use pi_coding_agent::bundled_extensions::agents_core::shared::empty_agent;
    use pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent;
    let test = test_dir();
    let store = store_with_persistence(&test);
    let mut child = empty_agent("child");
    child.parent_id = Some("parent".into());
    child.lifecycle = AgentLifecycle::Running;
    child.revision = 3;
    store.publish_lifecycle_coordinator_snapshot(&child).unwrap();
    let mut stale = child.clone();
    stale.revision = 2;
    stale.lifecycle = AgentLifecycle::Completed;
    bootstrap_multi_agent_agent(&test.control_db_path, &test.session_path, &child.id, &json!(stale)).unwrap();
    let mut grandchild = child.clone();
    grandchild.id = "grandchild".into();
    grandchild.parent_id = Some(child.id.clone());
    grandchild.revision = 1;
    bootstrap_multi_agent_agent(
        &test.control_db_path,
        &test.session_path,
        &grandchild.id,
        &json!(grandchild),
    )
    .unwrap();
    let mut unrelated = child.clone();
    unrelated.id = "unrelated".into();
    unrelated.parent_id = None;
    bootstrap_multi_agent_agent(
        &test.control_db_path,
        &test.session_path,
        &unrelated.id,
        &json!(unrelated),
    )
    .unwrap();
    let parent_id = JsString::from("parent");
    let waiting = wait_for_active_descendants(&store, &parent_id, None);
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut waiting)
            .await
            .is_err()
    );
    assert_eq!(
        store.get_agent(&child.id).unwrap().revision,
        3,
        "stale persisted state must not replace newer local state"
    );
    assert!(
        store.get_agent(&grandchild.id).is_some(),
        "missing persisted descendants must be published"
    );
    assert!(store.get_agent(&unrelated.id).is_none());
    child.lifecycle = AgentLifecycle::Completed;
    child.revision = 4;
    grandchild.lifecycle = AgentLifecycle::Completed;
    grandchild.revision = 2;
    bootstrap_multi_agent_agent(&test.control_db_path, &test.session_path, &child.id, &json!(child)).unwrap();
    bootstrap_multi_agent_agent(
        &test.control_db_path,
        &test.session_path,
        &grandchild.id,
        &json!(grandchild),
    )
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), waiting)
        .await
        .expect("250ms persisted poll")
        .unwrap();
    assert_eq!(store.get_agent(&child.id).unwrap().revision, 4);
    assert_eq!(store.get_agent(&child.id).unwrap().lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        store.get_agent(&grandchild.id).unwrap().lifecycle,
        AgentLifecycle::Completed
    );
}

#[tokio::test]
async fn cancel_agent_requests_cancellation_for_owned_runtime() {
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let handles = create_multi_agent_runtime_handles();
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

#[tokio::test(start_paused = true)]
async fn cancellation_uses_configured_timeout_and_propagates_dispatch_failure() {
    use futures_util::FutureExt;
    use pi_coding_agent::bundled_extensions::agents_core::control_runtime::{
        OwnedAgentRuntime, cancel_owned_agent_runtime,
    };
    use pi_coding_agent::bundled_extensions::agents_core::lifecycle_runtime::create_lifecycle_coordinator;
    use pi_coding_agent::core::lifecycle_coordinator::OwnedLifecycleCommandInput;
    use pi_coding_agent::core::session_control_db::read_multi_agent_runtime_ownership;
    for failure in [None, Some(JsString::from("dispatch settlement failed"))] {
        let test = test_dir();
        let store = store_with_persistence(&test);
        let agent = seed_child(&store, &test, "owned", "owned-transcript");
        let mut handles = create_multi_agent_runtime_handles();
        handles.cancellation_settlement_timeout_ms = Some(17);
        let ownership = read_multi_agent_runtime_ownership(&test.control_db_path, &test.session_path, &agent.id)
            .unwrap()
            .unwrap();
        handles.ownerships.lock().unwrap().insert(
            agent.id.clone(),
            Arc::new(OwnedAgentRuntime {
                abort_controller: pi_js_compat::abort::AbortController::new(),
                coordinator: create_lifecycle_coordinator(&store).unwrap(),
                lifecycle: OwnedLifecycleCommandInput {
                    agent: agent.clone(),
                    ownership,
                },
            }),
        );
        let dispatch_error = failure.clone();
        handles.dispatches.lock().unwrap().insert(
            agent.id.clone(),
            async move {
                if let Some(error) = dispatch_error {
                    Err(error)
                } else {
                    std::future::pending().await
                }
            }
            .boxed()
            .shared(),
        );
        let start = tokio::time::Instant::now();
        let result = cancel_owned_agent_runtime(&store, &handles, &agent.id, None).await;
        if let Some(error) = failure {
            assert_eq!(result.err().expect("source rejects failed dispatch").message, error);
        } else {
            assert!(result.is_ok());
            assert_eq!(start.elapsed(), std::time::Duration::from_millis(17));
            assert_eq!(
                store.get_agent(&agent.id).unwrap().lifecycle,
                AgentLifecycle::Cancelling
            );
            assert!(
                handles.dispatches.lock().unwrap().contains_key(&agent.id),
                "timeout must not cancel the dispatch"
            );
        }
    }
}

#[tokio::test]
async fn recovery_resumes_unjournaled_child_transcript() {
    use pi_coding_agent::bundled_extensions::agents_core::shared::AttachedSessionFactory;
    use pi_coding_agent::bundled_extensions::agents_core::spawn_attach_runtime::{RecoverAgentsInput, recover_agents};
    let test = test_dir();
    let store = store_with_persistence(&test);
    let (_runner, ctx) = test_context(&test, None, None, false).await;
    let agent = seed_child(&store, &test, "unjournaled", "saved-transcript");
    let handles = create_multi_agent_runtime_handles();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    recover_agents(&RecoverAgentsInput {
        create_attached_session: Some(AttachedSessionFactory::new(move |input| {
            observed.lock().unwrap().push((input.agent.id, input.prompt));
            Box::pin(async { Ok(Arc::new(FakeSession::new("recovered", None)) as Arc<dyn ChildAgentSession>) })
        })),
        ctx,
        desktop_notifier: Arc::new(RecordingNotifier::default()),
        dispatches: Arc::clone(&handles.dispatches),
        handles: Some(Arc::clone(&handles.sessions)),
        ownerships: Arc::clone(&handles.ownerships),
        store: Arc::clone(&store),
        waiting_desktop_notifications: Default::default(),
    })
    .unwrap();
    let dispatch = handles
        .dispatches
        .lock()
        .unwrap()
        .get(&agent.id)
        .cloned()
        .expect("must dispatch without journal");
    assert_eq!(dispatch.await.unwrap().lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![(
            agent.id,
            JsString::from(
                "Continue the conversation from where it left off without asking the user any further questions. Resume directly from the saved session context."
            )
        )]
    );
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
    let handler = create_multi_agent_pi_request_handler(&options);
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
    let handler = create_multi_agent_pi_request_handler(&options);
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
    let result = spawn_agent(SpawnAgentRuntimeInput {
        active_tool_call_id: None,
        create_child_session: Some(fake_factory("finished", true)),
        ctx: ctx.clone(),
        desktop_notifier: Arc::new(RecordingNotifier::default()),
        dispatches: Arc::clone(&handles.dispatches),
        handles: Some(Arc::clone(&handles.sessions)),
        ownerships: Arc::clone(&handles.ownerships),
        params: json!({"prompt": "work"}),
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
    let handler = create_multi_agent_pi_request_handler(&options);
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
