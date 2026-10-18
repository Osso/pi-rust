use indexmap::IndexMap;

use serde::{Deserialize, Serialize};

use crate::types::{Message, TextOrImageContent, UserContent};

/// `X-Initiator` value: whether the request is user- or agent-initiated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CopilotInitiator {
    User,
    Agent,
}

/// Copilot expects `X-Initiator` to indicate whether the request is user-initiated
/// or agent-initiated (e.g. follow-up after assistant/tool messages).
pub fn infer_copilot_initiator(messages: &[Message]) -> CopilotInitiator {
    match messages.last() {
        Some(Message::User(_)) | None => CopilotInitiator::User,
        Some(_) => CopilotInitiator::Agent,
    }
}

/// Copilot requires `Copilot-Vision-Request` when sending images.
pub fn has_copilot_vision_input(messages: &[Message]) -> bool {
    messages.iter().any(|message| match message {
        Message::User(user) => match &user.content {
            UserContent::Text(_) => false,
            UserContent::Blocks(blocks) => blocks_have_image(blocks),
        },
        Message::ToolResult(result) => blocks_have_image(&result.content),
        Message::Assistant(_) => false,
    })
}

fn blocks_have_image(blocks: &[TextOrImageContent]) -> bool {
    blocks.iter().any(|block| matches!(block, TextOrImageContent::Image(_)))
}

pub fn build_copilot_dynamic_headers(messages: &[Message], has_images: bool) -> IndexMap<String, String> {
    let mut headers = IndexMap::new();
    headers.insert(
        "X-Initiator".to_owned(),
        match infer_copilot_initiator(messages) {
            CopilotInitiator::User => "user",
            CopilotInitiator::Agent => "agent",
        }
        .to_owned(),
    );
    headers.insert("Openai-Intent".to_owned(), "conversation-edits".to_owned());
    if has_images {
        headers.insert("Copilot-Vision-Request".to_owned(), "true".to_owned());
    }
    headers
}
