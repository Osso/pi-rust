#[path = "differential_agent_harness/support.rs"]
mod support;

use pi_js_compat::js_json::JsString;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pi_agent_core::harness::agent_harness::{
    AgentHarness, AgentHarnessOptions, CreateHarnessError, HarnessEventListener, HarnessHookHandler, HarnessTool,
    HarnessUnavailable, HookName, QueueInput,
};
use pi_agent_core::harness::session::types::{OperationIntent, RecordBody, SessionMetadata};
use pi_agent_core::harness::session::{InMemorySessionStorage, Session};
use pi_agent_core::harness::telemetry::{
    AiSpanName, HarnessSpanName, agent_telemetry_schemas, ai_telemetry_schema, harness_telemetry_schema, start_ai_span,
    start_harness_span,
};
use pi_agent_core::harness::{QueueMode, ThinkingLevel};
use pi_ai::models::{CreateModelsOptions, create_models};
use pi_ai::providers::all::get_builtin_model;
use pi_ai::request::{BoxFuture, SimpleStreamOptions};
use pi_ai::types::Model;
use pi_telemetry::{AttributeValue, InMemoryTelemetryContext, SpanAttributes};
use serde_json::{Value, json};

const SOURCE_MODULES: [&str; 6] = [
    "packages/agent/src/harness/agent-harness.ts",
    "packages/agent/src/harness/telemetry.ts",
    "packages/agent/src/harness/session/index.ts",
    "packages/ai/src/models.ts",
    "packages/ai/src/compat.ts",
    "packages/telemetry/src/index.ts",
];

#[derive(Debug)]
struct Failure;

impl Display for Failure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("failure")
    }
}

impl Error for Failure {}

fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(number) => Value::from(number.as_f64().expect("differential numbers must be finite")),
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(name, value)| (name.clone(), normalize(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Key order of every schema map (`spans`, `startAttributes`, `endAttributes`, `events`, `attributes`).
fn map_key_orders(value: &Value, path: &str) -> Vec<(String, Vec<String>)> {
    const MAP_FIELDS: [&str; 5] = ["spans", "startAttributes", "endAttributes", "events", "attributes"];
    let mut orders = Vec::new();
    match value {
        Value::Object(entries) => {
            for (name, child) in entries {
                let child_path = format!("{path}/{name}");
                if MAP_FIELDS.contains(&name.as_str())
                    && let Value::Object(map) = child
                {
                    orders.push((child_path.clone(), map.keys().cloned().collect()));
                }
                orders.extend(map_key_orders(child, &child_path));
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                orders.extend(map_key_orders(item, &format!("{path}[{index}]")));
            }
        }
        _ => {}
    }
    orders.sort();
    orders
}

fn request(oracle: &mut support::TsOracle, scenario: &str) -> Value {
    let response = oracle.request(&json!({ "id": scenario, "operation": scenario }));
    assert_eq!(response["ok"], true, "oracle scenario {scenario} failed: {response}");
    response["output"].clone()
}

fn assert_matches(oracle: &mut support::TsOracle, scenario: &str, actual: Value) {
    let expected = normalize(&request(oracle, scenario));
    let actual = normalize(&actual);
    assert_eq!(actual, expected, "scenario {scenario}");
    // Value equality ignores key order; schema maps keep TypeScript insertion order.
    assert_eq!(
        map_key_orders(&actual, ""),
        map_key_orders(&expected, ""),
        "scenario {scenario} key order"
    );
}

fn model(provider: &str, id: &str) -> Model {
    get_builtin_model(provider, id).expect("builtin model must exist")
}

fn create_session(id: &str) -> Session<SessionMetadata> {
    Session::new(Arc::new(InMemorySessionStorage::new(SessionMetadata {
        id: id.to_owned(),
        created_at: 1,
        parent_session_id: None,
    })))
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
        .unwrap()
        .harness
}

fn model_identity(model: &Model) -> Value {
    json!({ "provider": model.provider, "id": model.id })
}

fn resource_names(resources: &pi_agent_core::harness::AgentHarnessResources) -> Value {
    json!({
        "skills": resources.skills.as_ref().map(|skills| skills.iter().map(|skill| skill.name.clone()).collect::<Vec<_>>()),
        "promptTemplates": resources.prompt_templates.as_ref().map(|templates| templates.iter().map(|template| template.name.clone()).collect::<Vec<_>>()),
    })
}

async fn harness_defaults(harness: &AgentHarness) -> Value {
    let resources = harness.get_resources().await;
    let retry = harness.get_retry_policy().await;
    let compaction = harness.get_compaction_settings().await;
    let stream_options = harness.get_stream_options().await;
    json!({
        "name": harness.name(),
        "leafId": harness.get_leaf_id().await.unwrap(),
        "model": model_identity(&harness.get_model().await),
        "thinkingLevel": harness.get_thinking_level().await.as_str(),
        "activeTools": harness.get_active_tools().await,
        "tools": harness.get_tools().await.iter().map(HarnessTool::name).collect::<Vec<_>>(),
        "resources": resource_names(&resources),
        "streamOptions": { "maxTokens": stream_options.stream.max_tokens },
        "retry": { "enabled": retry.enabled, "maxRetries": retry.max_retries, "baseDelayMs": retry.base_delay_ms },
        "compaction": {
            "enabled": compaction.enabled,
            "reserveTokens": compaction.reserve_tokens,
            "keepRecentTokens": compaction.keep_recent_tokens,
        },
        "steeringMode": harness.get_steering_mode().await.as_str(),
        "followUpMode": harness.get_follow_up_mode().await.as_str(),
    })
}

async fn harness_config_roundtrip(harness: &AgentHarness) -> Value {
    harness.set_model(model("anthropic", "claude-sonnet-4-5")).await;
    harness.set_thinking_level(ThinkingLevel::High).await;

    let mut active_tools = vec!["one".to_owned()];
    harness.set_active_tools(active_tools.clone()).await;
    active_tools.push("mutated".to_owned());
    let mut read_active_tools = harness.get_active_tools().await;
    read_active_tools.push("mutated".to_owned());

    let mut tools = vec![HarnessTool::new(Arc::new(EchoTool::new("tool")))];
    harness.set_tools(tools.clone(), None).await;
    tools.push(HarnessTool::new(Arc::new(EchoTool::new("mutated"))));
    let mut read_tools = harness.get_tools().await;
    read_tools.push(HarnessTool::new(Arc::new(EchoTool::new("mutated"))));

    let mut resources = pi_agent_core::harness::AgentHarnessResources {
        skills: Some(vec![pi_agent_core::harness::Skill {
            name: JsString::from("skill"),
            description: "desc".to_owned(),
            content: "body".to_owned(),
            file_path: JsString::from("/tmp/SKILL.md"),
            disable_model_invocation: None,
        }]),
        prompt_templates: Some(vec![pi_agent_core::harness::PromptTemplate {
            name: JsString::from("template"),
            description: None,
            content: "body".to_owned(),
        }]),
    };
    harness.set_resources(resources.clone()).await;
    resources.skills.as_mut().unwrap().push(pi_agent_core::harness::Skill {
        name: JsString::from("mutated"),
        description: "desc".to_owned(),
        content: "body".to_owned(),
        file_path: JsString::from("/tmp/OTHER.md"),
        disable_model_invocation: None,
    });
    let mut read_resources = harness.get_resources().await;
    read_resources
        .skills
        .as_mut()
        .unwrap()
        .push(pi_agent_core::harness::Skill {
            name: JsString::from("mutated"),
            description: "desc".to_owned(),
            content: "body".to_owned(),
            file_path: JsString::from("/tmp/OTHER.md"),
            disable_model_invocation: None,
        });

    let mut stream_options = SimpleStreamOptions::default();
    stream_options.stream.max_tokens = Some(10.0);
    harness.set_stream_options(stream_options.clone()).await;
    stream_options.stream.max_tokens = Some(20.0);
    let mut read_stream_options = harness.get_stream_options().await;
    read_stream_options.stream.max_tokens = Some(30.0);

    let retry_policy = pi_ai::utils::retry::RetryPolicy {
        enabled: true,
        max_retries: 2,
        base_delay_ms: 10.0,
    };
    harness.set_retry_policy(retry_policy).await;

    let compaction_settings = pi_agent_core::harness::CompactionSettings {
        enabled: false,
        reserve_tokens: 1.0,
        keep_recent_tokens: 2.0,
    };
    harness.set_compaction_settings(compaction_settings).await;

    harness.set_steering_mode(QueueMode::All).await;
    harness.set_follow_up_mode(QueueMode::All).await;

    let final_resources = harness.get_resources().await;
    let final_retry = harness.get_retry_policy().await;
    let final_compaction = harness.get_compaction_settings().await;
    let final_stream_options = harness.get_stream_options().await;
    json!({
        "model": model_identity(&harness.get_model().await),
        "thinkingLevel": harness.get_thinking_level().await.as_str(),
        "activeTools": harness.get_active_tools().await,
        "tools": harness.get_tools().await.iter().map(HarnessTool::name).collect::<Vec<_>>(),
        "resources": resource_names(&final_resources),
        "streamOptions": { "maxTokens": final_stream_options.stream.max_tokens },
        "retry": {
            "enabled": final_retry.enabled,
            "maxRetries": final_retry.max_retries,
            "baseDelayMs": final_retry.base_delay_ms,
        },
        "compaction": {
            "enabled": final_compaction.enabled,
            "reserveTokens": final_compaction.reserve_tokens,
            "keepRecentTokens": final_compaction.keep_recent_tokens,
        },
        "steeringMode": harness.get_steering_mode().await.as_str(),
        "followUpMode": harness.get_follow_up_mode().await.as_str(),
    })
}

async fn rejection<T>(operation: &str, result: Result<T, HarnessUnavailable>) -> Value {
    match result {
        Ok(_) => json!({ "operation": operation, "resolved": true }),
        Err(error) => json!({
            "operation": operation,
            "name": error.name(),
            "operationField": error.operation(),
        }),
    }
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

async fn unfinished_rejections(harness: &AgentHarness, callback_called: &Arc<AtomicBool>) -> Vec<Value> {
    let message = user_message();
    vec![
        rejection("prompt", harness.prompt("hello", None).await).await,
        rejection("skill", harness.skill("skill", None).await).await,
        rejection(
            "promptFromTemplate",
            harness.prompt_from_template("template", None).await,
        )
        .await,
        rejection("compact", harness.compact(None).await).await,
        rejection("navigateTree", harness.navigate_tree(None, None).await).await,
        rejection("resume", harness.resume().await).await,
        rejection("abort", harness.abort().await).await,
        rejection(
            "steer",
            harness
                .steer(QueueInput::Message(Box::new(message.clone())), None)
                .await,
        )
        .await,
        rejection(
            "followUp",
            harness
                .follow_up(QueueInput::Message(Box::new(message.clone())), None)
                .await,
        )
        .await,
        rejection(
            "nextRun",
            harness
                .next_run(QueueInput::Message(Box::new(message.clone())), None)
                .await,
        )
        .await,
        rejection("cancelQueued", harness.cancel_queued("queued").await).await,
        rejection("recordUsage", harness.record_usage(usage(), None).await).await,
        rejection("waitForIdle", harness.wait_for_idle().await).await,
        rejection(
            "runWhenIdle",
            harness
                .run_when_idle({
                    let callback_called = Arc::clone(callback_called);
                    move || callback_called.store(true, Ordering::SeqCst)
                })
                .await,
        )
        .await,
        rejection("peekAction", harness.peek_action().await).await,
        rejection("executeAction", harness.execute_action().await).await,
        rejection("runToCompletion", harness.run_to_completion().await).await,
        rejection("watch", harness.watch().await).await,
        rejection("lane", harness.lane("main").await).await,
        rejection("createLane", harness.create_lane("thread", None).await).await,
        rejection("lanes", harness.lanes().await).await,
        rejection("watchSession", harness.watch_session().await).await,
    ]
}

async fn harness_unfinished(harness: &AgentHarness) -> Value {
    let callback_called = Arc::new(AtomicBool::new(false));
    let rejections = unfinished_rejections(harness, &callback_called).await;
    harness.close().await;
    let closed: Vec<Value> = unfinished_rejections(harness, &callback_called)
        .await
        .into_iter()
        .map(|entry| json!({ "operation": entry["operation"], "name": entry["name"] }))
        .collect();

    let hook_handler: HarnessHookHandler =
        Arc::new(|_event: Value| Box::pin(async { Value::Null }) as BoxFuture<'static, Value>);
    let listener: HarnessEventListener = Arc::new(|_event: Value| Box::pin(async {}) as BoxFuture<'static, ()>);
    let hooks_error = match harness.hooks.on(HookName::BeforeRun, hook_handler, None) {
        Ok(_) => panic!("hooks.on must reject"),
        Err(error) => error,
    };
    let events_error = match harness.events.on("event", listener, None) {
        Ok(_) => panic!("events.on must reject"),
        Err(error) => error,
    };

    json!({
        "rejections": rejections,
        "closed": closed,
        "callbackCalled": callback_called.load(Ordering::SeqCst),
        "hooksError": { "name": hooks_error.name(), "operation": hooks_error.operation() },
        "eventsError": { "name": events_error.name(), "operation": events_error.operation() },
    })
}

async fn harness_create_empty() -> Value {
    let session = create_session("session");
    let created = AgentHarness::create(options(session.clone())).await.unwrap();
    let leaf_id_before = created.harness.get_leaf_id().await.unwrap();
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
    let leaf_id_after = created.harness.get_leaf_id().await.unwrap();
    created.harness.close().await;
    json!({
        "suspended": created.suspended.len(),
        "name": created.harness.name(),
        "leafIdBefore": leaf_id_before,
        "leafMatchesAppended": leaf_id_after.as_deref() == Some(entry_id.as_str()),
    })
}

async fn harness_create_restore() -> Value {
    let session = create_session("recorded");
    session
        .append_record(RecordBody::OperationStarted {
            id: "run".to_owned(),
            lane: "main".to_owned(),
            source_leaf_id: None,
            intent: OperationIntent::Run {
                original_prompt: Vec::new(),
                initial_messages: Vec::new(),
                system_prompt_override: None,
                resume_data: None,
            },
        })
        .await
        .unwrap();
    match AgentHarness::create(options(session)).await {
        Ok(_) => json!({ "resolved": true }),
        Err(CreateHarnessError::NotImplemented(error)) => {
            json!({ "name": error.name(), "operation": error.operation })
        }
        Err(CreateHarnessError::Session(error)) => panic!("unexpected session failure: {error}"),
    }
}

fn attributes(pairs: &[(&str, AttributeValue)]) -> SpanAttributes {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

fn string(value: &str) -> AttributeValue {
    AttributeValue::String(value.to_owned())
}

fn boolean(value: bool) -> AttributeValue {
    AttributeValue::Boolean(value)
}

async fn telemetry_spans() -> Value {
    let context = InMemoryTelemetryContext::new();
    start_ai_span(
        &context,
        AiSpanName::PiAiRequest,
        attributes(&[
            ("pi.ai.operation", string("stream")),
            ("pi.ai.provider", string("provider")),
            ("pi.ai.model", string("model")),
            ("pi.ai.api", string("api")),
            ("pi.ai.streaming", boolean(true)),
        ]),
        |span| async move {
            span.set_attributes(attributes(&[("pi.ai.response.stop_reason", string("tool_use"))]));
            Ok::<_, Failure>(())
        },
    )
    .await
    .unwrap();
    start_harness_span(
        &context,
        HarnessSpanName::PiHarnessRun,
        attributes(&[
            ("pi.session.id", string("session")),
            ("pi.lane.name", string("main")),
            ("pi.operation.id", string("operation")),
            ("pi.operation.kind", string("run")),
            ("pi.operation.recovery", boolean(false)),
        ]),
        |span| async move {
            span.set_attributes(attributes(&[("pi.operation.outcome", string("completed"))]));
            Ok::<_, Failure>(())
        },
    )
    .await
    .unwrap();
    serde_json::to_value(context.get_spans()).expect("spans must serialize")
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
        PARAMETERS.get_or_init(|| json!({"type": "object", "properties": {}}))
    }

    fn label(&self) -> &str {
        &self.name
    }

    fn execute(
        &self,
        _tool_call_id: String,
        _params: Value,
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

#[tokio::test]
async fn agent_harness_and_telemetry_match_the_pinned_typescript_source() {
    let mut oracle = support::TsOracle::spawn(&SOURCE_MODULES);

    let defaults = create_harness().await;
    assert_matches(&mut oracle, "harness_defaults", harness_defaults(&defaults).await);

    let roundtrip = create_harness().await;
    assert_matches(
        &mut oracle,
        "harness_config_roundtrip",
        harness_config_roundtrip(&roundtrip).await,
    );

    let unfinished = create_harness().await;
    assert_matches(&mut oracle, "harness_unfinished", harness_unfinished(&unfinished).await);

    assert_matches(&mut oracle, "harness_create_empty", harness_create_empty().await);
    assert_matches(&mut oracle, "harness_create_restore", harness_create_restore().await);

    let schemas = json!({
        "ai": ai_telemetry_schema(),
        "harness": harness_telemetry_schema(),
        "agent": agent_telemetry_schemas().iter().map(|schema| schema.spans.keys().cloned().collect::<Vec<_>>()).collect::<Vec<_>>(),
    });
    assert_matches(&mut oracle, "telemetry_schemas", schemas);
    assert_matches(&mut oracle, "telemetry_spans", telemetry_spans().await);
}
