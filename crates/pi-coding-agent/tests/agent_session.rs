//! Behavioral tests for `core/agent-session.ts`.

#[path = "support/image_fixtures.rs"]
mod image_fixtures;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
    AgentSession, AgentSessionConfig, AgentSessionEvent, CycleDirection, ExtensionBindings, MultiAgentRuntimeRole,
    NavigateTreeOptions, PromptOptions, StreamingBehavior, SupervisorDecisionRequester, ToolDefinition,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{ExtensionMode, ExtensionUiContext, ExtensionUiDialogOptions};
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::core::tool_detach_registry::ToolDetachHandle;
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

#[tokio::test]
async fn jsonl_export_preserves_javascript_value_bytes() {
    let fixture = fixture().await;
    let value = serde_json::from_str(r#"{"2":2.0,"1":1.0,"x":9007199254740993}"#).unwrap();
    fixture
        .session
        .session_manager()
        .append_custom_entry("numeric", Some(value))
        .unwrap();
    let path = fixture._temp_dir.path().join("export.jsonl");
    fixture.session.export_to_jsonl(Some(path.to_str().unwrap())).unwrap();
    let bytes = std::fs::read_to_string(path).unwrap();
    assert!(
        bytes.contains(r#""data":{"1":1,"2":2,"x":9007199254740992}"#),
        "{bytes}"
    );
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
        FixtureOverrides::default(),
    )
    .await
}

#[derive(Default)]
struct FixtureOverrides {
    multi_agent: Option<(Arc<pi_coding_agent::core::multi_agent_store::MultiAgentStore>, String)>,
    persist_session: bool,
    supervisor_decision_requester: Option<pi_coding_agent::core::agent_session::SupervisorDecisionRequester>,
    thinking_phase_timeout_ms: Option<u64>,
    multi_agent_runtime_role: Option<pi_coding_agent::core::agent_session::MultiAgentRuntimeRole>,
    custom_tool_definitions: Vec<pi_coding_agent::core::extensions::types::ToolDefinition>,
    extensions: Option<pi_coding_agent::core::extensions::types::LoadExtensionsResult>,
}

async fn fixture_with_multi_agent(
    settings: pi_coding_agent::core::settings_manager::Settings,
    base_tools: Vec<Arc<dyn pi_agent_core::agent::types::AgentTool>>,
    initial_active_tool_names: Option<Vec<String>>,
    allowed_tool_names: Option<Vec<String>>,
    overrides: FixtureOverrides,
) -> Fixture {
    let FixtureOverrides {
        multi_agent,
        persist_session,
        supervisor_decision_requester,
        thinking_phase_timeout_ms,
        multi_agent_runtime_role,
        custom_tool_definitions,
        extensions,
    } = overrides;
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
        resource_loader: Arc::new(pi_coding_agent::core::resource_loader::StaticResourceLoader {
            extensions,
            ..Default::default()
        }),
        model_runtime: runtime,
        initial_active_tool_names,
        allowed_tool_names,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(temp_dir.path().display().to_string()),
        control_db_path: Some(temp_dir.path().join("control.db").display().to_string()),
        base_tools,
        custom_tool_definitions,
        custom_tools: Vec::<ToolDefinition>::new(),
        multi_agent_runtime_role,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms,
        supervisor_decision_requester,
        multi_agent_store: multi_agent.as_ref().map(|(store, _)| Arc::clone(store)),
        multi_agent_agent_id: multi_agent.as_ref().map(|(_, agent_id)| agent_id.clone()),
        multi_agent_parent_session_id: None,
        multi_agent_requires_agent_id: multi_agent.is_some(),
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

fn retry_completion_response() -> pi_ai::types::AssistantMessage {
    faux_assistant_message(
        vec![pi_ai::providers::faux::faux_tool_call(
            "end_turn",
            serde_json::Map::from_iter([("reason".to_owned(), json!("done"))]),
            Some("retry-complete".to_owned()),
        )],
        FauxAssistantMessageOptions::default(),
    )
}

#[tokio::test]
async fn repin_settings_retry_delay_grows_with_attempt() {
    let settings = serde_json::from_value(json!({
        "retry": {"enabled": true, "maxRetries": 3, "baseDelayMs": 10,
                  "maxBackoffMs": 80, "provider": {"maxRetries": 0}}
    }))
    .unwrap();
    let fixture = fixture_with(settings, Vec::new(), None, Some(Vec::new())).await;
    let delays = Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = Arc::clone(&delays);
    fixture.session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::AutoRetryStart(retry) = event {
            observed.lock().unwrap().push(retry.delay_ms);
        }
    }));
    let retry_error = faux_assistant_message(
        "",
        FauxAssistantMessageOptions {
            stop_reason: Some(pi_ai::types::StopReason::Error),
            error_message: Some("overloaded_error: server overloaded".to_owned()),
            ..Default::default()
        },
    );
    fixture.faux.set_responses(vec![
        retry_error.clone().into(),
        retry_error.into(),
        retry_completion_response().into(),
    ]);
    fixture.session.prompt("retry", PromptOptions::default()).await.unwrap();
    let delays = delays.lock().unwrap();
    assert_eq!(delays.len(), 2, "two session retries must occur");
    assert!((8.0..=12.0).contains(&delays[0]), "first delay: {}", delays[0]);
    assert!((16.0..=24.0).contains(&delays[1]), "second delay: {}", delays[1]);
}

#[tokio::test]
async fn repin_settings_invalid_compaction_threshold_rejects_before_request() {
    let settings = serde_json::from_value(json!({"compaction": {"thresholdPercent": 101}})).unwrap();
    let fixture = fixture_with(settings, Vec::new(), None, Some(Vec::new())).await;
    fixture.faux.set_responses(vec![retry_completion_response().into()]);
    let error = fixture
        .session
        .prompt("invalid settings", PromptOptions::default())
        .await
        .expect_err("invalid compaction setting must fail before a provider request");
    assert!(
        error
            .to_string()
            .contains("Invalid compaction.thresholdPercent setting"),
        "{error}"
    );
    assert_eq!(fixture.faux.get_pending_response_count(), 1, "provider was not called");
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
async fn session_name_round_trips_through_session_metadata_state() {
    let fixture = fixture().await;
    let events = record_events(&fixture.session);
    fixture.session.set_session_name("My session").expect("set name");
    assert_eq!(fixture.session.session_name().as_deref(), Some("My session"));
    assert!(
        fixture
            .session
            .session_manager()
            .get_entries()
            .iter()
            .all(|entry| entry.entry_type() != "session_info"),
        "session names never write session_info entries"
    );
    assert!(fixture.session.session_manager().has_session_name_state());
    fixture.session.clear_session_name().expect("clear name");
    assert_eq!(fixture.session.session_name(), None);
    assert!(fixture.session.session_manager().has_session_name_state());
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
        FixtureOverrides {
            multi_agent: Some((Arc::clone(&store), "child-agent".to_owned())),
            ..Default::default()
        },
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
        Box::pin(async { Ok(()) })
            as pi_ai::request::BoxFuture<'static, Result<(), pi_coding_agent::core::extensions::types::HandlerError>>
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
    runner
        .activate_tool_result_relocation()
        .await
        .expect("activate relocation");
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
        FixtureOverrides {
            persist_session: true,
            ..Default::default()
        },
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
        FixtureOverrides {
            persist_session: true,
            ..Default::default()
        },
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
        multi_agent_requires_agent_id: false,
        disable_runtime_coordination_inbound: false,
        on_shared_channel_message_delivered: None,
        extension_runner_ref: None,
        session_start_event: None,
    })
    .expect("session");

    let lifecycle = session.detached_job_lifecycle();
    assert!(lifecycle.is_some(), "expected a bound detached-job lifecycle");
    let context_lifecycle = session
        .extension_runner()
        .create_context()
        .detached_job_lifecycle()
        .expect("extension context");
    assert!(
        context_lifecycle.is_some(),
        "extension context must expose the bound detached-job lifecycle"
    );
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
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            persist_session: true,
            ..Default::default()
        },
    )
    .await;
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

    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            persist_session: true,
            ..Default::default()
        },
    )
    .await;
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

// ---------------------------------------------------------------------------
// Human "Allow always" and read-only LLM auto-approval
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ScriptedApprovalUi {
    selections: std::sync::Mutex<VecDeque<Option<String>>>,
    titles: std::sync::Mutex<Vec<String>>,
    options: std::sync::Mutex<Vec<Vec<String>>>,
}

impl ScriptedApprovalUi {
    fn with_selection(selection: &str) -> Self {
        let ui = Self::default();
        ui.selections.lock().unwrap().push_back(Some(selection.to_owned()));
        ui
    }
}

impl ExtensionUiContext for ScriptedApprovalUi {
    fn select<'a>(
        &'a self,
        title: &str,
        options: &[String],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> pi_ai::request::BoxFuture<'a, Result<Option<String>, String>> {
        self.titles.lock().unwrap().push(title.to_owned());
        self.options.lock().unwrap().push(options.to_vec());
        let selection = self.selections.lock().unwrap().pop_front().flatten();
        Box::pin(async move { Ok(selection) })
    }
}

struct SpawnRecorderTool {
    calls: Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
}

impl pi_agent_core::agent::types::AgentTool for SpawnRecorderTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn description(&self) -> &str {
        "Spawn a background agent"
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "agentType": {"type": "string"},
                    "displayName": {"type": "string"},
                    "prompt": {"type": "string"}
                },
                "required": ["prompt"]
            })
        })
    }

    fn label(&self) -> &str {
        "Spawn Agent"
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: serde_json::Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: pi_agent_core::agent::types::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<pi_agent_core::agent::types::AgentToolResult, pi_agent_core::agent::types::ToolError>,
    > {
        self.calls.lock().unwrap().push((tool_call_id, params));
        Box::pin(async move { Ok(pi_agent_core::agent::types::AgentToolResult::text("spawned")) })
    }
}

#[tokio::test]
async fn human_allow_always_spawns_the_permission_config_agent() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let spawned = Arc::new(std::sync::Mutex::new(Vec::new()));
    let ui = Arc::new(ScriptedApprovalUi::with_selection("Allow always"));
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("approvalPolicy".to_owned(), json!("on-request"));
    settings.insert("approvalPreset".to_owned(), json!("ask-me"));
    let fixture = fixture_with(
        settings,
        vec![
            Arc::new(PermissionEchoTool {
                executed: Arc::clone(&executed),
            }),
            Arc::new(SpawnRecorderTool {
                calls: Arc::clone(&spawned),
            }),
        ],
        Some(vec!["echo".to_owned(), "spawn_agent".to_owned()]),
        None,
    )
    .await;
    fixture
        .session
        .bind_extensions(ExtensionBindings {
            ui_context: Some(ui.clone() as Arc<dyn ExtensionUiContext>),
            mode: Some(ExtensionMode::Tui),
            ..Default::default()
        })
        .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            vec![pi_ai::types::AssistantContent::ToolCall(pi_ai::types::ToolCall::new(
                "call-1",
                "echo",
                serde_json::from_str(r#"{"text":"hello","2":2.0,"1":1.0}"#).unwrap(),
            ))],
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

    assert_eq!(executed.lock().unwrap().as_slice(), ["hello".to_owned()]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while spawned.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let spawned = spawned.lock().unwrap().clone();
    assert_eq!(spawned.len(), 1, "expected one allow-always spawn");
    let (tool_call_id, params) = &spawned[0];
    assert_eq!(tool_call_id, "allow-always:call-1");
    assert_eq!(
        params.get("agentType").and_then(serde_json::Value::as_str),
        Some("permission-config")
    );
    assert_eq!(
        params.get("displayName").and_then(serde_json::Value::as_str),
        Some("Allow command")
    );
    let prompt = params
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .expect("prompt");
    assert!(
        prompt.contains("Target project: /syncthing/Sync/Projects/claude/claude-bash-hook"),
        "prompt: {prompt}"
    );
    assert!(prompt.contains("Original cwd: "), "prompt: {prompt}");
    assert!(prompt.contains("Tool name: echo"), "prompt: {prompt}");
    assert!(prompt.contains("\"text\": \"hello\""), "prompt: {prompt}");
    let expected_input = "{\n  \"1\": 1,\n  \"2\": 2,\n  \"text\": \"hello\"\n}";
    assert!(prompt.contains(expected_input), "prompt: {prompt}");
    let titles = ui.titles.lock().unwrap().clone();
    assert_eq!(titles.len(), 1);
    assert_eq!(titles[0], format!("Approve echo?\n{expected_input}"));
    let options = ui.options.lock().unwrap().clone();
    assert_eq!(
        options[0],
        ["Allow once".to_owned(), "Allow always".to_owned(), "Deny".to_owned()]
    );
}

fn recording_supervisor(
    response: pi_coding_agent::core::session_control_db::SupervisorResponse,
) -> (SupervisorDecisionRequester, Arc<std::sync::Mutex<Vec<String>>>) {
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&calls);
    let requester: SupervisorDecisionRequester = Arc::new(move |input| {
        recorded.lock().unwrap().push(
            input
                .payload
                .get("toolName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        );
        let response = response.clone();
        Box::pin(async move { Ok(response) })
    });
    (requester, calls)
}

#[tokio::test]
async fn llm_reviewer_auto_approves_read_only_builtin_tools() {
    let (requester, supervisor_calls) =
        recording_supervisor(pi_coding_agent::core::session_control_db::SupervisorResponse::Reject {
            reason: "should not be consulted".to_owned(),
        });
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("approvalPolicy".to_owned(), json!("on-request"));
    settings.insert("approvalPreset".to_owned(), json!("llm-approved-ask"));
    let fixture = fixture_with_multi_agent(
        settings,
        Vec::new(),
        Some(vec!["grep".to_owned()]),
        None,
        FixtureOverrides {
            supervisor_decision_requester: Some(requester),
            ..Default::default()
        },
    )
    .await;
    std::fs::write(fixture._temp_dir.path().join("a.txt"), "phpstan composer.json").expect("fixture file");
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            pi_ai::providers::faux::FauxContent::Blocks(vec![pi_ai::providers::faux::faux_tool_call(
                "grep",
                json!({"pattern": "phpstan"}).as_object().cloned().expect("arguments"),
                Some("call-1".to_owned()),
            )]),
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
        .prompt("search composer.json", PromptOptions::default())
        .await
        .expect("prompt");

    assert!(
        supervisor_calls.lock().unwrap().is_empty(),
        "read-only tools must not reach the Supervisor"
    );
    let tool_result = fixture
        .session
        .messages()
        .into_iter()
        .find_map(|message| message.as_tool_result().cloned())
        .expect("tool result");
    let text = tool_result
        .content
        .iter()
        .filter_map(|content| match content {
            pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    assert!(text.contains("a.txt"), "tool result: {text}");
    assert!(!tool_result.is_error);
}

fn custom_grep_definition(
    executed: Arc<std::sync::Mutex<Vec<String>>>,
) -> pi_coding_agent::core::extensions::types::ToolDefinition {
    pi_coding_agent::core::extensions::types::ToolDefinition {
        name: "grep".to_owned(),
        label: "Custom Grep".to_owned(),
        description: "Custom tool using a built-in name".to_owned(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type": "object", "properties": {"pattern": {"type": "string"}}}),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_id, _params, _signal, _on_update, _ctx| {
            let executed = Arc::clone(&executed);
            Box::pin(async move {
                executed.lock().unwrap().push("grep".to_owned());
                Ok(pi_agent_core::AgentToolResult::text("executed"))
            })
        }),
        render_call: None,
        render_result: None,
    }
}

#[tokio::test]
async fn llm_reviewer_does_not_auto_approve_extension_tools_reusing_a_read_only_name() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (requester, supervisor_calls) =
        recording_supervisor(pi_coding_agent::core::session_control_db::SupervisorResponse::Reject {
            reason: "custom tool".to_owned(),
        });
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("approvalPolicy".to_owned(), json!("on-request"));
    settings.insert("approvalPreset".to_owned(), json!("llm-approved-deny"));
    let fixture = fixture_with_multi_agent(
        settings,
        Vec::new(),
        Some(vec!["grep".to_owned()]),
        None,
        FixtureOverrides {
            supervisor_decision_requester: Some(requester),
            custom_tool_definitions: vec![custom_grep_definition(Arc::clone(&executed))],
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            pi_ai::providers::faux::FauxContent::Blocks(vec![pi_ai::providers::faux::faux_tool_call(
                "grep",
                json!({"pattern": "phpstan"}).as_object().cloned().expect("arguments"),
                Some("call-1".to_owned()),
            )]),
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
        .prompt("run custom grep", PromptOptions::default())
        .await
        .expect("prompt");

    assert!(executed.lock().unwrap().is_empty(), "custom grep must stay blocked");
    assert_eq!(supervisor_calls.lock().unwrap().as_slice(), ["grep".to_owned()]);
}

// ---------------------------------------------------------------------------
// Thinking-phase deadline
// ---------------------------------------------------------------------------

struct GatedTool {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

impl pi_agent_core::agent::types::AgentTool for GatedTool {
    fn name(&self) -> &str {
        "slow"
    }

    fn description(&self) -> &str {
        "Slow tool"
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| json!({"type": "object", "properties": {}}))
    }

    fn label(&self) -> &str {
        "Slow"
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
        let started = Arc::clone(&self.started);
        let release = Arc::clone(&self.release);
        Box::pin(async move {
            started.notify_one();
            release.notified().await;
            Ok(pi_agent_core::agent::types::AgentToolResult::text("done"))
        })
    }
}

fn pending_response() -> pi_ai::providers::faux::FauxResponseStep {
    pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(|_context, _options, _state, _model| {
        Box::pin(std::future::pending())
    }))
}

fn tool_call_response(name: &str, arguments: serde_json::Value, id: &str) -> pi_ai::providers::faux::FauxResponseStep {
    faux_assistant_message(
        pi_ai::providers::faux::FauxContent::Blocks(vec![pi_ai::providers::faux::faux_tool_call(
            name,
            arguments.as_object().cloned().unwrap_or_default(),
            Some(id.to_owned()),
        )]),
        FauxAssistantMessageOptions {
            stop_reason: Some(pi_ai::types::StopReason::ToolUse),
            ..Default::default()
        },
    )
    .into()
}

async fn drive_until_finished(
    handle: &mut tokio::task::JoinHandle<Result<(), pi_coding_agent::core::agent_session::AgentSessionError>>,
) {
    for _ in 0..40 {
        if handle.is_finished() {
            return;
        }
        tokio::time::advance(std::time::Duration::from_millis(500)).await;
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn thinking_phase_deadline_caps_main_session_thinking_but_not_tools() {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let fixture = fixture_with_multi_agent(
        Default::default(),
        vec![Arc::new(GatedTool {
            started: Arc::clone(&started),
            release: Arc::clone(&release),
        })],
        Some(vec!["slow".to_owned()]),
        None,
        FixtureOverrides {
            thinking_phase_timeout_ms: Some(1_000),
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![
        tool_call_response("slow", json!({}), "call-1"),
        pending_response(),
    ]);
    let session = fixture.session.clone();
    let mut handle = tokio::spawn(async move { session.prompt("hi", PromptOptions::default()).await });

    started.notified().await;
    tokio::time::advance(std::time::Duration::from_secs(60)).await;
    tokio::task::yield_now().await;
    assert!(!handle.is_finished(), "tool execution must not be capped");

    release.notify_one();
    drive_until_finished(&mut handle).await;
    let error = handle
        .await
        .expect("prompt task")
        .expect_err("thinking deadline must abort the run");
    assert_eq!(error.message(), "Main session thinking phase exceeded 15 minutes");
}

#[tokio::test(start_paused = true)]
async fn thinking_phase_deadline_reports_the_child_subject() {
    use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};

    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| "2026-06-21T00:00:00.000Z".to_owned())),
    }));
    store.publish_lifecycle_coordinator_snapshot(&running_agent("child-agent"));
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            multi_agent: Some((store, "child-agent".to_owned())),
            thinking_phase_timeout_ms: Some(1_000),
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![pending_response()]);
    let session = fixture.session.clone();
    let mut handle = tokio::spawn(async move { session.prompt("hi", PromptOptions::default()).await });

    drive_until_finished(&mut handle).await;
    let error = handle
        .await
        .expect("prompt task")
        .expect_err("thinking deadline must abort the run");
    assert_eq!(error.message(), "Child agent thinking phase exceeded 15 minutes");
}

#[tokio::test(start_paused = true)]
async fn thinking_phase_deadline_skips_observer_sessions() {
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            multi_agent_runtime_role: Some(MultiAgentRuntimeRole::Observer),
            thinking_phase_timeout_ms: Some(1_000),
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![pending_response()]);
    let session = fixture.session.clone();
    let handle = tokio::spawn(async move { session.prompt("hi", PromptOptions::default()).await });

    for _ in 0..10 {
        tokio::time::advance(std::time::Duration::from_secs(10)).await;
        tokio::task::yield_now().await;
    }
    assert!(!handle.is_finished(), "observer thinking must not be capped");
    fixture.session.abort().await.expect("abort");
    handle
        .await
        .expect("prompt task")
        .expect("observer run settles without a thinking timeout");
    let aborted = fixture
        .session
        .messages()
        .into_iter()
        .rev()
        .find_map(|message| message.as_assistant().cloned())
        .expect("assistant message");
    assert_eq!(aborted.stop_reason, pi_ai::types::StopReason::Aborted);
}

// ---------------------------------------------------------------------------
// Activity publication errors surface through the agent event listener
// ---------------------------------------------------------------------------

struct FakePersistence;

impl pi_coding_agent::core::multi_agent_store::SessionPersistence for FakePersistence {
    fn metadata_control_db_path(&self) -> Option<String> {
        Some("/tmp/pi-session-final/control.db".to_owned())
    }

    fn session_file(&self) -> Option<String> {
        Some("/tmp/pi-session-final/session.jsonl".to_owned())
    }
}

#[tokio::test]
async fn activity_publication_errors_fail_the_agent_run() {
    use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};

    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.publish_lifecycle_coordinator_snapshot(&running_agent("child-agent"));
    store.set_persistence_session_manager(Some(&FakePersistence));
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            multi_agent: Some((store, "child-agent".to_owned())),
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message("done", FauxAssistantMessageOptions::default()).into(),
    ]);

    let error = fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect_err("publication failure must abort the run");
    assert_eq!(
        error.message(),
        "Persisted agent child-agent activity update requires exact runtime ownership"
    );
}

struct FailingApprovalUi;

impl ExtensionUiContext for FailingApprovalUi {
    fn select<'a>(
        &'a self,
        _title: &str,
        _options: &[String],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> pi_ai::request::BoxFuture<'a, Result<Option<String>, String>> {
        Box::pin(async { Err("dialog failed".to_owned()) })
    }
}

fn tool_result_text(fixture: &Fixture) -> (bool, String) {
    let result = fixture
        .session
        .messages()
        .into_iter()
        .find_map(|message| message.as_tool_result().cloned())
        .expect("tool result");
    let text = result
        .content
        .iter()
        .filter_map(|content| match content {
            pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    (result.is_error, text)
}

#[tokio::test]
async fn human_approval_ui_errors_abort_the_tool_call() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("approvalPolicy".to_owned(), json!("on-request"));
    settings.insert("approvalPreset".to_owned(), json!("ask-me"));
    let fixture = fixture_with(
        settings,
        vec![Arc::new(PermissionEchoTool {
            executed: Arc::clone(&executed),
        })],
        Some(vec!["echo".to_owned()]),
        None,
    )
    .await;
    fixture
        .session
        .bind_extensions(ExtensionBindings {
            ui_context: Some(Arc::new(FailingApprovalUi) as Arc<dyn ExtensionUiContext>),
            mode: Some(ExtensionMode::Tui),
            ..Default::default()
        })
        .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            permission_tool_call("hello"),
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
        .expect("prompt resolves with the run failure message");

    assert!(executed.lock().unwrap().is_empty(), "tool must not run");
    let (is_error, text) = tool_result_text(&fixture);
    assert!(is_error);
    assert_eq!(text, "dialog failed");
}

#[tokio::test]
async fn supervisor_requester_errors_abort_the_tool_call() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let requester: SupervisorDecisionRequester =
        Arc::new(|_input| Box::pin(async { Err("supervisor unavailable".to_owned()) }));
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert("approvalPolicy".to_owned(), json!("on-request"));
    settings.insert("approvalPreset".to_owned(), json!("llm-approved-deny"));
    let fixture = fixture_with_multi_agent(
        settings,
        vec![Arc::new(PermissionEchoTool {
            executed: Arc::clone(&executed),
        })],
        Some(vec!["echo".to_owned()]),
        None,
        FixtureOverrides {
            supervisor_decision_requester: Some(requester),
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            permission_tool_call("hello"),
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
        .expect("prompt resolves with the run failure message");

    assert!(
        executed.lock().unwrap().is_empty(),
        "supervisor failure must not allow the tool"
    );
    let (is_error, text) = tool_result_text(&fixture);
    assert!(is_error);
    assert_eq!(text, "supervisor unavailable");
}

struct RecordingDetachHandle {
    detached: Arc<AtomicBool>,
    accepts: bool,
}

impl ToolDetachHandle for RecordingDetachHandle {
    fn detach(&self) -> bool {
        self.detached.store(true, Ordering::SeqCst);
        self.accepts
    }
}

#[tokio::test]
async fn detach_running_tool_and_detach_bash_tool_delegate_to_the_tool_detach_registry() {
    let fixture = fixture().await;
    let context = fixture.session.extension_runner().create_context();
    let registry = context
        .tool_detach_registry()
        .expect("registry getter")
        .expect("session registry");

    assert!(!fixture.session.has_detachable_tool());
    assert!(!fixture.session.has_detachable_bash_tool());
    assert!(!fixture.session.detach_running_tool());
    assert!(!fixture.session.detach_bash_tool());

    let detached = Arc::new(AtomicBool::new(false));
    let subscription = registry.register(Arc::new(RecordingDetachHandle {
        detached: Arc::clone(&detached),
        accepts: false,
    }));
    assert!(fixture.session.has_detachable_tool());
    assert!(fixture.session.has_detachable_bash_tool());
    assert!(!fixture.session.detach_running_tool());
    assert!(detached.load(Ordering::SeqCst));

    let accepted = Arc::new(AtomicBool::new(false));
    let accepted_subscription = registry.register(Arc::new(RecordingDetachHandle {
        detached: Arc::clone(&accepted),
        accepts: true,
    }));
    assert!(fixture.session.detach_bash_tool());
    assert!(accepted.load(Ordering::SeqCst));
    assert!(
        fixture.session.has_detachable_tool(),
        "detach does not unregister the handle; the tool's subscription does"
    );

    drop(accepted_subscription);
    drop(subscription);
    assert!(!fixture.session.has_detachable_tool());
    assert!(!fixture.session.has_detachable_bash_tool());
}

async fn fixture_with_test_command() -> Fixture {
    use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
    use pi_coding_agent::core::extensions::{
        ExtensionCommandContext, ExtensionFactory, HandlerError, create_extension_runtime, load_extension_from_factory,
    };
    let runtime = create_extension_runtime();
    let factory: ExtensionFactory = Arc::new(|api| {
        api.register_command(
            "testcmd",
            Some("Test command".to_owned()),
            None,
            Arc::new(|_args: String, _ctx: ExtensionCommandContext| {
                Box::pin(async { Ok(()) })
                    as pi_coding_agent::core::extensions::BoxFuture<'static, Result<(), HandlerError>>
            }),
        )
        .expect("register testcmd");
        api.on_turn_start(
            |_event: pi_coding_agent::core::extensions::TurnStartEvent,
             _ctx: pi_coding_agent::core::extensions::ExtensionContext| {
                Box::pin(async { Ok(None) })
                    as pi_coding_agent::core::extensions::BoxFuture<'static, Result<Option<()>, HandlerError>>
            },
        )
        .expect("register turn_start");
        Box::pin(async { Ok(()) })
    });
    let event_bus: Arc<dyn EventBus> = Arc::new(EventBusController::new());
    let extension = load_extension_from_factory(factory, "/tmp", event_bus, runtime.clone(), "/tmp/testcmd.ts")
        .await
        .expect("extension factory succeeds");
    fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        Some(Vec::new()),
        FixtureOverrides {
            extensions: Some(pi_coding_agent::core::extensions::types::LoadExtensionsResult {
                extensions: vec![extension],
                errors: Vec::new(),
                runtime,
            }),
            ..Default::default()
        },
    )
    .await
}

const QUEUED_EXTENSION_COMMAND_ERROR: &str =
    "Extension command \"/testcmd\" cannot be queued. Use prompt() or execute the command when not streaming.";

#[tokio::test]
async fn steer_rejects_a_registered_extension_command() {
    let fixture = fixture_with_test_command().await;
    let error = fixture
        .session
        .steer("/testcmd queued", &[])
        .await
        .expect_err("extension commands cannot be steered");
    assert_eq!(error.message(), QUEUED_EXTENSION_COMMAND_ERROR);
    assert!(fixture.session.get_steering_messages().is_empty());
}

#[tokio::test]
async fn follow_up_rejects_a_registered_extension_command() {
    let fixture = fixture_with_test_command().await;
    let error = fixture
        .session
        .follow_up("/testcmd", &[])
        .await
        .expect_err("extension commands cannot be queued as follow-ups");
    assert_eq!(error.message(), QUEUED_EXTENSION_COMMAND_ERROR);
    assert!(fixture.session.get_follow_up_messages().is_empty());
}

#[tokio::test]
async fn has_extension_handlers_reports_registered_event_handlers() {
    let fixture = fixture_with_test_command().await;
    assert!(fixture.session.has_extension_handlers("turn_start"));
    assert!(!fixture.session.has_extension_handlers("turn_end"));
}

#[tokio::test]
async fn persist_failures_are_uncaught_like_the_typescript_event_handler() {
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            persist_session: true,
            ..Default::default()
        },
    )
    .await;
    std::fs::remove_dir_all(fixture._temp_dir.path().join("sessions")).expect("remove session dir");
    let session = fixture.session.clone();
    let handle = tokio::spawn(async move { session.prompt("persist failure", PromptOptions::default()).await });
    let join = handle.await.expect_err("persist failure must panic");
    let payload = join.into_panic();
    let error = payload
        .downcast_ref::<pi_coding_agent::utils::uncaught::UncaughtError>()
        .expect("uncaught payload");
    assert_eq!(error.label, pi_coding_agent::utils::uncaught::ERROR);
    assert!(!error.message.is_empty());
}
