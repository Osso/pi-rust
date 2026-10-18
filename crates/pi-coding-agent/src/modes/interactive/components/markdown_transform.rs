//! Ported from `coding-agent/src/modes/interactive/components/markdown-transform.ts`.

use std::rc::Rc;

/// `MarkdownTransformContext.messageType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownMessageType {
    User,
    Assistant,
    AssistantThinking,
}

/// Context passed to every registered markdown transformer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownTransformContext {
    pub message_type: MarkdownMessageType,
    pub is_streaming: bool,
    pub available_width: usize,
}

/// A registered transformer. `None` covers both `void` and a thrown error: the
/// source keeps the current markdown in either case.
pub type MarkdownTransformer = Rc<dyn Fn(&str, &MarkdownTransformContext) -> Option<String>>;

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
                available_width,
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
        if let Some(transformed) = transformer(&transformed_markdown, context) {
            transformed_markdown = transformed;
        }
    }
    transformed_markdown
}
