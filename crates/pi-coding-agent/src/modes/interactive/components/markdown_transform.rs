//! Ported from `coding-agent/src/modes/interactive/components/markdown-transform.ts`.

use std::rc::Rc;

use crate::core::extensions::types::{MarkdownMessageType, MarkdownTransformContext, MarkdownTransformer};

/// `(markdown, availableWidth) => string`.
pub type MarkdownTransformFn = Rc<dyn Fn(&str, usize) -> String>;

/// `createMarkdownTransform`: bind a message type and streaming flag to the
/// transformer list.
pub fn create_markdown_transform(
    message_type: MarkdownMessageType,
    is_streaming: bool,
    transformers: Vec<MarkdownTransformer>,
) -> MarkdownTransformFn {
    Rc::new(move |markdown: &str, available_width: usize| {
        apply_markdown_transformers(
            markdown,
            &MarkdownTransformContext {
                message_type,
                is_streaming,
                available_width: available_width as f64,
            },
            &transformers,
        )
    })
}

fn apply_markdown_transformers(
    markdown: &str,
    context: &MarkdownTransformContext,
    transformers: &[MarkdownTransformer],
) -> String {
    let mut transformed_markdown = markdown.to_string();
    for transformer in transformers {
        transformed_markdown = transformer(&transformed_markdown, context);
    }
    transformed_markdown
}
