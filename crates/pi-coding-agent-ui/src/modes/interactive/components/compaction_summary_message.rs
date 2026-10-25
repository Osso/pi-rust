//! Ported from `coding-agent/src/modes/interactive/components/compaction-summary-message.ts`.

use std::rc::Rc;

use pi_js_compat::{js_format, js_json::JsString, number::to_fixed};
use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::text::StyleFn;
use pi_tui::tui::component;
use pi_tui::{Component, Markdown, MarkdownOptions, MarkdownTheme, Spacer, Text};

use crate::core::messages::CompactionSummaryMessage;
use crate::modes::interactive::components::keybinding_hints::key_text;
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, get_markdown_theme, with_theme};

/// Renders a compaction message, collapsed or expanded.
pub struct CompactionSummaryMessageComponent {
    expanded: bool,
    message: CompactionSummaryMessage,
    markdown_theme: Rc<dyn MarkdownTheme>,
    container: BoxComponent,
}

impl CompactionSummaryMessageComponent {
    pub fn new(message: CompactionSummaryMessage, markdown_theme: Option<Rc<dyn MarkdownTheme>>) -> Self {
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

        let token_summary = format_compaction_token_summary(&self.message);
        let duration_text = self.message.duration_ms.and_then(format_compaction_duration);
        let duration_suffix = duration_text.map_or_else(JsString::default, |text| js_format!(" in {}", text));
        let remote_result_suffix = self
            .message
            .compacted_result_tokens
            .map_or_else(JsString::default, |tokens| {
                js_format!("; remote result {} tokens", format_token_count(tokens))
            });
        let label = with_theme(|theme| theme.fg(ThemeColor::CustomMessageLabel, "\u{1b}[1m[compaction]\u{1b}[22m"));
        self.container.add_child(component(Text::new(label, 0, 0)));
        self.container.add_child(component(Spacer::new(1)));

        if self.expanded {
            let header = js_format!("**{}{}{}**\n\n", token_summary, remote_result_suffix, duration_suffix);
            let color: StyleFn =
                Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::CustomMessageText, text)));
            self.container.add_child(component(Markdown::with_shared_theme(
                header + &self.message.summary,
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
                theme.fg(
                    ThemeColor::CustomMessageText,
                    &js_format!("{}{}{} (", token_summary, remote_result_suffix, duration_suffix),
                ) + &theme.fg(ThemeColor::Dim, key_text("app.tools.expand"))
                    + &theme.fg(ThemeColor::CustomMessageText, " to expand)")
            });
            self.container.add_child(component(Text::new(text, 0, 0)));
        }
    }
}

fn format_compaction_token_summary(message: &CompactionSummaryMessage) -> JsString {
    let tokens_before = format_token_count(message.tokens_before);
    let Some(tokens_after) = message.tokens_after else {
        return js_format!("Compacted from {} tokens", tokens_before);
    };
    let tokens_after = format_token_count(tokens_after);
    let kept_suffix = message
        .kept_from_previous_context_tokens
        .map_or_else(JsString::default, |tokens| {
            js_format!("; kept {} tokens", format_token_count(tokens))
        });
    js_format!(
        "Compacted from {} to {} tokens{}",
        tokens_before,
        tokens_after,
        kept_suffix
    )
}

fn format_token_count(tokens: f64) -> JsString {
    if tokens < 1000.0 {
        return pi_js_compat::number::js_number_to_string(tokens).into();
    }
    js_format!("{}k", to_fixed(tokens / 1000.0, 1))
}

fn format_compaction_duration(duration_ms: f64) -> Option<JsString> {
    if duration_ms < 1000.0 {
        return Some(js_format!(
            "{}ms",
            pi_js_compat::number::js_number_to_string(duration_ms)
        ));
    }
    Some(js_format!("{}s", to_fixed(duration_ms / 1000.0, 1)))
}

impl Component for CompactionSummaryMessageComponent {
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
