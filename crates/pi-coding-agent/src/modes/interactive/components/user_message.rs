//! Ported from `coding-agent/src/modes/interactive/components/user-message.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::rc::Rc;

use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::text::StyleFn;
use pi_tui::tui::component;
use pi_tui::{Component, Container, Markdown, MarkdownOptions, MarkdownTheme};

use crate::core::extensions::types::MarkdownTransformer;
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, get_markdown_theme, with_theme};

const OSC133_ZONE_START: &str = "\u{1b}]133;A\u{7}";
const OSC133_ZONE_END: &str = "\u{1b}]133;B\u{7}";
const OSC133_ZONE_FINAL: &str = "\u{1b}]133;C\u{7}";

/// Renders a user message.
pub struct UserMessageComponent {
    text: JsString,
    markdown_theme: Rc<dyn MarkdownTheme>,
    output_pad: usize,
    container: Container,
}

impl UserMessageComponent {
    pub fn new(
        text: impl Into<JsString>,
        markdown_theme: Option<Rc<dyn MarkdownTheme>>,
        output_pad: usize,
        _markdown_transformers: Vec<MarkdownTransformer>,
    ) -> Self {
        let mut component = Self {
            text: text.into(),
            markdown_theme: markdown_theme.unwrap_or_else(|| Rc::from(get_markdown_theme())),
            output_pad,
            container: Container::new(),
        };
        component.rebuild();
        component
    }

    pub fn set_output_pad(&mut self, padding: usize) {
        self.output_pad = padding;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        self.container.clear();
        let mut content_box = BoxComponent::new(
            self.output_pad,
            1,
            Some(Rc::new(|content: &JsString| {
                with_theme(|theme| theme.bg(ThemeBg::UserMessageBg, content))
            })),
        );
        let color: StyleFn =
            Rc::new(|content: &JsString| with_theme(|theme| theme.fg(ThemeColor::UserMessageText, content)));
        content_box.add_child(component(Markdown::with_shared_theme(
            self.text.clone(),
            0,
            0,
            self.markdown_theme.clone(),
            Some(pi_tui::DefaultTextStyle {
                color: Some(Box::new(move |content: &JsString| color(content))),
                ..Default::default()
            }),
            MarkdownOptions {
                preserve_ordered_list_markers: true,
                preserve_backslash_escapes: true,

                ..Default::default()
            },
        )));
        self.container.add_child(component(content_box));
    }
}

impl Component for UserMessageComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = self.container.render(width);
        if lines.is_empty() {
            return lines;
        }

        lines[0] = js_format!("{}{}", OSC133_ZONE_START, &lines[0]);
        let last = lines.len() - 1;
        lines[last] = js_format!("{}{}{}", OSC133_ZONE_END, OSC133_ZONE_FINAL, &lines[last]);
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
        self.rebuild();
    }
}
