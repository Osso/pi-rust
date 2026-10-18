//! Normalization of image blocks returned by tool results.
//!
//! Tools that produce images themselves (extensions, MCP bridges, screenshot
//! tools) hand back arbitrary base64 payloads. Oversized images make providers
//! reject the whole conversation, so they are normalized as they enter history.
//! Unlike the `read` tool, a block that cannot be processed is kept as-is.

use std::borrow::Cow;

use pi_agent_core::harness::tools::image::decode_base64;
use pi_ai::types::{ImageContent, TextContent, TextOrImageContent};

use crate::utils::image_process::{ProcessImageOptions, ProcessImageResult, process_image};

pub type ToolResultContent = TextOrImageContent;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NormalizeToolResultImagesOptions {
    /// Whether oversized images are resized to inline provider limits. Default: true.
    pub auto_resize_images: Option<bool>,
}

/// Normalizes image blocks, returning the input slice unchanged when nothing
/// changed so callers can skip rewriting the result.
pub async fn normalize_tool_result_images<'a>(
    content: &'a [ToolResultContent],
    options: Option<NormalizeToolResultImagesOptions>,
) -> Cow<'a, [ToolResultContent]> {
    if !content.iter().any(is_image) {
        return Cow::Borrowed(content);
    }

    let auto_resize_images = options.unwrap_or_default().auto_resize_images;
    let mut normalized = Vec::with_capacity(content.len());
    let mut changed = false;

    for block in content {
        let TextOrImageContent::Image(image) = block else {
            normalized.push(block.clone());
            continue;
        };

        let processed = process_image(
            &decode_base64(&image.data),
            &image.mime_type,
            Some(ProcessImageOptions {
                auto_resize_images,
                resize_options: None,
            }),
        )
        .await;

        let ProcessImageResult::Processed { data, mime_type, hints } = processed else {
            normalized.push(block.clone());
            continue;
        };

        if data == image.data && mime_type == image.mime_type && hints.is_empty() {
            normalized.push(block.clone());
            continue;
        }

        normalized.push(TextOrImageContent::Image(ImageContent::new(data, mime_type)));
        if !hints.is_empty() {
            normalized.push(TextOrImageContent::Text(TextContent::new(hints.join("\n"))));
        }
        changed = true;
    }

    if changed {
        Cow::Owned(normalized)
    } else {
        Cow::Borrowed(content)
    }
}

fn is_image(block: &ToolResultContent) -> bool {
    matches!(block, TextOrImageContent::Image(_))
}
