use pi_ai::api::github_copilot_headers::{
    CopilotInitiator, build_copilot_dynamic_headers, has_copilot_vision_input, infer_copilot_initiator,
};
use pi_ai::types::{
    AssistantMessage, ImageContent, Message, TextContent, TextOrImageContent, ToolResultMessage, UserContent,
    UserMessage,
};
use serde_json::json;

fn user_text(text: &str) -> Message {
    Message::User(UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: UserContent::Text((text.to_owned()).into()),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })
}

fn user_blocks(blocks: Vec<TextOrImageContent>) -> Message {
    Message::User(UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: UserContent::Blocks(blocks),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })
}

fn assistant() -> Message {
    Message::Assistant(Box::new(AssistantMessage::empty(
        "openai-completions",
        "github-copilot",
        "gpt-5",
        1.0,
    )))
}

fn tool_result(blocks: Vec<TextOrImageContent>) -> Message {
    Message::ToolResult(ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: "call_1".to_owned(),
        tool_name: "read".to_owned(),
        content: blocks,
        details: None,
        is_error: false,
        timestamp: 1.0,
    })
}

#[test]
fn infers_the_initiator_from_the_last_message() {
    assert_eq!(infer_copilot_initiator(&[]), CopilotInitiator::User);
    assert_eq!(infer_copilot_initiator(&[user_text("hi")]), CopilotInitiator::User);
    assert_eq!(
        infer_copilot_initiator(&[user_text("hi"), assistant()]),
        CopilotInitiator::Agent
    );
    assert_eq!(
        infer_copilot_initiator(&[tool_result(vec![TextOrImageContent::Text(TextContent::new("ok"))])]),
        CopilotInitiator::Agent
    );
}

#[test]
fn detects_images_in_user_and_tool_result_content() {
    let image = TextOrImageContent::Image(ImageContent::new("AAAA", "image/png"));
    assert!(!has_copilot_vision_input(&[user_text("hi")]));
    assert!(has_copilot_vision_input(&[user_blocks(vec![image.clone()])]));
    assert!(has_copilot_vision_input(&[tool_result(vec![image])]));
    assert!(!has_copilot_vision_input(&[assistant()]));
}

#[test]
fn builds_dynamic_headers_with_optional_vision_flag() {
    let headers = build_copilot_dynamic_headers(&[user_text("hi")], false);
    assert_eq!(headers["X-Initiator"], "user");
    assert_eq!(headers["Openai-Intent"], "conversation-edits");
    assert!(!headers.contains_key("Copilot-Vision-Request"));

    let headers = build_copilot_dynamic_headers(&[assistant()], true);
    assert_eq!(headers["X-Initiator"], "agent");
    assert_eq!(headers["Copilot-Vision-Request"], "true");
    assert_eq!(
        serde_json::to_value(headers).expect("headers serialize"),
        json!({"X-Initiator": "agent", "Openai-Intent": "conversation-edits", "Copilot-Vision-Request": "true"})
    );
}
