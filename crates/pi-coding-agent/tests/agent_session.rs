//! Behavioral tests for `core/agent-session.ts`.

#[path = "agent_session_bash_parity.rs"]
mod bash_parity;
#[path = "agent_session_effort_prefix.rs"]
mod effort_prefix;
#[path = "support/image_fixtures.rs"]
mod image_fixtures;
#[path = "agent_session_management_lifecycle.rs"]
mod management_lifecycle;
#[path = "agent_session_manual_tool_compaction.rs"]
mod manual_tool_compaction;
#[path = "agent_session_prompt_images.rs"]
mod prompt_images;
#[path = "agent_session_thinking_cycle.rs"]
mod thinking_cycle;
#[path = "support/oracle.rs"]
mod tools_oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
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
use pi_coding_agent::core::model_registry::ModelRegistry;
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
        name: Some("Faux Model".into()),
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

async fn model_registry(faux: &FauxProviderHandle) -> ModelRegistry {
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(auth_data())).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    runtime
}

fn stream_fn(faux: &FauxProviderHandle) -> pi_agent_core::agent::types::StreamFn {
    let provider = Arc::clone(&faux.provider);
    Arc::new(move |model: Model, context: Context, options: SimpleStreamOptions| {
        let stream = provider.stream_simple(model, context, options);
        Box::pin(async move { stream.map_err(pi_agent_core::agent::AgentError::from) })
    })
}

fn assert_restriction_context(entry: &pi_coding_agent::core::session_manager::SessionEntry, request: &str) {
    let known = entry.known().expect("known restriction entry");
    let pi_coding_agent::core::session_manager::KnownSessionEntry::Custom(context) = known.as_ref() else {
        panic!("expected restriction-context entry");
    };
    assert_eq!(context.custom_type, "codex_restriction_context");
    let data = context.data.as_ref().expect("restriction context data");
    let request_id = data.get("requestId").and_then(JsValue::as_string).expect("request ID");
    assert!(!request_id.is_empty());
    assert_eq!(
        data,
        &pi_js_compat::js_json!({"requestId": request_id, "userRequest": request})
    );
}

#[tokio::test]
async fn jsonl_export_preserves_javascript_value_bytes() {
    let fixture = fixture().await;
    let value = serde_json::from_str(r#"{"2":2.0,"1":1.0,"x":9007199254740993}"#).unwrap();
    fixture
        .session
        .session_manager()
        .append_custom_entry(&JsString::from("numeric"), Some(value))
        .unwrap();
    let path = fixture._temp_dir.path().join("export.jsonl");
    fixture
        .session
        .export_to_jsonl(Some(&JsString::from(path.to_str().expect("utf-8 test path"))))
        .unwrap();
    let bytes = std::fs::read_to_string(path).unwrap();
    assert!(
        bytes.contains(r#""data":{"1":1,"2":2,"x":9007199254740992}"#),
        "{bytes}"
    );
}

#[tokio::test]
async fn jsonl_export_directory_target_preserves_frozen_native_error() {
    use std::error::Error;
    let fixture = fixture().await;
    let directory = fixture._temp_dir.path().join("directory-target");
    std::fs::create_dir(&directory).expect("target directory");
    let file = fixture._temp_dir.path().join("file-parent");
    std::fs::write(&file, "existing file").expect("file parent");
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    for path in [directory, file.join("export.jsonl"), file.join("nested/export.jsonl")] {
        let output = JsString::from(path.display().to_string());
        let expected = oracle.request(&pi_js_compat::js_json!({
            "operation": "export-error", "outputPath": output,
            "dir": fixture._temp_dir.path().join("oracle").display().to_string()
        }));
        assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)));
        let failure = fixture
            .session
            .export_to_jsonl(Some(&output))
            .expect_err("invalid export target");
        let actual = failure
            .source()
            .and_then(|source| source.downcast_ref::<pi_js_compat::node_error::NativeNodeError>())
            .map(|source| source.value())
            .unwrap_or_else(|| pi_js_compat::js_json!({"name": "Error", "message": failure.message()}));
        assert_eq!(
            actual,
            expected.get("value").expect("oracle value").clone(),
            "path={path:?}"
        );
    }
    assert_eq!(std::fs::read_to_string(file).expect("existing file"), "existing file");
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
    thinking_phase_timeout_ms: Option<f64>,
    faux_api: Option<String>,
    faux_provider_id: Option<String>,
    context_window: Option<f64>,
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
        faux_api,
        faux_provider_id,
        context_window,
        multi_agent_runtime_role,
        custom_tool_definitions,
        extensions,
    } = overrides;
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let mut definition = faux_model();
    if let Some(context_window) = context_window {
        definition.context_window = Some(context_window);
    }
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some(faux_provider_id.unwrap_or_else(|| "faux".to_owned())),
        api: faux_api,
        models: Some(vec![definition]),
        ..Default::default()
    });
    let runtime = model_registry(&faux).await;
    let model = faux.get_model(None).expect("faux model");
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            model: Some(model),
            system_prompt: Some(JsString::default()),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(Vec::new()),
            messages: None,
        }),
        stream_fn: Some(stream_fn(&faux)),
        ..Default::default()
    })
    .expect("agent");
    let cwd = temp_dir.path().display().to_string();
    let session_manager = if persist_session {
        let session_dir = temp_dir.path().join("sessions").display().to_string();
        SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
            .expect("persisted session")
    } else {
        SessionManager::in_memory(Some(&JsString::from(&cwd)), None).expect("in-memory session")
    };
    let settings_manager = SettingsManager::in_memory(settings, SettingsManagerCreateOptions::default());
    let session = AgentSession::new(AgentSessionConfig {
        agent,
        session_manager: Arc::new(std::sync::Mutex::new(session_manager)),
        settings_manager: Arc::new(std::sync::Mutex::new(settings_manager)),
        cwd: JsString::from(temp_dir.path().display().to_string()),
        scoped_models: Vec::new(),
        resource_loader: Arc::new(pi_coding_agent::core::resource_loader::StaticResourceLoader {
            extensions,
            ..Default::default()
        }),
        model_registry: runtime,
        initial_active_tool_names,
        allowed_tool_names,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(JsString::from(temp_dir.path().display().to_string())),
        control_db_path: Some(JsString::from(temp_dir.path().join("control.db").display().to_string())),
        base_tools,
        custom_tool_definitions,
        custom_tools: Vec::<ToolDefinition>::new(),
        multi_agent_runtime_role,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms,
        no_supervisor: None,
        supervisor_decision_requester,
        multi_agent_store: multi_agent.as_ref().map(|(store, _)| Arc::clone(store)),
        multi_agent_agent_id: (multi_agent.as_ref().map(|(_, agent_id)| agent_id.clone())).map(JsString::from),
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
async fn default_session_registers_available_builtin_tools() {
    let fixture = fixture_with(Default::default(), Vec::new(), None, None).await;
    let mut oracle = tools_oracle::TsOracle::spawn(
        "coding-agent-tools-index-oracle.ts",
        "packages/coding-agent/src/core/tools/index.ts",
    );
    let expected = oracle.request(&json!({
        "id": 8,
        "operation": "availability",
        "cwd": fixture._temp_dir.path().to_str().unwrap(),
        "path": std::env::var("PATH").unwrap(),
        "injected": false,
    }));
    assert_eq!(expected["ok"], json!(true), "{expected}");
    let expected: Vec<String> = serde_json::from_value(expected["output"].clone()).unwrap();
    let mut names: Vec<String> = fixture
        .session
        .get_all_tools()
        .into_iter()
        .map(|tool| tool.name)
        .collect();
    let mut expected_names = expected.clone();
    names.sort();
    expected_names.sort();
    assert_eq!(names, expected_names);
    assert!(names.contains(&"read".to_owned()));
    assert!(names.contains(&"end_turn".to_owned()));
    assert_eq!(fixture.session.get_active_tool_names(), expected);
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
            AgentSessionEvent::CompactionStart(_) => "compaction_start",
            AgentSessionEvent::CompactionEnd(_) => "compaction_end",
            AgentSessionEvent::EntryAppended(_) => "entry_appended",
            AgentSessionEvent::BashMessagesCommitted(_) => "bash_messages_committed",
            AgentSessionEvent::SessionInfoChanged(_) => "session_info_changed",
            AgentSessionEvent::ThinkingLevelChanged(_) => "thinking_level_changed",
            AgentSessionEvent::AutoRetryStart(_) => "auto_retry_start",
            AgentSessionEvent::AutoRetryEnd(_) => "auto_retry_end",
            AgentSessionEvent::ProviderStreamRetry(_) => "provider_stream_retry",
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
            serde_json::Map::from_iter([("reason".to_owned(), json!("done"))])
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
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
            error_message: Some("overloaded_error: server overloaded".into()),
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
async fn repin_short_length_stop_below_threshold_does_not_force_overflow_compaction() {
    let settings = serde_json::from_value(json!({"compaction": {"keepRecentTokens": 1}})).unwrap();
    let fixture = fixture_with(settings, Vec::new(), None, Some(Vec::new())).await;
    fixture.faux.set_responses(vec![retry_completion_response().into()]);
    fixture
        .session
        .prompt("seed history", PromptOptions::default())
        .await
        .unwrap();
    let events = record_events(&fixture.session);
    let mut response = faux_assistant_message("truncated", FauxAssistantMessageOptions::default());
    response.stop_reason = pi_ai::types::StopReason::Length;
    response.usage.input = 100.0;
    response.usage.output = 16.0;
    response.usage.total_tokens = 116.0;
    fixture
        .faux
        .set_responses(vec![response.into(), retry_completion_response().into()]);
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        fixture.session.prompt("short length stop", PromptOptions::default()),
    )
    .await
    .expect("bounded prompt")
    .expect("prompt");
    assert!(
        !event_types(&events).contains(&"compaction_end".to_owned()),
        "{:?}",
        event_types(&events)
    );
}

#[tokio::test]
async fn repin_settings_invalid_compaction_threshold_rejects_before_next_request() {
    let settings = serde_json::from_value(json!({"compaction": {"thresholdPercent": 101}})).unwrap();
    let fixture = fixture_with(settings, Vec::new(), None, Some(Vec::new())).await;
    let first_answer = faux_assistant_message("first answer without end_turn", FauxAssistantMessageOptions::default());
    fixture
        .faux
        .set_responses(vec![first_answer.into(), retry_completion_response().into()]);
    let error = fixture
        .session
        .prompt("invalid settings", PromptOptions::default())
        .await
        .expect_err("invalid compaction setting must fail before the next provider request");
    assert!(
        error
            .to_string()
            .contains("Invalid compaction.thresholdPercent setting"),
        "{error}"
    );
    assert_eq!(
        fixture.faux.get_pending_response_count(),
        1,
        "second provider response was not consumed"
    );
}

#[tokio::test]
async fn scripted_sessions_keep_their_responses_across_other_registry_refreshes() {
    // Frozen agent-session.ts uses the injected agent.streamFn for each session.
    // Shared API names and refreshes must not change a test session's scripted transport.
    let first = idle_fixture("openai-codex-responses", json!({"retry":{"enabled":false}})).await;
    let second = idle_fixture("openai-codex-responses", json!({"retry":{"enabled":false}})).await;
    first.faux.set_responses(vec![completed_text_response("first answer")]);
    second
        .faux
        .set_responses(vec![completed_text_response("second answer")]);
    first.session.model_registry().refresh().unwrap();
    let (first_result, second_result) = tokio::join!(
        first.session.prompt("first request", PromptOptions::default()),
        second.session.prompt("second request", PromptOptions::default()),
    );
    first_result.expect("first session prompt");
    second_result.expect("second session prompt");
    assert_eq!(first.session.get_last_assistant_text(), Some("first answer".into()));
    assert_eq!(second.session.get_last_assistant_text(), Some("second answer".into()));
    assert_eq!(first.faux.get_pending_response_count(), 0);
    assert_eq!(second.faux.get_pending_response_count(), 0);
    first.session.dispose();
    second.session.dispose();
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
                    serde_json::Map::from_iter([("reason".to_owned(), serde_json::json!("done"))])
                        .into_iter()
                        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                        .collect::<pi_js_compat::js_json::JsObject>(),
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

    // Frozen agent-session.ts:2785-2800,3083 persists authorization before the user message.
    let entries = fixture.session.session_manager().get_entries();
    assert_eq!(entries.len(), 4, "restriction context + all three messages");
    assert_restriction_context(&entries[0], "hi");
    for (entry, message) in entries[1..].iter().zip(&messages) {
        let known = entry.known().expect("known message entry");
        let pi_coding_agent::core::session_manager::KnownSessionEntry::Message(persisted) = known.as_ref() else {
            panic!("expected a message entry");
        };
        assert_eq!(persisted.message.as_ref(), message);
    }
    for pair in entries.windows(2) {
        assert_eq!(pair[1].parent_id(), pair[0].id());
    }

    let types = event_types(&events);
    assert_eq!(types.first().map(String::as_str), Some("agent_start"));
    assert!(types.contains(&"message_start".to_owned()));
    assert!(types.contains(&"message_end".to_owned()));
    assert_eq!(types.last().map(String::as_str), Some("agent_end"));
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
async fn queued_previews_keep_input_provenance_and_are_non_consuming_snapshots() {
    let fixture = fixture().await;
    let gate = Arc::new(tokio::sync::Notify::new());
    let entered = Arc::new(tokio::sync::Notify::new());
    let stream_gate = Arc::clone(&gate);
    let stream_entered = Arc::clone(&entered);
    fixture.faux.set_responses(vec![
        pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(move |_, _, _, _| {
            let gate = Arc::clone(&stream_gate);
            let entered = Arc::clone(&stream_entered);
            Box::pin(async move {
                entered.notify_one();
                gate.notified().await;
                Ok(faux_assistant_message("done", FauxAssistantMessageOptions::default()))
            })
        })),
        faux_assistant_message("queued", FauxAssistantMessageOptions::default()).into(),
    ]);
    let session = fixture.session.clone();
    let prompt_task = tokio::spawn(async move { session.prompt("start", PromptOptions::default()).await });
    entered.notified().await;
    let raw = JsString::from("steer ") + JsString::from_utf16(vec![0xd800]);
    fixture
        .session
        .prompt(
            raw.clone(),
            PromptOptions {
                streaming_behavior: Some(StreamingBehavior::Steer),
                source: Some(pi_ai::types::InputSource::Extension),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    fixture.session.follow_up("later", &[]).await.unwrap();
    let mut snapshot = fixture.session.get_queued_message_previews();
    let actual = pi_js_compat::js_json::to_value(&snapshot.steering).unwrap();
    let expected = pi_js_compat::js_json!([{"text":raw,"inputSource":"extension"}]);
    assert_eq!(actual, expected);
    assert_eq!(snapshot.follow_up[0].text, "later");
    assert_eq!(snapshot.follow_up[0].input_source, None);
    snapshot.steering[0].text = "mutated snapshot".into();
    assert_eq!(fixture.session.get_steering_messages(), vec![raw.clone()]);
    assert_eq!(fixture.session.pending_message_count(), 2);
    let (steering, follow_up) = fixture.session.clear_queue();
    assert_eq!(steering, vec![raw]);
    assert_eq!(follow_up, vec![JsString::from("later")]);
    assert!(fixture.session.get_queued_message_previews().steering.is_empty());
    assert!(fixture.session.get_queued_message_previews().follow_up.is_empty());
    assert_eq!(snapshot.follow_up[0].text, "later");
    gate.notify_one();
    prompt_task.await.unwrap().unwrap();
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
    fixture
        .session
        .set_session_name(&JsString::from("My session"))
        .expect("set name");
    assert_eq!(
        fixture.session.session_name().as_ref(),
        Some(&JsString::from("My session"))
    );
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
                    serde_json::Map::from_iter([("reason".to_owned(), serde_json::json!("done"))])
                        .into_iter()
                        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                        .collect::<pi_js_compat::js_json::JsObject>(),
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
                    serde_json::Map::from_iter([("reason".to_owned(), serde_json::json!("done"))])
                        .into_iter()
                        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                        .collect::<pi_js_compat::js_json::JsObject>(),
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
        fixture.session.get_last_assistant_text().as_ref(),
        Some(&JsString::from("final answer"))
    );
}

#[tokio::test]
async fn navigate_tree_summary_requires_auth_before_calling_provider_like_frozen() {
    let fixture = fixture().await;
    let first = fixture
        .session
        .session_manager()
        .append_message(AgentMessage::User(pi_ai::UserMessage {
            role: pi_ai::UserRole::Tag,
            content: pi_ai::UserContent::Text("first request".into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        }))
        .expect("user");
    fixture
        .session
        .session_manager()
        .append_message(AgentMessage::Assistant(Box::new(faux_assistant_message(
            "old answer",
            Default::default(),
        ))))
        .expect("assistant");
    let mut model = fixture.session.model();
    model.provider = "unconfigured-provider".to_owned();
    fixture.session.agent().set_model(model);
    fixture.faux.set_responses(vec![
        faux_assistant_message("unauthenticated summary", Default::default()).into(),
    ]);
    let package_dir = pi_coding_agent::config::get_package_dir().expect("package dir");
    let mut oracle = tools_oracle::TsOracle::spawn_with_environment(
        "agent-session-oracle.ts",
        "packages/coding-agent/src/core/sdk.ts",
        &[(
            "PI_PACKAGE_DIR",
            std::ffi::OsString::from(encode_node_utf8(&package_dir)),
        )],
    );
    let expected = oracle.request(&json!({
        "operation": "navigation-auth", "dir": fixture._temp_dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(expected["ok"], true, "oracle: {expected}");
    assert!(
        expected["value"]["error"]
            .as_str()
            .expect("auth error")
            .contains("unconfigured-provider")
    );
    let result = fixture
        .session
        .navigate_tree(
            &first,
            NavigateTreeOptions {
                summarize: Some(true),
                ..Default::default()
            },
        )
        .await;
    let actual = match result {
        Ok(_) => json!({"success": true}),
        Err(error) => json!({"error": error.message()}),
    };
    assert_eq!(actual, expected["value"]);
}

#[tokio::test]
async fn navigate_tree_empty_extension_summary_and_label_match_frozen() {
    use pi_coding_agent::core::extensions::types::{HandlerError, SessionBeforeTreeResult, SessionTreeSummary};
    let fixture = fixture_with_timeout_extension(Arc::new(|api| {
        let registered = api.on_session_before_tree(|_event, _ctx| {
            Box::pin(async {
                Ok(Some(SessionBeforeTreeResult {
                    summary: Some(SessionTreeSummary {
                        summary: JsString::default(),
                        details: None,
                        usage: None,
                    }),
                    label: Some(JsString::default()),
                    ..Default::default()
                }))
            }) as pi_ai::request::BoxFuture<'static, Result<Option<SessionBeforeTreeResult>, HandlerError>>
        });
        Box::pin(async move { registered.map_err(HandlerError::new) })
    }))
    .await;
    let first = fixture
        .session
        .session_manager()
        .append_message(AgentMessage::User(pi_ai::UserMessage {
            role: pi_ai::UserRole::Tag,
            content: pi_ai::UserContent::Text("first request".into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        }))
        .expect("user");
    fixture
        .session
        .session_manager()
        .append_message(AgentMessage::Assistant(Box::new(faux_assistant_message(
            "old answer",
            Default::default(),
        ))))
        .expect("assistant");
    let result = fixture
        .session
        .navigate_tree(
            &first,
            NavigateTreeOptions {
                summarize: Some(true),
                label: Some(JsString::default()),
                ..Default::default()
            },
        )
        .await
        .expect("navigate");
    let manager = fixture.session.session_manager();
    let entries = manager.get_entries();
    let actual = json!({
        "hasSummary": result.summary_entry.is_some(), "atRoot": manager.get_leaf_id().is_none(),
        "summaries": entries.iter().filter(|entry| entry.entry_type() == "branch_summary").count(),
        "labels": entries.iter().filter(|entry| entry.entry_type() == "label").count(),
    });
    drop(manager);
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({
        "operation": "navigation-empty-summary", "dir": fixture._temp_dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(
        expected,
        json!({"ok": true, "value": {"hasSummary": false, "atRoot": true, "summaries": 0, "labels": 0}})
    );
    assert_eq!(actual, expected["value"]);
}

#[tokio::test]
async fn navigate_tree_current_leaf_is_noop_during_streaming_like_frozen() {
    let fixture = fixture().await;
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (start_provider, release_provider) = (Arc::clone(&started), Arc::clone(&release));
    fixture
        .faux
        .set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_, _, _, _| {
                let (started, release) = (Arc::clone(&start_provider), Arc::clone(&release_provider));
                Box::pin(async move {
                    started.notify_one();
                    release.notified().await;
                    Ok(faux_assistant_message("answer", Default::default()))
                })
            },
        ))]);
    let session = fixture.session.clone();
    let prompt = tokio::spawn(async move { session.prompt("first request", PromptOptions::default()).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
        .await
        .expect("provider started");
    let target = fixture
        .session
        .session_manager()
        .get_leaf_id()
        .expect("current leaf")
        .clone();
    let result = fixture
        .session
        .navigate_tree(&target, NavigateTreeOptions::default())
        .await;
    let leaf_after_navigation = fixture.session.session_manager().get_leaf_id().cloned();
    let actual = match result {
        Ok(result) => json!({"cancelled": result.cancelled}),
        Err(error) => json!({"error": error.message()}),
    };
    release.notify_one();
    fixture.session.abort().await.expect("abort");
    prompt.await.expect("prompt task").expect("prompt result");
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({
        "operation": "navigation-streaming", "dir": fixture._temp_dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(expected, json!({"ok": true, "value": {"cancelled": false}}));
    assert_eq!(actual, expected["value"]);
    assert_eq!(leaf_after_navigation.as_ref(), Some(&target));
}

#[tokio::test]
async fn navigate_tree_moves_leaf_and_returns_editor_text() {
    let fixture = fixture().await;
    // Frozen agent-loop.ts:308 continues until end_turn; keep this fixture to one complete turn.
    fixture.faux.set_responses(vec![completed_text_response("hello")]);
    fixture
        .session
        .prompt("first", PromptOptions::default())
        .await
        .expect("prompt");
    let entries = fixture.session.session_manager().get_entries();
    assert_eq!(
        entries.len(),
        4,
        "restriction context + user + assistant + end_turn result"
    );
    assert_eq!(entries[0].parent_id(), None);
    for (index, role) in [(1, "user"), (2, "assistant"), (3, "toolResult")] {
        let known = entries[index].known().expect("known message");
        let pi_coding_agent::core::session_manager::KnownSessionEntry::Message(message) = known.as_ref() else {
            panic!("expected message at index {index}");
        };
        assert_eq!(message.message.role(), role);
    }
    for pair in entries.windows(2) {
        assert_eq!(pair[1].parent_id(), pair[0].id());
    }
    assert_restriction_context(&entries[0], "first");
    let user_entry = entries[1].known().expect("known user entry");
    let pi_coding_agent::core::session_manager::KnownSessionEntry::Message(user) = user_entry.as_ref() else {
        panic!("expected user message after restriction context");
    };
    assert_eq!(user.message.role(), "user");
    assert_eq!(entries[1].parent_id(), entries[0].id());
    let user_entry_id = entries[1].id().expect("user entry id").to_owned();
    let result = fixture
        .session
        .navigate_tree(&user_entry_id, NavigateTreeOptions::default())
        .await
        .expect("navigate");
    assert!(!result.cancelled);
    assert_eq!(result.editor_text.as_ref(), Some(&JsString::from("first")));
    // Frozen :6129-6132 branches to the user's parent, which is the restriction context.
    assert_eq!(fixture.session.session_manager().get_leaf_id(), entries[0].id());
    assert!(fixture.session.messages().is_empty());
}

#[tokio::test]
async fn navigate_tree_summarizes_the_abandoned_branch_and_attaches_the_summary() {
    use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry};

    let fixture = fixture().await;
    // Frozen agent-loop.ts:308 continues until end_turn; give each prompt its own completed turn.
    fixture
        .faux
        .set_responses(vec![completed_text_response("hello"), completed_text_response("hi")]);
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
    let entries = fixture.session.session_manager().get_entries();
    assert_eq!(
        entries.len(),
        8,
        "two restriction contexts and two user/assistant/end_turn triples"
    );
    assert_eq!(entries[0].parent_id(), None);
    assert_restriction_context(&entries[0], "first");
    assert_restriction_context(&entries[4], "second");
    for (index, role) in [
        (1, "user"),
        (2, "assistant"),
        (3, "toolResult"),
        (5, "user"),
        (6, "assistant"),
        (7, "toolResult"),
    ] {
        let known = entries[index].known().expect("known message");
        let KnownSessionEntry::Message(message) = known.as_ref() else {
            panic!("expected message at index {index}");
        };
        assert_eq!(message.message.role(), role);
    }
    for pair in entries.windows(2) {
        assert_eq!(pair[1].parent_id(), pair[0].id());
    }
    let first_user_entry = entries[1].id().expect("first user entry").to_owned();

    fixture.faux.set_responses(vec![
        faux_assistant_message("BRANCH SUMMARY", FauxAssistantMessageOptions::default()).into(),
    ]);
    let result = fixture
        .session
        .navigate_tree(
            &first_user_entry,
            NavigateTreeOptions {
                summarize: Some(true),
                label: Some(JsString::from("abandoned")),
                ..Default::default()
            },
        )
        .await
        .expect("navigate with summary");
    assert!(!result.cancelled);
    assert_eq!(result.editor_text.as_ref(), Some(&JsString::from("first")));
    let summary_entry = result.summary_entry.expect("branch summary entry");
    let SessionEntry::Known(KnownSessionEntry::BranchSummary(summary)) = &summary_entry else {
        panic!("expected a branch summary entry");
    };
    assert!(summary.summary.contains("BRANCH SUMMARY"));
    // Frozen :6129-6132,6154-6158 attaches the summary at the target user's parent.
    assert_eq!(summary.parent_id.as_ref(), entries[0].id());
    assert_eq!(
        summary.from_id,
        entries[0].id().expect("restriction context id").clone()
    );
    assert_eq!(summary.from_hook, Some(false));
    let manager = fixture.session.session_manager();
    assert_eq!(manager.get_label(&summary.id), Some(&JsString::from("abandoned")));
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

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Echo text".into()
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| json!({"type": "object", "properties": {"text": {"type": "string"}}}))
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "echo".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
        _on_update: pi_agent_core::agent::types::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<pi_agent_core::agent::types::AgentToolResult, pi_agent_core::agent::types::ToolError>,
    > {
        Box::pin(async move {
            let text = params
                .get("text")
                .and_then(JsValue::as_string)
                .cloned()
                .unwrap_or_default();
            Ok(pi_agent_core::agent::types::AgentToolResult::text(text))
        })
    }
}

fn running_agent(id: &str) -> pi_coding_agent::core::multi_agent_store::AgentSnapshot {
    use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, AgentPermission, AgentSnapshot};
    AgentSnapshot {
        key_order: Vec::new(),
        id: JsString::from(id.to_owned()),
        parent_id: None,
        display_name: JsString::from("Child"),
        agent_type: "worker".into(),
        origin: None,
        lifecycle: AgentLifecycle::Running,
        revision: 1,
        created_at: JsString::from("2026-06-21T00:00:00.000Z"),
        updated_at: JsString::from("2026-06-21T00:00:00.000Z"),
        cwd: JsString::from("/repo"),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: "on-request".into(),
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
        extra: pi_js_compat::js_json::JsObject::new(),
    }
}

#[tokio::test]
async fn publishes_multi_agent_activity_across_agent_and_tool_events() {
    use pi_coding_agent::core::multi_agent_store::{AgentCurrentActivity, MultiAgentStore, MultiAgentStoreOptions};

    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-06-21T00:00:00.000Z".into()))),
    }));
    store
        .publish_lifecycle_coordinator_snapshot(&running_agent("child-agent"))
        .expect("publish child lifecycle");
    let activities = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&activities);
    let _subscription = store.subscribe_agent_updates(move |_previous, current| {
        recorded.lock().unwrap().push(current.current_activity.clone());
        Ok(())
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
                    (json!({"text": "hi", "query": " inspect\n  files "})
                        .as_object()
                        .cloned()
                        .expect("arguments"))
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
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
        activities.iter().any(|activity| matches!(activity,
            Some(AgentCurrentActivity::Tool { detail: Some(detail), .. }) if detail == "inspect files"
        )),
        "tool arguments must be summarized in published activity: {activities:?}"
    );
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
    let relocate = Arc::new(move |cwd: &JsString| {
        recorded
            .lock()
            .unwrap()
            .push(format!("relocate:{}", encode_node_utf8(cwd)));
        Box::pin(async { Ok(()) })
            as pi_ai::request::BoxFuture<'static, Result<(), pi_coding_agent::core::extensions::types::HandlerError>>
    });
    let recorded = Arc::clone(&calls);
    let prepare = Arc::new(move |cwd: &JsString| {
        recorded
            .lock()
            .unwrap()
            .push(format!("prepare:{}", encode_node_utf8(cwd)));
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
        .relocate_after_tool_result("call-1", &JsString::from("/next"))
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
async fn runtime_mailbox_delivery_prompts_the_session_and_removes_the_accepted_row() {
    use pi_coding_agent::core::session_control_db::{
        EnqueueStoredRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind,
        RuntimeMailboxStoreRef, enqueue_stored_runtime_mailbox_message, list_runtime_mailbox_messages,
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
        message_id: JsString::from("mailbox-1"),
        session_path: session_file,
    };
    let created_at = pi_coding_agent::utils::iso_time::now_iso_string();
    enqueue_stored_runtime_mailbox_message(
        &control_db_path,
        &EnqueueStoredRuntimeMailboxMessageInput {
            recipient: RuntimeMailboxAddress {
                agent_id: None,
                session_id,
            },
            sender: RuntimeMailboxAddress {
                agent_id: Some(JsString::from("parent")),
                session_id: JsString::from("parent-session"),
            },
            kind: RuntimeMailboxMessageKind::Message,
            store_ref,
            recipient_host: None,
            message: JsValue::from(json!({
                "id": "mailbox-1",
                "fromAgentId": "parent",
                "kind": "message",
                "status": "pending",
                "createdAt": created_at,
                "updatedAt": created_at,
                "body": "hello from parent",
            })),
            updated_at: None,
        },
    )
    .expect("enqueue mailbox message");
    fixture.faux.set_responses(vec![
        faux_assistant_message("ack", FauxAssistantMessageOptions::default()).into(),
    ]);
    fixture.session.model_registry().refresh().unwrap();

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
                pi_ai::types::UserContent::Blocks(blocks) => JsString::join(
                    blocks.iter().filter_map(|block| match block {
                        pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                        pi_ai::types::TextOrImageContent::Image(_) => None,
                    }),
                    "",
                ),
            }),
            _ => None,
        })
        .expect("delivered mailbox prompt");
    let user_text_utf8 = encode_node_utf8(&user_text);
    assert!(user_text.contains("From:"), "{user_text_utf8}");
    assert!(user_text.contains("- session: parent-session"), "{user_text_utf8}");
    assert!(user_text.contains("hello from parent"), "{user_text_utf8}");

    let rows = list_runtime_mailbox_messages(&control_db_path).expect("mailbox rows");
    assert!(rows.is_empty(), "accepted mailbox row must be removed: {rows:?}");
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
    fixture.session.model_registry().refresh().unwrap();
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
                session_id: JsString::from("other-session"),
            },
            body: JsString::from("hello channel"),
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
        custom.get("customType").and_then(JsValue::as_string),
        Some(&JsString::from("shared_channel"))
    );
    let content = custom
        .get("content")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_default();
    let content_utf8 = encode_node_utf8(&content);
    assert!(content.contains("From shared channel:"), "{content_utf8}");
    assert!(content.contains("- session: other-session"), "{content_utf8}");
    assert!(content.contains("hello channel"), "{content_utf8}");
    assert_eq!(custom.get("display").and_then(JsValue::as_bool), Some(true));
}

#[tokio::test]
async fn cycle_model_matches_frozen_for_unregistered_scope_and_changed_metadata() {
    use pi_coding_agent::core::model_resolver::ScopedModel;

    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    for scopes in [
        vec![("faux-1", "Faux Model"), ("removed-from-catalog", "Saved Model")],
        vec![("other", "Other"), ("faux-1", "Updated Display Name")],
    ] {
        let fixture = fixture().await;
        let expected = oracle.request(&json!({
            "operation": "cycle-models",
            "dir": fixture._temp_dir.path().join("oracle").display().to_string(),
            "scopes": scopes.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
            "directions": ["forward", "backward"],
        }));
        assert_eq!(expected["ok"], true, "oracle: {expected}");
        let model = fixture.session.model();
        fixture.session.set_scoped_models(
            scopes
                .into_iter()
                .map(|(id, name)| {
                    let mut model = model.clone();
                    model.id = id.to_owned();
                    model.name = name.into();
                    ScopedModel {
                        model,
                        thinking_level: Some(ThinkingLevel::High),
                    }
                })
                .collect(),
        );
        let mut actual = Vec::new();
        for direction in [CycleDirection::Forward, CycleDirection::Backward] {
            let result = fixture.session.cycle_model(direction).await.expect("cycle");
            actual.push(result.map(|result| {
                json!({
                    "id": result.model.id,
                    "name": result.model.name,
                    "thinkingLevel": result.thinking_level.as_str(),
                    "isScoped": result.is_scoped,
                })
            }));
        }
        assert_eq!(json!(actual), expected["value"]);
    }
}

#[tokio::test]
async fn quota_fallback_visits_all_three_accounts_once_like_frozen() {
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        Some(Vec::new()),
        FixtureOverrides {
            faux_provider_id: Some("openai-codex".to_owned()),
            ..Default::default()
        },
    )
    .await;
    fixture.session.set_auto_retry_enabled(false);
    let model = fixture.session.model();
    for provider in ["openai-codex-gc", "openai-codex-team"] {
        fixture
            .session
            .model_registry()
            .register_provider(
                provider,
                pi_coding_agent::core::model_registry::ProviderConfigInput {
                    api: Some(model.api.clone()),
                    base_url: Some(model.base_url.clone()),
                    api_key: Some("faux-key".into()),
                    models: Some(vec![model.clone().into()]),
                    ..Default::default()
                },
            )
            .expect("fallback provider");
    }
    let providers = Arc::new(std::sync::Mutex::new(Vec::new()));
    let requested = Arc::clone(&providers);
    fixture
        .session
        .agent()
        .set_stream_fn(Arc::new(move |model, context, options| {
            requested.lock().unwrap().push(model.provider.clone());
            let account = faux_provider(RegisterFauxProviderOptions {
                provider: Some(model.provider.clone()),
                api: Some(model.api.clone()),
                ..Default::default()
            });
            account.set_responses(vec![
                faux_assistant_message(
                    "",
                    FauxAssistantMessageOptions {
                        stop_reason: Some(pi_ai::StopReason::Error),
                        error_message: Some("usage limit".into()),
                        ..Default::default()
                    },
                )
                .into(),
            ]);
            let stream = account.provider.stream_simple(model, context, options);
            Box::pin(async move { stream.map_err(pi_agent_core::agent::AgentError::from) })
        }));
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({
        "operation": "quota-fallback", "dir": fixture._temp_dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(expected["ok"], true, "oracle: {expected}");
    assert_eq!(
        expected["value"]["requestedProviders"],
        json!(["openai-codex", "openai-codex-gc", "openai-codex-team"])
    );
    fixture
        .session
        .prompt("quota test", PromptOptions::default())
        .await
        .expect("prompt");
    assert_eq!(
        json!({"requestedProviders": *providers.lock().unwrap(), "provider": fixture.session.model().provider}),
        expected["value"]
    );
}

#[tokio::test]
async fn idle_steer_returns_before_the_provider_turn_finishes() {
    let fixture = fixture().await;
    fixture
        .session
        .agent()
        .append_message(AgentMessage::User(pi_ai::UserMessage {
            role: pi_ai::UserRole::Tag,
            content: pi_ai::UserContent::Text("earlier request".into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        }));
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (started_provider, release_provider) = (Arc::clone(&started), Arc::clone(&release));
    fixture
        .faux
        .set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_context, _options, _state, _model| {
                let (started, release) = (Arc::clone(&started_provider), Arc::clone(&release_provider));
                Box::pin(async move {
                    started.notify_one();
                    release.notified().await;
                    Ok(faux_assistant_message("steered", Default::default()))
                })
            },
        ))]);
    let session = fixture.session.clone();
    let mut steer = tokio::spawn(async move { session.steer("idle steer", &[]).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
        .await
        .expect("provider started");
    let returned = tokio::time::timeout(std::time::Duration::from_millis(100), &mut steer).await;
    release.notify_one();
    if returned.is_err() {
        steer.await.expect("steer task").expect("steer result");
    }
    fixture.session.agent().wait_for_idle().await;
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({
        "operation": "idle-steer", "dir": fixture._temp_dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(expected, json!({"ok": true, "value": true}));
    assert_eq!(
        returned.is_ok(),
        expected["value"].as_bool().expect("oracle result"),
        "steer must return without waiting for the provider response"
    );
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
    let updates = Arc::new(std::sync::Mutex::new(Vec::<JsString>::new()));
    let sink = Arc::clone(&updates);
    let committed = Arc::new(AtomicUsize::new(0));
    let committed_sink = Arc::clone(&committed);
    fixture.session.subscribe(Arc::new(move |event| match event {
        AgentSessionEvent::BashMessagesCommitted(_) => {
            committed_sink.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    }));

    let result = fixture
        .session
        .execute_bash(
            &JsString::from("printf 'hello'"),
            Some(Arc::new(move |delta: &JsString| {
                sink.lock().unwrap().push(delta.clone());
                Ok(())
            })),
            ExecuteBashOptions::default(),
        )
        .await
        .expect("bash succeeds");

    assert!(result.output.contains("hello"), "output: {:?}", result.output);
    assert_eq!(result.exit_code, Some(0.0));
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
    let message_end_roles = Arc::new(std::sync::Mutex::new(Vec::<JsString>::new()));
    let sink = Arc::clone(&message_end_roles);
    fixture.session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::Agent(pi_agent_core::agent::types::AgentEvent::MessageEnd { message }) = event {
            sink.lock().unwrap().push(message.role());
        }
    }));

    fixture.session.record_bash_result(
        &JsString::from("echo hi"),
        &BashResult {
            output: JsString::from("hi"),
            exit_code: Some(0.0),
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

#[tokio::test]
async fn bash_exit_numbers_use_js_spelling_in_session_jsonl_and_rpc_bytes() {
    use pi_coding_agent::core::bash_executor::BashResult;
    use pi_coding_agent::modes::rpc::serialize_json_line;
    use pi_js_compat::js_json::{encode_node_utf8, stringify_lossless, to_value};

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
    for (code, spelling) in [
        (0.0, "0"),
        (7.0, "7"),
        (-0.0, "0"),
        (1.5, "1.5"),
        (1e21, "1e+21"),
        (f64::NAN, "null"),
    ] {
        let result = BashResult {
            output: "payload".into(),
            exit_code: Some(code),
            cancelled: false,
            truncated: false,
            full_output_path: None,
        };
        let expected =
            format!("{{\"output\":\"payload\",\"exitCode\":{spelling},\"cancelled\":false,\"truncated\":false}}");
        let value = to_value(&result).unwrap();
        assert_eq!(stringify_lossless(&value), expected);
        assert_eq!(serialize_json_line(&value).unwrap(), expected.clone() + "\n");
        fixture.session.record_bash_result(&"probe".into(), &result, None);
        fixture.session.session_manager().persist_for_recovery().unwrap();
        let path = fixture.session.session_file().unwrap();
        let bytes = std::fs::read_to_string(encode_node_utf8(&path)).unwrap();
        let line = bytes.lines().last().unwrap();
        // Exact numeric token in the actual persisted message, not a parsed-value equality.
        let suffix = line.split_once("\"exitCode\":").expect("persisted bash code").1;
        assert_eq!(suffix.split(',').next().unwrap(), spelling, "{line}");
    }
}

struct BlockingBashOperations;

impl pi_coding_agent::core::tools::bash::BashOperations for BlockingBashOperations {
    fn exec<'a>(
        &'a self,
        _command: &'a JsString,
        _cwd: &'a JsString,
        options: pi_coding_agent::core::tools::bash::BashExecOptions,
    ) -> pi_ai::request::BoxFuture<
        'a,
        Result<pi_coding_agent::core::tools::bash::BashExecResult, pi_agent_core::agent::ToolError>,
    > {
        Box::pin(async move {
            if let Some(signal) = options.signal {
                signal.wait().await;
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
                &JsString::from("sleep"),
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
async fn passes_through_oversized_tool_result_images_unchanged() {
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
                serde_json::Map::new()
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
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

#[tokio::test]
async fn keeps_oversized_tool_result_images_when_auto_resize_is_disabled() {
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert(
        JsString::from("images"),
        JsValue::from(serde_json::json!({"autoResize": false})),
    );
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
                serde_json::Map::new()
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
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

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Return an oversized screenshot".into()
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| serde_json::json!({"type": "object", "properties": {}}))
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "Screenshot".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: JsValue,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
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
    executed: Arc<std::sync::Mutex<Vec<JsString>>>,
}

impl pi_agent_core::agent::types::AgentTool for PermissionEchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Echo text back".into()
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}))
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "Echo".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
        _on_update: pi_agent_core::agent::types::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<pi_agent_core::agent::types::AgentToolResult, pi_agent_core::agent::types::ToolError>,
    > {
        let text = params
            .get("text")
            .and_then(JsValue::as_string)
            .cloned()
            .unwrap_or_default();
        self.executed.lock().unwrap().push(text.clone());
        Box::pin(async move { Ok(pi_agent_core::agent::types::AgentToolResult::text(text)) })
    }
}

struct ApprovalPromptTool {
    name: String,
    calls: Arc<std::sync::Mutex<Vec<JsValue>>>,
    response: serde_json::Value,
}

impl pi_agent_core::agent::types::AgentTool for ApprovalPromptTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Approve tool calls".into()
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

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "Approval".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
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
        [(JsString::from("text"), JsValue::from(json!(text)))]
            .into_iter()
            .collect(),
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
    settings.insert(
        JsString::from("permissionPromptTool"),
        JsValue::from(json!("mcp__approval__prompt")),
    );
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
    assert_eq!(calls[0]["tool_name"], JsValue::from(json!("echo")));
    assert_eq!(calls[0]["input"]["text"], JsValue::from(json!("original")));
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
    settings.insert(
        JsString::from("permissionPromptTool"),
        JsValue::from(json!("mcp__approval__prompt")),
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
    settings.insert(
        JsString::from("permissionPromptTool"),
        JsValue::from(json!("mcp__approval__prompt")),
    );
    settings.insert(
        JsString::from("permissionRules"),
        JsValue::from(json!({"allow":{"echo":[json!({"text":"original"}).to_string()]}})),
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
    let runtime = model_registry(&faux).await;
    let model = faux.get_model(None).expect("faux model");
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            model: Some(model),
            system_prompt: Some(JsString::default()),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(Vec::new()),
            messages: None,
        }),
        stream_fn: Some(stream_fn(&faux)),
        ..Default::default()
    })
    .expect("agent");
    let cwd = temp_dir.path().display().to_string();
    let session_dir = temp_dir.path().join("sessions").display().to_string();
    let control_db = temp_dir.path().join("control.db").display().to_string();
    let mut session_manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
        .expect("session manager");
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    session_manager
        .append_message(AgentMessage::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text(JsString::from("hello")),
            input_source: None,
            provider_native: None,
            timestamp: 1_700_000_000_000.0,
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
        cwd: JsString::from(cwd.clone()),
        scoped_models: Vec::new(),
        resource_loader: pi_coding_agent::core::resource_loader::StaticResourceLoader::new(),
        model_registry: runtime,
        initial_active_tool_names: None,
        allowed_tool_names: None,
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(JsString::from(cwd)),
        control_db_path: Some(JsString::from(control_db)),
        base_tools: Vec::new(),
        custom_tool_definitions: Vec::new(),
        custom_tools: Vec::<ToolDefinition>::new(),
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: None,
        no_supervisor: None,
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
                content: UserContent::Text(JsString::from("kept")),
                input_source: None,
                provider_native: None,
                timestamp: 1.0,
            }))
            .expect("user message");
        let id = manager
            .append_compaction(
                &JsString::from("original summary"),
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
    if std::path::Path::new(&encode_node_utf8(&session_file)).exists() {
        std::fs::remove_file(&encode_node_utf8(&session_file)).expect("remove session file");
    }
    std::fs::create_dir(&encode_node_utf8(&session_file)).expect("replace with directory");

    let before = session.messages();
    let error = session
        .update_compaction_summary(&compaction_id, &JsString::from("edited summary"))
        .expect_err("persistence failure must surface");
    assert!(!error.message().is_empty());
    assert_eq!(session.messages(), before);
    let compaction = session
        .session_manager()
        .get_entry(&compaction_id)
        .and_then(|entry| entry.as_compaction())
        .map(std::borrow::Cow::into_owned)
        .expect("compaction entry");
    assert_eq!(compaction.summary, "original summary");
}

// ---------------------------------------------------------------------------
// Human "Allow always" and read-only LLM auto-approval
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ScriptedApprovalUi {
    selections: std::sync::Mutex<VecDeque<Option<JsString>>>,
    titles: std::sync::Mutex<Vec<JsString>>,
    options: std::sync::Mutex<Vec<Vec<JsString>>>,
}

impl ScriptedApprovalUi {
    fn with_selection(selection: &str) -> Self {
        let ui = Self::default();
        ui.selections
            .lock()
            .unwrap()
            .push_back(Some(JsString::from(selection.to_owned())));
        ui
    }
}

impl ExtensionUiContext for ScriptedApprovalUi {
    fn select<'a>(
        &'a self,
        title: &JsString,
        options: &[JsString],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> pi_ai::request::BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.titles.lock().unwrap().push(title.clone());
        self.options.lock().unwrap().push(options.to_vec());
        let selection = self.selections.lock().unwrap().pop_front().flatten();
        Box::pin(async move { Ok(selection) })
    }
}

struct SpawnRecorderTool {
    calls: Arc<std::sync::Mutex<Vec<(String, JsValue)>>>,
}

impl pi_agent_core::agent::types::AgentTool for SpawnRecorderTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Spawn a background agent".into()
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

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "Spawn Agent".into()
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
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
    settings.insert(JsString::from("approvalPolicy"), JsValue::from(json!("on-request")));
    settings.insert(JsString::from("approvalPreset"), JsValue::from(json!("ask-me")));
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
        .await
        .expect("bind extensions");
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
        params.get("agentType").and_then(JsValue::as_string),
        Some(&JsString::from("permission-config"))
    );
    assert_eq!(
        params.get("displayName").and_then(JsValue::as_string),
        Some(&JsString::from("Allow command"))
    );
    let prompt = params.get("prompt").and_then(JsValue::as_string).expect("prompt");
    assert!(
        prompt.contains("Target project: /syncthing/Sync/Projects/claude/claude-bash-hook"),
        "prompt: {prompt:?}"
    );
    assert!(prompt.contains("Original cwd: "), "prompt: {prompt:?}");
    assert!(prompt.contains("Tool name: echo"), "prompt: {prompt:?}");
    assert!(prompt.contains("\"text\": \"hello\""), "prompt: {prompt:?}");
    let expected_input = "{\n  \"1\": 1,\n  \"2\": 2,\n  \"text\": \"hello\"\n}";
    assert!(prompt.contains(expected_input), "prompt: {prompt:?}");
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
) -> (SupervisorDecisionRequester, Arc<std::sync::Mutex<Vec<JsString>>>) {
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = Arc::clone(&calls);
    let requester: SupervisorDecisionRequester = Arc::new(move |input| {
        recorded.lock().unwrap().push(
            input
                .payload
                .get("toolName")
                .and_then(JsValue::as_string)
                .cloned()
                .unwrap_or_default(),
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
            reason: JsString::from("should not be consulted"),
        });
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert(JsString::from("approvalPolicy"), JsValue::from(json!("on-request")));
    settings.insert(
        JsString::from("approvalPreset"),
        JsValue::from(json!("llm-approved-ask")),
    );
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
                (json!({"pattern": "phpstan"}).as_object().cloned().expect("arguments"))
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
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
        .collect::<Vec<_>>();
    let text = JsString::join(text, "");
    assert!(text.contains("a.txt"), "tool result: {}", encode_node_utf8(&text));
    assert!(!tool_result.is_error);
}

fn custom_grep_definition(
    executed: Arc<std::sync::Mutex<Vec<String>>>,
) -> pi_coding_agent::core::extensions::types::ToolDefinition {
    pi_coding_agent::core::extensions::types::ToolDefinition {
        name: "grep".to_owned(),
        label: "Custom Grep".into(),
        description: "Custom tool using a built-in name".into(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type": "object", "properties": {"pattern": {"type": "string"}}}),
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
            reason: JsString::from("custom tool"),
        });
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert(JsString::from("approvalPolicy"), JsValue::from(json!("on-request")));
    settings.insert(
        JsString::from("approvalPreset"),
        JsValue::from(json!("llm-approved-deny")),
    );
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
                (json!({"pattern": "phpstan"}).as_object().cloned().expect("arguments"))
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
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

    fn description(&self) -> pi_js_compat::js_json::JsString {
        "Slow tool".into()
    }

    fn parameters(&self) -> &serde_json::Value {
        static PARAMETERS: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| json!({"type": "object", "properties": {}}))
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        "Slow".into()
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: JsValue,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
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
    pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(|_context, options, _state, _model| {
        Box::pin(async move {
            options
                .and_then(|options| options.stream.request.signal)
                .expect("provider signal")
                .wait()
                .await;
            Ok(faux_assistant_message("Never delivered", Default::default()))
        })
    }))
}

fn tool_call_response(name: &str, arguments: serde_json::Value, id: &str) -> pi_ai::providers::faux::FauxResponseStep {
    faux_assistant_message(
        pi_ai::providers::faux::FauxContent::Blocks(vec![pi_ai::providers::faux::faux_tool_call(
            name,
            arguments
                .as_object()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
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
            thinking_phase_timeout_ms: Some(1_000.0),
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![
        tool_call_response("slow", json!({}), "call-1"),
        pending_response(),
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
    assert_eq!(error.message(), "Main session thinking phase exceeded 20 minutes");
}

#[tokio::test(start_paused = true)]
async fn thinking_phase_deadline_reports_the_child_subject() {
    use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};

    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-06-21T00:00:00.000Z".into()))),
    }));
    store
        .publish_lifecycle_coordinator_snapshot(&running_agent("child-agent"))
        .expect("publish child lifecycle");
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            multi_agent: Some((store, "child-agent".to_owned())),
            thinking_phase_timeout_ms: Some(1_000.0),
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![pending_response(), pending_response()]);
    let session = fixture.session.clone();
    let mut handle = tokio::spawn(async move { session.prompt("hi", PromptOptions::default()).await });

    drive_until_finished(&mut handle).await;
    let error = handle
        .await
        .expect("prompt task")
        .expect_err("thinking deadline must abort the run");
    assert_eq!(error.message(), "Child agent thinking phase exceeded 20 minutes");
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
            thinking_phase_timeout_ms: Some(1_000.0),
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

// Frozen agent-session-thinking-recovery.test.ts: one allowance per public dispatch.
async fn timeout_fixture(timeout: Option<f64>) -> Fixture {
    fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            thinking_phase_timeout_ms: timeout,
            ..Default::default()
        },
    )
    .await
}

async fn wait_for_faux_calls(fixture: &Fixture, count: u64) {
    for _ in 0..100 {
        if fixture.faux.state().lock().unwrap().call_count >= count {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("expected {count} provider calls");
}

fn completed_text_response(text: &str) -> pi_ai::providers::faux::FauxResponseStep {
    faux_assistant_message(
        vec![
            pi_ai::providers::faux::faux_text(text),
            pi_ai::providers::faux::faux_tool_call(
                "end_turn",
                pi_js_compat::js_json::JsObject::from_iter([("reason".into(), JsValue::String(text.into()))]),
                Some(format!("end-{text}")),
            ),
        ],
        FauxAssistantMessageOptions::default(),
    )
    .into()
}

fn completed_timeout_response(text: &str) -> pi_ai::providers::faux::FauxResponseStep {
    tool_call_response("end_turn", json!({"reason":text}), text)
}

#[tokio::test(start_paused = true)]
async fn thinking_timeout_default_recovers_once_without_duplicate_input() {
    let fixture = timeout_fixture(None).await;
    fixture
        .faux
        .set_responses(vec![pending_response(), completed_timeout_response("Recovered")]);
    let session = fixture.session.clone();
    let task = tokio::spawn(async move { session.prompt("Original", Default::default()).await });
    wait_for_faux_calls(&fixture, 1).await;
    tokio::time::advance(std::time::Duration::from_millis(20 * 60 * 1000 - 1)).await;
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    assert_eq!(fixture.faux.state().lock().unwrap().call_count, 1);
    tokio::time::advance(std::time::Duration::from_millis(1)).await;
    task.await.unwrap().unwrap();
    assert_eq!(fixture.faux.state().lock().unwrap().call_count, 2);
    assert_eq!(
        fixture
            .session
            .messages()
            .iter()
            .filter(|message| matches!(message, AgentMessage::User(_)))
            .count(),
        1
    );
    assert!(!fixture.session.has_active_retry());
}

#[tokio::test(start_paused = true)]
async fn thinking_timeout_second_failure_leaves_next_response_for_new_prompt() {
    let fixture = timeout_fixture(Some(1000.0)).await;
    fixture.faux.set_responses(vec![
        pending_response(),
        pending_response(),
        completed_timeout_response("Next"),
    ]);
    let session = fixture.session.clone();
    let mut task = tokio::spawn(async move { session.prompt("Work", Default::default()).await });
    wait_for_faux_calls(&fixture, 1).await;
    tokio::time::advance(std::time::Duration::from_millis(1000)).await;
    wait_for_faux_calls(&fixture, 2).await;
    assert!(!task.is_finished());
    assert!(fixture.session.has_active_retry());
    drive_until_finished(&mut task).await;
    assert_eq!(
        task.await.unwrap().unwrap_err().message(),
        "Main session thinking phase exceeded 20 minutes"
    );
    assert_eq!(fixture.faux.get_pending_response_count(), 1);
    assert!(!fixture.session.has_active_retry());
    fixture.session.prompt("New", Default::default()).await.unwrap();
    assert_eq!(fixture.faux.get_pending_response_count(), 0);
}

#[tokio::test(start_paused = true)]
async fn thinking_timeout_allowance_resets_for_prompt_and_continuation() {
    let fixture = timeout_fixture(Some(1000.0)).await;
    for continuing in [false, false, true] {
        fixture
            .faux
            .set_responses(vec![pending_response(), completed_timeout_response("Recovered")]);
        let session = fixture.session.clone();
        let mut task = tokio::spawn(async move {
            if continuing {
                session.continue_run().await
            } else {
                session.prompt("New", Default::default()).await
            }
        });
        drive_until_finished(&mut task).await;
        task.await.unwrap().unwrap();
        assert!(!fixture.session.has_active_retry());
    }
    assert_eq!(
        fixture
            .session
            .messages()
            .iter()
            .filter(|message| matches!(message, AgentMessage::User(_)))
            .count(),
        2
    );
}

#[tokio::test(start_paused = true)]
async fn thinking_timeout_manual_and_retry_cancellation_stop_recovery() {
    for (recover_first, retry_api) in [(false, false), (true, false), (true, true)] {
        let fixture = timeout_fixture(Some(1000.0)).await;
        let retry_ends = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = retry_ends.clone();
        fixture.session.subscribe(Arc::new(move |event| {
            if let AgentSessionEvent::AutoRetryEnd(event) = event {
                sink.lock().unwrap().push((event.success, event.final_error.clone()));
            }
        }));
        fixture.faux.set_responses(vec![
            pending_response(),
            pending_response(),
            completed_timeout_response("Never"),
        ]);
        let session = fixture.session.clone();
        let task = tokio::spawn(async move { session.prompt("Work", Default::default()).await });
        wait_for_faux_calls(&fixture, 1).await;
        if recover_first {
            tokio::time::advance(std::time::Duration::from_millis(1000)).await;
            wait_for_faux_calls(&fixture, 2).await;
        }
        if retry_api {
            fixture.session.abort_retry();
        } else {
            fixture.session.abort().await.unwrap();
        }
        task.await.unwrap().unwrap();
        assert_eq!(
            fixture.faux.state().lock().unwrap().call_count,
            if recover_first { 2 } else { 1 }
        );
        assert!(!fixture.session.has_active_retry());
        if recover_first {
            assert_eq!(
                *retry_ends.lock().unwrap(),
                vec![(false, Some(JsString::from("Retry cancelled")))]
            );
        } else {
            assert!(retry_ends.lock().unwrap().is_empty());
        }
    }
}

// ---------------------------------------------------------------------------
// Activity publication errors surface through the agent event listener
// ---------------------------------------------------------------------------

struct FakePersistence;

impl pi_coding_agent::core::multi_agent_store::SessionPersistence for FakePersistence {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some("/tmp/pi-session-final/control.db".into())
    }

    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some("/tmp/pi-session-final/session.jsonl".into())
    }
}

/// Frozen agent-session.ts:1650,1766-1774 throws from `_handleAgentEvent`, an Agent listener;
/// frozen agent.ts:615-620 reports listener errors and continues the run.
#[tokio::test]
async fn activity_publication_errors_are_reported_without_aborting_the_run() {
    use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};

    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store
        .publish_lifecycle_coordinator_snapshot(&running_agent("child-agent"))
        .expect("publish child lifecycle");
    store.set_persistence_session_manager(Some(&FakePersistence));
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            multi_agent: Some((Arc::clone(&store), "child-agent".to_owned())),
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message("done", FauxAssistantMessageOptions::default()).into(),
    ]);

    fixture
        .session
        .prompt("hi", PromptOptions::default())
        .await
        .expect("listener publication errors do not abort the run");
    assert!(
        fixture
            .session
            .messages()
            .iter()
            .any(|message| matches!(message, AgentMessage::Assistant(_)))
    );
    let child = store.get_agent(&JsString::from("child-agent")).expect("child agent");
    assert_eq!(child.current_activity, None, "rejected publication left no activity");
}

struct FailingApprovalUi;

impl ExtensionUiContext for FailingApprovalUi {
    fn select<'a>(
        &'a self,
        _title: &JsString,
        _options: &[JsString],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> pi_ai::request::BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        Box::pin(async {
            Err(pi_ai::request::CallbackError::from_rejection(
                JsValue::String(JsString::from("dialog failed")).into(),
            ))
        })
    }
}

fn tool_result_text(fixture: &Fixture) -> (bool, JsString) {
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
        .collect::<Vec<_>>();
    (result.is_error, JsString::join(text, ""))
}

#[tokio::test]
async fn human_approval_ui_errors_abort_the_tool_call() {
    let executed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert(JsString::from("approvalPolicy"), JsValue::from(json!("on-request")));
    settings.insert(JsString::from("approvalPreset"), JsValue::from(json!("ask-me")));
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
        .await
        .expect("bind extensions");
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
        Arc::new(|_input| Box::pin(async { Err(JsString::from("supervisor unavailable")) }));
    let mut settings = pi_coding_agent::core::settings_manager::Settings::new();
    settings.insert(JsString::from("approvalPolicy"), JsValue::from(json!("on-request")));
    settings.insert(
        JsString::from("approvalPreset"),
        JsValue::from(json!("llm-approved-deny")),
    );
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
    fn detach(&self) -> Result<bool, pi_js_compat::callback_error::CallbackError> {
        self.detached.store(true, Ordering::SeqCst);
        Ok(self.accepts)
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
    assert!(!fixture.session.detach_running_tool().expect("fixture detach callback"));
    assert!(!fixture.session.detach_bash_tool().expect("fixture detach callback"));

    let detached = Arc::new(AtomicBool::new(false));
    let subscription = registry.register(Arc::new(RecordingDetachHandle {
        detached: Arc::clone(&detached),
        accepts: false,
    }));
    assert!(fixture.session.has_detachable_tool());
    assert!(fixture.session.has_detachable_bash_tool());
    assert!(!fixture.session.detach_running_tool().expect("fixture detach callback"));
    assert!(detached.load(Ordering::SeqCst));

    let accepted = Arc::new(AtomicBool::new(false));
    let accepted_subscription = registry.register(Arc::new(RecordingDetachHandle {
        detached: Arc::clone(&accepted),
        accepts: true,
    }));
    assert!(fixture.session.detach_bash_tool().expect("fixture detach callback"));
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

#[tokio::test]
async fn registered_commands_precede_terminal_outbox_drain_and_normal_prompt_coordinates_afterward() {
    use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
    use pi_coding_agent::core::extensions::{
        ExtensionCommandContext, ExtensionFactory, HandlerError, create_extension_runtime, load_extension_from_factory,
    };
    use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let db_path = temp.path().join("control.sqlite");
    let db_name = db_path.to_str().unwrap();
    let events = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let command_events = Arc::clone(&events);
    let runtime = create_extension_runtime();
    let factory: ExtensionFactory = Arc::new(move |api| {
        let command_events = Arc::clone(&command_events);
        api.register_command(
            "ordercmd",
            None,
            None,
            Arc::new(move |_args: JsString, _ctx: ExtensionCommandContext| {
                let command_events = Arc::clone(&command_events);
                Box::pin(async move {
                    command_events.lock().unwrap().push("command".to_owned());
                    Ok(())
                }) as pi_coding_agent::core::extensions::BoxFuture<'static, Result<(), HandlerError>>
            }),
        )
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let bus: Arc<dyn EventBus> = Arc::new(EventBusController::new());
    let extension = load_extension_from_factory(
        factory,
        &cwd.into(),
        bus,
        runtime.clone(),
        &"private-order-fixture".into(),
    )
    .await
    .unwrap();
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![faux_model()]),
        ..Default::default()
    });
    faux.set_responses(vec![faux_assistant_message("done", Default::default()).into()]);
    let registry = model_registry(&faux).await;
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            model: Some(faux.get_model(None).unwrap()),
            system_prompt: Some("".into()),
            thinking_level: Some(ThinkingLevel::Off),
            tools: Some(Vec::new()),
            messages: None,
        }),
        stream_fn: Some(stream_fn(&faux)),
        ..Default::default()
    })
    .unwrap();
    let mut manager = SessionManager::create(
        &cwd.into(),
        Some(&temp.path().join("sessions").to_str().unwrap().into()),
        None,
    )
    .unwrap();
    manager
        .set_metadata_control_db_path(Some(&db_name.into()), Default::default())
        .unwrap();
    manager.persist_for_recovery().unwrap();
    let session_path = manager.get_session_file().unwrap().clone();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&manager));
    let session = AgentSession::new(AgentSessionConfig {
        agent,
        session_manager: Arc::new(std::sync::Mutex::new(manager)),
        settings_manager: Arc::new(std::sync::Mutex::new(SettingsManager::in_memory(
            Default::default(),
            Default::default(),
        ))),
        cwd: cwd.into(),
        scoped_models: Vec::new(),
        resource_loader: Arc::new(pi_coding_agent::core::resource_loader::StaticResourceLoader {
            extensions: Some(pi_coding_agent::core::extensions::types::LoadExtensionsResult {
                extensions: vec![extension],
                errors: Vec::new(),
                runtime,
            }),
            ..Default::default()
        }),
        model_registry: registry,
        initial_active_tool_names: Some(Vec::new()),
        allowed_tool_names: Some(Vec::new()),
        excluded_tool_names: None,
        permission_prompt_tool: None,
        agent_dir: Some(cwd.into()),
        control_db_path: Some(db_name.into()),
        base_tools: Vec::new(),
        custom_tool_definitions: Vec::new(),
        custom_tools: Vec::new(),
        multi_agent_runtime_role: None,
        multi_agent_execution_capability: None,
        thinking_phase_timeout_ms: None,
        no_supervisor: None,
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
    .unwrap();
    let db = pi_coding_agent::core::sqlite::create_sqlite_database(db_name).unwrap();
    db.execute("INSERT INTO multi_agent_terminal_outbox(session_path,agent_id,terminal_revision,event_kind,status,attempt_count,updated_at) VALUES (?,'missing',2,'completed','pending',0,?)",
        rusqlite::params![session_path, pi_coding_agent::utils::iso_time::now_iso_string()]).unwrap();
    let at_start = Arc::new(std::sync::Mutex::new(None));
    let observed_start = Arc::clone(&at_start);
    let event_sink = Arc::clone(&events);
    let event_db = db_name.to_owned();
    session.subscribe(Arc::new(move |event| {
        if matches!(
            event,
            AgentSessionEvent::Agent(pi_agent_core::agent::types::AgentEvent::AgentStart)
        ) {
            event_sink.lock().unwrap().push("prompt".to_owned());
            let db = pi_coding_agent::core::sqlite::create_sqlite_database(&event_db).unwrap();
            *observed_start.lock().unwrap() = Some(
                db.query_row(
                    "SELECT attempt_count FROM multi_agent_terminal_outbox WHERE agent_id='missing'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            );
        }
    }));
    session.prompt("/ordercmd", Default::default()).await.unwrap();
    let after_command: i64 = db
        .query_row(
            "SELECT attempt_count FROM multi_agent_terminal_outbox WHERE agent_id='missing'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    session.prompt("hello", Default::default()).await.unwrap();
    let after_prompt: i64 = db
        .query_row(
            "SELECT attempt_count FROM multi_agent_terminal_outbox WHERE agent_id='missing'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    session.dispose();
    let source_dir = temp.path().join("frozen");
    std::fs::create_dir(&source_dir).unwrap();
    let mut source = tools_oracle::TsOracle::spawn("storage-prompt-order-oracle.mts", "packages/coding-agent/src/core");
    let expected = source.request(&json!({"id":1,"dir":source_dir}));
    assert_eq!(expected["ok"], true, "{expected}");
    let actual = json!({"afterCommand":after_command,"atStart":*at_start.lock().unwrap(),"afterPrompt":after_prompt,"events":events.lock().unwrap().clone()});
    assert_eq!(actual, expected["output"]);
    assert_eq!(after_command, 0);
    assert_eq!(*at_start.lock().unwrap(), Some(0));
    assert!(after_prompt > 0);
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
            Some("Test command".into()),
            None,
            Arc::new(|_args: JsString, _ctx: ExtensionCommandContext| {
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
    let extension = load_extension_from_factory(
        factory,
        &JsString::from("/tmp"),
        event_bus,
        runtime.clone(),
        &JsString::from("/tmp/testcmd.ts"),
    )
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

#[path = "support/scripted_registry.rs"]
mod scripted_registry;

async fn idle_fixture(api: &str, settings: serde_json::Value) -> Fixture {
    fixture_with_multi_agent(
        serde_json::from_value(settings).unwrap(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            faux_api: Some(api.to_owned()),
            context_window: Some(2_000_000.0),
            ..Default::default()
        },
    )
    .await
}

fn idle_compaction_count(fixture: &Fixture) -> usize {
    fixture
        .session
        .session_manager()
        .get_entries()
        .iter()
        .filter(|entry| entry.entry_type() == "compaction")
        .count()
}

async fn wait_for_idle_compaction(fixture: &Fixture) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while idle_compaction_count(fixture) == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("idle compaction settles");
}

async fn idle_conversation(fixture: &Fixture, large: bool) {
    fixture.faux.set_responses(vec![
        completed_timeout_response("first"),
        completed_timeout_response("second"),
        faux_assistant_message("idle summary", Default::default()).into(),
    ]);
    let prompt = if large { "x".repeat(410_000) } else { "small".to_owned() };
    fixture.session.prompt(prompt, Default::default()).await.unwrap();
    fixture.session.prompt("follow-up", Default::default()).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn idle_compaction_runs_once_at_provider_cache_deadline() {
    for (api, minutes) in [("openai-codex-responses", 27), ("claude-bridge", 54)] {
        let fixture = idle_fixture(api, json!({})).await;
        let reasons = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = reasons.clone();
        fixture.session.subscribe(Arc::new(move |event| {
            if let AgentSessionEvent::CompactionEnd(event) = event {
                sink.lock().unwrap().push((event.reason.clone(), event.aborted));
            }
        }));
        idle_conversation(&fixture, true).await;
        tokio::time::advance(std::time::Duration::from_secs(minutes * 60 - 1)).await;
        tokio::task::yield_now().await;
        assert_eq!(idle_compaction_count(&fixture), 0);
        tokio::time::advance(std::time::Duration::from_secs(2)).await;
        wait_for_idle_compaction(&fixture).await;
        assert_eq!(idle_compaction_count(&fixture), 1, "{api}");
        assert_eq!(*reasons.lock().unwrap(), vec![("idle".to_owned(), false)]);
        tokio::time::advance(std::time::Duration::from_secs(3 * 60 * 60)).await;
        tokio::task::yield_now().await;
        assert_eq!(idle_compaction_count(&fixture), 1);
        fixture.session.dispose();
    }
}

#[tokio::test(start_paused = true)]
async fn idle_compaction_skips_ineligible_context_and_changed_reference() {
    for (api, settings, large, replace) in [
        ("faux", json!({}), true, false),
        ("openai-codex-responses", json!({}), false, false),
        (
            "openai-codex-responses",
            json!({"compaction":{"idle":false}}),
            true,
            false,
        ),
        (
            "openai-codex-responses",
            json!({"compaction":{"enabled":false}}),
            true,
            false,
        ),
        ("openai-codex-responses", json!({}), true, true),
    ] {
        let fixture = idle_fixture(api, settings).await;
        idle_conversation(&fixture, large).await;
        if replace {
            fixture.session.agent().set_messages(fixture.session.messages());
        }
        tokio::time::advance(std::time::Duration::from_secs(3 * 60 * 60)).await;
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
        assert_eq!(idle_compaction_count(&fixture), 0, "{api}, replaced={replace}");
        fixture.session.dispose();
    }
}

#[tokio::test(start_paused = true)]
async fn idle_compaction_rechecks_queue_model_disposal_and_manual_boundary() {
    for change in ["queued", "model", "disposed", "manual"] {
        let fixture = idle_fixture("openai-codex-responses", json!({})).await;
        idle_conversation(&fixture, true).await;
        match change {
            "queued" => fixture
                .session
                .agent()
                .follow_up(AgentMessage::User(pi_ai::UserMessage {
                    role: pi_ai::UserRole::Tag,
                    content: pi_ai::UserContent::Text("queued".into()),
                    input_source: None,
                    provider_native: None,
                    timestamp: 0.0,
                })),
            "model" => {
                let mut model = fixture.session.model();
                model.id = "switched".to_owned();
                fixture.session.set_model(model).await.unwrap();
            }
            "disposed" => fixture.session.dispose(),
            "manual" => {
                fixture.session.compact(None).await.unwrap();
            }
            _ => unreachable!(),
        }
        let before = idle_compaction_count(&fixture);
        tokio::time::advance(std::time::Duration::from_secs(3 * 60 * 60)).await;
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
        assert_eq!(idle_compaction_count(&fixture), before, "{change}");
        fixture.session.dispose();
    }
}

fn end_turn_at(text: &str, timestamp: f64) -> pi_ai::providers::faux::FauxResponseStep {
    let pi_ai::providers::faux::FauxResponseStep::Message(mut message) = completed_timeout_response(text) else {
        unreachable!()
    };
    message.timestamp = timestamp;
    pi_ai::providers::faux::FauxResponseStep::Message(message)
}

#[tokio::test(start_paused = true)]
async fn idle_compaction_replaces_deadline_with_latest_request_timestamp() {
    let fixture = idle_fixture("openai-codex-responses", json!({})).await;
    let now = pi_ai::request::now_millis() as f64;
    let lifetime = 27.0 * 60_000.0;
    fixture.faux.set_responses(vec![
        end_turn_at("first", now - lifetime + 60_000.0),
        end_turn_at("second", now - lifetime + 300_000.0),
        faux_assistant_message("idle summary", Default::default()).into(),
    ]);
    fixture
        .session
        .prompt("x".repeat(410_000), Default::default())
        .await
        .unwrap();
    fixture.session.prompt("refresh", Default::default()).await.unwrap();
    tokio::time::advance(std::time::Duration::from_secs(61)).await;
    for _ in 0..30 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        idle_compaction_count(&fixture),
        0,
        "old request timer must be cancelled"
    );
    tokio::time::advance(std::time::Duration::from_secs(240)).await;
    wait_for_idle_compaction(&fixture).await;
    assert_eq!(idle_compaction_count(&fixture), 1);
    fixture.session.dispose();
}

#[tokio::test(start_paused = true)]
async fn idle_compaction_defers_busy_tool_then_uses_completed_request_deadline() {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let fixture = fixture_with_multi_agent(
        Default::default(),
        vec![Arc::new(GatedTool {
            started: started.clone(),
            release: release.clone(),
        })],
        Some(vec!["slow".to_owned()]),
        None,
        FixtureOverrides {
            faux_api: Some("openai-codex-responses".to_owned()),
            context_window: Some(2_000_000.0),
            ..Default::default()
        },
    )
    .await;
    let now = pi_ai::request::now_millis() as f64;
    let lifetime = 27.0 * 60_000.0;
    fixture.faux.set_responses(vec![
        end_turn_at("first", now - lifetime + 60_000.0),
        tool_call_response("slow", json!({}), "slow-call"),
        end_turn_at("after-tool", now - lifetime + 300_000.0),
        faux_assistant_message("idle summary", Default::default()).into(),
    ]);
    fixture
        .session
        .prompt("x".repeat(410_000), Default::default())
        .await
        .unwrap();
    let session = fixture.session.clone();
    let task = tokio::spawn(async move { session.prompt("slow operation", Default::default()).await });
    started.notified().await;
    tokio::time::advance(std::time::Duration::from_secs(120)).await;
    for _ in 0..30 {
        tokio::task::yield_now().await;
    }
    assert_eq!(idle_compaction_count(&fixture), 0);
    release.notify_one();
    task.await.unwrap().unwrap();
    tokio::time::advance(std::time::Duration::from_secs(301)).await;
    wait_for_idle_compaction(&fixture).await;
    assert_eq!(idle_compaction_count(&fixture), 1);
    fixture.session.dispose();
}

#[tokio::test(start_paused = true)]
async fn idle_compaction_arms_restored_context_at_startup_and_reload_but_not_observer_or_error() {
    for scenario in ["startup", "reload", "observer", "error", "aborted"] {
        let fixture = fixture_with_multi_agent(
            serde_json::from_value(json!({"compaction":{"keepRecentTokens":1}})).unwrap(),
            Vec::new(),
            None,
            None,
            FixtureOverrides {
                faux_api: Some("openai-codex-responses".to_owned()),
                context_window: Some(2_000_000.0),
                multi_agent_runtime_role: (scenario == "observer").then_some(MultiAgentRuntimeRole::Observer),
                ..Default::default()
            },
        )
        .await;
        if scenario == "reload" {
            fixture
                .session
                .bind_extensions(ExtensionBindings {
                    ui_context: Some(Arc::new(FailingApprovalUi)),
                    ..Default::default()
                })
                .await
                .unwrap();
        }
        let timestamp = pi_ai::request::now_millis() as f64 - 27.0 * 60_000.0 - 1000.0;
        {
            let mut manager = fixture.session.session_manager();
            for index in 0..2 {
                manager
                    .append_message(AgentMessage::User(pi_ai::UserMessage {
                        role: pi_ai::UserRole::Tag,
                        content: pi_ai::UserContent::Text(format!("restored input {index}").into()),
                        input_source: None,
                        provider_native: None,
                        timestamp,
                    }))
                    .unwrap();
                let mut message = faux_assistant_message("restored answer", Default::default());
                message.api = "openai-codex-responses".to_owned();
                message.timestamp = timestamp;
                message.usage.input = 100_001.0;
                message.usage.total_tokens = 100_001.0;
                if scenario == "error" {
                    message.stop_reason = pi_ai::StopReason::Error;
                }
                if scenario == "aborted" {
                    message.stop_reason = pi_ai::StopReason::Aborted;
                }
                manager
                    .append_message(AgentMessage::Assistant(Box::new(message)))
                    .unwrap();
            }
            let (context, identities) = manager.build_session_context_with_identities();
            fixture
                .session
                .agent()
                .set_messages_with_identities(context.messages, identities)
                .unwrap();
        }
        fixture.faux.set_responses(vec![
            faux_assistant_message("restored idle summary", Default::default()).into(),
            faux_assistant_message("restored turn prefix", Default::default()).into(),
        ]);
        if scenario == "reload" {
            // Reload resets registered API transports before loading extension registrations.
            let registry = fixture.session.model_registry().clone();
            let provider = fixture.faux.provider.clone();
            fixture
                .session
                .reload(Some(Arc::new(move || {
                    scripted_registry::register_scripted_provider(&registry, provider.clone());
                    Box::pin(async {})
                })))
                .await
                .unwrap();
        } else {
            fixture.session.bind_extensions(Default::default()).await.unwrap();
        }
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        if ["observer", "error"].contains(&scenario) {
            for _ in 0..100 {
                tokio::task::yield_now().await;
            }
            assert_eq!(idle_compaction_count(&fixture), 0, "{scenario}");
        } else {
            wait_for_idle_compaction(&fixture).await;
            assert_eq!(idle_compaction_count(&fixture), 1, "{scenario}");
        }
        fixture.session.dispose();
    }
}

async fn fixture_with_timeout_extension(factory: pi_coding_agent::core::extensions::ExtensionFactory) -> Fixture {
    use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
    use pi_coding_agent::core::extensions::{create_extension_runtime, load_extension_from_factory};
    let runtime = create_extension_runtime();
    let bus: Arc<dyn EventBus> = Arc::new(EventBusController::new());
    let extension =
        load_extension_from_factory(factory, &"/tmp".into(), bus, runtime.clone(), &"/tmp/timeout.ts".into())
            .await
            .unwrap();
    fixture_with_multi_agent(
        serde_json::from_value(json!({"retry":{"enabled":false},"compaction":{"enabled":false}})).unwrap(),
        Vec::new(),
        None,
        None,
        FixtureOverrides {
            thinking_phase_timeout_ms: Some(1000.0),
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

#[tokio::test(start_paused = true)]
async fn thinking_timeout_delivers_agent_end_correction_exactly_once_before_recovery() {
    use pi_coding_agent::core::extensions::types::{AgentEndEvent, ExtensionContext};
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let session_cell: Arc<std::sync::Mutex<Option<AgentSession>>> = Arc::new(std::sync::Mutex::new(None));
    let cell = session_cell.clone();
    let factory: ExtensionFactory = Arc::new(move |api| {
        let cell = cell.clone();
        api.on_agent_end(move |event: AgentEndEvent, _ctx: ExtensionContext| {
            let session = cell.lock().unwrap().clone().expect("session initialized");
            Box::pin(async move {
                if event.messages.iter().any(|message| {
                    message
                        .as_assistant()
                        .is_some_and(|message| message.stop_reason == pi_ai::StopReason::Aborted)
                }) {
                    session
                        .follow_up("Queued correction", &[])
                        .await
                        .map_err(|error| HandlerError::new(error.message().clone()))?;
                }
                Ok(None)
            }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    *session_cell.lock().unwrap() = Some(fixture.session.clone());
    fixture
        .faux
        .set_responses(vec![pending_response(), completed_timeout_response("Recovered")]);
    let session = fixture.session.clone();
    let mut task = tokio::spawn(async move { session.prompt("Work", Default::default()).await });
    drive_until_finished(&mut task).await;
    task.await.unwrap().unwrap();
    let users = fixture
        .session
        .messages()
        .into_iter()
        .filter_map(|message| match message {
            AgentMessage::User(message) => Some(message.content),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(users.len(), 2, "original plus one correction");
    assert_eq!(
        serde_json::to_value(&users).unwrap(),
        json!([
            [{"type":"text","text":"Work"}],
            [{"type":"text","text":"Queued correction"}],
        ])
    );
    assert!(fixture.session.get_follow_up_messages().is_empty());
    assert_eq!(fixture.faux.state().lock().unwrap().call_count, 2);
    *session_cell.lock().unwrap() = None;
    fixture.session.dispose();
}

#[tokio::test(start_paused = true)]
async fn thinking_timeout_manual_abort_wins_while_recovery_waits_for_turn_lock() {
    use pi_coding_agent::core::extensions::types::{ExtensionContext, InputEvent, InputEventResult};
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let input_started = Arc::new(tokio::sync::Notify::new());
    let release_input = Arc::new(tokio::sync::Notify::new());
    let started = input_started.clone();
    let release = release_input.clone();
    let factory: ExtensionFactory = Arc::new(move |api| {
        let started = started.clone();
        let release = release.clone();
        api.on_input(move |event: InputEvent, _ctx: ExtensionContext| {
            let started = started.clone();
            let release = release.clone();
            Box::pin(async move {
                if event.text == "Hold lock" {
                    started.notify_one();
                    release.notified().await;
                    Ok(Some(InputEventResult::Handled))
                } else {
                    Ok(Some(InputEventResult::Continue))
                }
            }) as pi_ai::request::BoxFuture<'static, Result<Option<InputEventResult>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    let retry_started = Arc::new(tokio::sync::Notify::new());
    let retry = retry_started.clone();
    fixture.session.subscribe(Arc::new(move |event| {
        if matches!(event, AgentSessionEvent::AutoRetryStart(_)) {
            retry.notify_one();
        }
    }));
    fixture
        .faux
        .set_responses(vec![pending_response(), completed_timeout_response("Never")]);
    let session = fixture.session.clone();
    let task = tokio::spawn(async move { session.prompt("Work", Default::default()).await });
    wait_for_faux_calls(&fixture, 1).await;
    let session = fixture.session.clone();
    let held = tokio::spawn(async move {
        session
            .prompt(
                "Hold lock",
                PromptOptions {
                    streaming_behavior: Some(StreamingBehavior::FollowUp),
                    ..Default::default()
                },
            )
            .await
    });
    input_started.notified().await;
    tokio::time::advance(std::time::Duration::from_millis(1000)).await;
    retry_started.notified().await;
    fixture.session.abort().await.unwrap();
    release_input.notify_one();
    held.await.unwrap().unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(fixture.faux.state().lock().unwrap().call_count, 1);
    assert_eq!(fixture.faux.get_pending_response_count(), 1);
    assert!(!fixture.session.has_active_retry());
    fixture.session.dispose();
}

#[tokio::test(start_paused = true)]
async fn thinking_timeout_shares_one_allowance_with_post_run_compaction() {
    use pi_coding_agent::core::extensions::types::{CompactionEvent, CompactionEventResult, ExtensionContext};
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let factory: ExtensionFactory = Arc::new(move |api| {
        api.on_compaction(|event: CompactionEvent, _ctx: ExtensionContext| {
            Box::pin(async move {
                Ok(Some(CompactionEventResult {
                    cancel: None,
                    compaction: Some(
                        serde_json::from_value(json!({
                            "summary":"Saved work","firstKeptEntryId":event.preparation.first_kept_entry_id,
                            "tokensBefore":event.preparation.tokens_before,
                        }))
                        .unwrap(),
                    ),
                }))
            }) as pi_ai::request::BoxFuture<'static, Result<Option<CompactionEventResult>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    fixture.faux.set_responses(vec![completed_timeout_response("Earlier")]);
    fixture
        .session
        .prompt("Earlier input", Default::default())
        .await
        .unwrap();
    let mut model = fixture.session.model();
    model.context_window = 8192.0;
    fixture.session.set_model(model).await.unwrap();
    fixture.session.settings_manager().apply_overrides(
        &serde_json::from_value(json!({"compaction":{"enabled":true,"reserveTokens":100,"keepRecentTokens":1}}))
            .unwrap(),
    );
    let compactions = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = compactions.clone();
    let compaction_completed = Arc::new(tokio::sync::Notify::new());
    let completed = compaction_completed.clone();
    fixture.session.subscribe(Arc::new(move |event| {
        if let AgentSessionEvent::CompactionEnd(event) = event {
            sink.lock().unwrap().push((event.will_retry, event.aborted));
            completed.notify_one();
        }
    }));
    fixture.faux.set_responses(vec![
        pending_response(),
        faux_assistant_message(
            "Length-limited work ".repeat(3000),
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::StopReason::Length),
                ..Default::default()
            },
        )
        .into(),
        pending_response(),
        completed_timeout_response("Never"),
    ]);
    let session = fixture.session.clone();
    let task = tokio::spawn(async move { session.prompt("Work", Default::default()).await });
    wait_for_faux_calls(&fixture, 2).await;
    tokio::time::advance(std::time::Duration::from_millis(1000)).await;
    // Do not accelerate the clock through the deliberately long streamed response.
    compaction_completed.notified().await;
    wait_for_faux_calls(&fixture, 4).await;
    tokio::time::advance(std::time::Duration::from_millis(1000)).await;
    let result = task.await.unwrap().unwrap_err();
    assert_eq!(result.message(), "Main session thinking phase exceeded 20 minutes");
    assert_eq!(fixture.faux.state().lock().unwrap().call_count, 4);
    assert_eq!(fixture.faux.get_pending_response_count(), 1);
    assert_eq!(*compactions.lock().unwrap(), vec![(true, false)]);
    fixture.session.dispose();
}

const RESTRICTION_ORIGINAL: &str =
    "In my own repository, list source filenames and explain the code; do not read credentials.";
const RESTRICTION_FAILURE: &str =
    "Codex error: This content was flagged for possible cybersecurity risk. OpenAI request ID: req_500";
const RESTRICTION_TASK: &str = "List source filenames only, without opening credentials or running commands.";

fn restriction_proposal() -> serde_json::Value {
    json!({"kind":"rescope","task":RESTRICTION_TASK,"basisQuote":"list source filenames",
        "reason":"A metadata-only subset of the authorized repository inspection."})
}

fn restriction_response() -> pi_ai::AssistantMessage {
    faux_assistant_message(
        "",
        FauxAssistantMessageOptions {
            stop_reason: Some(pi_ai::StopReason::Error),
            error_message: Some(RESTRICTION_FAILURE.into()),
            timestamp: Some(0.0),
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn restriction_execution_matches_frozen_session_scenarios() {
    let mut source = tools_oracle::TsOracle::spawn(
        "restriction-session-oracle.ts",
        "packages/coding-agent/src/core/agent-session.ts",
    );
    for scenario in [
        "approved",
        "denied",
        "malformed",
        "blocked",
        "validationTool",
        "transient",
        "repeatFlag",
        "modelDuringValidationStart",
        "adviceModel",
        "adviceGoal",
        "adviceRequest",
        "adviceMessages",
        "adviceMetadata",
        "adviceStatus",
        "adviceReserveInput",
        "adviceAbort",
        "adviceReviewCancel",
        "adviceToolChange",
        "adviceQueued",
        "quota",
        "overflow",
        "validationToolChange",
        "validationSameTools",
    ] {
        let expected = source.request(&pi_js_compat::js_json!({"id":scenario,"scenario":scenario}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let request_sink = requests.clone();
        let session_cell: Arc<std::sync::Mutex<Option<AgentSession>>> = Arc::new(std::sync::Mutex::new(None));
        let request_session = session_cell.clone();
        let advice_signals = Arc::new(std::sync::Mutex::new(Vec::new()));
        let signal_sink = advice_signals.clone();
        let reserved_input = Arc::new(std::sync::Mutex::new(None));
        let reservation_sink = reserved_input.clone();
        let answer = if scenario == "blocked" {
            json!({"kind":"blocked","reason":"No authorized subset"})
        } else {
            restriction_proposal()
        }
        .to_string();
        let mut settings = json!({"retry":{"enabled":true,"maxRetries":3,"baseDelayMs":1}});
        if scenario == "overflow" {
            settings["compaction"] = json!({"keepRecentTokens":1,"reserveTokens":100});
        }
        let fixture = fixture_with_multi_agent(
            serde_json::from_value(settings).unwrap(),
            vec![Arc::new(EchoTool)],
            Some(vec!["echo".to_owned()]),
            None,
            FixtureOverrides {
                faux_api: Some("openai-codex-responses".to_owned()),
                faux_provider_id: (scenario == "quota").then(|| "openai-codex".to_owned()),
                supervisor_decision_requester: Some(Arc::new(move |input| {
                    request_sink
                        .lock()
                        .unwrap()
                        .push(pi_js_compat::js_json::to_value(&input.kind).unwrap());
                    let answer = answer.clone();
                    let session = request_session.lock().unwrap().clone().expect("session initialized");
                    let signals = signal_sink.clone();
                    let reservation = reservation_sink.clone();
                    Box::pin(async move {
                        match scenario {
                            "adviceModel" => { session.agent().set_model(session.agent().model()); },
                            "adviceGoal" => {session.session_manager().set_session_goal_json(&"{\"objective\":\"new goal\"}".into()).unwrap();},
                            "adviceRequest" => {session.session_manager().append_custom_entry(&"codex_restriction_context".into(),Some(pi_js_compat::js_json!({"requestId":"changed-request","userRequest":"Different authorization"}))).unwrap();},
                            "adviceMessages" => session.agent().set_messages(session.messages()),
                            "adviceMetadata" => {session.session_manager().append_custom_entry(&"status".into(),Some(pi_js_compat::js_json!({"message":"Still inspecting"}))).unwrap();},
                            "adviceStatus" => session.send_custom_message(&"supervisor-status".into(),pi_agent_core::harness::messages::CustomMessageContent::Text("Still inspecting".into()),true,None,false,None).await.unwrap(),
                            "adviceReserveInput" => {*reservation.lock().unwrap()=Some(session.reserve_external_user_input());},
                            "adviceAbort" => {session.abort().await.unwrap();},
                            "adviceReviewCancel" => {assert!(session.cancel_supervisor_review());},
                            "adviceToolChange" => {session.set_active_tools_by_name(&[]).unwrap();},
                            "adviceQueued" => session.agent().follow_up(AgentMessage::User(pi_ai::UserMessage {role:pi_ai::UserRole::Tag,content:pi_ai::UserContent::Text("new input".into()),input_source:None,provider_native:None,timestamp:0.0})),
                            _=>{},
                        }
                        signals.lock().unwrap().push(input.signal.as_ref().map(|signal| signal.aborted()));
                        Ok(pi_js_compat::js_json::from_value(
                            &pi_js_compat::js_json!({"kind":"advisory","answer":answer}),
                        )
                        .unwrap())
                    })
                })),
                ..Default::default()
            },
        )
        .await;
        if scenario == "quota" {
            let model = fixture.session.model();
            fixture
                .session
                .model_registry()
                .register_provider(
                    "openai-codex-gc",
                    pi_coding_agent::core::model_registry::ProviderConfigInput {
                        api: Some(model.api.clone()),
                        base_url: Some(model.base_url.clone()),
                        api_key: Some("faux-key".into()),
                        models: Some(vec![model.into()]),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        *session_cell.lock().unwrap() = Some(fixture.session.clone());
        fixture.session.set_active_tools_by_name(&["echo".to_owned()]).unwrap();
        if scenario == "modelDuringValidationStart" {
            let agent = fixture.session.agent().clone();
            fixture.session.subscribe(Arc::new(move |event| {
                if let AgentSessionEvent::Message(event) = event
                    && let pi_agent_core::agent::AgentEvent::MessageStart {
                        message: AgentMessage::Custom(object),
                    } = &event.event
                    && object.get("customType").and_then(JsValue::as_string)
                        == Some(&JsString::from("supervisor_restriction_validation"))
                {
                    agent.set_model(agent.model());
                }
            }));
        }
        let validation = if scenario == "malformed" {
            faux_assistant_message("not JSON", Default::default())
        } else if scenario == "validationTool" {
            let pi_ai::providers::faux::FauxResponseStep::Message(message) =
                tool_call_response("echo", json!({"text":"forbidden"}), "validate-tool")
            else {
                unreachable!()
            };
            *message
        } else {
            faux_assistant_message(
                json!({"allowed":scenario!="denied","task":RESTRICTION_TASK,
                "reason":"Repository metadata only.","basisQuote":"list source filenames"})
                .to_string(),
                Default::default(),
            )
        };
        let operation = match scenario {
            "repeatFlag" => restriction_response(),
            "quota" | "overflow" => faux_assistant_message(
                "",
                FauxAssistantMessageOptions {
                    stop_reason: Some(pi_ai::StopReason::Error),
                    error_message: Some(
                        if scenario == "quota" {
                            "You have hit your ChatGPT usage limit"
                        } else {
                            "maximum context length exceeded"
                        }
                        .into(),
                    ),
                    ..Default::default()
                },
            ),
            "transient" => faux_assistant_message(
                "",
                FauxAssistantMessageOptions {
                    stop_reason: Some(pi_ai::StopReason::Error),
                    error_message: Some("overloaded_error".into()),
                    ..Default::default()
                },
            ),
            _ => faux_assistant_message("Source names listed", Default::default()),
        };
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let providers = Arc::new(std::sync::Mutex::new(Vec::new()));
        fixture.faux.set_responses(
            vec![
                restriction_response(),
                validation,
                operation,
                faux_assistant_message("Unexpected resend", Default::default()),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, message)| {
                let calls = calls.clone();
                let providers = providers.clone();
                let cell = session_cell.clone();
                pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(move |context, _options, _state, model| {
                    calls.lock().unwrap().push(
                        context
                            .tools
                            .unwrap_or_default()
                            .iter()
                            .map(|tool| tool.name.clone())
                            .collect::<Vec<_>>(),
                    );
                    providers.lock().unwrap().push(model.provider);
                    if index == 1 && ["validationToolChange", "validationSameTools"].contains(&scenario) {
                        let names = if scenario == "validationToolChange" {
                            vec![]
                        } else {
                            vec!["echo".to_owned()]
                        };
                        cell.lock()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .set_active_tools_by_name(&names)
                            .unwrap();
                    }
                    let message = message.clone();
                    Box::pin(async move { Ok(message) })
                }))
            })
            .collect(),
        );
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = events.clone();
        fixture
            .session
            .subscribe(Arc::new(move |event| sink.lock().unwrap().push(event.clone())));
        fixture
            .session
            .prompt(RESTRICTION_ORIGINAL, Default::default())
            .await
            .unwrap();
        let messages = fixture.session.messages();
        let events = events.lock().unwrap();
        let ends = events
            .iter()
            .filter_map(|event| match event {
                AgentSessionEvent::AgentEnd(event) => Some(
                    pi_js_compat::js_json!({"willRetry":event.will_retry,"continuation":event.session_continuation}),
                ),
                _ => None,
            })
            .collect::<Vec<_>>();
        let users = messages
            .iter()
            .filter_map(|message| match message {
                AgentMessage::User(message) => Some(message.content.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let assistants = messages
            .iter()
            .filter_map(|message| message.as_assistant().map(|message| message.content.clone()))
            .collect::<Vec<_>>();
        let custom=messages.iter().filter_map(|message|match message {AgentMessage::Custom(object)=>Some(pi_js_compat::js_json!({"type":object.get("customType").cloned().unwrap_or(JsValue::Null),"content":object.get("content").cloned().unwrap_or(JsValue::Null)})),_=>None}).collect::<Vec<_>>();
        let tools=messages.iter().filter_map(|message|match message {AgentMessage::ToolResult(result)=>Some(pi_js_compat::js_json!({"name":result.tool_name,"content":result.content,"isError":result.is_error})),_=>None}).collect::<Vec<_>>();
        let actual = pi_js_compat::js_json!({"requests":requests.lock().unwrap().clone(),"adviceSignals":advice_signals.lock().unwrap().clone(),"calls":calls.lock().unwrap().clone(),"providers":providers.lock().unwrap().clone(),
            "pending":fixture.faux.get_pending_response_count(),"activeTools":fixture.session.get_active_tool_names(),"model":fixture.session.model().id,
            "ends":ends,"autoRetryStarts":events.iter().filter(|event|matches!(event,AgentSessionEvent::AutoRetryStart(_))).count(),
            "autoRetryEnds":events.iter().filter_map(|event| match event {AgentSessionEvent::AutoRetryEnd(event)=>Some(event),_=>None}).collect::<Vec<_>>(),
            "compactionEnds":events.iter().filter(|event|matches!(event,AgentSessionEvent::CompactionEnd(_))).count(),
            "userContents":users,"assistantContents":assistants,"customContents":custom,"toolResults":tools});
        assert_eq!(actual, expected["output"], "{scenario}");
        reserved_input.lock().unwrap().take();
        *session_cell.lock().unwrap() = None;
        fixture.session.dispose();
    }
}

fn restriction_advisory_reply() -> pi_coding_agent::core::session_control_db::SupervisorResponse {
    pi_js_compat::js_json::from_value(
        &pi_js_compat::js_json!({"kind":"advisory","answer":restriction_proposal().to_string()}),
    )
    .unwrap()
}

fn restriction_allowed_validation() -> pi_ai::AssistantMessage {
    faux_assistant_message(json!({"allowed":true,"task":RESTRICTION_TASK,"reason":"Repository metadata only.","basisQuote":"list source filenames"}).to_string(),Default::default())
}

async fn fixture_with_restriction_requester(requester: SupervisorDecisionRequester, timeout: Option<f64>) -> Fixture {
    let fixture = fixture_with_multi_agent(
        serde_json::from_value(json!({"retry":{"enabled":true,"maxRetries":3,"baseDelayMs":1}})).unwrap(),
        vec![Arc::new(EchoTool)],
        Some(vec!["echo".to_owned()]),
        None,
        FixtureOverrides {
            faux_api: Some("openai-codex-responses".to_owned()),
            supervisor_decision_requester: Some(requester),
            thinking_phase_timeout_ms: timeout,
            ..Default::default()
        },
    )
    .await;
    fixture.session.set_active_tools_by_name(&["echo".to_owned()]).unwrap();
    fixture
}

#[tokio::test]
async fn restriction_discards_independently_delayed_advice_after_state_change_or_cancellation() {
    for change in [
        "abort", "reserve", "review", "model", "goal", "request", "messages", "queued", "dispose",
    ] {
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let signal = Arc::new(std::sync::Mutex::new(None));
        let calls = Arc::new(AtomicUsize::new(0));
        let started = entered.clone();
        let released = release.clone();
        let captured = signal.clone();
        let count = calls.clone();
        let fixture = fixture_with_restriction_requester(
            Arc::new(move |request| {
                let started = started.clone();
                let released = released.clone();
                let captured = captured.clone();
                let count = count.clone();
                Box::pin(async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    *captured.lock().unwrap() = request.signal;
                    started.notify_one();
                    released.notified().await;
                    Ok(restriction_advisory_reply())
                })
            }),
            None,
        )
        .await;
        fixture.faux.set_responses(vec![
            restriction_response().into(),
            faux_assistant_message("Queued processed", Default::default()).into(),
        ]);
        let session = fixture.session.clone();
        let task = tokio::spawn(async move { session.prompt(RESTRICTION_ORIGINAL, Default::default()).await });
        entered.notified().await;
        let mut reservation = None;
        match change {
            "abort" => {
                fixture.session.abort().await.unwrap();
            }
            "reserve" => {
                reservation = Some(fixture.session.reserve_external_user_input());
            }
            "review" => {
                assert!(fixture.session.cancel_supervisor_review());
            }
            "model" => fixture.session.agent().set_model(fixture.session.agent().model()),
            "goal" => {
                fixture
                    .session
                    .session_manager()
                    .set_session_goal_json(&"{\"objective\":\"new goal\"}".into())
                    .unwrap();
            }
            "request" => {
                fixture
                    .session
                    .session_manager()
                    .append_custom_entry(
                        &"codex_restriction_context".into(),
                        Some(pi_js_compat::js_json!({"requestId":"new request","userRequest":"Changed scope"})),
                    )
                    .unwrap();
            }
            "messages" => fixture.session.agent().set_messages(fixture.session.messages()),
            "queued" => fixture
                .session
                .agent()
                .follow_up(AgentMessage::User(pi_ai::UserMessage {
                    role: pi_ai::UserRole::Tag,
                    content: pi_ai::UserContent::Text("Queued correction".into()),
                    input_source: None,
                    provider_native: None,
                    timestamp: 0.0,
                })),
            "dispose" => fixture.session.dispose(),
            _ => unreachable!(),
        }
        let was_aborted = signal.lock().unwrap().as_ref().unwrap().aborted();
        release.notify_one();
        task.await.unwrap().unwrap();
        assert_eq!(
            was_aborted,
            ["abort", "reserve", "review", "dispose"].contains(&change),
            "{change}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            fixture.faux.state().lock().unwrap().call_count,
            if change == "queued" { 2 } else { 1 },
            "{change}"
        );
        assert!(fixture.session.messages().iter().all(|message| !matches!(message,AgentMessage::Custom(object) if object.get("customType").and_then(JsValue::as_string).is_some_and(|name|name.starts_with("supervisor_restriction")))),"{change}");
        drop(reservation);
        fixture.session.dispose();
    }
}

#[tokio::test(start_paused = true)]
async fn restriction_timeout_and_abort_restore_tools_and_reset_for_a_new_human_retry() {
    for (operative, timeout) in [(false, false), (false, true), (true, false), (true, true)] {
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let fixture = fixture_with_restriction_requester(
            Arc::new(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(restriction_advisory_reply()) })
            }),
            Some(1000.0),
        )
        .await;
        let retry_starts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = retry_starts.clone();
        fixture.session.subscribe(Arc::new(move |event| {
            if let AgentSessionEvent::AutoRetryStart(event) = event {
                sink.lock().unwrap().push(event.error_message.clone());
            }
        }));
        let mut responses = vec![restriction_response().into()];
        if operative {
            responses.push(restriction_allowed_validation().into());
        }
        responses.push(pending_response());
        responses.push(faux_assistant_message("Never replayed", Default::default()).into());
        fixture.faux.set_responses(responses);
        let session = fixture.session.clone();
        let task = tokio::spawn(async move { session.prompt(RESTRICTION_ORIGINAL, Default::default()).await });
        let calls = if operative { 3 } else { 2 };
        wait_for_faux_calls(&fixture, calls).await;
        assert_eq!(
            fixture.session.get_active_tool_names(),
            if operative { vec!["echo".to_owned()] } else { vec![] }
        );
        if timeout {
            tokio::time::advance(std::time::Duration::from_millis(1000)).await;
        } else {
            fixture.session.abort().await.unwrap();
        }
        let result = task.await.unwrap();
        if timeout {
            assert_eq!(
                result.unwrap_err().message(),
                "Main session thinking phase exceeded 20 minutes"
            );
        } else {
            result.unwrap();
        }
        assert_eq!(fixture.faux.state().lock().unwrap().call_count, calls);
        assert_eq!(fixture.faux.get_pending_response_count(), 1);
        assert_eq!(fixture.session.get_active_tool_names(), vec!["echo".to_owned()]);
        assert!(retry_starts.lock().unwrap().is_empty());
        fixture.faux.set_responses(vec![
            faux_assistant_message(
                "",
                FauxAssistantMessageOptions {
                    stop_reason: Some(pi_ai::StopReason::Error),
                    error_message: Some("overloaded_error".into()),
                    ..Default::default()
                },
            )
            .into(),
            faux_assistant_message("Fresh request done", Default::default()).into(),
        ]);
        fixture
            .session
            .prompt("Explain filename extensions", Default::default())
            .await
            .unwrap();
        assert_eq!(fixture.faux.state().lock().unwrap().call_count, calls + 2);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert_eq!(*retry_starts.lock().unwrap(), vec![JsString::from("overloaded_error")]);
        fixture.session.dispose();
    }
}

#[tokio::test]
async fn restriction_discards_late_validation_without_overwriting_new_tool_selection() {
    for change in ["abort", "toolsEmpty", "toolsSame", "messages", "model", "reserve"] {
        let fixture = fixture_with_restriction_requester(
            Arc::new(|_| Box::pin(async { Ok(restriction_advisory_reply()) })),
            None,
        )
        .await;
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let started = entered.clone();
        let released = release.clone();
        fixture.faux.set_responses(vec![
            restriction_response().into(),
            pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(move |context, _options, _state, _model| {
                assert!(
                    context.tools.unwrap_or_default().is_empty(),
                    "validation must remain tool-free"
                );
                let started = started.clone();
                let released = released.clone();
                Box::pin(async move {
                    started.notify_one();
                    released.notified().await;
                    Ok(restriction_allowed_validation())
                })
            })),
            faux_assistant_message("Must stay queued", Default::default()).into(),
        ]);
        let session = fixture.session.clone();
        let task = tokio::spawn(async move { session.prompt(RESTRICTION_ORIGINAL, Default::default()).await });
        entered.notified().await;
        assert!(fixture.session.get_active_tool_names().is_empty());
        let mut reservation = None;
        match change {
            "abort" => {
                fixture.session.abort().await.unwrap();
            }
            "toolsEmpty" => fixture.session.set_active_tools_by_name(&[]).unwrap(),
            "toolsSame" => fixture.session.set_active_tools_by_name(&["echo".to_owned()]).unwrap(),
            "messages" => fixture.session.agent().set_messages(fixture.session.messages()),
            "model" => fixture.session.agent().set_model(fixture.session.agent().model()),
            "reserve" => {
                reservation = Some(fixture.session.reserve_external_user_input());
            }
            _ => unreachable!(),
        }
        release.notify_one();
        task.await.unwrap().unwrap();
        assert_eq!(fixture.faux.state().lock().unwrap().call_count, 2, "{change}");
        assert_eq!(fixture.faux.get_pending_response_count(), 1, "{change}");
        assert_eq!(
            fixture.session.get_active_tool_names(),
            if change == "toolsEmpty" {
                vec![]
            } else {
                vec!["echo".to_owned()]
            },
            "{change}"
        );
        drop(reservation);
        fixture.session.dispose();
    }
}
