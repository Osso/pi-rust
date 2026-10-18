//! Behavioral tests for `core/agent-session.ts`.

#[path = "support/image_fixtures.rs"]
mod image_fixtures;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_agent_core::agent::AgentOptions;
use pi_agent_core::agent::types::AgentMessage;
use pi_agent_core::agent::{Agent, AgentInitialState};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions,
    faux_assistant_message, faux_provider,
};
use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{Context, Model};
use pi_coding_agent::core::agent_session::{
    AgentSession, AgentSessionConfig, AgentSessionEvent, CycleDirection, NavigateTreeOptions, PromptOptions,
    StreamingBehavior, ToolDefinition,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use serde_json::json;

struct Fixture {
    session: AgentSession,
    faux: FauxProviderHandle,
    _temp_dir: tempfile::TempDir,
}

fn faux_model() -> FauxModelDefinition {
    FauxModelDefinition {
        id: "faux-1".to_owned(),
        name: Some("Faux Model".to_owned()),
        reasoning: Some(true),
        input: None,
        cost: None,
        context_window: Some(100_000.0),
        max_tokens: Some(4096.0),
    }
}

fn auth_data() -> serde_json::Map<String, serde_json::Value> {
    let mut data = serde_json::Map::new();
    data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    data
}

async fn model_runtime(faux: &FauxProviderHandle) -> ModelRuntime {
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(auth_data()))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("model runtime must initialize");
    runtime.register_native_provider(Arc::clone(&faux.provider));
    runtime
}

fn stream_fn(runtime: ModelRuntime) -> pi_agent_core::agent::types::StreamFn {
    Arc::new(move |model: Model, context: Context, options: SimpleStreamOptions| {
        let runtime = runtime.clone();
        Box::pin(async move { runtime.stream_simple(model, context, options) })
    })
}

async fn fixture() -> Fixture {
    fixture_with(Default::default(), Vec::new(), None).await
}

async fn fixture_with(
    settings: pi_coding_agent::core::settings_manager::Settings,
    base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
    initial_active_tool_names: Option<Vec<String>>,
) -> Fixture {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![faux_model()]),
        ..Default::default()
    });
    let runtime = model_runtime(&faux).await;
    let model = faux.get_model(None).expect("faux model");
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            model: Some(model),
            system_prompt: Some(String::new()),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(Vec::new()),
            messages: None,
        }),
        stream_fn: Some(stream_fn(runtime.clone())),
        ..Default::default()
    })
    .expect("agent");
    let session_manager =
        SessionManager::in_memory(Some(&temp_dir.path().display().to_string()), None).expect("in-memory session");
    let settings_manager = SettingsManager::in_memory(settings, SettingsManagerCreateOptions::default());
    let session = AgentSession::new(AgentSessionConfig {
        agent,
        session_manager: Arc::new(std::sync::Mutex::new(session_manager)),
        settings_manager: Arc::new(std::sync::Mutex::new(settings_manager)),
        cwd: temp_dir.path().display().to_string(),
        scoped_models: Vec::new(),
        resource_loader: pi_coding_agent::core::resource_loader::StaticResourceLoader::new(),
        model_runtime: runtime,
        initial_active_tool_names,
        allowed_tool_names: None,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(temp_dir.path().display().to_string()),
        control_db_path: None,
        base_tools,
        custom_tools: Vec::<ToolDefinition>::new(),
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: None,
        extension_runner_ref: None,
        session_start_event: None,
    })
    .expect("session");
    Fixture {
        session,
        faux,
        _temp_dir: temp_dir,
    }
}

fn event_types(events: &Arc<std::sync::Mutex<Vec<String>>>) -> Vec<String> {
    events.lock().unwrap().clone()
}

fn record_events(session: &AgentSession) -> Arc<std::sync::Mutex<Vec<String>>> {
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    session.subscribe(Arc::new(move |event| {
        let name = match event {
            AgentSessionEvent::Agent(agent_event) => agent_event_name(agent_event),
            AgentSessionEvent::Message(message_event) => agent_event_name(&message_event.event),
            AgentSessionEvent::AgentEnd(_) => "agent_end",
            AgentSessionEvent::QueueUpdate(_) => "queue_update",
            AgentSessionEvent::SteeringMessageQueued(_) => "steering_message_queued",
            AgentSessionEvent::AgentSettled(_) => "agent_settled",
            AgentSessionEvent::CompactionStart(_) => "compaction_start",
            AgentSessionEvent::CompactionEnd(_) => "compaction_end",
            AgentSessionEvent::EntryAppended(_) => "entry_appended",
            AgentSessionEvent::BashMessagesCommitted(_) => "bash_messages_committed",
            AgentSessionEvent::SessionInfoChanged(_) => "session_info_changed",
            AgentSessionEvent::ThinkingLevelChanged(_) => "thinking_level_changed",
            AgentSessionEvent::AutoRetryStart(_) => "auto_retry_start",
            AgentSessionEvent::AutoRetryEnd(_) => "auto_retry_end",
            AgentSessionEvent::ProviderStreamRetry(_) => "provider_stream_retry",
            AgentSessionEvent::BashExecutionUpdate(_) => "bash_execution_update",
        };
        sink.lock().unwrap().push(name.to_owned());
    }));
    events
}

fn agent_event_name(event: &pi_agent_core::agent::types::AgentEvent) -> &'static str {
    match event {
        pi_agent_core::agent::types::AgentEvent::AgentStart => "agent_start",
        pi_agent_core::agent::types::AgentEvent::AgentEnd { .. } => "agent_end",
        pi_agent_core::agent::types::AgentEvent::TurnStart => "turn_start",
        pi_agent_core::agent::types::AgentEvent::TurnEnd { .. } => "turn_end",
        pi_agent_core::agent::types::AgentEvent::ModelRequestStart => "model_request_start",
        pi_agent_core::agent::types::AgentEvent::ModelRequestEnd => "model_request_end",
        pi_agent_core::agent::types::AgentEvent::MessageStart { .. } => "message_start",
        pi_agent_core::agent::types::AgentEvent::MessageUpdate { .. } => "message_update",
        pi_agent_core::agent::types::AgentEvent::MessageEnd { .. } => "message_end",
        pi_agent_core::agent::types::AgentEvent::ToolExecutionStart { .. } => "tool_execution_start",
        pi_agent_core::agent::types::AgentEvent::ToolExecutionUpdate { .. } => "tool_execution_update",
        pi_agent_core::agent::types::AgentEvent::ToolExecutionEnd { .. } => "tool_execution_end",
    }
}

#[tokio::test]
async fn prompt_persists_user_and_assistant_messages() {
    let fixture = fixture().await;
    let events = record_events(&fixture.session);
    fixture.faux.set_responses(vec![
        faux_assistant_message("hello", FauxAssistantMessageOptions::default()).into(),
    ]);

    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt must succeed");

    let messages = fixture.session.messages();
    assert_eq!(messages.len(), 2, "user + assistant messages");
    assert!(matches!(messages[0], AgentMessage::User(_)));
    assert!(matches!(messages[1], AgentMessage::Assistant(_)));

    let entries = fixture.session.session_manager().get_entries();
    assert_eq!(entries.len(), 2, "both messages are persisted");
    assert_eq!(entries[0].entry_type(), "message");
    assert_eq!(entries[1].entry_type(), "message");

    let types = event_types(&events);
    assert_eq!(types.first().map(String::as_str), Some("agent_start"));
    assert!(types.contains(&"message_start".to_owned()));
    assert!(types.contains(&"message_end".to_owned()));
    assert_eq!(types.last().map(String::as_str), Some("agent_settled"));
}

#[tokio::test]
async fn prompt_rejects_unknown_slash_command() {
    let fixture = fixture().await;
    let error = fixture
        .session
        .prompt("/does-not-exist", PromptOptions::default())
        .await
        .expect_err("unknown slash command must fail");
    assert_eq!(error.message(), "Unknown slash command: /does-not-exist");
}

#[tokio::test]
async fn steering_and_follow_up_messages_are_tracked_and_queued() {
    let fixture = fixture().await;
    let events = record_events(&fixture.session);
    let gate = Arc::new(tokio::sync::Notify::new());
    let gate_for_stream = Arc::clone(&gate);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_stream = Arc::clone(&calls);
    fixture.faux.set_responses(vec![
        pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_context: Context,
                  _options: Option<SimpleStreamOptions>,
                  _state: pi_ai::providers::faux::FauxProviderStateSnapshot,
                  _model: Model| {
                let gate = Arc::clone(&gate_for_stream);
                let calls = Arc::clone(&calls_for_stream);
                Box::pin(async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    gate.notified().await;
                    Ok(faux_assistant_message("first", FauxAssistantMessageOptions::default()))
                })
            },
        )),
        faux_assistant_message("second", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("third", FauxAssistantMessageOptions::default()).into(),
    ]);

    let session = fixture.session.clone();
    let prompt_task = tokio::spawn(async move {
        session.prompt("start", PromptOptions::default()).await.expect("prompt");
    });
    while calls.load(Ordering::SeqCst) == 0 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    fixture.session.steer("steer me", &[]).await.expect("steer");
    fixture.session.follow_up("follow up", &[]).await.expect("follow up");
    assert_eq!(fixture.session.get_steering_messages(), vec!["steer me"]);
    assert_eq!(fixture.session.get_follow_up_messages(), vec!["follow up"]);
    gate.notify_waiters();
    prompt_task.await.expect("prompt task");

    let types = event_types(&events);
    assert!(types.contains(&"steering_message_queued".to_owned()));
    assert!(
        types.iter().filter(|name| *name == "queue_update").count() >= 2,
        "queue updates are emitted for both queues"
    );
    assert!(fixture.session.get_steering_messages().is_empty());
    assert!(fixture.session.get_follow_up_messages().is_empty());
}

#[tokio::test]
async fn prompt_while_streaming_requires_streaming_behavior() {
    let fixture = fixture().await;
    let gate = Arc::new(tokio::sync::Notify::new());
    let gate_for_stream = Arc::clone(&gate);
    fixture.faux.set_responses(vec![
        pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_context: Context,
                  _options: Option<SimpleStreamOptions>,
                  _state: pi_ai::providers::faux::FauxProviderStateSnapshot,
                  _model: Model| {
                let gate = Arc::clone(&gate_for_stream);
                Box::pin(async move {
                    gate.notified().await;
                    Ok(faux_assistant_message("done", FauxAssistantMessageOptions::default()))
                })
            },
        )),
        faux_assistant_message("queued", FauxAssistantMessageOptions::default()).into(),
    ]);
    let session = fixture.session.clone();
    let prompt_task = tokio::spawn(async move {
        session.prompt("start", PromptOptions::default()).await.expect("prompt");
    });
    while !fixture.session.is_streaming() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let error = fixture
        .session
        .prompt("second", PromptOptions::default())
        .await
        .expect_err("streaming prompt without behavior must fail");
    assert_eq!(
        error.message(),
        "Agent is already processing. Specify streamingBehavior ('steer' or 'followUp') to queue the message."
    );
    gate.notify_waiters();
    prompt_task.await.expect("prompt task");
}

#[tokio::test]
async fn clear_queue_returns_pending_messages() {
    let fixture = fixture().await;
    let gate = Arc::new(tokio::sync::Notify::new());
    let gate_for_stream = Arc::clone(&gate);
    fixture.faux.set_responses(vec![
        pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_context: Context,
                  _options: Option<SimpleStreamOptions>,
                  _state: pi_ai::providers::faux::FauxProviderStateSnapshot,
                  _model: Model| {
                let gate = Arc::clone(&gate_for_stream);
                Box::pin(async move {
                    gate.notified().await;
                    Ok(faux_assistant_message("done", FauxAssistantMessageOptions::default()))
                })
            },
        )),
        faux_assistant_message("next", FauxAssistantMessageOptions::default()).into(),
    ]);
    let session = fixture.session.clone();
    let prompt_task = tokio::spawn(async move {
        session.prompt("start", PromptOptions::default()).await.expect("prompt");
    });
    while !fixture.session.is_streaming() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    fixture.session.steer("one", &[]).await.expect("steer");
    fixture.session.follow_up("two", &[]).await.expect("follow up");
    let (steering, follow_up) = fixture.session.clear_queue();
    assert_eq!(steering, vec!["one"]);
    assert_eq!(follow_up, vec!["two"]);
    assert_eq!(fixture.session.pending_message_count(), 0);
    gate.notify_waiters();
    prompt_task.await.expect("prompt task");
}

#[tokio::test]
async fn set_thinking_level_clamps_to_model_capabilities() {
    let fixture = fixture().await;
    fixture.session.set_thinking_level(ThinkingLevel::High);
    assert_eq!(fixture.session.thinking_level(), ThinkingLevel::High);
    fixture.session.set_thinking_level(ThinkingLevel::Ultra);
    assert_eq!(fixture.session.thinking_level(), ThinkingLevel::High);
    assert!(fixture.session.supports_thinking());
}

#[tokio::test]
async fn cycle_thinking_level_wraps_available_levels() {
    let fixture = fixture().await;
    fixture.session.set_thinking_level(ThinkingLevel::Off);
    let next = fixture.session.cycle_thinking_level();
    assert_eq!(next, Some(ThinkingLevel::Minimal));
    fixture.session.set_thinking_level(ThinkingLevel::High);
    assert_eq!(
        fixture.session.cycle_thinking_level(),
        Some(ThinkingLevel::Off),
        "cycling wraps to the first available level"
    );
}

#[tokio::test]
async fn abort_clears_queues_and_reports_idle() {
    let fixture = fixture().await;
    let gate = Arc::new(tokio::sync::Notify::new());
    let gate_for_stream = Arc::clone(&gate);
    fixture
        .faux
        .set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_context: Context,
                  _options: Option<SimpleStreamOptions>,
                  _state: pi_ai::providers::faux::FauxProviderStateSnapshot,
                  _model: Model| {
                let gate = Arc::clone(&gate_for_stream);
                Box::pin(async move {
                    gate.notified().await;
                    Ok(faux_assistant_message("done", FauxAssistantMessageOptions::default()))
                })
            },
        ))]);
    let session = fixture.session.clone();
    let prompt_task = tokio::spawn(async move {
        let _ = session.prompt("start", PromptOptions::default()).await;
    });
    while !fixture.session.is_streaming() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    fixture.session.steer("queued", &[]).await.expect("steer");
    fixture.session.abort().await.expect("abort");
    assert!(!fixture.session.is_streaming());
    assert_eq!(fixture.session.pending_message_count(), 0);
    gate.notify_waiters();
    let _ = prompt_task.await;
}

#[tokio::test]
async fn set_model_updates_state_for_authenticated_model() {
    let fixture = fixture().await;
    let model = fixture.faux.get_model(None).expect("faux model");
    let mut next = model.clone();
    next.id = "faux-2".to_owned();
    fixture.session.set_model(next).await.expect("set model");
    assert_eq!(fixture.session.model().id, "faux-2");
}

#[tokio::test]
async fn session_name_round_trips_through_session_info() {
    let fixture = fixture().await;
    let events = record_events(&fixture.session);
    fixture.session.set_session_name("My session");
    assert_eq!(fixture.session.session_name().as_deref(), Some("My session"));
    assert_eq!(
        fixture.session.session_manager().get_entries()[0].entry_type(),
        "session_info"
    );
    fixture.session.clear_session_name();
    assert_eq!(fixture.session.session_name(), None);
    assert_eq!(
        event_types(&events),
        vec!["session_info_changed", "session_info_changed"]
    );
}

#[tokio::test]
async fn session_stats_count_messages_tools_and_usage() {
    let fixture = fixture().await;
    fixture.faux.set_responses(vec![
        faux_assistant_message("hello", FauxAssistantMessageOptions::default()).into(),
    ]);
    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");
    let stats = fixture.session.get_session_stats();
    assert_eq!(stats.user_messages, 1);
    assert_eq!(stats.assistant_messages, 1);
    assert_eq!(stats.tool_calls, 0);
    assert_eq!(stats.total_messages, 2);
}

#[tokio::test]
async fn get_last_assistant_text_skips_empty_aborted_messages() {
    let fixture = fixture().await;
    assert_eq!(fixture.session.get_last_assistant_text(), None);
    fixture.faux.set_responses(vec![
        faux_assistant_message("  final answer  ", FauxAssistantMessageOptions::default()).into(),
    ]);
    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");
    assert_eq!(
        fixture.session.get_last_assistant_text().as_deref(),
        Some("final answer")
    );
}

#[tokio::test]
async fn navigate_tree_moves_leaf_and_returns_editor_text() {
    let fixture = fixture().await;
    fixture.faux.set_responses(vec![
        faux_assistant_message("hello", FauxAssistantMessageOptions::default()).into(),
    ]);
    fixture
        .session
        .prompt("first", PromptOptions::default())
        .await
        .expect("prompt");
    let entries = fixture.session.session_manager().get_entries();
    let user_entry_id = entries[0].id().expect("user entry id").to_owned();
    let result = fixture
        .session
        .navigate_tree(&user_entry_id, NavigateTreeOptions::default())
        .await
        .expect("navigate");
    assert!(!result.cancelled);
    assert_eq!(result.editor_text.as_deref(), Some("first"));
    assert_eq!(fixture.session.session_manager().get_leaf_id(), None);
    assert!(fixture.session.messages().is_empty());
}

#[tokio::test]
async fn cycle_model_without_narrow_scope_returns_none() {
    let fixture = fixture().await;
    assert_eq!(
        fixture
            .session
            .cycle_model(CycleDirection::Forward)
            .await
            .expect("cycle"),
        None
    );
}

#[tokio::test]
async fn execute_bash_records_streams_and_commits() {
    use pi_coding_agent::core::agent_session::ExecuteBashOptions;

    let fixture = fixture().await;
    let updates = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&updates);
    let committed = Arc::new(AtomicUsize::new(0));
    let committed_sink = Arc::clone(&committed);
    fixture.session.subscribe(Arc::new(move |event| match event {
        AgentSessionEvent::BashExecutionUpdate(update) => sink.lock().unwrap().push(update.delta.clone()),
        AgentSessionEvent::BashMessagesCommitted(_) => {
            committed_sink.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    }));

    let result = fixture
        .session
        .execute_bash("printf 'hello'", None, ExecuteBashOptions::default())
        .await
        .expect("bash succeeds");

    assert!(result.output.contains("hello"), "output: {:?}", result.output);
    assert_eq!(result.exit_code, Some(0));
    assert!(!fixture.session.is_bash_running());
    assert!(!updates.lock().unwrap().is_empty());
    assert_eq!(committed.load(Ordering::SeqCst), 1);
    let messages = fixture.session.messages();
    assert_eq!(messages.last().expect("bash message").role(), "bashExecution");
}

#[tokio::test]
async fn record_bash_result_commits_immediately_without_message_end() {
    use pi_coding_agent::core::bash_executor::BashResult;

    let fixture = fixture().await;
    let message_end_roles = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&message_end_roles);
    fixture.session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::Agent(pi_agent_core::agent::types::AgentEvent::MessageEnd { message }) = event {
            sink.lock().unwrap().push(message.role().to_owned());
        }
    }));

    fixture.session.record_bash_result(
        "echo hi",
        &BashResult {
            output: "hi".to_owned(),
            exit_code: Some(0),
            cancelled: false,
            truncated: false,
            full_output_path: None,
        },
        None,
    );

    assert!(!fixture.session.has_pending_bash_messages());
    let messages = fixture.session.messages();
    assert_eq!(messages.last().expect("bash message").role(), "bashExecution");
    assert!(message_end_roles.lock().unwrap().is_empty());
}

struct BlockingBashOperations;

impl pi_coding_agent::core::tools::bash::BashOperations for BlockingBashOperations {
    fn exec<'a>(
        &'a self,
        _command: &'a str,
        _cwd: &'a str,
        options: pi_coding_agent::core::tools::bash::BashExecOptions,
    ) -> pi_ai::request::BoxFuture<
        'a,
        Result<pi_coding_agent::core::tools::bash::BashExecResult, pi_agent_core::agent::ToolError>,
    > {
        Box::pin(async move {
            if let Some(signal) = options.signal {
                signal.cancelled().await;
            }
            Err(pi_agent_core::agent::ToolError::new("aborted"))
        })
    }
}

#[tokio::test]
async fn abort_bash_cancels_running_execution() {
    use pi_coding_agent::core::agent_session::ExecuteBashOptions;

    let fixture = fixture().await;
    let operations = Arc::new(BlockingBashOperations);
    let session = fixture.session.clone();
    let task = tokio::spawn(async move {
        session
            .execute_bash(
                "sleep",
                None,
                ExecuteBashOptions {
                    operations: Some(operations),
                    ..Default::default()
                },
            )
            .await
            .expect("bash returns a cancelled result")
    });
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    assert!(fixture.session.is_bash_running());
    fixture.session.abort_bash();
    let result = task.await.expect("bash task");
    assert!(result.cancelled);
    assert!(!fixture.session.is_bash_running());
}

#[tokio::test]
async fn duplicate_turn_guard_marks_events_and_skips_persistence() {
    use pi_coding_agent::core::runtime_message_markers::RuntimeMessageMarker;

    let fixture = fixture().await;
    fixture.faux.set_responses(vec![
        faux_assistant_message("same", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("same", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("done", FauxAssistantMessageOptions::default()).into(),
    ]);
    let markers = Arc::new(std::sync::Mutex::new(
        Vec::<(String, Option<RuntimeMessageMarker>)>::new(),
    ));
    let sink = Arc::clone(&markers);
    fixture.session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::Message(message_event) = event {
            let name = match &message_event.event {
                pi_agent_core::agent::types::AgentEvent::MessageStart { .. } => "message_start",
                pi_agent_core::agent::types::AgentEvent::MessageEnd { .. } => "message_end",
                _ => return,
            };
            sink.lock()
                .unwrap()
                .push((name.to_owned(), message_event.runtime_message_marker));
        }
    }));

    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");
    fixture.session.continue_run().await.expect("continue");

    let markers = markers.lock().unwrap().clone();
    assert!(
        markers.contains(&(
            "message_end".to_owned(),
            Some(RuntimeMessageMarker::DuplicateTurnAssistant)
        )),
        "markers: {markers:?}"
    );
    assert!(
        markers.contains(&(
            "message_start".to_owned(),
            Some(RuntimeMessageMarker::DuplicateTurnGuard)
        )),
        "markers: {markers:?}"
    );
    let entries = fixture.session.session_manager().get_entries();
    let guard_persisted = entries.iter().any(|entry| {
        entry.as_message().is_some_and(|entry| entry.message.role() == "user")
            && serde_json::to_string(entry)
                .is_ok_and(|json| json.contains("You repeated the same response. Stop looping."))
    });
    assert!(!guard_persisted, "guard message must not be persisted");
}

#[tokio::test]
async fn normalizes_oversized_tool_result_images() {
    let fixture = fixture_with(
        Default::default(),
        vec![Arc::new(ScreenshotTool)],
        Some(vec!["screenshot".to_owned()]),
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            vec![pi_ai::providers::faux::faux_tool_call(
                "screenshot",
                serde_json::Map::new(),
                Some("call-1".to_owned()),
            )],
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::types::StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
        faux_assistant_message("done", FauxAssistantMessageOptions::default()).into(),
    ]);

    fixture
        .session
        .prompt("take a screenshot", PromptOptions::default())
        .await
        .expect("prompt");

    let image = tool_result_image(&fixture.session);
    let bytes = pi_js_compat::base64::atob(&image).expect("base64 image");
    use image::GenericImageView;
    let (width, height) = image::load_from_memory(&bytes).expect("decodable image").dimensions();
    assert!(width <= 2000 && height <= 2000, "{width}x{height}");
}

#[tokio::test]
async fn keeps_oversized_tool_result_images_when_auto_resize_is_disabled() {
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("images".to_owned(), serde_json::json!({"autoResize": false}));
    let fixture = fixture_with(
        settings,
        vec![Arc::new(ScreenshotTool)],
        Some(vec!["screenshot".to_owned()]),
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            vec![pi_ai::providers::faux::faux_tool_call(
                "screenshot",
                serde_json::Map::new(),
                Some("call-1".to_owned()),
            )],
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::types::StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
        faux_assistant_message("done", FauxAssistantMessageOptions::default()).into(),
    ]);

    fixture
        .session
        .prompt("take a screenshot", PromptOptions::default())
        .await
        .expect("prompt");

    assert_eq!(tool_result_image(&fixture.session), oversized_png_base64());
}

struct ScreenshotTool;

fn oversized_png_base64() -> String {
    image_fixtures::encode(&image_fixtures::checker_png(2400, 4800))
}

fn tool_result_image(session: &AgentSession) -> String {
    session
        .messages()
        .iter()
        .find_map(|message| match message {
            AgentMessage::ToolResult(result) => result.content.iter().find_map(|block| match block {
                pi_ai::types::TextOrImageContent::Image(image) => Some(image.data.clone()),
                pi_ai::types::TextOrImageContent::Text(_) => None,
            }),
            _ => None,
        })
        .expect("tool result image")
}

impl pi_agent_core::agent::types::AgentTool for ScreenshotTool {
    fn name(&self) -> &str {
        "screenshot"
    }

    fn description(&self) -> &str {
        "Return an oversized screenshot"
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| serde_json::json!({"type": "object", "properties": {}}))
    }

    fn label(&self) -> &str {
        "Screenshot"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: serde_json::Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: pi_agent_core::agent::types::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<pi_agent_core::agent::types::AgentToolResult, pi_agent_core::agent::types::ToolError>,
    > {
        Box::pin(async move {
            let mut result = pi_agent_core::agent::types::AgentToolResult::text("captured");
            result.content.push(pi_ai::types::TextOrImageContent::Image(
                pi_ai::types::ImageContent::new(oversized_png_base64(), "image/png"),
            ));
            Ok(result)
        })
    }
}

#[tokio::test]
async fn prompt_with_streaming_steer_queues_instead_of_failing() {
    let fixture = fixture().await;
    let gate = Arc::new(tokio::sync::Notify::new());
    let gate_for_stream = Arc::clone(&gate);
    fixture.faux.set_responses(vec![
        pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_context: Context,
                  _options: Option<SimpleStreamOptions>,
                  _state: pi_ai::providers::faux::FauxProviderStateSnapshot,
                  _model: Model| {
                let gate = Arc::clone(&gate_for_stream);
                Box::pin(async move {
                    gate.notified().await;
                    Ok(faux_assistant_message("done", FauxAssistantMessageOptions::default()))
                })
            },
        )),
        faux_assistant_message("steered", FauxAssistantMessageOptions::default()).into(),
    ]);
    let session = fixture.session.clone();
    let prompt_task = tokio::spawn(async move {
        session.prompt("start", PromptOptions::default()).await.expect("prompt");
    });
    while !fixture.session.is_streaming() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    fixture
        .session
        .prompt(
            "steered",
            PromptOptions {
                streaming_behavior: Some(StreamingBehavior::Steer),
                ..Default::default()
            },
        )
        .await
        .expect("steer prompt");
    assert_eq!(fixture.session.get_steering_messages(), vec!["steered"]);
    gate.notify_waiters();
    prompt_task.await.expect("prompt task");
    assert!(fixture.session.get_steering_messages().is_empty());
}
