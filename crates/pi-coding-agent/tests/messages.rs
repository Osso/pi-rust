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
        timestamp: 1.0,
    });
    let assistant = AgentMessage::Assistant(Box::new(pi_ai::types::AssistantMessage::empty(
        "anthropic-messages",
        "anthropic",
        "test-model",
        1.0,
    )));
    let tool_result = AgentMessage::ToolResult(pi_ai::types::ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: "tool-1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![pi_ai::types::TextOrImageContent::Text(TextContent::new("result"))],
        details: None,
        is_error: false,
        timestamp: 1.0,
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

// Frozen coding-agent core/messages.ts:214-238: images before the last successful
// assistant response become a text marker; error/aborted responses do not count.
#[test]
fn omits_images_processed_before_the_last_successful_response() {
    let image = r#"{"type":"image","data":"aGk=","mimeType":"image/png"}"#;
    let assistant = |stop: &str| {
        format!(
            r#"{{"role":"assistant","content":[],"api":"anthropic-messages","provider":"anthropic","model":"m","usage":{{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}}},"stopReason":"{stop}","timestamp":1}}"#
        )
    };
    let json = format!(
        r#"[{{"role":"user","content":[{{"type":"text","text":"look"}},{image}],"timestamp":1}},
            {{"role":"toolResult","toolCallId":"t","toolName":"read","content":[{image}],"isError":false,"timestamp":1}},
            {{"role":"user","content":"plain","timestamp":1}},
            {ok},
            {{"role":"user","content":[{image}],"timestamp":1}},
            {error}]"#,
        ok = assistant("stop"),
        error = assistant("error"),
    );
    let messages: Vec<AgentMessage> = serde_json::from_str(&json).expect("fixture messages");
    let converted = convert_to_llm(&messages).expect("convert messages");
    let marker = || pi_ai::types::TextOrImageContent::Text(TextContent::new("[Previously processed image omitted]"));
    let image_block = || pi_ai::types::TextOrImageContent::Image(pi_ai::types::ImageContent::new("aGk=", "image/png"));
    let pi_ai::types::Message::User(first) = &converted[0] else {
        panic!("user")
    };
    assert_eq!(
        first.content,
        UserContent::Blocks(vec![
            pi_ai::types::TextOrImageContent::Text(TextContent::new("look")),
            marker()
        ])
    );
    let pi_ai::types::Message::ToolResult(result) = &converted[1] else {
        panic!("tool result")
    };
    assert_eq!(result.content, vec![marker()]);
    let pi_ai::types::Message::User(plain) = &converted[2] else {
        panic!("user")
    };
    assert_eq!(plain.content, UserContent::Text(JsString::from("plain")));
    let pi_ai::types::Message::User(after) = &converted[4] else {
        panic!("user")
    };
    assert_eq!(after.content, UserContent::Blocks(vec![image_block()]));
}

#[test]
fn keeps_images_without_a_successful_response() {
    let json = r#"[{"role":"user","content":[{"type":"image","data":"aGk=","mimeType":"image/png"}],"timestamp":1}]"#;
    let messages: Vec<AgentMessage> = serde_json::from_str(json).expect("fixture messages");
    let converted = convert_to_llm(&messages).expect("convert messages");
    let pi_ai::types::Message::User(user) = &converted[0] else {
        panic!("user")
    };
    assert_eq!(
        user.content,
        UserContent::Blocks(vec![pi_ai::types::TextOrImageContent::Image(
            pi_ai::types::ImageContent::new("aGk=", "image/png")
        )])
    );
}
