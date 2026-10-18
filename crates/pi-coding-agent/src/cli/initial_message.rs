//! Initial message assembly for non-interactive mode (`cli/initial-message.ts`).

use pi_ai::types::ImageContent;

use super::args::Args;

/// Inputs consumed by [`build_initial_message`].
#[derive(Debug)]
pub struct InitialMessageInput<'a> {
    pub parsed: &'a mut Args,
    pub file_text: Option<&'a str>,
    pub file_images: Option<&'a [ImageContent]>,
    pub stdin_content: Option<&'a str>,
}

/// Combined initial prompt and images.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InitialMessageResult {
    pub initial_message: Option<String>,
    pub initial_images: Option<Vec<ImageContent>>,
}

/// Combine stdin content, `@file` text, and the first CLI message into a single
/// initial prompt. The first message is shifted off `parsed.messages`.
pub fn build_initial_message(input: InitialMessageInput<'_>) -> InitialMessageResult {
    let mut parts: Vec<String> = Vec::new();
    if let Some(stdin_content) = input.stdin_content {
        parts.push(stdin_content.to_owned());
    }
    if let Some(file_text) = input.file_text.filter(|text| !text.is_empty()) {
        parts.push(file_text.to_owned());
    }
    if !input.parsed.messages.is_empty() {
        parts.push(input.parsed.messages.remove(0));
    }

    InitialMessageResult {
        initial_message: (!parts.is_empty()).then(|| parts.concat()),
        initial_images: input
            .file_images
            .filter(|images| !images.is_empty())
            .map(<[ImageContent]>::to_vec),
    }
}
