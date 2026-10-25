//! Ported from `coding-agent/src/modes/interactive/components/branch-summary-message.ts`.

use pi_js_compat::js_json::JsString;
use std::rc::Rc;

use pi_agent_core::harness::messages::BranchSummaryMessage;
use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::text::StyleFn;
use pi_tui::tui::component;
use pi_tui::{Component, Markdown, MarkdownOptions, MarkdownTheme, Spacer, Text};

use crate::modes::interactive::components::keybinding_hints::key_text;
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, get_markdown_theme, with_theme};

/// Renders a branch summary message, collapsed or expanded.
pub struct BranchSummaryMessageComponent {
    expanded: bool,
    message: BranchSummaryMessage,
    markdown_theme: Rc<dyn MarkdownTheme>,
    container: BoxComponent,
}

impl BranchSummaryMessageComponent {
    pub fn new(message: BranchSummaryMessage, markdown_theme: Option<Rc<dyn MarkdownTheme>>) -> Self {
        let container = BoxComponent::new(
            1,
            1,
            Some(Rc::new(|text: &JsString| {
                with_theme(|theme| theme.bg(ThemeBg::CustomMessageBg, text))
            })),
        );
        let mut component = Self {
            expanded: false,
            message,
            markdown_theme: markdown_theme.unwrap_or_else(|| Rc::from(get_markdown_theme())),
            container,
        };
        component.update_display();
        component
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
        self.update_display();
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    fn update_display(&mut self) {
        self.container.clear();
        let label = with_theme(|theme| theme.fg(ThemeColor::CustomMessageLabel, "\u{1b}[1m[branch]\u{1b}[22m"));
        self.container.add_child(component(Text::new(label, 0, 0)));
        self.container.add_child(component(Spacer::new(1)));

        if self.expanded {
            let header = "**Branch Summary**\n\n";
            let color: StyleFn =
                Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::CustomMessageText, text)));
            self.container.add_child(component(Markdown::with_shared_theme(
                JsString::from(header) + &self.message.summary,
                0,
                0,
                self.markdown_theme.clone(),
                Some(pi_tui::DefaultTextStyle {
                    color: Some(Box::new(move |text: &JsString| color(text))),
                    ..Default::default()
                }),
                MarkdownOptions::default(),
            )));
        } else {
            let text = with_theme(|theme| {
                theme.fg(ThemeColor::CustomMessageText, "Branch summary (")
                    + &theme.fg(ThemeColor::Dim, key_text("app.tools.expand"))
                    + &theme.fg(ThemeColor::CustomMessageText, " to expand)")
            });
            self.container.add_child(component(Text::new(text, 0, 0)));
        }
    }
}

impl Component for BranchSummaryMessageComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
        self.update_display();
    }
}
