use pi_ai::types::{AssistantContent, TextContent, TextOrImageContent, ToolCall};
use pi_ai::utils::text::{ContentText, content_text};

fn assistant_blocks() -> Vec<AssistantContent> {
    vec![
        AssistantContent::Thinking(pi_ai::types::ThinkingContent::new("reasoning")),
        AssistantContent::Text(TextContent::new("first")),
        AssistantContent::ToolCall(ToolCall::new("1", "read", Default::default())),
        AssistantContent::Text(TextContent::new("second")),
    ]
}

#[test]
fn extracts_assistant_text_blocks_with_default_separator() {
    assert_eq!(
        content_text(ContentText::Assistant(&assistant_blocks()), "\n"),
        "first\nsecond"
    );
}

#[test]
fn supports_custom_separators() {
    assert_eq!(
        content_text(ContentText::Assistant(&assistant_blocks()), ""),
        "firstsecond"
    );
}

#[test]
fn passes_string_content_through() {
    assert_eq!(content_text(ContentText::String("hello"), "\n"), "hello");
}

#[test]
fn extracts_text_from_tool_result_content() {
    let blocks = vec![
        TextOrImageContent::Text(TextContent::new("first")),
        TextOrImageContent::Image(pi_ai::types::ImageContent::new("...", "image/png")),
        TextOrImageContent::Text(TextContent::new("second")),
    ];
    assert_eq!(content_text(ContentText::TextOrImage(&blocks), ""), "firstsecond");
}
