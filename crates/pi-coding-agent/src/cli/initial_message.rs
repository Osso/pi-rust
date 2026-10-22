//! Initial message assembly for non-interactive mode (`cli/initial-message.ts`).

use pi_ai::types::ImageContent;
use pi_js_compat::js_json::JsString;

use super::args::Args;

/// Inputs consumed by [`build_initial_message`].
#[derive(Debug)]
pub struct InitialMessageInput<'a> {
    pub parsed: &'a mut Args,
    pub file_text: Option<&'a JsString>,
    pub file_images: Option<&'a [ImageContent]>,
    pub stdin_content: Option<&'a str>,
}

/// Combined initial prompt and images.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InitialMessageResult {
    pub initial_message: Option<JsString>,
    pub initial_images: Option<Vec<ImageContent>>,
}

/// Combine stdin content, `@file` text, and the first CLI message into a single
/// initial prompt. The first message is shifted off `parsed.messages`.
pub fn build_initial_message(input: InitialMessageInput<'_>) -> InitialMessageResult {
    let mut parts: Vec<JsString> = Vec::new();
    if let Some(stdin_content) = input.stdin_content {
        parts.push(stdin_content.into());
    }
    if let Some(file_text) = input.file_text.filter(|text| !text.is_empty()) {
        parts.push(file_text.clone());
    }
    if !input.parsed.messages.is_empty() {
        parts.push(input.parsed.messages.remove(0).into());
    }

    InitialMessageResult {
        initial_message: (!parts.is_empty()).then(|| JsString::join(parts, "")),
        initial_images: input
            .file_images
            .filter(|images| !images.is_empty())
            .map(<[ImageContent]>::to_vec),
    }
}
