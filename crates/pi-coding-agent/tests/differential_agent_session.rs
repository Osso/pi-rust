//! Differential coverage for `core/agent-session.ts` + `core/sdk.ts`.
//!
//! Runs one multi-turn scenario through the real TypeScript SDK and the Rust
//! port, then compares event summaries and normalized session JSONL.

#[path = "support/oracle.rs"]
mod oracle;

use std::sync::Arc;

use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, RegisterFauxProviderOptions, faux_assistant_message,
    faux_provider,
};
use pi_coding_agent::core::agent_session::PromptOptions;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::CreateAgentSessionOptions;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::SettingsManager;
use serde_json::{Value, json};

struct EndTurnTool;

impl pi_agent_core::agent::types::AgentTool for EndTurnTool {
    fn name(&self) -> &str {
        "end_turn"
    }

    fn description(&self) -> &str {
        "End the current model turn. Provide a concise reason explaining why the turn is finished."
    }

    fn parameters(&self) -> &Value {
        static PARAMETERS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        PARAMETERS.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {"reason": {"type": "string"}},
                "required": ["reason"]
            })
        })
    }

    fn label(&self) -> &str {
        "end_turn"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<tokio_util::sync::CancellationToken>,
        _on_update: pi_agent_core::agent::types::AgentToolUpdateCallback,
        _execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> pi_ai::request::BoxFuture<
        'static,
        Result<pi_agent_core::agent::types::AgentToolResult, pi_agent_core::agent::types::ToolError>,
    > {
        Box::pin(async move {
            let reason = params
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            if reason.is_empty() {
                return Err(pi_agent_core::agent::types::ToolError::new(
                    "end_turn reason must be a non-empty string",
                ));
            }
            let mut result = pi_agent_core::agent::types::AgentToolResult::text(format!("Turn ended: {reason}"));
            result.details = Some(json!({"reason": reason}));
            result.terminate = Some(true);
            Ok(result)
        })
    }
}

fn faux() -> pi_ai::providers::faux::FauxProviderHandle {
    faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        api: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".to_owned()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        token_size: Some(pi_ai::providers::faux::FauxTokenSize {
            min: Some(1.0),
            max: Some(1.0),
        }),
        tokens_per_second: Some(1_000_000.0),
        ..Default::default()
    })
}

async fn run_rust(dir: &str) -> Value {
    let faux = faux();
    let model = faux.get_model(None).expect("faux model");
    let tool_call = |id: &str| {
        pi_ai::types::AssistantContent::ToolCall(pi_ai::types::ToolCall {
            r#type: pi_ai::types::ToolCallTag::Tag,
            id: id.to_owned(),
            name: "end_turn".to_owned(),
            arguments: json!({"reason": "done"}).as_object().cloned().unwrap_or_default(),
            execution: None,
            thought_signature: None,
            namespace: None,
        })
    };
    faux.set_responses(vec![
        faux_assistant_message(
            pi_ai::providers::faux::FauxContent::Blocks(vec![
                pi_ai::providers::faux::faux_text("world"),
                tool_call("call-1"),
            ]),
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::types::StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
        faux_assistant_message(
            pi_ai::providers::faux::FauxContent::Blocks(vec![
                pi_ai::providers::faux::faux_text("again"),
                tool_call("call-2"),
            ]),
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::types::StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
    ]);
    let mut data = serde_json::Map::new();
    data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(data))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("runtime");
    runtime.register_native_provider(Arc::clone(&faux.provider));
    let session_dir = std::path::Path::new(dir).join("sessions").display().to_string();
    let session_manager = SessionManager::create(dir, Some(&session_dir), None).expect("session manager");
    let settings_manager =
        SettingsManager::create(dir, Some(std::path::Path::new(dir)), Default::default()).expect("settings");
    let result = pi_coding_agent::core::sdk::create_agent_session(CreateAgentSessionOptions {
        cwd: Some(dir.to_owned()),
        agent_dir: Some(dir.to_owned()),
        model_runtime: Some(runtime),
        model: Some(model),
        resource_loader: Some(StaticResourceLoader::new()),
        session_manager: Some(session_manager),
        settings_manager: Some(Arc::new(std::sync::Mutex::new(settings_manager))),
        control_db_path: Some(std::path::Path::new(dir).join("control.db").display().to_string()),
        base_tools: vec![Arc::new(EndTurnTool)],
        ..Default::default()
    })
    .await
    .expect("session");
    let session = result.session;
    let events = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let sink = Arc::clone(&events);
    session.subscribe(Arc::new(move |event| {
        let (name, value) = event_name_and_value(event);
        let mut entry = serde_json::Map::new();
        entry.insert("type".to_owned(), Value::String(name));
        if let Some(Value::Object(fields)) = value {
            for (key, field) in fields {
                entry.insert(key, field);
            }
        }
        if entry.get("type").and_then(Value::as_str) == Some("") {
            return;
        }
        sink.lock().unwrap().push(Value::Object(entry));
    }));
    session
        .prompt("hello", PromptOptions::default())
        .await
        .expect("prompt 1");
    session.set_thinking_level(pi_agent_core::harness::types::ThinkingLevel::High);
    session
        .prompt("second", PromptOptions::default())
        .await
        .expect("prompt 2");
    session.set_session_name("Named session");
    let stats = session.get_session_stats();
    let session_file = session.session_file().expect("session file");
    let lines: Vec<Value> = std::fs::read_to_string(&session_file)
        .expect("session file")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("jsonl line"))
        .collect();
    let messages: Vec<Value> = session.messages().iter().map(message_summary).collect();
    session.abort().await.expect("abort");
    session.dispose();
    let events = events.lock().unwrap().clone();
    json!({
        "events": events,
        "stats": {
            "userMessages": stats.user_messages,
            "assistantMessages": stats.assistant_messages,
            "toolCalls": stats.tool_calls,
            "toolResults": stats.tool_results,
            "totalMessages": stats.total_messages,
        },
        "messages": messages,
        "lines": lines,
        "dir": dir,
    })
}

fn event_name_and_value(event: &pi_coding_agent::core::agent_session::AgentSessionEvent) -> (String, Option<Value>) {
    use pi_coding_agent::core::agent_session::AgentSessionEvent;
    let (agent_event, runtime_message_marker) = match event {
        AgentSessionEvent::Agent(agent_event) => (Some(agent_event), None),
        AgentSessionEvent::Message(message_event) => (Some(&message_event.event), message_event.runtime_message_marker),
        _ => (None, None),
    };
    if let Some(agent_event) = agent_event {
        use pi_agent_core::agent::types::AgentEvent;
        return match agent_event {
            AgentEvent::MessageUpdate {
                message,
                assistant_message_event,
            } => (
                "message_update".to_owned(),
                Some(json!({
                    "message": message_summary(message),
                    "assistantMessageEvent": assistant_message_event,
                })),
            ),
            AgentEvent::MessageStart { message } | AgentEvent::MessageEnd { message } => {
                let mut value = json!({"message": message_summary(message)});
                if let (Some(marker), Value::Object(object)) = (runtime_message_marker, &mut value) {
                    object.insert("runtimeMessageMarker".to_owned(), json!(marker));
                }
                let name = if matches!(agent_event, AgentEvent::MessageStart { .. }) {
                    "message_start"
                } else {
                    "message_end"
                };
                (name.to_owned(), Some(value))
            }
            other => {
                let value = serde_json::to_value(other).ok();
                let name = value
                    .as_ref()
                    .and_then(|value| value.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("agent")
                    .to_owned();
                (name, None)
            }
        };
    }
    match event {
        AgentSessionEvent::AgentEnd(event) => ("agent_end".to_owned(), Some(json!({"willRetry": event.will_retry}))),
        AgentSessionEvent::ThinkingLevelChanged(event) => (
            "thinking_level_changed".to_owned(),
            Some(json!({"level": event.level.as_str()})),
        ),
        AgentSessionEvent::QueueUpdate(event) => (
            "queue_update".to_owned(),
            Some(json!({"steering": event.steering, "followUp": event.follow_up})),
        ),
        other => {
            let value = serde_json::to_value(other).ok();
            let name = value
                .as_ref()
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("event")
                .to_owned();
            (name, None)
        }
    }
}

fn message_summary(message: &pi_agent_core::agent::types::AgentMessage) -> Value {
    let value = serde_json::to_value(message).expect("message json");
    message_summary_value(&value)
}

fn message_summary_value(value: &Value) -> Value {
    let role = value.get("role").and_then(Value::as_str).unwrap_or_default();
    let text = match value.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<String>(),
        _ => String::new(),
    };
    let mut summary = serde_json::Map::new();
    summary.insert("role".to_owned(), Value::String(role.to_owned()));
    summary.insert("text".to_owned(), Value::String(text));
    if let Some(stop_reason) = value.get("stopReason").filter(|value| !value.is_null()) {
        summary.insert("stopReason".to_owned(), stop_reason.clone());
    }
    if let Some(error_message) = value.get("errorMessage").filter(|value| !value.is_null()) {
        summary.insert("errorMessage".to_owned(), error_message.clone());
    }
    Value::Object(summary)
}

fn normalize(value: &Value, ids: &mut std::collections::HashMap<String, String>, dir: &str) -> Value {
    match value {
        Value::String(text) => {
            if text == dir || text.starts_with(&format!("{dir}/")) {
                return Value::String(text.replace(dir, "<dir>"));
            }
            if is_generated_id(text) || is_short_id(text) || is_uuid(text) {
                let next = format!("<id-{}>", ids.len());
                let marker = ids.entry(text.clone()).or_insert(next).clone();
                return Value::String(marker);
            }
            if is_iso_timestamp(text) {
                return Value::String("<ts>".to_owned());
            }
            Value::String(text.clone())
        }
        Value::Number(number) => {
            if number.as_i64().is_some_and(|value| value > 1_000_000_000_000) {
                Value::String("<ts>".to_owned())
            } else {
                Value::Number(number.clone())
            }
        }
        Value::Array(items) => Value::Array(items.iter().map(|item| normalize(item, ids, dir)).collect()),
        Value::Object(object) => {
            let is_usage = object.contains_key("totalTokens");
            Value::Object(
                object
                    .iter()
                    .map(|(key, value)| {
                        if is_usage
                            && matches!(
                                key.as_str(),
                                "input" | "output" | "cacheRead" | "cacheWrite" | "totalTokens"
                            )
                        {
                            return (key.clone(), Value::String("<n>".to_owned()));
                        }
                        (key.clone(), normalize(value, ids, dir))
                    })
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.chars().enumerate().all(|(index, character)| {
            if [8, 13, 18, 23].contains(&index) {
                character == '-'
            } else {
                character.is_ascii_hexdigit()
            }
        })
}

fn is_short_id(text: &str) -> bool {
    text.len() == 8 && text.chars().all(|character| character.is_ascii_hexdigit())
}

fn is_generated_id(text: &str) -> bool {
    if let Some(rest) = text.strip_prefix("call-")
        && !rest.is_empty()
        && rest.chars().all(|character| character.is_ascii_digit())
    {
        return true;
    }
    let Some((prefix, rest)) = text.split_once(':') else {
        return false;
    };
    if prefix != "tool" && prefix != "faux" {
        return false;
    }
    let Some((timestamp, random)) = rest.split_once(':') else {
        return false;
    };
    timestamp.chars().all(|character| character.is_ascii_digit())
        && random.chars().all(|character| character.is_ascii_alphanumeric())
}

fn is_iso_timestamp(text: &str) -> bool {
    text.len() >= 20 && text.as_bytes().get(4) == Some(&b'-') && text.ends_with('Z')
}

#[test]
fn agent_session_multi_turn_matches_typescript() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state_dir = tempfile::tempdir().expect("state dir");
    let dir_path = dir.path().display().to_string();
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "agent-session-oracle.ts",
        "packages/coding-agent/src/core/sdk.ts",
        &[(
            "PI_CODING_AGENT_STATE_DIR",
            state_dir.path().display().to_string().into(),
        )],
    );
    let response = oracle.request(&json!({"dir": dir_path}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let expected = &response["value"];
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let actual = runtime.block_on(run_rust(&dir_path));
    let mut ids = std::collections::HashMap::new();
    let expected = normalize(expected, &mut ids, &dir_path);
    let mut actual_ids = std::collections::HashMap::new();
    let actual = normalize(&actual, &mut actual_ids, &dir_path);
    assert_json_seq(&actual["events"], &expected["events"], "event stream");
    assert_eq!(actual["stats"], expected["stats"], "stats mismatch");
    assert_eq!(actual["messages"], expected["messages"], "message mismatch");
    assert_eq!(actual["lines"], expected["lines"], "session JSONL mismatch");
}

fn assert_json_seq(actual: &Value, expected: &Value, label: &str) {
    let actual = actual.as_array().expect("actual array");
    let expected = expected.as_array().expect("expected array");
    assert_eq!(actual.len(), expected.len(), "{label} length mismatch");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "{label} mismatch at index {index}");
    }
}
