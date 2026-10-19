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
    fixture_with(Default::default(), Vec::new(), None, Some(Vec::new())).await
}

async fn fixture_with(
    settings: pi_coding_agent::core::settings_manager::Settings,
    base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
    initial_active_tool_names: Option<Vec<String>>,
    allowed_tool_names: Option<Vec<String>>,
) -> Fixture {
    fixture_with_multi_agent(
        settings,
        base_tools,
        initial_active_tool_names,
        allowed_tool_names,
        None,
        false,
    )
    .await
}

async fn fixture_with_multi_agent(
    settings: pi_coding_agent::core::settings_manager::Settings,
    base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
    initial_active_tool_names: Option<Vec<String>>,
    allowed_tool_names: Option<Vec<String>>,
    multi_agent: Option<(Arc<pi_coding_agent::core::multi_agent_store::MultiAgentStore>, String)>,
    persist_session: bool,
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
    let cwd = temp_dir.path().display().to_string();
    let session_manager = if persist_session {
        let session_dir = temp_dir.path().join("sessions").display().to_string();
        SessionManager::create(&cwd, Some(&session_dir), None).expect("persisted session")
    } else {
        SessionManager::in_memory(Some(&cwd), None).expect("in-memory session")
    };
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
        allowed_tool_names,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(temp_dir.path().display().to_string()),
        control_db_path: Some(temp_dir.path().join("control.db").display().to_string()),
        base_tools,
        custom_tool_definitions: Vec::new(),
        custom_tools: Vec::<ToolDefinition>::new(),
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: None,
        supervisor_decision_requester: None,
        multi_agent_store: multi_agent.as_ref().map(|(store, _)| Arc::clone(store)),
        multi_agent_agent_id: multi_agent.as_ref().map(|(_, agent_id)| agent_id.clone()),
        multi_agent_parent_session_id: None,
        disable_runtime_coordination_inbound: false,
        on_shared_channel_message_delivered: None,
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

#[tokio::test]
async fn default_session_registers_every_builtin_tool() {
    let fixture = fixture_with(Default::default(), Vec::new(), None, None).await;
    let names: Vec<String> = fixture
        .session
        .get_all_tools()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    assert_eq!(names.len(), 19);
    assert!(names.contains(&"read".to_owned()));
    assert!(names.contains(&"end_turn".to_owned()));
    assert_eq!(
        fixture.session.get_active_tool_names(),
        pi_coding_agent::core::tools::DEFAULT_ACTIVE_TOOL_NAMES
            .iter()
            .map(|name| name.as_str().to_owned())
            .collect::<Vec<_>>()
    );
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
        faux_assistant_message(
            vec![
                pi_ai::providers::faux::faux_text("hello"),
                pi_ai::providers::faux::faux_tool_call(
                    "end_turn",
                    serde_json::Map::from_iter([("reason".to_owned(), serde_json::json!("done"))]),
                    Some("call-end".to_owned()),
                ),
            ],
            FauxAssistantMessageOptions::default(),
        )
        .into(),
    ]);

    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt must succeed");

    let messages = fixture.session.messages();
    assert_eq!(messages.len(), 3, "user + assistant + end_turn result");
    assert!(matches!(messages[0], AgentMessage::User(_)));
    assert!(matches!(messages[1], AgentMessage::Assistant(_)));

    let entries = fixture.session.session_manager().get_entries();
    assert_eq!(entries.len(), 3, "every message is persisted");
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
    gate.notify_one();
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
    gate.notify_one();
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
    gate.notify_one();
    prompt_task.await.expect("prompt task");
}

#[tokio::test]
async fn set_thinking_level_clamps_to_model_capabilities() {
    let fixture = fixture().await;
    fixture
        .session
        .set_thinking_level(ThinkingLevel::High)
        .expect("set thinking level");
    assert_eq!(fixture.session.thinking_level(), ThinkingLevel::High);
    fixture
        .session
        .set_thinking_level(ThinkingLevel::Ultra)
        .expect("set thinking level");
    assert_eq!(fixture.session.thinking_level(), ThinkingLevel::High);
    assert!(fixture.session.supports_thinking());
}

#[tokio::test]
async fn cycle_thinking_level_wraps_available_levels() {
    let fixture = fixture().await;
    fixture
        .session
        .set_thinking_level(ThinkingLevel::Off)
        .expect("set thinking level");
    let next = fixture.session.cycle_thinking_level().expect("cycle thinking level");
    assert_eq!(next, Some(ThinkingLevel::Minimal));
    fixture
        .session
        .set_thinking_level(ThinkingLevel::High)
        .expect("set thinking level");
    assert_eq!(
        fixture.session.cycle_thinking_level().expect("cycle thinking level"),
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
    gate.notify_one();
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
        faux_assistant_message(
            vec![
                pi_ai::providers::faux::faux_text("hello"),
                pi_ai::providers::faux::faux_tool_call(
                    "end_turn",
                    serde_json::Map::from_iter([("reason".to_owned(), serde_json::json!("done"))]),
                    Some("call-end".to_owned()),
                ),
            ],
            FauxAssistantMessageOptions::default(),
        )
        .into(),
    ]);
    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");
    let stats = fixture.session.get_session_stats();
    assert_eq!(stats.user_messages, 1);
    assert_eq!(stats.assistant_messages, 1);
    assert_eq!(stats.tool_calls, 1);
    assert_eq!(stats.total_messages, 3);
}

#[tokio::test]
async fn get_last_assistant_text_skips_empty_aborted_messages() {
    let fixture = fixture().await;
    assert_eq!(fixture.session.get_last_assistant_text(), None);
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            vec![
                pi_ai::providers::faux::faux_text("  final answer  "),
                pi_ai::providers::faux::faux_tool_call(
                    "end_turn",
                    serde_json::Map::from_iter([("reason".to_owned(), serde_json::json!("done"))]),
                    Some("call-end".to_owned()),
                ),
            ],
            FauxAssistantMessageOptions::default(),
        )
        .into(),
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
async fn navigate_tree_summarizes_the_abandoned_branch_and_attaches_the_summary() {
    use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry};

    let fixture = fixture().await;
    fixture.faux.set_responses(vec![
        faux_assistant_message("hello", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("hi", FauxAssistantMessageOptions::default()).into(),
    ]);
    fixture
        .session
        .prompt("first", PromptOptions::default())
        .await
        .expect("first prompt");
    fixture
        .session
        .prompt("second", PromptOptions::default())
        .await
        .expect("second prompt");
    let first_user_entry = fixture.session.session_manager().get_entries()[0]
        .id()
        .expect("first user entry")
        .to_owned();

    fixture.faux.set_responses(vec![
        faux_assistant_message("BRANCH SUMMARY", FauxAssistantMessageOptions::default()).into(),
    ]);
    let result = fixture
        .session
        .navigate_tree(
            &first_user_entry,
            NavigateTreeOptions {
                summarize: Some(true),
                label: Some("abandoned".to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("navigate with summary");
    assert!(!result.cancelled);
    assert_eq!(result.editor_text.as_deref(), Some("first"));
    let summary_entry = result.summary_entry.expect("branch summary entry");
    let SessionEntry::Known(KnownSessionEntry::BranchSummary(summary)) = &summary_entry else {
        panic!("expected a branch summary entry");
    };
    assert!(summary.summary.contains("BRANCH SUMMARY"));
    assert_eq!(summary.parent_id, None);
    assert_eq!(summary.from_id, "root");
    assert_eq!(summary.from_hook, Some(false));
    let manager = fixture.session.session_manager();
    assert_eq!(manager.get_label(&summary.id), Some("abandoned"));
    let leaf = manager.get_leaf_entry().expect("label entry leaf");
    assert_eq!(leaf.entry_type(), "label");
    assert_eq!(leaf.id(), manager.get_leaf_id());
    assert_eq!(
        fixture.session.messages().len(),
        1,
        "context is the summary at the root"
    );
}

struct EchoTool;

impl pi_agent_core::agent::types::AgentTool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo text"
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| json!({"type": "object", "properties": {"text": {"type": "string"}}}))
    }

    fn label(&self) -> &str {
        "echo"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: serde_json::Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: pi_agent_core::agent::types::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<pi_agent_core::agent::types::AgentToolResult, pi_agent_core::agent::types::ToolError>,
    > {
        Box::pin(async move {
            let text = params
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Ok(pi_agent_core::agent::types::AgentToolResult::text(text))
        })
    }
}

fn running_agent(id: &str) -> pi_coding_agent::core::multi_agent_store::AgentSnapshot {
    use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, AgentPermission, AgentSnapshot};
    AgentSnapshot {
        id: id.to_owned(),
        parent_id: None,
        display_name: "Child".to_owned(),
        agent_type: "worker".to_owned(),
        origin: None,
        lifecycle: AgentLifecycle::Running,
        revision: 1,
        created_at: "2026-06-21T00:00:00.000Z".to_owned(),
        updated_at: "2026-06-21T00:00:00.000Z".to_owned(),
        cwd: "/repo".to_owned(),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: "on-request".to_owned(),
            inherited_from: None,
            narrowed: true,
        },
        slot: None,
        transcript: None,
        event_stream: None,
        worker: None,
        detached: None,
        current_activity: None,
        last_activity: None,
        result: None,
        error: None,
        extra: serde_json::Map::new(),
    }
}

#[tokio::test]
async fn publishes_multi_agent_activity_across_agent_and_tool_events() {
    use pi_coding_agent::core::multi_agent_store::{AgentCurrentActivity, MultiAgentStore, MultiAgentStoreOptions};

    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| "2026-06-21T00:00:00.000Z".to_owned())),
    }));
    store.publish_lifecycle_coordinator_snapshot(&running_agent("child-agent"));
    let activities = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&activities);
    let _subscription = store.subscribe_agent_updates(move |_previous, current| {
        recorded.lock().unwrap().push(current.current_activity.clone());
    });

    let fixture = fixture_with_multi_agent(
        Default::default(),
        vec![Arc::new(EchoTool)],
        Some(vec!["echo".to_owned()]),
        None,
        Some((Arc::clone(&store), "child-agent".to_owned())),
        false,
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            vec![
                pi_ai::providers::faux::faux_text("working"),
                pi_ai::providers::faux::faux_tool_call(
                    "echo",
                    json!({"text": "hi"}).as_object().cloned().expect("arguments"),
                    Some("call-1".to_owned()),
                ),
            ],
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
        .prompt("run", PromptOptions::default())
        .await
        .expect("prompt");

    let activities = activities.lock().unwrap().clone();
    assert!(
        matches!(activities.first(), Some(Some(AgentCurrentActivity::Thinking { .. }))),
        "agent_start publishes thinking: {activities:?}"
    );
    assert!(
        activities.iter().any(|activity| matches!(
            activity,
            Some(AgentCurrentActivity::Tool { tool_name, .. }) if tool_name == "echo"
        )),
        "tool activity must be published: {activities:?}"
    );
    assert_eq!(activities.last(), Some(&None), "agent_end clears activity");
}

#[tokio::test]
async fn binds_extension_relocation_handlers_on_the_session() {
    let fixture = fixture().await;
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&calls);
    let relocate = Arc::new(move |cwd: &str| {
        recorded.lock().unwrap().push(format!("relocate:{cwd}"));
        Box::pin(async { Ok(()) })
            as pi_ai::request::BoxFuture<'static, Result<(), pi_coding_agent::core::extensions::types::HandlerError>>
    });
    let recorded = Arc::clone(&calls);
    let prepare = Arc::new(move |cwd: &str| {
        recorded.lock().unwrap().push(format!("prepare:{cwd}"));
        Box::pin(async { Ok(()) })
            as pi_ai::request::BoxFuture<'static, Result<(), pi_coding_agent::core::extensions::types::HandlerError>>
    });
    let recorded = Arc::clone(&calls);
    let activate = Arc::new(move || {
        recorded.lock().unwrap().push("activate".to_owned());
        Box::pin(async {}) as pi_ai::request::BoxFuture<'static, ()>
    });
    fixture
        .session
        .bind_session_runtime_relocation(relocate, prepare, activate);

    let runner = fixture.session.extension_runner();
    runner
        .create_context()
        .relocate_after_tool_result("call-1", "/next")
        .expect("schedule relocation");
    runner
        .deliver_tool_result_relocation("call-1")
        .await
        .expect("deliver relocation");
    assert!(runner.has_prepared_tool_result_relocation());
    assert_eq!(calls.lock().unwrap().as_slice(), ["prepare:/next"]);
    runner.activate_tool_result_relocation().await;
    assert_eq!(calls.lock().unwrap().as_slice(), ["prepare:/next", "activate"]);
}

#[tokio::test]
async fn runtime_mailbox_delivery_prompts_the_session_and_marks_the_message_delivered() {
    use pi_coding_agent::core::session_control_db::{
        EnqueueStoredRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind,
        RuntimeMailboxMessageStatus, RuntimeMailboxStoreRef, enqueue_stored_runtime_mailbox_message,
        list_runtime_mailbox_messages,
    };

    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::<Arc<dyn pi_agent_core::agent::types::AgentTool>>::new(),
        None,
        None,
        None,
        true,
    )
    .await;
    let session_file = fixture.session.session_file().expect("session file");
    let control_db_path = fixture.session.control_db_path().expect("control db");
    let session_id = fixture.session.session_id();
    let store_ref = RuntimeMailboxStoreRef {
        message_id: "mailbox-1".to_owned(),
        session_path: session_file,
    };
    enqueue_stored_runtime_mailbox_message(
        &control_db_path,
        &EnqueueStoredRuntimeMailboxMessageInput {
            recipient: RuntimeMailboxAddress {
                agent_id: None,
                session_id,
            },
            sender: RuntimeMailboxAddress {
                agent_id: Some("parent".to_owned()),
                session_id: "parent-session".to_owned(),
            },
            kind: RuntimeMailboxMessageKind::Message,
            store_ref,
            message: json!({
                "id": "mailbox-1",
                "fromAgentId": "parent",
                "kind": "message",
                "status": "pending",
                "createdAt": "2026-06-21T00:00:00.000Z",
                "updatedAt": "2026-06-21T00:00:00.000Z",
                "body": "hello from parent",
            }),
            updated_at: None,
        },
    )
    .expect("enqueue mailbox message");
    fixture.faux.set_responses(vec![
        faux_assistant_message("ack", FauxAssistantMessageOptions::default()).into(),
    ]);
    fixture
        .session
        .model_runtime()
        .refresh(Default::default())
        .await
        .expect("refresh configured providers");

    fixture
        .session
        .drain_runtime_coordination()
        .await
        .expect("drain coordination");

    let messages = fixture.session.messages();
    let user_text = messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::User(user) => Some(match &user.content {
                pi_ai::types::UserContent::Text(text) => text.clone(),
                pi_ai::types::UserContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|block| match block {
                        pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.as_str()),
                        pi_ai::types::TextOrImageContent::Image(_) => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            }),
            _ => None,
        })
        .expect("delivered mailbox prompt");
    assert!(user_text.contains("From:"), "{user_text}");
    assert!(user_text.contains("- session: parent-session"), "{user_text}");
    assert!(user_text.contains("hello from parent"), "{user_text}");

    let rows = list_runtime_mailbox_messages(&control_db_path).expect("mailbox rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, RuntimeMailboxMessageStatus::Delivered);
}

#[tokio::test]
async fn shared_channel_delivery_formats_and_queues_messages_after_the_initial_cursor() {
    use pi_coding_agent::core::session_control_db::{
        PostSharedChannelMessageInput, RuntimeMailboxAddress, post_shared_channel_message,
    };

    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::<Arc<dyn pi_agent_core::agent::types::AgentTool>>::new(),
        None,
        None,
        None,
        true,
    )
    .await;
    let control_db_path = fixture.session.control_db_path().expect("control db");
    fixture
        .session
        .model_runtime()
        .refresh(Default::default())
        .await
        .expect("refresh configured providers");
    fixture
        .session
        .drain_runtime_coordination()
        .await
        .expect("initialize cursor");
    post_shared_channel_message(
        &control_db_path,
        &PostSharedChannelMessageInput {
            sender: RuntimeMailboxAddress {
                agent_id: None,
                session_id: "other-session".to_owned(),
            },
            body: "hello channel".to_owned(),
        },
    )
    .expect("post channel message");
    fixture.faux.set_responses(vec![
        faux_assistant_message("channel ack", FauxAssistantMessageOptions::default()).into(),
    ]);
    fixture
        .session
        .drain_runtime_coordination()
        .await
        .expect("drain shared channel");

    let messages = fixture.session.messages();
    let custom = messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::Custom(object) => Some(object.clone()),
            _ => None,
        })
        .expect("shared channel custom message");
    assert_eq!(
        custom.get("customType").and_then(serde_json::Value::as_str),
        Some("shared_channel")
    );
    let content = custom
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(content.contains("From shared channel:"), "{content}");
    assert!(content.contains("- session: other-session"), "{content}");
    assert!(content.contains("hello channel"), "{content}");
    assert_eq!(custom.get("display").and_then(serde_json::Value::as_bool), Some(true));
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
        None,
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
        None,
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

struct PermissionEchoTool {
    executed: Arc<std::sync::Mutex<Vec<String>>>,
}

impl pi_agent_core::agent::types::AgentTool for PermissionEchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo text back"
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}))
    }

    fn label(&self) -> &str {
        "Echo"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: serde_json::Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: pi_agent_core::agent::types::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<pi_agent_core::agent::types::AgentToolResult, pi_agent_core::agent::types::ToolError>,
    > {
        let text = params
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        self.executed.lock().unwrap().push(text.clone());
        Box::pin(async move { Ok(pi_agent_core::agent::types::AgentToolResult::text(text)) })
    }
}

struct ApprovalPromptTool {
    name: String,
    calls: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    response: serde_json::Value,
}

impl pi_agent_core::agent::types::AgentTool for ApprovalPromptTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Approve tool calls"
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| {
            json!({
                "type":"object",
                "properties":{
                    "tool_name":{"type":"string"},
                    "input":{},
                    "tool_use_id":{"type":"string"},
                    "cwd":{"type":"string"}
                },
                "required":["tool_name","input","tool_use_id","cwd"]
            })
        })
    }

    fn label(&self) -> &str {
        "Approval"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: serde_json::Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: pi_agent_core::agent::types::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<pi_agent_core::agent::types::AgentToolResult, pi_agent_core::agent::types::ToolError>,
    > {
        self.calls.lock().unwrap().push(params);
        let text = self.response.to_string();
        Box::pin(async move {
            let mut result = pi_agent_core::agent::types::AgentToolResult::text(text);
            result.is_error = Some(false);
            Ok(result)
        })
    }
}

fn permission_tool_call(text: &str) -> pi_ai::providers::faux::FauxContent {
    pi_ai::providers::faux::FauxContent::Blocks(vec![pi_ai::providers::faux::faux_tool_call(
        "echo",
        [("text".to_owned(), json!(text))].into_iter().collect(),
        Some("call-1".to_owned()),
    )])
}

async fn permission_fixture(
    settings: pi_coding_agent::core::settings_manager::Settings,
    tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
) -> Fixture {
    let fixture = fixture_with(
        settings,
        tools,
        Some(vec![
            "echo".to_owned(),
            "mcp__approval__prompt".to_owned(),
            "mcp__other__approval_prompt".to_owned(),
        ]),
        None,
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            permission_tool_call("original"),
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::types::StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
        faux_assistant_message("done", FauxAssistantMessageOptions::default()).into(),
    ]);
    fixture
}

#[tokio::test]
async fn permission_prompt_tool_updates_input_before_execution() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let approval = Arc::new(ApprovalPromptTool {
        name: "mcp__approval__prompt".to_owned(),
        calls: Arc::clone(&calls),
        response: json!({"behavior":"allow","updatedInput":{"text":"approved"}}),
    });
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("permissionPromptTool".to_owned(), json!("mcp__approval__prompt"));
    let fixture = permission_fixture(
        settings,
        vec![
            Arc::new(PermissionEchoTool {
                executed: Arc::clone(&executed),
            }),
            approval,
        ],
    )
    .await;

    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");

    assert_eq!(executed.lock().unwrap().as_slice(), ["approved".to_owned()]);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["tool_name"], json!("echo"));
    assert_eq!(calls[0]["input"]["text"], json!("original"));
}

#[tokio::test]
async fn discovers_single_permission_prompt_protocol_tool() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let approval = Arc::new(ApprovalPromptTool {
        name: "mcp__project-approval__approval_prompt".to_owned(),
        calls: Arc::clone(&calls),
        response: json!({"behavior":"allow","updatedInput":{"text":"hook-approved"}}),
    });
    let fixture = fixture_with(
        Default::default(),
        vec![
            Arc::new(PermissionEchoTool {
                executed: Arc::clone(&executed),
            }),
            approval,
        ],
        Some(vec![
            "echo".to_owned(),
            "mcp__project-approval__approval_prompt".to_owned(),
        ]),
        None,
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            permission_tool_call("original"),
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
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");

    assert_eq!(executed.lock().unwrap().as_slice(), ["hook-approved".to_owned()]);
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn ambiguous_permission_prompt_tools_fall_back_to_native_approval() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let first_calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let second_calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let fixture = fixture_with(
        Default::default(),
        vec![
            Arc::new(PermissionEchoTool {
                executed: Arc::clone(&executed),
            }),
            Arc::new(ApprovalPromptTool {
                name: "mcp__claude-bash-hook-approval__approval_prompt".to_owned(),
                calls: Arc::clone(&first_calls),
                response: json!({"behavior":"deny","message":"should not run"}),
            }),
            Arc::new(ApprovalPromptTool {
                name: "mcp__other-approval__approval_prompt".to_owned(),
                calls: Arc::clone(&second_calls),
                response: json!({"behavior":"deny","message":"should not run"}),
            }),
        ],
        Some(vec![
            "echo".to_owned(),
            "mcp__claude-bash-hook-approval__approval_prompt".to_owned(),
            "mcp__other-approval__approval_prompt".to_owned(),
        ]),
        None,
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            permission_tool_call("original"),
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
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");

    assert_eq!(executed.lock().unwrap().as_slice(), ["original".to_owned()]);
    assert!(first_calls.lock().unwrap().is_empty());
    assert!(second_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn denied_permission_prompt_blocks_tool_execution() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("permissionPromptTool".to_owned(), json!("mcp__approval__prompt"));
    let fixture = permission_fixture(
        settings,
        vec![
            Arc::new(PermissionEchoTool {
                executed: Arc::clone(&executed),
            }),
            Arc::new(ApprovalPromptTool {
                name: "mcp__approval__prompt".to_owned(),
                calls: Arc::clone(&calls),
                response: json!({"behavior":"deny","message":"blocked by policy"}),
            }),
        ],
    )
    .await;

    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");

    assert!(executed.lock().unwrap().is_empty());
    let blocked = fixture.session.messages().iter().any(|message| {
        match message {
        AgentMessage::ToolResult(result) => {
            result.is_error
                && result.content.iter().any(|block| {
                    matches!(block, pi_ai::types::TextOrImageContent::Text(text) if text.text == "blocked by policy")
                })
        }
        _ => false,
    }
    });
    assert!(blocked, "expected an error tool result with the denial reason");
}

#[tokio::test]
async fn persisted_allow_rule_skips_permission_prompt_tool() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("permissionPromptTool".to_owned(), json!("mcp__approval__prompt"));
    settings.insert(
        "permissionRules".to_owned(),
        json!({"allow":{"echo":[json!({"text":"original"}).to_string()]}}),
    );
    let fixture = permission_fixture(
        settings,
        vec![
            Arc::new(PermissionEchoTool {
                executed: Arc::clone(&executed),
            }),
            Arc::new(ApprovalPromptTool {
                name: "mcp__approval__prompt".to_owned(),
                calls: Arc::clone(&calls),
                response: json!({"behavior":"deny","message":"should not run"}),
            }),
        ],
    )
    .await;

    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("prompt");

    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(executed.lock().unwrap().as_slice(), ["original".to_owned()]);
}

#[tokio::test]
async fn binds_detached_job_lifecycle_when_a_store_is_configured() {
    use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};

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
    let cwd = temp_dir.path().display().to_string();
    let session_dir = temp_dir.path().join("sessions").display().to_string();
    let control_db = temp_dir.path().join("control.db").display().to_string();
    let mut session_manager = SessionManager::create(&cwd, Some(&session_dir), None).expect("session manager");
    session_manager
        .set_metadata_control_db_path(Some(&control_db), Default::default())
        .expect("metadata control db");
    session_manager
        .append_message(AgentMessage::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text("hello".to_owned()),
            input_source: None,
            provider_native: None,
            timestamp: 1_700_000_000_000,
        }))
        .expect("append");
    session_manager.persist_for_recovery().expect("persist");
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&session_manager));

    let settings_manager = SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default());
    let session = AgentSession::new(AgentSessionConfig {
        agent,
        session_manager: Arc::new(std::sync::Mutex::new(session_manager)),
        settings_manager: Arc::new(std::sync::Mutex::new(settings_manager)),
        cwd: cwd.clone(),
        scoped_models: Vec::new(),
        resource_loader: pi_coding_agent::core::resource_loader::StaticResourceLoader::new(),
        model_runtime: runtime,
        initial_active_tool_names: None,
        allowed_tool_names: None,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(cwd),
        control_db_path: Some(control_db),
        base_tools: Vec::new(),
        custom_tool_definitions: Vec::new(),
        custom_tools: Vec::<ToolDefinition>::new(),
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: None,
        supervisor_decision_requester: None,
        multi_agent_store: Some(store),
        multi_agent_agent_id: None,
        multi_agent_parent_session_id: None,
        disable_runtime_coordination_inbound: false,
        on_shared_channel_message_delivered: None,
        extension_runner_ref: None,
        session_start_event: None,
    })
    .expect("session");

    let lifecycle = session.detached_job_lifecycle();
    assert!(lifecycle.is_some(), "expected a bound detached-job lifecycle");
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
    gate.notify_one();
    prompt_task.await.expect("prompt task");
    assert!(fixture.session.get_steering_messages().is_empty());
}

#[tokio::test]
async fn bash_tool_exposes_live_session_environment_variables() {
    let fixture = fixture_with_multi_agent(Default::default(), Vec::new(), None, None, None, true).await;
    let (session_id, session_file) = {
        let manager = fixture.session.session_manager();
        (
            manager.get_session_id().to_owned(),
            manager.get_session_file().map(str::to_owned),
        )
    };
    let definition = fixture.session.get_tool_definition("bash").expect("bash definition");
    let result = definition
        .tool
        .execute(
            "call-1".to_owned(),
            json!({
                "command": "printf '%s|%s|%s|%s|%s' \"$PI_SESSION_ID\" \"$PI_SESSION_FILE\" \"$PI_PROVIDER\" \"$PI_MODEL\" \"$PI_REASONING_LEVEL\""
            }),
            None,
            Arc::new(|_result| {}),
            None,
        )
        .await
        .expect("bash execution");
    let text = result
        .content
        .iter()
        .filter_map(|entry| match entry {
            pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
            pi_ai::types::TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        text,
        format!(
            "{session_id}|{}|faux|faux-1|off",
            session_file.expect("persisted session file")
        )
    );
}

#[tokio::test]
async fn update_compaction_summary_returns_the_persistence_error() {
    use pi_ai::types::{UserContent, UserMessage, UserRole};

    let fixture = fixture_with_multi_agent(Default::default(), Vec::new(), None, None, None, true).await;
    let session = &fixture.session;
    let (compaction_id, session_file) = {
        let mut manager = session.session_manager();
        let first_kept = manager
            .append_message(AgentMessage::User(UserMessage {
                role: UserRole::Tag,
                content: UserContent::Text("kept".to_owned()),
                input_source: None,
                provider_native: None,
                timestamp: 1,
            }))
            .expect("user message");
        let id = manager
            .append_compaction(
                "original summary",
                &first_kept,
                42_000.0,
                pi_coding_agent::core::session_manager::CompactionAppendOptions {
                    from_hook: Some(true),
                    duration_ms: Some(321.0),
                    ..Default::default()
                },
            )
            .expect("compaction");
        (
            id,
            manager.get_session_file().expect("persisted session file").to_owned(),
        )
    };
    if std::path::Path::new(&session_file).exists() {
        std::fs::remove_file(&session_file).expect("remove session file");
    }
    std::fs::create_dir(&session_file).expect("replace with directory");

    let before = session.messages();
    let error = session
        .update_compaction_summary(&compaction_id, "edited summary")
        .expect_err("persistence failure must surface");
    assert!(!error.message().is_empty());
    assert_eq!(session.messages(), before);
    let compaction = session
        .session_manager()
        .get_entry(&compaction_id)
        .and_then(|entry| entry.as_compaction())
        .cloned()
        .expect("compaction entry");
    assert_eq!(compaction.summary, "original summary");
}
