use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{AssistantContent, TextContent, UserContent, UserMessage, UserRole};
use pi_coding_agent::core::messages::{convert_to_llm, is_cut_point_message};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;

#[test]
fn accepts_conversational_boundaries_and_rejects_continuation_messages() {
    let user = AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(JsString::from("continue")),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    });
    let assistant = AgentMessage::Assistant(Box::new(pi_ai::types::AssistantMessage::empty(
        "anthropic-messages",
        "anthropic",
        "test-model",
        1,
    )));
    let tool_result = AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: "tool-1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![pi_ai::types::TextOrImageContent::Text(TextContent::new("result"))],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: false,
        timestamp: 1,
    });
    let custom = AgentMessage::Custom(
        (serde_json::Map::from_iter([
            ("role".to_owned(), serde_json::json!("custom")),
            ("customType".to_owned(), serde_json::json!("context")),
            ("content".to_owned(), serde_json::json!("extension context")),
            ("display".to_owned(), serde_json::json!(true)),
            ("timestamp".to_owned(), serde_json::json!(1)),
        ]))
        .into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>(),
    );

    assert!(is_cut_point_message(&user));
    assert!(is_cut_point_message(&assistant));
    assert!(!is_cut_point_message(&tool_result));
    assert!(!is_cut_point_message(&custom));
}

#[test]
fn converts_extension_custom_messages_to_user_text() {
    let custom = AgentMessage::Custom(
        (serde_json::Map::from_iter([
            ("role".to_owned(), serde_json::json!("custom")),
            ("customType".to_owned(), serde_json::json!("context")),
            ("content".to_owned(), serde_json::json!("extension context")),
            ("display".to_owned(), serde_json::json!(true)),
            ("timestamp".to_owned(), serde_json::json!(7)),
        ]))
        .into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>(),
    );
    let messages = convert_to_llm(&[custom]).expect("convert custom message");
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        pi_ai::types::Message::User(user) => {
            assert_eq!(
                user.content,
                UserContent::Blocks(vec![pi_ai::types::TextOrImageContent::Text(TextContent::new(
                    "extension context"
                ))])
            );
        }
        other => panic!("expected user message, got {other:?}"),
    }
    let _ = AssistantContent::Text(TextContent::new(""));
}
