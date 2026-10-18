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

fn faux() -> pi_ai::providers::faux::FauxProviderHandle {
    faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".to_owned()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    })
}

async fn run_rust(dir: &str) -> Value {
    let faux = faux();
    let model = faux.get_model(None).expect("faux model");
    faux.set_responses(vec![
        faux_assistant_message("world", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("again", FauxAssistantMessageOptions::default()).into(),
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
    match event {
        AgentSessionEvent::Agent(agent_event) => {
            let value = serde_json::to_value(agent_event).ok();
            let name = value
                .as_ref()
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("agent")
                .to_owned();
            if matches!(
                agent_event,
                pi_agent_core::agent::types::AgentEvent::MessageStart { .. }
                    | pi_agent_core::agent::types::AgentEvent::MessageEnd { .. }
            ) {
                let message = value.as_ref().and_then(|value| value.get("message")).cloned();
                return (
                    name,
                    message.map(|message| json!({"message": message_summary_value(&message)})),
                );
            }
            (name, None)
        }
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
    json!({
        "role": role,
        "text": text,
        "stopReason": value.get("stopReason"),
        "errorMessage": value.get("errorMessage"),
    })
}

fn normalize(value: &Value, ids: &mut std::collections::HashMap<String, String>, dir: &str) -> Value {
    match value {
        Value::String(text) => {
            if text == dir || text.starts_with(&format!("{dir}/")) {
                return Value::String(text.replace(dir, "<dir>"));
            }
            if is_uuid(text) {
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
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), normalize(value, ids, dir)))
                .collect(),
        ),
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
    assert_eq!(actual["events"], expected["events"], "event stream mismatch");
    assert_eq!(actual["stats"], expected["stats"], "stats mismatch");
    assert_eq!(actual["messages"], expected["messages"], "message mismatch");
    assert_eq!(actual["lines"], expected["lines"], "session JSONL mismatch");
}
