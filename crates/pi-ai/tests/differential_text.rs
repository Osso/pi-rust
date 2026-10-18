#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::types::{AssistantContent, TextContent, TextOrImageContent};
use pi_ai::utils::text::{ContentText, content_text};
use serde_json::{Value, json};

fn requests() -> Vec<Value> {
    let assistant = vec![
        json!({"type": "thinking", "thinking": "reasoning"}),
        json!({"type": "text", "text": "first"}),
        json!({"type": "toolCall", "id": "1", "name": "read", "arguments": {}}),
        json!({"type": "text", "text": "second"}),
    ];
    let tool_result = vec![
        json!({"type": "text", "text": "first"}),
        json!({"type": "image", "data": "...", "mimeType": "image/png"}),
        json!({"type": "text", "text": "second"}),
    ];
    vec![
        json!({"id": 0, "kind": "assistant", "content": assistant}),
        json!({"id": 1, "kind": "assistant", "content": assistant, "separator": ""}),
        json!({"id": 2, "kind": "string", "content": "hello"}),
        json!({"id": 3, "kind": "textOrImage", "content": tool_result, "separator": ""}),
        json!({"id": 4, "kind": "textOrImage", "content": [], "separator": "|"}),
        json!({"id": 5, "kind": "assistant", "content": [{"type": "toolCall", "id": "1", "name": "read", "arguments": {}}]}),
    ]
}

fn observe(request: &Value) -> Value {
    let content = &request["content"];
    let separator = request["separator"].as_str().unwrap_or("\n");
    let text = if let Some(text) = content.as_str() {
        content_text(ContentText::String(text), separator)
    } else if request["kind"] == "textOrImage" {
        let blocks: Vec<TextOrImageContent> = serde_json::from_value(content.clone()).expect("tool result blocks");
        content_text(ContentText::TextOrImage(&blocks), separator)
    } else {
        let blocks: Vec<AssistantContent> = serde_json::from_value(content.clone()).expect("assistant blocks");
        content_text(ContentText::Assistant(&blocks), separator)
    };
    json!({"id": request["id"], "ok": true, "output": {"text": text}})
}

#[test]
fn content_text_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("text-oracle.ts", "packages/ai/src/utils/text.ts");
    for request in requests() {
        assert_eq!(observe(&request), oracle.request(&request), "request {request}");
    }
}

#[test]
fn assistant_text_blocks_match_typescript() {
    let blocks = vec![
        AssistantContent::Thinking(pi_ai::types::ThinkingContent::new("reasoning")),
        AssistantContent::Text(TextContent::new("first")),
        AssistantContent::Text(TextContent::new("second")),
    ];
    assert_eq!(content_text(ContentText::Assistant(&blocks), "\n"), "first\nsecond");
}
