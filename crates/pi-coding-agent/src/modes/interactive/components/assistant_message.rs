//! Ported from `coding-agent/src/modes/interactive/components/assistant-message.ts`.

use std::rc::Rc;

use pi_ai::types::{AssistantContent, AssistantMessage, StopReason};
use pi_js_compat::string::js_trim;
use pi_tui::components::text::StyleFn;
use pi_tui::tui::component;
use pi_tui::{Component, Container, Markdown, MarkdownOptions, MarkdownTheme, Spacer, Text};

use crate::core::extensions::types::MarkdownTransformer;
use crate::modes::interactive::components::markdown_transform::create_markdown_transform;
use crate::modes::interactive::theme::{ThemeColor, get_markdown_theme, with_theme};

const OSC133_ZONE_START: &str = "\u{1b}]133;A\u{7}";
const OSC133_ZONE_END: &str = "\u{1b}]133;B\u{7}";
const OSC133_ZONE_FINAL: &str = "\u{1b}]133;C\u{7}";

/// Renders a complete assistant message.
pub struct AssistantMessageComponent {
    content_container: Container,
    hide_thinking_block: bool,
    markdown_theme: Rc<dyn MarkdownTheme>,
    hidden_thinking_label: String,
    output_pad: usize,
    markdown_transformers: Vec<MarkdownTransformer>,
    last_message: Option<AssistantMessage>,
    has_tool_calls: bool,
    is_streaming: bool,
}

impl AssistantMessageComponent {
    pub fn new(
        message: Option<AssistantMessage>,
        hide_thinking_block: bool,
        markdown_theme: Option<Rc<dyn MarkdownTheme>>,
        hidden_thinking_label: Option<String>,
        output_pad: usize,
        markdown_transformers: Vec<MarkdownTransformer>,
    ) -> Self {
        let mut component = Self {
            content_container: Container::new(),
            hide_thinking_block,
            markdown_theme: markdown_theme.unwrap_or_else(|| Rc::from(get_markdown_theme())),
            hidden_thinking_label: hidden_thinking_label.unwrap_or_else(|| "Thinking...".to_owned()),
            output_pad,
            markdown_transformers,
            last_message: None,
            has_tool_calls: false,
            is_streaming: false,
        };
        if let Some(message) = message {
            component.update_content(message, component.is_streaming);
        }
        component
    }

    pub fn set_hide_thinking_block(&mut self, hide: bool) {
        self.hide_thinking_block = hide;
        if let Some(message) = self.last_message.clone() {
            self.update_content(message, self.is_streaming);
        }
    }

    pub fn set_hidden_thinking_label(&mut self, label: impl Into<String>) {
        self.hidden_thinking_label = label.into();
        if let Some(message) = self.last_message.clone() {
            self.update_content(message, self.is_streaming);
        }
    }

    pub fn set_output_pad(&mut self, padding: usize) {
        self.output_pad = padding;
        if let Some(message) = self.last_message.clone() {
            self.update_content(message, self.is_streaming);
        }
    }

    pub fn update_content(&mut self, message: AssistantMessage, is_streaming: bool) {
        self.last_message = Some(message.clone());
        self.is_streaming = is_streaming;
        self.content_container.clear();

        let has_visible_content = message.content.iter().any(is_visible_content);
        if has_visible_content {
            self.content_container.add_child(component(Spacer::new(1)));
        }

        let mut index = 0;
        while index < message.content.len() {
            match &message.content[index] {
                AssistantContent::Text(text) if !js_trim(&text.text).is_empty() => {
                    self.content_container.add_child(component(Markdown::with_shared_theme(
                        js_trim(&text.text),
                        self.output_pad,
                        0,
                        self.markdown_theme.clone(),
                        None,
                        MarkdownOptions {
                            transform: Some({
                                let transform = create_markdown_transform(
                                    crate::core::extensions::types::MarkdownMessageType::Assistant,
                                    self.is_streaming,
                                    self.markdown_transformers.clone(),
                                );
                                Box::new(move |markdown: &str, width: usize| transform(markdown, width))
                            }),
                            ..Default::default()
                        },
                    )));
                    index += 1;
                }
                AssistantContent::Thinking(_) => {
                    let mut thinking_blocks = Vec::new();
                    while index < message.content.len() {
                        let AssistantContent::Thinking(thinking) = &message.content[index] else {
                            break;
                        };
                        let thinking = js_trim(&thinking.thinking);
                        if !thinking.is_empty() {
                            thinking_blocks.push(thinking.to_owned());
                        }
                        index += 1;
                    }

                    if thinking_blocks.is_empty() {
                        continue;
                    }

                    let has_visible_content_after = message.content[index..].iter().any(is_visible_content);

                    if self.hide_thinking_block {
                        let text = with_theme(|theme| {
                            theme.italic(&theme.fg(ThemeColor::ThinkingText, &self.hidden_thinking_label))
                        });
                        self.content_container
                            .add_child(component(Text::new(text, self.output_pad, 0)));
                    } else {
                        let color: StyleFn =
                            Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::ThinkingText, text)));
                        self.content_container.add_child(component(Markdown::with_shared_theme(
                            thinking_blocks.join("\n\n"),
                            self.output_pad,
                            0,
                            self.markdown_theme.clone(),
                            Some(pi_tui::DefaultTextStyle {
                                color: Some(Box::new(move |text: &str| color(text))),
                                italic: true,
                                ..Default::default()
                            }),
                            MarkdownOptions {
                                transform: Some({
                                    let transform = create_markdown_transform(
                                        crate::core::extensions::types::MarkdownMessageType::AssistantThinking,
                                        self.is_streaming,
                                        self.markdown_transformers.clone(),
                                    );
                                    Box::new(move |markdown: &str, width: usize| transform(markdown, width))
                                }),
                                ..Default::default()
                            },
                        )));
                    }
                    if has_visible_content_after {
                        self.content_container.add_child(component(Spacer::new(1)));
                    }
                }
                _ => {
                    index += 1;
                }
            }
        }

        let has_tool_calls = message
            .content
            .iter()
            .any(|content| matches!(content, AssistantContent::ToolCall(_)));
        self.has_tool_calls = has_tool_calls;
        if message.stop_reason == StopReason::Length {
            self.content_container.add_child(component(Spacer::new(1)));
            self.content_container.add_child(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Error, "Response was truncated before completion.")),
                self.output_pad,
                0,
            )));
        } else if !has_tool_calls {
            match message.stop_reason {
                StopReason::Aborted => {
                    let abort_message = match message.error_message.as_deref() {
                        Some(error) if error != "Request was aborted" => error,
                        _ => "Operation aborted",
                    };
                    self.content_container.add_child(component(Spacer::new(1)));
                    self.content_container.add_child(component(Text::new(
                        with_theme(|theme| theme.fg(ThemeColor::Error, abort_message)),
                        self.output_pad,
                        0,
                    )));
                }
                StopReason::Error => {
                    let error = message.error_message.as_deref().unwrap_or("Unknown error");
                    self.content_container.add_child(component(Spacer::new(1)));
                    self.content_container.add_child(component(Text::new(
                        with_theme(|theme| theme.fg(ThemeColor::Error, &format!("Error: {error}"))),
                        self.output_pad,
                        0,
                    )));
                }
                _ => {}
            }
        }
    }
}

fn is_visible_content(content: &AssistantContent) -> bool {
    match content {
        AssistantContent::Text(text) => !js_trim(&text.text).is_empty(),
        AssistantContent::Thinking(thinking) => !js_trim(&thinking.thinking).is_empty(),
        AssistantContent::ToolCall(_) => false,
    }
}

impl Component for AssistantMessageComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.content_container.render(width);
        if self.has_tool_calls || lines.is_empty() {
            return lines;
        }

        lines[0] = format!("{OSC133_ZONE_START}{}", lines[0]);
        let last = lines.len() - 1;
        lines[last] = format!("{OSC133_ZONE_END}{OSC133_ZONE_FINAL}{}", lines[last]);
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.content_container.invalidate();
        if let Some(message) = self.last_message.clone() {
            self.update_content(message, self.is_streaming);
        }
    }
}
