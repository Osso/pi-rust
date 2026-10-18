use crate::types::{AssistantContent, TextOrImageContent};

/// `string | (TextContent | ImageContent)[] | AssistantContent[]` input of `contentText`.
pub enum ContentText<'a> {
    String(&'a str),
    Assistant(&'a [AssistantContent]),
    TextOrImage(&'a [TextOrImageContent]),
}

/// Extract and join text from message content.
pub fn content_text(content: ContentText<'_>, separator: &str) -> String {
    match content {
        ContentText::String(text) => text.to_owned(),
        ContentText::Assistant(blocks) => join_text_blocks(blocks.iter().filter_map(assistant_block_text), separator),
        ContentText::TextOrImage(blocks) => {
            join_text_blocks(blocks.iter().filter_map(text_or_image_block_text), separator)
        }
    }
}

fn join_text_blocks<'a>(blocks: impl Iterator<Item = &'a str>, separator: &str) -> String {
    blocks.collect::<Vec<_>>().join(separator)
}

fn assistant_block_text(block: &AssistantContent) -> Option<&str> {
    match block {
        AssistantContent::Text(text) => Some(&text.text),
        AssistantContent::Thinking(_) | AssistantContent::ToolCall(_) => None,
    }
}

fn text_or_image_block_text(block: &TextOrImageContent) -> Option<&str> {
    match block {
        TextOrImageContent::Text(text) => Some(&text.text),
        TextOrImageContent::Image(_) => None,
    }
}
