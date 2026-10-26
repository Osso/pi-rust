//! Ported from `coding-agent/src/modes/interactive/components/user-message.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::settings_manager::MermaidRenderingMode;
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, get_markdown_theme, with_theme};
use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::text::StyleFn;
use pi_tui::tui::component;
use pi_tui::{Component, Container, Markdown, MarkdownOptions, MarkdownTheme, Text};

const OSC133_ZONE_START: &str = "\u{1b}]133;A\u{7}";
const OSC133_ZONE_END: &str = "\u{1b}]133;B\u{7}";
const OSC133_ZONE_FINAL: &str = "\u{1b}]133;C\u{7}";

/// Renders a user message.
pub struct UserMessageComponent {
    text: JsString,
    markdown_theme: Rc<dyn MarkdownTheme>,
    output_pad: usize,
    in_transit: bool,
    container: Container,
    markdown: Option<Rc<RefCell<Markdown>>>,
    mermaid_mode_source: Option<Rc<dyn Fn() -> MermaidRenderingMode>>,
}

impl UserMessageComponent {
    pub fn new(text: impl Into<JsString>, markdown_theme: Option<Rc<dyn MarkdownTheme>>, output_pad: usize) -> Self {
        let mut component = Self {
            text: text.into(),
            markdown_theme: markdown_theme.unwrap_or_else(|| Rc::from(get_markdown_theme())),
            output_pad,
            in_transit: false,
            container: Container::new(),
            markdown: None,
            mermaid_mode_source: None,
        };
        component.rebuild();
        component
    }

    pub fn set_mermaid_mode_source(&mut self, source: Rc<dyn Fn() -> MermaidRenderingMode>) {
        self.mermaid_mode_source = Some(source);
    }

    pub fn set_output_pad(&mut self, padding: usize) {
        self.output_pad = padding;
        self.rebuild();
    }

    pub fn set_in_transit(&mut self, in_transit: bool) {
        if self.in_transit == in_transit {
            return;
        }
        self.in_transit = in_transit;
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
        let color_key = if self.in_transit {
            ThemeColor::Dim
        } else {
            ThemeColor::UserMessageText
        };
        let color: StyleFn = Rc::new(move |content: &JsString| with_theme(|theme| theme.fg(color_key, content)));
        let markdown = Rc::new(RefCell::new(Markdown::with_shared_theme(
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
            },
        )));
        content_box.add_child(markdown.clone());
        self.markdown = Some(markdown);
        if self.in_transit {
            let label = with_theme(|theme| theme.fg(ThemeColor::Muted, "⧗ sending…"));
            content_box.add_child(component(Text::new(label, 0, 0)));
        }
        self.container.add_child(component(content_box));
    }
}

impl Component for UserMessageComponent {
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        if let (Some(source), Some(markdown)) = (&self.mermaid_mode_source, &self.markdown) {
            markdown
                .borrow_mut()
                .set_mermaid_rendering(source() != MermaidRenderingMode::Off, false);
        }
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
