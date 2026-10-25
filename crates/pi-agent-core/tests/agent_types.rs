use pi_agent_core::agent::{
    AgentContext, AgentError, AgentEvent, AgentMessage, AgentToolResult, BeforeToolCallContext,
    ShouldStopAfterTurnContext,
};
use pi_ai::types::{AssistantMessage, TextOrImageContent, ToolCall, UserMessage};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Map, Value, json};

fn user(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: pi_ai::types::UserContent::Text(JsString::from(text.to_owned())),
        input_source: None,
        provider_native: None,
        timestamp: 1000.0,
    })
}

#[test]
fn agent_message_round_trips_standard_roles() {
    let message = user("hello");
    let value = serde_json::to_value(&message).unwrap();
    assert_eq!(value, json!({"role": "user", "content": "hello", "timestamp": 1000}));
    let parsed: AgentMessage = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, message);
}

#[test]
fn agent_message_round_trips_custom_objects_verbatim() {
    let value = json!({"role": "notification", "text": "hi", "timestamp": 5, "extra": [1, 2]});
    let parsed: AgentMessage = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(parsed.role(), "notification");
    assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
}

#[test]
fn agent_message_round_trips_assistant_and_tool_result() {
    let assistant = AssistantMessage::empty("openai-responses", "openai", "mock", 2000.0);
    let message = AgentMessage::Assistant(Box::new(assistant.clone()));
    let value = serde_json::to_value(&message).unwrap();
    assert_eq!(value["role"], json!("assistant"));
    assert_eq!(serde_json::from_value::<AgentMessage>(value).unwrap(), message);

    let tool_result = AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: "call-1".to_owned(),
        tool_name: "echo".to_owned(),
        content: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new("ok"))],
        details: None,
        is_error: false,
        timestamp: 3000.0,
    });
    let value = serde_json::to_value(&tool_result).unwrap();
    assert_eq!(value["role"], json!("toolResult"));
    assert_eq!(serde_json::from_value::<AgentMessage>(value).unwrap(), tool_result);
}

#[test]
fn agent_event_serializes_typescript_wire_names() {
    let events = [
        AgentEvent::AgentStart,
        AgentEvent::TurnStart,
        AgentEvent::ModelRequestStart,
        AgentEvent::ModelRequestEnd,
        AgentEvent::AgentEnd {
            messages: vec![user("hi")],
        },
        AgentEvent::TurnEnd {
            message: user("hi"),
            tool_results: Vec::new(),
        },
        AgentEvent::MessageStart { message: user("hi") },
        AgentEvent::MessageEnd { message: user("hi") },
        AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_owned(),
            tool_name: "echo".to_owned(),
            args: pi_js_compat::js_json!({"value": "x"}),
            started_at: 10.0,
        },
        AgentEvent::ToolExecutionUpdate {
            tool_call_id: "call-1".to_owned(),
            tool_name: "echo".to_owned(),
            args: pi_js_compat::js_json!({"value": "x"}),
            partial_result: AgentToolResult::text("partial"),
        },
        AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-1".to_owned(),
            tool_name: "echo".to_owned(),
            result: AgentToolResult::text("done"),
            is_error: false,
            started_at: 10.0,
            finished_at: 20.0,
        },
    ];

    let values: Vec<Value> = events
        .iter()
        .map(|event| serde_json::to_value(event).unwrap())
        .collect();
    assert_eq!(values[0], json!({"type": "agent_start"}));
    assert_eq!(values[2], json!({"type": "model_request_start"}));
    assert_eq!(
        values[4],
        json!({"type": "agent_end", "messages": [{"role": "user", "content": "hi", "timestamp": 1000}]})
    );
    assert_eq!(values[5]["type"], json!("turn_end"));
    assert_eq!(values[5]["toolResults"], json!([]));
    assert_eq!(values[8]["toolCallId"], json!("call-1"));
    assert_eq!(values[8]["startedAt"], json!(10.0));
    assert_eq!(values[9]["partialResult"]["content"][0]["text"], json!("partial"));
    assert_eq!(values[10]["isError"], json!(false));
    assert_eq!(values[10]["finishedAt"], json!(20.0));
}

#[test]
fn agent_tool_result_omits_undefined_fields() {
    let result = AgentToolResult::text("hello");
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({"content": [{"type": "text", "text": "hello"}], "details": {}})
    );

    let result = AgentToolResult {
        content: Vec::new(),
        details: Some(JsValue::from(json!({"a": 1}))),
        is_error: Some(true),
        terminate: Some(false),
    };
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({
            "content": [],
            "details": {"a": 1},
            "isError": true,
            "terminate": false
        })
    );
}

#[test]
fn frozen_tool_result_does_not_expose_old_metadata_fields() {
    let result: AgentToolResult = serde_json::from_value(json!({
        "content": [], "details": {}, "addedToolNames": ["old-only"]
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({"content": [], "details": {}})
    );
}

#[test]
fn agent_error_messages_match_typescript() {
    assert_eq!(
        AgentError::already_processing_prompt().to_string(),
        "Agent is already processing a prompt. Use steer() or followUp() to queue messages, or wait for completion."
    );
    assert_eq!(
        AgentError::already_processing_continue().to_string(),
        "Agent is already processing. Wait for completion before continuing."
    );
    assert_eq!(
        AgentError::no_messages_to_continue().to_string(),
        "No messages to continue from"
    );
    assert_eq!(
        AgentError::cannot_continue_empty_context().to_string(),
        "Cannot continue: no messages in context"
    );
    assert_eq!(AgentError::run_aborted().to_string(), "Agent run aborted");
}

#[test]
fn agent_context_and_hook_contexts_are_constructible() {
    let context = AgentContext {
        system_prompt: JsString::from("sys"),
        messages: vec![user("hi")],
        tools: None,
    };
    let call = ToolCall::new(
        "call-1",
        "echo",
        Map::new()
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    );
    let before = BeforeToolCallContext {
        assistant_message: AssistantMessage::empty("api", "provider", "model", 1.0),
        tool_call: call.clone(),
        args: std::sync::Arc::new(std::sync::Mutex::new(JsValue::from(json!({})))),
        context: context.clone(),
    };
    assert_eq!(before.args_snapshot(), pi_js_compat::js_json!({}));
    assert_eq!(before.context.system_prompt, "sys");

    let stop = ShouldStopAfterTurnContext {
        message: before.assistant_message.clone(),
        tool_results: Vec::new(),
        context,
        new_messages: Vec::new(),
    };
    assert!(stop.tool_results.is_empty());
}
