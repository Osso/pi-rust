//! Behavioral tests for the bundled `<first-party:goal>` extension.
//!
//! Ports the observable cases from
//! `packages/coding-agent/test/goal-extension.test.ts` against the real
//! `ExtensionRunner` boundary.

use pi_js_compat::js_json::JsString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::bundled_extensions::goal::goal_args::{ParseGoalArgsResult, ParsedGoalArgs, parse_goal_args};
use pi_coding_agent::bundled_extensions::goal::goal_objective::deduplicate_current_goal_scope;
use pi_coding_agent::bundled_extensions::goal::goal_parsing::{parse_goal, parse_goal_json};
use pi_coding_agent::bundled_extensions::goal::goal_presentation::{
    goal_footer_status, goal_startup_message, goal_system_block, goal_view_message,
};
use pi_coding_agent::bundled_extensions::goal::goal_turn::{
    did_last_assistant_abort, did_last_assistant_return_empty, find_last_assistant_message,
};
use pi_coding_agent::bundled_extensions::goal::goal_types::{
    Goal, GoalExtensionOptions, GoalReviewInput, GoalSupervisorResponse, GoalSupervisorReview,
};
use pi_coding_agent::bundled_extensions::goal::install_goal_extension;
use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
use pi_coding_agent::core::extensions::types::{
    AgentEndEvent, CustomMessagePayload, ExtensionEvent, ExtensionStatus, ExtensionUiContext, InputSource, NotifyType,
    SessionStartEvent, SessionStartReason, ToolResultEvent, UserMessageContent,
};
use pi_coding_agent::core::extensions::{
    ExtensionActions, ExtensionContextActions, ExtensionRunner, create_extension_runtime, load_extension_from_factory,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::session_manager::{SessionManager, SharedSessionManager, lock_session_manager};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

#[derive(Default)]
struct Records {
    user_messages: Vec<UserMessageContent>,
    messages: Vec<CustomMessagePayload>,
    entries: Vec<(JsString, Option<Value>)>,
    statuses: Vec<(JsString, Option<JsString>)>,
    notifications: Vec<(JsString, Option<NotifyType>)>,
    call_tools: Vec<(String, Value)>,
}

struct TestUi {
    records: Arc<Mutex<Records>>,
}

impl ExtensionUiContext for TestUi {
    fn notify(
        &self,
        message: &JsString,
        notify_type: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.records
            .lock()
            .unwrap()
            .notifications
            .push((message.to_owned(), notify_type));
        Ok(())
    }

    fn set_status(&self, key: &JsString, text: Option<ExtensionStatus>) -> Result<(), pi_ai::request::CallbackError> {
        let text = text.map(|text| text.evaluate(None)).transpose()?.flatten();
        self.records.lock().unwrap().statuses.push((key.to_owned(), text));
        Ok(())
    }
}

struct Harness {
    runner: ExtensionRunner,
    session: SharedSessionManager,
    records: Arc<Mutex<Records>>,
    review_payloads: Arc<Mutex<Vec<(String, Value)>>>,
    idle: Arc<AtomicBool>,
    pending: Arc<AtomicBool>,
    active_agents: Arc<Mutex<f64>>,
    cwd: String,
}

fn event_bus() -> Arc<dyn EventBus> {
    Arc::new(EventBusController::new())
}

async fn model_registry() -> Arc<ModelRegistry> {
    Arc::new(
        ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            serde_json::Map::new(),
        ))
        .unwrap(),
    )
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
        get_thinking_level: Arc::new(|| Ok(pi_agent_core::harness::types::ThinkingLevel::Off)),
        set_thinking_level: Arc::new(|_| Ok(())),
    }
}

fn context_actions() -> ExtensionContextActions {
    ExtensionContextActions {
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
        get_control_db_path: None,
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
    }
}

fn temp_dir(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "pi-goal-ext-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_str().expect("utf-8 test path").to_owned()
}

async fn harness(name: &str, default_decision: GoalSupervisorResponse) -> Harness {
    let cwd = temp_dir(name);
    let records = Arc::new(Mutex::new(Records::default()));
    let review_payloads = Arc::new(Mutex::new(Vec::new()));
    let idle = Arc::new(AtomicBool::new(true));
    let pending = Arc::new(AtomicBool::new(false));
    let active_agents = Arc::new(Mutex::new(0.0));
    let review_payloads_for_review = Arc::clone(&review_payloads);
    let default_decision = Arc::new(Mutex::new(default_decision));
    let review_goal: GoalSupervisorReview = Arc::new(move |input: GoalReviewInput| {
        review_payloads_for_review
            .lock()
            .unwrap()
            .push((input.kind.as_str().to_owned(), Value::Object(input.payload.clone())));
        let decision = default_decision.lock().unwrap().clone();
        Box::pin(async move { Ok(decision) })
            as pi_coding_agent::core::extensions::types::BoxFuture<
                'static,
                Result<GoalSupervisorResponse, pi_coding_agent::core::extensions::types::HandlerError>,
            >
    });
    let runtime = create_extension_runtime();
    let setup_cwd = cwd.clone();
    let factory: pi_coding_agent::core::extensions::ExtensionFactory =
        Arc::new(move |api: &pi_coding_agent::core::extensions::ExtensionApi| {
            let result = install_goal_extension(
                api,
                GoalExtensionOptions {
                    review_goal: Some(Arc::clone(&review_goal)),
                },
            );
            let _ = setup_cwd;
            Box::pin(async move { result })
                as pi_coding_agent::core::extensions::types::BoxFuture<
                    'static,
                    Result<(), pi_coding_agent::core::extensions::types::HandlerError>,
                >
        });
    let extension = load_extension_from_factory(
        factory,
        &JsString::from(&cwd),
        event_bus(),
        runtime.clone(),
        &JsString::from("goal"),
    )
    .await
    .unwrap();
    let session = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from(&cwd)), None).unwrap(),
    ));
    let registry = model_registry().await;
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd.clone(),
        Arc::clone(&session),
        registry,
        None,
    );
    runner.set_ui_context(
        Some(Arc::new(TestUi {
            records: Arc::clone(&records),
        })),
        pi_coding_agent::core::extensions::types::ExtensionMode::Tui,
    );

    let records_for_actions = Arc::clone(&records);
    let mut actions = actions();
    actions.send_message = Arc::new(move |message, _| {
        records_for_actions.lock().unwrap().messages.push(message);
        Ok(())
    });
    let records_for_user = Arc::clone(&records);
    actions.send_user_message = Arc::new(move |content, _| {
        records_for_user.lock().unwrap().user_messages.push(content);
        Ok(())
    });
    let records_for_entries = Arc::clone(&records);
    actions.append_entry = Arc::new(move |custom_type, data| {
        records_for_entries
            .lock()
            .unwrap()
            .entries
            .push((custom_type.to_owned(), data));
        Ok(())
    });
    let records_for_tools = Arc::clone(&records);
    let active_agents_for_tools = Arc::clone(&active_agents);
    actions.call_tool = Arc::new(move |name, params, _signal, _id| {
        records_for_tools
            .lock()
            .unwrap()
            .call_tools
            .push((name.clone(), params.clone()));
        let active = *active_agents_for_tools.lock().unwrap();
        Box::pin(async move {
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new("ok"))],
                details: Some(json!({"activeCount": active, "agents": []})),
                is_error: None,
                terminate: None,
            })
        })
    });
    let mut context_actions = context_actions();
    let idle_for_context = Arc::clone(&idle);
    context_actions.is_idle = Arc::new(move || idle_for_context.load(Ordering::SeqCst));
    let pending_for_context = Arc::clone(&pending);
    context_actions.has_pending_messages = Arc::new(move || pending_for_context.load(Ordering::SeqCst));
    runner.bind_core(actions, context_actions, None).unwrap();

    Harness {
        runner,
        session,
        records,
        review_payloads,
        idle,
        pending,
        active_agents,
        cwd,
    }
}

fn assistant_message(text: &str, stop_reason: &str) -> pi_agent_core::agent::AgentMessage {
    let value = json!({
        "role": "assistant",
        "content": [{"type": "text", "text": text}],
        "api": "openai-completions",
        "provider": "test",
        "model": "test-model",
        "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                  "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
        "stopReason": stop_reason,
        "timestamp": 1
    });
    pi_js_compat::js_json::from_value(&value).unwrap()
}

impl Harness {
    fn stored_goal(&self) -> Option<Goal> {
        lock_session_manager(&self.session)
            .get_session_goal_json()
            .unwrap()
            .and_then(|json| parse_goal_json(&json))
    }

    async fn run_command(&self, args: &str) {
        let command = self.runner.get_command("goal").expect("goal command");
        let ctx = self.runner.create_command_context();
        (command.handler)(args.to_owned().into(), ctx).await.unwrap();
    }

    async fn run_tool(&self, params: Value) -> AgentToolResult {
        let tool = self
            .runner
            .get_tool_definition("manage_goal")
            .expect("manage_goal tool");
        (tool.execute)(
            "call-1".to_owned(),
            params,
            None,
            None,
            Some(self.runner.create_context()),
        )
        .await
        .unwrap()
    }

    async fn run_agent_end(&self, messages: Vec<pi_agent_core::agent::AgentMessage>) {
        let mut event = ExtensionEvent::AgentEnd(AgentEndEvent {
            messages,
            session_continuation: None,
        });
        self.runner.emit(&mut event).await;
    }

    async fn run_input(&self, text: &str) {
        self.runner
            .emit_input(&JsString::from(text), None, InputSource::Interactive, None)
            .await;
    }

    async fn run_end_turn(&self, reason: &str, is_error: bool) {
        let mut input = pi_js_compat::js_json::JsObject::new();
        input.insert(JsString::from("reason"), Value::String(reason.to_owned().into()));
        let _ = self
            .runner
            .emit_tool_result(ToolResultEvent {
                tool_name: pi_coding_agent::core::extensions::types::ToolName::parse("end_turn"),
                tool_call_id: "end-turn-1".to_owned(),
                input,
                content: vec![TextOrImageContent::Text(TextContent::new("Turn ended"))],
                is_error,
                usage: None,
                details: Some(json!({"reason": reason})),
            })
            .await;
    }
}

fn text_of(result: &AgentToolResult) -> JsString {
    JsString::join(
        result
            .content
            .iter()
            .filter_map(|item| match item {
                TextOrImageContent::Text(text) => Some(text.text.clone()),
                TextOrImageContent::Image(_) => None,
            })
            .collect::<Vec<_>>(),
        "\n",
    )
}

// ---------------------------------------------------------------------------
// Deterministic module cases
// ---------------------------------------------------------------------------

#[test]
fn parses_goal_args_like_the_command_parser() {
    assert_eq!(parse_goal_args(""), ParseGoalArgsResult::Parsed(ParsedGoalArgs::View));
    assert_eq!(
        parse_goal_args("set ship the thing"),
        ParseGoalArgsResult::Parsed(ParsedGoalArgs::Set {
            objective: "ship the thing".to_owned().into()
        })
    );
    assert_eq!(
        parse_goal_args("set"),
        ParseGoalArgsResult::Error("Use /goal set <objective> to set a goal".to_owned())
    );
    assert_eq!(
        parse_goal_args("pause"),
        ParseGoalArgsResult::Parsed(ParsedGoalArgs::Pause)
    );
    assert_eq!(
        parse_goal_args("pause now"),
        ParseGoalArgsResult::Error("Use /goal set <objective> to set a goal".to_owned())
    );
    assert_eq!(
        parse_goal_args("ship it"),
        ParseGoalArgsResult::Error("Use /goal set <objective> to set a goal".to_owned())
    );
    assert_eq!(
        parse_goal_args("--token-budget 10"),
        ParseGoalArgsResult::Error("/goal --token-budget is no longer supported".to_owned())
    );
    assert_eq!(
        parse_goal_args("--wall-clock-minutes=5"),
        ParseGoalArgsResult::Error("/goal --wall-clock-minutes is no longer supported".to_owned())
    );
    assert_eq!(
        parse_goal_args("--replace"),
        ParseGoalArgsResult::Error("Goal flags are no longer supported".to_owned())
    );
}

#[test]
fn parses_goal_json_and_drops_invalid_fields() {
    let goal = parse_goal(&json!({
        "objective": "  ship it  ",
        "branch": "main",
        "createdAt": "2026-01-01T00:00:00.000Z",
        "continuationTurns": 2,
        "pauseReason": "  blocked  ",
        "reviewEvidence": [
            {"kind": "user", "text": "hello"},
            {"kind": "end_turn", "reason": "  "},
            {"kind": "end_turn", "reason": "done"}
        ]
    }))
    .unwrap();
    assert_eq!(goal.objective, "ship it");
    assert_eq!(goal.continuation_turns, Some(2.0));
    assert_eq!(goal.pause_reason.clone(), Some(JsString::from("blocked")));
    assert_eq!(goal.review_evidence.unwrap().len(), 2);

    assert!(parse_goal(&json!({"branch": "main", "createdAt": "x"})).is_none());
    assert!(parse_goal(&json!({"objective": "x", "createdAt": "x"})).is_none());
    assert!(parse_goal_json(&JsString::from("not json")).is_none());
    assert!(parse_goal(&json!(["x"])).is_none());
}

#[test]
fn presents_goal_status_and_system_block() {
    let goal = Goal {
        objective: "ship it".to_owned().into(),
        branch: "main".to_owned().into(),
        created_at: "2026-01-01T00:00:00.000Z".to_owned().into(),
        completed_at: None,
        completion_reason: None,
        continuation_turns: Some(3.0),
        paused_at: None,
        pause_reason: None,
        review_evidence: None,
    };
    assert_eq!(goal_footer_status(&goal), "goal: ship it");
    assert_eq!(goal_startup_message(&goal), "Active goal: ship it");
    assert_eq!(goal_view_message(&goal), "Goal: ship it");
    assert_eq!(
        goal_system_block(&goal),
        "<goal>\nLong-running objective: ship it\n(set on main at 2026-01-01T00:00:00.000Z)\nContinuation turns used: 3\n\nKeep working toward this objective across turns until it is achieved.\nWhen it is achieved, call the manage_goal tool with action \"complete\".\nDo not call manage_goal with action set for this objective; it is already active.\nIf you cannot make further progress, say what is blocking it rather than stopping silently.\n</goal>"
    );
    let paused = Goal {
        paused_at: Some("2026-01-02T00:00:00.000Z".to_owned().into()),
        pause_reason: None,
        ..goal
    };
    assert_eq!(goal_footer_status(&paused), "goal paused: No pause reason recorded");
    assert_eq!(goal_view_message(&paused), "Goal paused: No pause reason recorded");
}

#[test]
fn deduplicates_restated_goal_scope() {
    assert_eq!(
        deduplicate_current_goal_scope(&JsString::from("Ship A; Ship B"), &JsString::from("Ship A")),
        "Ship A; Ship B"
    );
    assert_eq!(
        deduplicate_current_goal_scope(&JsString::from("Ship A; Ship A; Ship B"), &JsString::from("Ship A")),
        "Ship A; Ship B"
    );
    assert_eq!(
        deduplicate_current_goal_scope(&JsString::from("Ship A"), &JsString::from("Ship A")),
        "Ship A"
    );
    assert_eq!(
        deduplicate_current_goal_scope(&JsString::from("other"), &JsString::from("Ship A")),
        "other"
    );
}

#[test]
fn classifies_the_last_assistant_turn() {
    let event = AgentEndEvent {
        messages: vec![assistant_message("still working", "stop")],
        session_continuation: None,
    };
    assert!(!did_last_assistant_abort(&event));
    assert!(!did_last_assistant_return_empty(&event));
    assert_eq!(
        find_last_assistant_message(&event).unwrap().stop_reason,
        pi_ai::types::StopReason::Stop
    );

    let aborted = AgentEndEvent {
        messages: vec![assistant_message("", "aborted")],
        session_continuation: None,
    };
    assert!(did_last_assistant_abort(&aborted));
    assert!(!did_last_assistant_return_empty(&aborted));

    let empty = AgentEndEvent {
        messages: vec![assistant_message("   ", "stop")],
        session_continuation: None,
    };
    assert!(did_last_assistant_return_empty(&empty));
}

// ---------------------------------------------------------------------------
// Extension behavior
// ---------------------------------------------------------------------------

#[tokio::test]
async fn registers_manage_goal_and_goal_command_without_budget_fields() {
    let harness = harness(
        "register",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    let tools = harness.runner.get_all_registered_tools();
    let names: Vec<_> = tools.iter().map(|tool| tool.definition.name.as_str()).collect();
    assert_eq!(names, vec!["manage_goal"]);
    let parameters = &tools[0].definition.parameters;
    assert!(parameters["properties"].get("action").is_some());
    assert!(parameters["properties"].get("objective").is_some());
    assert!(parameters["properties"].get("reason").is_some());
    assert!(parameters["properties"].get("completionReport").is_some());
    assert!(parameters["properties"].get("tokenBudget").is_none());
    assert!(parameters["properties"].get("wallClockMinutes").is_none());
    let command = harness.runner.get_command("goal").unwrap();
    assert!(command.description.unwrap().starts_with("Set, view, pause"));
    let _ = &harness.cwd;
}

#[tokio::test]
async fn sets_goal_from_command_and_continues_when_idle() {
    let harness = harness(
        "command-set",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;

    let goal = harness.stored_goal().expect("goal stored");
    assert_eq!(goal.objective, "Ship it");
    assert_eq!(goal.branch, "(no branch)");
    assert_eq!(goal.continuation_turns, Some(0.0));
    let records = harness.records.lock().unwrap();
    assert_eq!(
        records.user_messages,
        vec![UserMessageContent::Text(
            "Continue working toward the active goal.".to_owned().into()
        )]
    );
    assert_eq!(
        records.statuses.last().unwrap(),
        &("goal".into(), Some("goal: Ship it".into()))
    );
    assert_eq!(
        records.notifications.last().unwrap(),
        &("Goal set — starting work".into(), Some(NotifyType::Info))
    );
}

#[tokio::test]
async fn rejects_bare_and_reserved_objectives() {
    let harness = harness(
        "reject",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("Ship it").await;
    assert!(harness.stored_goal().is_none());
    assert_eq!(
        harness.records.lock().unwrap().notifications.last().unwrap(),
        &(
            "Use /goal set <objective> to set a goal".into(),
            Some(NotifyType::Error)
        )
    );

    let result = harness.run_tool(json!({"action": "set", "objective": "pause"})).await;
    assert_eq!(text_of(&result), "Objective cannot be a goal control command: pause");
    assert!(harness.stored_goal().is_none());
}

#[tokio::test]
async fn rejects_objectives_longer_than_ten_thousand_utf16_units() {
    let harness = harness(
        "long",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    let accepted = "x".repeat(10_000);
    harness.run_command(&format!("set {accepted}")).await;
    assert_eq!(harness.stored_goal().unwrap().objective.len(), 10_000);

    let rejected = "y".repeat(10_001);
    harness.run_command(&format!("set {rejected}")).await;
    let records = harness.records.lock().unwrap();
    assert!(
        records
            .notifications
            .iter()
            .any(|(message, _)| message == "Objective too long (10001 > 10000 chars)")
    );
}

#[tokio::test]
async fn injects_the_running_objective_into_the_system_prompt() {
    let harness = harness(
        "inject",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;
    let combined = harness
        .runner
        .emit_before_agent_start(
            &JsString::from("prompt"),
            None,
            &JsString::from("base prompt"),
            pi_coding_agent::core::system_prompt::BuildSystemPromptOptions::default(),
        )
        .await
        .expect("system prompt modified");
    let prompt = combined.system_prompt.unwrap();
    assert!(prompt.starts_with("base prompt\n\n<goal>\n"));
    assert!(prompt.contains("Long-running objective: Ship it"));
    assert!(!prompt.contains("budget"));
}

#[tokio::test]
async fn continues_after_agent_end_with_supervisor_instructions() {
    let harness = harness(
        "agent-end",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "Continue working toward this objective until it is achieved: Ship it"
                .to_owned()
                .into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;
    harness.records.lock().unwrap().messages.clear();
    harness
        .run_agent_end(vec![assistant_message("still working", "stop")])
        .await;

    assert_eq!(harness.stored_goal().unwrap().continuation_turns, Some(1.0));
    let records = harness.records.lock().unwrap();
    assert_eq!(records.messages.len(), 1);
    assert_eq!(records.messages[0].custom_type, "supervisor");
    assert_eq!(
        records.messages[0].content,
        pi_coding_agent::core::extensions::types::CustomMessageContent::Text(
            "<supervisor-instruction>\nContinue working toward this objective until it is achieved: Ship it\n</supervisor-instruction>".to_owned().into()
        )
    );
}

#[tokio::test]
async fn pause_requires_reason_and_resume_clears_paused_state() {
    let harness = harness(
        "pause",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;

    let result = harness.run_tool(json!({"action": "pause"})).await;
    assert_eq!(text_of(&result), "Reason is required to pause a goal.");
    assert!(harness.stored_goal().unwrap().paused_at.is_none());

    let result = harness.run_tool(json!({"action": "pause", "reason": "blocked"})).await;
    assert_eq!(text_of(&result), "Goal paused: blocked");
    let goal = harness.stored_goal().unwrap();
    assert!(goal.paused_at.is_some());
    assert_eq!(goal.pause_reason.clone(), Some(JsString::from("blocked")));

    let result = harness.run_tool(json!({"action": "resume"})).await;
    assert_eq!(text_of(&result), "Goal resumed: Ship it");
    let goal = harness.stored_goal().unwrap();
    assert!(goal.paused_at.is_none());
    assert!(goal.pause_reason.is_none());
}

#[tokio::test]
async fn completion_requires_a_report_and_marks_the_goal_complete() {
    let harness = harness(
        "complete",
        GoalSupervisorResponse::Complete {
            display_id: None,
            reason: "verified".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;

    let result = harness.run_tool(json!({"action": "complete"})).await;
    assert_eq!(text_of(&result), "Completion report is required.");
    assert!(harness.stored_goal().unwrap().completed_at.is_none());

    let result = harness
        .run_tool(json!({"action": "complete", "completionReport": "tests pass"}))
        .await;
    assert_eq!(text_of(&result), "Goal marked complete: tests pass");
    let goal = harness.stored_goal().unwrap();
    assert!(goal.completed_at.is_some());
    assert_eq!(goal.completion_reason.clone(), Some(JsString::from("tests pass")));
}

#[tokio::test]
async fn review_evidence_accumulates_and_is_sent_to_completion_review() {
    let harness = harness(
        "evidence",
        GoalSupervisorResponse::Complete {
            display_id: None,
            reason: "verified".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;
    harness.run_input("hello").await;
    harness.run_end_turn("step done", false).await;
    harness.run_end_turn("failed", true).await;

    let goal = harness.stored_goal().unwrap();
    let evidence = goal.review_evidence.unwrap();
    assert_eq!(evidence.len(), 2);
    assert_eq!(
        evidence[0],
        pi_coding_agent::bundled_extensions::goal::goal_types::GoalReviewEvidenceEvent::User {
            text: "hello".to_owned().into()
        }
    );
    assert_eq!(
        evidence[1],
        pi_coding_agent::bundled_extensions::goal::goal_types::GoalReviewEvidenceEvent::EndTurn {
            reason: "step done".to_owned().into()
        }
    );

    let result = harness
        .run_tool(json!({"action": "complete", "completionReport": "done"}))
        .await;
    assert_eq!(text_of(&result), "Goal marked complete: done");
    let payloads = harness.review_payloads.lock().unwrap();
    let (kind, payload) = payloads.last().unwrap();
    assert_eq!(kind, "goal_completion_review");
    assert_eq!(payload["objective"], pi_js_compat::js_json!("Ship it"));
    assert_eq!(payload["completionReport"], pi_js_compat::js_json!("done"));
    assert_eq!(
        payload["conversationEvents"][0]["text"],
        pi_js_compat::js_json!("hello")
    );
    assert_eq!(
        payload["conversationEvents"][1]["reason"],
        pi_js_compat::js_json!("step done")
    );
    drop(payloads);

    let goal = harness.stored_goal().unwrap();
    assert!(goal.review_evidence.is_none());
}

#[tokio::test]
async fn status_and_clear_report_and_remove_the_goal() {
    let harness = harness(
        "status-clear",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;
    harness.run_command("").await;
    assert_eq!(
        harness.records.lock().unwrap().notifications.last().unwrap(),
        &("Goal: Ship it".into(), Some(NotifyType::Info))
    );
    harness.run_command("clear").await;
    assert!(harness.stored_goal().is_none());
    assert_eq!(
        harness.records.lock().unwrap().notifications.last().unwrap(),
        &("Goal cleared".into(), Some(NotifyType::Info))
    );
    harness.run_command("clear").await;
    assert_eq!(
        harness.records.lock().unwrap().notifications.last().unwrap(),
        &("No active goal".into(), Some(NotifyType::Info))
    );
}

#[tokio::test]
async fn session_start_restores_running_goal_and_requests_continuation() {
    let harness = harness(
        "session-start",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;
    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
        reason: SessionStartReason::Resume,
        previous_session_file: None,
    });
    harness.runner.emit(&mut event).await;
    assert_eq!(
        harness.records.lock().unwrap().notifications.last().unwrap(),
        &("Active goal: Ship it".into(), Some(NotifyType::Info))
    );
    let _ = (&harness.idle, &harness.pending, &harness.active_agents);
}

#[tokio::test]
async fn idle_review_with_wait_schedules_a_five_minute_review_status() {
    let harness = harness(
        "wait",
        GoalSupervisorResponse::Wait {
            display_id: None,
            reason: "agents running".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;
    *harness.active_agents.lock().unwrap() = 0.0;
    harness
        .run_agent_end(vec![assistant_message("still working", "stop")])
        .await;
    let records = harness.records.lock().unwrap();
    let statuses: Vec<_> = records
        .entries
        .iter()
        .filter(|(custom_type, _)| custom_type == "supervisor-status")
        .collect();
    assert!(!statuses.is_empty());
    let (_, data) = statuses.last().unwrap();
    assert_eq!(
        data.as_ref().unwrap()["message"],
        pi_js_compat::js_json!("Waiting: agents running")
    );
    assert!(data.as_ref().unwrap()["reviewAt"].is_string());
}

#[tokio::test]
async fn emits_abort_and_error_skips_without_reviewing() {
    let harness = harness(
        "abort",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;
    let before = harness.review_payloads.lock().unwrap().len();
    harness.run_agent_end(vec![assistant_message("", "aborted")]).await;
    assert_eq!(harness.review_payloads.lock().unwrap().len(), before);
    let records = harness.records.lock().unwrap();
    assert!(records.entries.iter().any(|(custom_type, data)| {
        custom_type == "supervisor-status"
            && data.as_ref().unwrap()["message"]
                == pi_js_compat::js_json!("Goal continuation skipped: the model turn was aborted.")
    }));
}

#[tokio::test]
async fn does_not_continue_when_session_continuation_is_set() {
    let harness = harness(
        "continuation",
        GoalSupervisorResponse::Continue {
            display_id: None,
            instructions: "continue".to_owned().into(),
            reason: "work remains".to_owned().into(),
        },
    )
    .await;
    harness.run_command("set Ship it").await;
    harness.records.lock().unwrap().messages.clear();
    let mut event = ExtensionEvent::AgentEnd(AgentEndEvent {
        messages: vec![assistant_message("still working", "stop")],
        session_continuation: Some(
            pi_coding_agent::core::extensions::types::AgentEndSessionContinuation::CwdRelocation,
        ),
    });
    harness.runner.emit(&mut event).await;
    assert!(harness.records.lock().unwrap().messages.is_empty());
    assert_eq!(harness.stored_goal().unwrap().continuation_turns, Some(0.0));
}
