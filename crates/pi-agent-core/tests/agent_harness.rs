use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pi_agent_core::harness::agent_harness::{
    AgentHarness, AgentHarnessOptions, CreateHarnessError, HarnessClosed, HarnessEventListener, HarnessHookHandler,
    HarnessNotImplemented, HarnessTool, HarnessUnavailable, HookName, QueueInput, Resources,
};
use pi_agent_core::harness::session::types::{OperationIntent, RecordBody, SessionMetadata};
use pi_agent_core::harness::session::{InMemorySessionStorage, Session};
use pi_agent_core::harness::types::{PromptTemplate, Skill, ThinkingLevel};
use pi_ai::models::{CreateModelsOptions, create_models};
use pi_ai::providers::all::get_builtin_model;
use pi_ai::request::BoxFuture;
use pi_ai::types::Model;
use serde_json::Value;

fn create_session(id: &str) -> Session<SessionMetadata> {
    Session::new(Arc::new(InMemorySessionStorage::new(SessionMetadata {
        id: id.to_owned(),
        created_at: 1,
        parent_session_id: None,
    })))
}

fn model(provider: &str, id: &str) -> Model {
    get_builtin_model(provider, id).expect("builtin model must exist")
}

fn options(session: Session<SessionMetadata>) -> AgentHarnessOptions {
    AgentHarnessOptions::new(
        session,
        Arc::new(create_models(CreateModelsOptions::default())),
        model("google", "gemini-2.5-flash"),
    )
}

async fn create_harness() -> AgentHarness {
    AgentHarness::create(options(create_session("session")))
        .await
        .expect("record-free session must open")
        .harness
}

fn operation_started(id: &str) -> RecordBody {
    RecordBody::OperationStarted {
        id: id.to_owned(),
        lane: "main".to_owned(),
        source_leaf_id: None,
        intent: OperationIntent::Run {
            original_prompt: Vec::new(),
            initial_messages: Vec::new(),
            system_prompt_override: None,
            resume_data: None,
        },
    }
}

fn skill(name: &str, file_path: &str) -> Skill {
    Skill {
        name: JsString::from(name.to_owned()),
        description: "desc".to_owned(),
        content: "body".to_owned(),
        file_path: JsString::from(file_path.to_owned()),
        disable_model_invocation: None,
    }
}

fn template(name: &str) -> PromptTemplate {
    PromptTemplate {
        name: JsString::from(name.to_owned()),
        description: None,
        content: "body".into(),
    }
}

fn hook_handler() -> HarnessHookHandler {
    Arc::new(|_event: Value| Box::pin(async { Value::Null }) as BoxFuture<'static, Value>)
}

fn event_listener() -> HarnessEventListener {
    Arc::new(|_event: Value| Box::pin(async {}) as BoxFuture<'static, ()>)
}

fn unimplemented_operation(error: &HarnessUnavailable) -> &str {
    match error {
        HarnessUnavailable::NotImplemented(error) => &error.operation,
        HarnessUnavailable::Closed(_) => panic!("expected HarnessNotImplemented, got HarnessClosed"),
    }
}

async fn assert_unimplemented<T>(operation: &str, result: Result<T, HarnessUnavailable>) {
    let error = match result {
        Ok(_) => panic!("{operation} must reject"),
        Err(error) => error,
    };
    assert_eq!(unimplemented_operation(&error), operation, "{operation}");
}

#[tokio::test]
async fn opens_only_record_free_sessions_before_restore_is_implemented() {
    let session = create_session("session");
    let created = AgentHarness::create(options(session.clone()))
        .await
        .expect("record-free session must open");

    assert!(created.suspended.is_empty());
    assert_eq!(created.harness.name(), "main");
    assert_eq!(created.harness.get_leaf_id().await.unwrap(), None);
    assert_eq!(session.get_leaf_id().await.unwrap(), None);

    created.harness.close().await;

    let recorded = create_session("recorded");
    recorded.append_record(operation_started("run")).await.unwrap();
    let error = match AgentHarness::create(options(recorded)).await {
        Ok(_) => panic!("recorded sessions cannot restore"),
        Err(error) => error,
    };
    match error {
        CreateHarnessError::NotImplemented(error) => {
            assert_eq!(error.name(), "HarnessNotImplemented");
            assert_eq!(error.operation, "create.restore");
            assert_eq!(error.to_string(), "AgentHarness.create.restore is not implemented yet");
        }
        CreateHarnessError::Session(error) => panic!("unexpected session failure: {error}"),
    }
}

#[tokio::test]
async fn shares_the_durable_session_with_the_original_handle() {
    let session = create_session("session");
    let harness = AgentHarness::create(options(session.clone())).await.unwrap().harness;

    let entry_id = session
        .append_message(pi_agent_core::agent::AgentMessage::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text(JsString::from("hello")),
            input_source: None,
            provider_native: None,
            timestamp: 1,
        }))
        .await
        .unwrap();

    assert_eq!(harness.get_leaf_id().await.unwrap(), Some(entry_id.clone()));
    assert_eq!(session.get_leaf_id().await.unwrap(), Some(entry_id));
}

#[tokio::test]
async fn keeps_scaffold_safe_configuration_as_defensive_copies() {
    let harness = create_harness().await;
    let anthropic = model("anthropic", "claude-sonnet-4-5");
    harness.set_model(anthropic.clone()).await;
    assert_eq!(harness.get_model().await, anthropic);

    harness.set_thinking_level(ThinkingLevel::High).await;
    assert_eq!(harness.get_thinking_level().await, ThinkingLevel::High);

    let mut active_tools = vec!["one".to_owned()];
    harness.set_active_tools(active_tools.clone()).await;
    active_tools.push("mutated".to_owned());
    assert_eq!(harness.get_active_tools().await, vec!["one"]);
    let mut read_active_tools = harness.get_active_tools().await;
    read_active_tools.push("mutated".to_owned());
    assert_eq!(harness.get_active_tools().await, vec!["one"]);

    let tool = HarnessTool::new(Arc::new(EchoTool::new("tool")));
    let mut tools = vec![tool];
    harness.set_tools(tools.clone(), None).await;
    tools.push(HarnessTool::new(Arc::new(EchoTool::new("mutated"))));
    assert_eq!(harness.get_tools().await.len(), 1);
    assert_eq!(harness.get_tools().await[0].name(), "tool");
    let mut read_tools = harness.get_tools().await;
    read_tools.push(HarnessTool::new(Arc::new(EchoTool::new("mutated"))));
    assert_eq!(harness.get_tools().await.len(), 1);

    let resources = Resources {
        skills: Some(vec![skill("skill", "/tmp/SKILL.md")]),
        prompt_templates: Some(vec![template("template")]),
    };
    harness.set_resources(resources.clone()).await;
    let mut mutable_resources = resources.clone();
    mutable_resources
        .skills
        .as_mut()
        .unwrap()
        .push(skill("mutated", "/tmp/OTHER.md"));
    assert_eq!(harness.get_resources().await.skills.unwrap()[0].name, "skill");
    let mut read_resources = harness.get_resources().await;
    read_resources
        .skills
        .as_mut()
        .unwrap()
        .push(skill("mutated", "/tmp/OTHER.md"));
    assert_eq!(harness.get_resources().await.skills.unwrap().len(), 1);

    let mut stream_options = pi_ai::request::SimpleStreamOptions::default();
    stream_options.stream.max_tokens = Some(10.0);
    harness.set_stream_options(stream_options.clone()).await;
    stream_options.stream.max_tokens = Some(20.0);
    assert_eq!(harness.get_stream_options().await.stream.max_tokens, Some(10.0));
    let mut read_stream_options = harness.get_stream_options().await;
    read_stream_options.stream.max_tokens = Some(30.0);
    assert_eq!(harness.get_stream_options().await.stream.max_tokens, Some(10.0));

    let retry_policy = pi_ai::utils::retry::RetryPolicy {
        enabled: true,
        max_retries: 2,
        base_delay_ms: 10.0,
    };
    harness.set_retry_policy(retry_policy).await;
    assert_eq!(harness.get_retry_policy().await.max_retries, 2);

    let compaction_settings = pi_agent_core::harness::CompactionSettings {
        enabled: false,
        reserve_tokens: 1.0,
        keep_recent_tokens: 2.0,
    };
    harness.set_compaction_settings(compaction_settings).await;
    assert_eq!(harness.get_compaction_settings().await.reserve_tokens, 1.0);

    harness.set_steering_mode(pi_agent_core::harness::QueueMode::All).await;
    assert_eq!(
        harness.get_steering_mode().await,
        pi_agent_core::harness::QueueMode::All
    );
    harness.set_follow_up_mode(pi_agent_core::harness::QueueMode::All).await;
    assert_eq!(
        harness.get_follow_up_mode().await,
        pi_agent_core::harness::QueueMode::All
    );
}

#[tokio::test]
async fn defaults_match_the_typescript_constructor() {
    let harness = create_harness().await;

    assert_eq!(harness.get_thinking_level().await, ThinkingLevel::Off);
    assert!(harness.get_active_tools().await.is_empty());
    assert!(harness.get_tools().await.is_empty());
    let resources = harness.get_resources().await;
    assert!(resources.skills.is_none());
    assert!(resources.prompt_templates.is_none());
    assert_eq!(harness.get_stream_options().await.stream.max_tokens, None);
    let retry = harness.get_retry_policy().await;
    assert!(!retry.enabled);
    assert_eq!(retry.max_retries, 0);
    assert_eq!(retry.base_delay_ms, 1000.0);
    let compaction = harness.get_compaction_settings().await;
    assert!(compaction.enabled);
    assert_eq!(compaction.reserve_tokens, 16384.0);
    assert_eq!(compaction.keep_recent_tokens, 20000.0);
    assert_eq!(
        harness.get_steering_mode().await,
        pi_agent_core::harness::QueueMode::OneAtATime
    );
    assert_eq!(
        harness.get_follow_up_mode().await,
        pi_agent_core::harness::QueueMode::OneAtATime
    );
    assert_eq!(harness.get_model().await.id, "gemini-2.5-flash");
}

#[tokio::test]
async fn active_tools_default_to_tool_names() {
    let mut options = options(create_session("session"));
    options.tools = Some(vec![
        HarnessTool::new(Arc::new(EchoTool::new("one"))),
        HarnessTool::new(Arc::new(EchoTool::new("two"))),
    ]);
    let harness = AgentHarness::create(options).await.unwrap().harness;

    assert_eq!(harness.get_active_tools().await, vec!["one", "two"]);
    assert_eq!(harness.get_tools().await.len(), 2);

    harness
        .set_tools(vec![HarnessTool::new(Arc::new(EchoTool::new("three")))], None)
        .await;
    assert_eq!(harness.get_active_tools().await, vec!["three"]);

    harness
        .set_tools(
            vec![HarnessTool::new(Arc::new(EchoTool::new("four")))],
            Some(vec!["kept".to_owned()]),
        )
        .await;
    assert_eq!(harness.get_active_tools().await, vec!["kept"]);
}

#[tokio::test]
async fn rejects_every_unfinished_public_operation_explicitly() {
    let harness = create_harness().await;
    let callback_called = Arc::new(AtomicBool::new(false));
    let message = user_message();

    assert_unimplemented("prompt", harness.prompt("hello", None).await).await;
    assert_unimplemented("skill", harness.skill("skill", None).await).await;
    assert_unimplemented(
        "promptFromTemplate",
        harness.prompt_from_template("template", None).await,
    )
    .await;
    assert_unimplemented("compact", harness.compact(None).await).await;
    assert_unimplemented("navigateTree", harness.navigate_tree(None, None).await).await;
    assert_unimplemented("resume", harness.resume().await).await;
    assert_unimplemented("abort", harness.abort().await).await;
    assert_unimplemented(
        "steer",
        harness
            .steer(QueueInput::Message(Box::new(message.clone())), None)
            .await,
    )
    .await;
    assert_unimplemented(
        "followUp",
        harness
            .follow_up(QueueInput::Message(Box::new(message.clone())), None)
            .await,
    )
    .await;
    assert_unimplemented(
        "nextRun",
        harness
            .next_run(QueueInput::Message(Box::new(message.clone())), None)
            .await,
    )
    .await;
    assert_unimplemented("cancelQueued", harness.cancel_queued("queued").await).await;
    assert_unimplemented("recordUsage", harness.record_usage(usage(), None).await).await;
    assert_unimplemented("waitForIdle", harness.wait_for_idle().await).await;
    let callback_called_inner = Arc::clone(&callback_called);
    assert_unimplemented(
        "runWhenIdle",
        harness
            .run_when_idle(move || callback_called_inner.store(true, Ordering::SeqCst))
            .await,
    )
    .await;
    assert_unimplemented("peekAction", harness.peek_action().await).await;
    assert_unimplemented("executeAction", harness.execute_action().await).await;
    assert_unimplemented("runToCompletion", harness.run_to_completion().await).await;
    assert_unimplemented("watch", harness.watch().await).await;
    assert_unimplemented("lane", harness.lane("main").await).await;
    assert_unimplemented("createLane", harness.create_lane("thread", None).await).await;
    assert_unimplemented("lanes", harness.lanes().await).await;
    assert_unimplemented("watchSession", harness.watch_session().await).await;
    assert!(!callback_called.load(Ordering::SeqCst));

    let error = match harness.hooks.on(HookName::BeforeRun, hook_handler(), None) {
        Ok(_) => panic!("hooks.on must reject"),
        Err(error) => error,
    };
    assert_eq!(unimplemented_operation(&error), "hooks.on");
    let error = match harness.events.on("event", event_listener(), None) {
        Ok(_) => panic!("events.on must reject"),
        Err(error) => error,
    };
    assert_eq!(unimplemented_operation(&error), "events.on");
}

#[tokio::test]
async fn reports_harness_closed_for_unfinished_operations_after_close() {
    let harness = create_harness().await;
    harness.close().await;

    let error = harness.prompt("hello", None).await.expect_err("closed harness rejects");
    assert!(matches!(error, HarnessUnavailable::Closed(HarnessClosed)));
    let error = harness.wait_for_idle().await.expect_err("closed harness rejects");
    assert!(matches!(error, HarnessUnavailable::Closed(HarnessClosed)));
    let error = match harness.hooks.on(HookName::BeforeRun, hook_handler(), None) {
        Ok(_) => panic!("closed hooks must reject"),
        Err(error) => error,
    };
    assert!(matches!(error, HarnessUnavailable::Closed(HarnessClosed)));
    let error = match harness.events.on("event", event_listener(), None) {
        Ok(_) => panic!("closed events must reject"),
        Err(error) => error,
    };
    assert!(matches!(error, HarnessUnavailable::Closed(HarnessClosed)));
    assert_eq!(
        HarnessClosed.to_string(),
        "AgentHarness was closed while the operation was active"
    );
}

#[tokio::test]
async fn unimplemented_error_exposes_the_operation_name() {
    let error = HarnessNotImplemented::new("prompt");
    assert_eq!(error.name(), "HarnessNotImplemented");
    assert_eq!(error.operation, "prompt");
    assert_eq!(error.to_string(), "AgentHarness.prompt is not implemented yet");
}

fn user_message() -> pi_agent_core::agent::AgentMessage {
    pi_agent_core::agent::AgentMessage::User(pi_ai::types::UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: pi_ai::types::UserContent::Text(JsString::from("hello")),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    })
}

fn usage() -> pi_ai::types::Usage {
    pi_ai::types::Usage {
        input: 1.0,
        output: 2.0,
        cache_read: 0.0,
        cache_write: 0.0,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: 3.0,
        cost: pi_ai::types::UsageCost::default(),
    }
}

struct EchoTool {
    name: String,
}

impl EchoTool {
    fn new(name: &str) -> Self {
        Self { name: name.to_owned() }
    }
}

impl pi_agent_core::agent::AgentTool for EchoTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "echo"
    }

    fn parameters(&self) -> &Value {
        static PARAMETERS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| serde_json::json!({"type": "object", "properties": {}}))
    }

    fn label(&self) -> &str {
        &self.name
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: JsValue,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: pi_agent_core::agent::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<pi_agent_core::agent::AgentToolResult, pi_agent_core::agent::ToolError>> {
        Box::pin(async {
            Ok(pi_agent_core::agent::AgentToolResult {
                content: Vec::new(),
                details: None,
                is_error: None,
                terminate: None,
            })
        })
    }
}
