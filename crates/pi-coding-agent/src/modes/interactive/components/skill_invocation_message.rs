//! Ported from `coding-agent/src/modes/interactive/components/skill-invocation-message.ts`.

use pi_js_compat::js_json::{JsString, encode_node_utf8};

use std::rc::Rc;
use std::sync::Arc;

use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::text::StyleFn;
use pi_tui::tui::component;
use pi_tui::{Component, Markdown, MarkdownOptions, MarkdownTheme, Text};

use crate::ParsedSkillBlock;
use crate::modes::interactive::components::keybinding_hints::key_text;

use crate::modes::interactive::theme::{Theme, ThemeBg, ThemeColor};

/// Renders a skill invocation block, collapsed or expanded.
pub struct SkillInvocationMessageComponent {
    expanded: bool,
    skill_block: ParsedSkillBlock,
    markdown_theme: Rc<dyn MarkdownTheme>,
    theme: Arc<Theme>,
    container: BoxComponent,
}

impl SkillInvocationMessageComponent {
    pub fn new(skill_block: ParsedSkillBlock, markdown_theme: Rc<dyn MarkdownTheme>, theme: Arc<Theme>) -> Self {
        let container = BoxComponent::new(
            1,
            1,
            Some({
                let theme = theme.clone();
                Rc::new(move |text: &str| theme.bg(ThemeBg::CustomMessageBg, text))
            }),
        );
        let mut component = Self {
            expanded: false,
            skill_block,
            markdown_theme,
            theme,
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
        if self.expanded {
            let label = self
                .theme
                .fg(ThemeColor::CustomMessageLabel, "\u{1b}[1m[skill]\u{1b}[22m");
            self.container.add_child(component(Text::new(label, 0, 0)));
            let header = JsString::from("**") + &self.skill_block.name + "**\n\n";
            let color: StyleFn = {
                let theme = self.theme.clone();
                Rc::new(move |text: &str| theme.fg(ThemeColor::CustomMessageText, text))
            };
            let markdown = Markdown::with_shared_theme(
                encode_node_utf8(&(header + &self.skill_block.content)),
                0,
                0,
                self.markdown_theme.clone(),
                Some(pi_tui::DefaultTextStyle {
                    color: Some(Box::new(move |text: &str| color(text))),
                    ..Default::default()
                }),
                MarkdownOptions::default(),
            );
            self.container.add_child(component(markdown));
        } else {
            let line = self
                .theme
                .fg(ThemeColor::CustomMessageLabel, "\u{1b}[1m[skill]\u{1b}[22m ")
                + &self
                    .theme
                    .fg(ThemeColor::CustomMessageText, &encode_node_utf8(&self.skill_block.name))
                + &self.theme.fg(
                    ThemeColor::Dim,
                    &format!(" ({} to expand)", key_text("app.tools.expand")),
                );
            self.container.add_child(component(Text::new(line, 0, 0)));
        }
    }
}

impl Component for SkillInvocationMessageComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.container.render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.update_display();
    }
}
