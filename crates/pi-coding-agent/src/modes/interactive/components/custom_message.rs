//! Ported from `coding-agent/src/modes/interactive/components/custom-message.ts`.

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::rc::Rc;

use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::StyleFn;
use pi_tui::components::text::Text;
use pi_tui::tui::{Component, ComponentRef, Container, component};
use pi_tui::{Markdown, MarkdownOptions, MarkdownTheme};

use crate::core::extensions::types::{CustomMessageContent, CustomMessageValue, MessageRenderOptions, MessageRenderer};
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, current_theme, get_markdown_theme};

/// Renders a custom message entry from an extension.
pub struct CustomMessageComponent {
    message: CustomMessageValue,
    custom_renderer: Option<MessageRenderer>,
    custom_component: Option<ComponentRef>,
    markdown_theme: Rc<dyn MarkdownTheme>,
    expanded: bool,
    output_pad: usize,
    container: Container,
}

impl CustomMessageComponent {
    pub fn new(
        message: CustomMessageValue,
        custom_renderer: Option<MessageRenderer>,
        markdown_theme: Option<Rc<dyn MarkdownTheme>>,
        output_pad: usize,
    ) -> Self {
        let mut message_component = Self {
            message,
            custom_renderer,
            custom_component: None,
            markdown_theme: markdown_theme.unwrap_or_else(|| Rc::from(get_markdown_theme())),
            expanded: false,
            output_pad,
            container: Container::new(),
        };
        message_component.container.add_child(component(Spacer::new(1)));
        message_component.rebuild();
        message_component
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        if self.expanded != expanded {
            self.expanded = expanded;
            self.rebuild();
        }
    }

    pub fn set_output_pad(&mut self, output_pad: usize) {
        if self.output_pad != output_pad {
            self.output_pad = output_pad;
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        self.container.clear();
        self.custom_component = None;
        self.container.add_child(component(Spacer::new(1)));

        if let Some(renderer) = &self.custom_renderer {
            let theme = current_theme();
            let rendered = renderer(
                &self.message,
                &MessageRenderOptions {
                    expanded: self.expanded,
                    output_pad: self.output_pad as f64,
                },
                theme.as_ref(),
            );
            if let Some(rendered) = rendered {
                self.custom_component = Some(Rc::clone(&rendered));
                self.container.add_child(rendered);
                return;
            }
        }

        let mut message_box = BoxComponent::new(
            1,
            1,
            Some(Rc::new(|text: &str| {
                crate::modes::interactive::theme::with_theme(|theme| theme.bg(ThemeBg::CustomMessageBg, text))
            })),
        );

        let label = crate::modes::interactive::theme::with_theme(|theme| {
            theme.fg(
                ThemeColor::CustomMessageLabel,
                &format!("\u{1b}[1m[{}]\u{1b}[22m", encode_node_utf8(&self.message.custom_type)),
            )
        });
        message_box.add_child(component(Text::new(label, 0, 0)));
        message_box.add_child(component(Spacer::new(1)));

        let text = match &self.message.content {
            CustomMessageContent::Text(text) => text.clone(),
            CustomMessageContent::Blocks(blocks) => JsString::join(
                blocks.iter().filter_map(|block| match block {
                    pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                    pi_ai::types::TextOrImageContent::Image(_) => None,
                }),
                "\n",
            ),
        };

        let color: StyleFn = Rc::new(|text: &str| {
            crate::modes::interactive::theme::with_theme(|theme| theme.fg(ThemeColor::CustomMessageText, text))
        });
        message_box.add_child(component(Markdown::with_shared_theme(
            encode_node_utf8(&text),
            0,
            0,
            self.markdown_theme.clone(),
            Some(pi_tui::DefaultTextStyle {
                color: Some(Box::new(move |text: &str| color(text))),
                ..Default::default()
            }),
            MarkdownOptions::default(),
        )));

        self.container.add_child(component(message_box));
    }
}

impl Component for CustomMessageComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.container.render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
        self.rebuild();
    }
}
