use crate::types::{AssistantContent, TextOrImageContent};
use pi_js_compat::js_json::JsString;

/// `string | (TextContent | ImageContent)[] | AssistantContent[]` input of `contentText`.
pub enum ContentText<'a> {
    String(&'a str),
    Utf16(&'a JsString),
    Assistant(&'a [AssistantContent]),
    TextOrImage(&'a [TextOrImageContent]),
}

/// Extract and join text without normalizing UTF-16 code units.
pub fn content_text(content: ContentText<'_>, separator: &str) -> JsString {
    match content {
        ContentText::String(text) => text.into(),
        ContentText::Utf16(text) => text.clone(),
        ContentText::Assistant(blocks) => JsString::join(blocks.iter().filter_map(assistant_block_text), separator),
        ContentText::TextOrImage(blocks) => {
            JsString::join(blocks.iter().filter_map(text_or_image_block_text), separator)
        }
    }
}

fn assistant_block_text(block: &AssistantContent) -> Option<&JsString> {
    match block {
        AssistantContent::Text(text) => Some(&text.text),
        AssistantContent::Thinking(_) | AssistantContent::ToolCall(_) => None,
    }
}

fn text_or_image_block_text(block: &TextOrImageContent) -> Option<&JsString> {
    match block {
        TextOrImageContent::Text(text) => Some(&text.text),
        TextOrImageContent::Image(_) => None,
    }
}
