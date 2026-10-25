//! Ported from `packages/coding-agent/src/modes/interactive/components/trust-selector.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::rc::Rc;

use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, component};

use super::super::theme::{ThemeColor, with_theme};
use super::dynamic_border::dynamic_border;
use super::keybinding_hints::{key_hint, raw_key_hint};
use crate::core::trust_manager::{
    ProjectTrustDecision, ProjectTrustOption, ProjectTrustStoreEntry, ProjectTrustUpdate, get_project_trust_options,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustSelection {
    pub trusted: bool,
    pub updates: Vec<ProjectTrustUpdate>,
}

pub struct TrustSelectorOptions {
    pub cwd: JsString,
    pub saved_decision: Option<ProjectTrustStoreEntry>,
    pub project_trusted: bool,
    pub on_select: Rc<dyn Fn(TrustSelection)>,
    pub on_cancel: Rc<dyn Fn()>,
}

fn format_decision(trust_path: Option<&JsString>, decision: Option<&ProjectTrustStoreEntry>) -> JsString {
    let Some(decision) = decision else {
        return "none".into();
    };
    let label = if decision.decision == ProjectTrustDecision::Trusted {
        "trusted"
    } else {
        "untrusted"
    };
    match trust_path {
        Some(trust_path) if &decision.path != trust_path => js_format!("{} (inherited from {})", label, &decision.path),
        _ => js_format!("{} ({})", label, &decision.path),
    }
}

/// Project trust selector with a saved-decision summary.
pub struct TrustSelectorComponent {
    selected_index: usize,
    list_container: Container,
    trust_options: Vec<ProjectTrustOption>,
    saved_decision: Option<ProjectTrustStoreEntry>,
    on_select: Rc<dyn Fn(TrustSelection)>,
    on_cancel: Rc<dyn Fn()>,
    prefix_children: Vec<ComponentRef>,
    suffix_children: Vec<ComponentRef>,
}

impl TrustSelectorComponent {
    pub fn new(options: TrustSelectorOptions) -> Result<Self, crate::utils::paths::PathError> {
        let trust_options = get_project_trust_options(&options.cwd, false)?;
        let saved_decision = options.saved_decision.clone();
        let saved_text = js_format!(
            "Saved decision: {}",
            format_decision(
                trust_options.first().and_then(|option| option.saved_path.as_ref()),
                saved_decision.as_ref(),
            )
        );
        let session_text = js_format!(
            "Current session: {}",
            if options.project_trusted {
                "trusted"
            } else {
                "untrusted"
            }
        );

        let prefix_children = vec![
            component(dynamic_border()),
            component(Spacer::new(1)),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, theme.bold("Project trust"))),
                1,
                0,
            )),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Muted, &options.cwd)),
                1,
                0,
            )),
            component(Spacer::new(1)),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Muted, &saved_text)),
                1,
                0,
            )),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Muted, &session_text)),
                1,
                0,
            )),
            component(Spacer::new(1)),
        ];
        let suffix_children = vec![
            component(Spacer::new(1)),
            component(Text::new(
                raw_key_hint("↑↓", "navigate")
                    + "  "
                    + &key_hint("tui.select.confirm", "save")
                    + "  "
                    + &key_hint("tui.select.cancel", "cancel"),
                1,
                0,
            )),
            component(Spacer::new(1)),
            component(dynamic_border()),
        ];

        let mut selector = Self {
            selected_index: 0,
            list_container: Container::new(),
            trust_options,
            saved_decision,
            on_select: options.on_select,
            on_cancel: options.on_cancel,
            prefix_children,
            suffix_children,
        };
        selector.selected_index = selector
            .trust_options
            .iter()
            .position(|option| selector.is_saved_option(option))
            .unwrap_or(0);
        selector.update_list();
        Ok(selector)
    }

    fn is_saved_option(&self, option: &ProjectTrustOption) -> bool {
        let Some(saved_path) = option.saved_path.as_ref() else {
            return false;
        };
        let Some(saved_decision) = &self.saved_decision else {
            return false;
        };
        let decision = if option.trusted {
            ProjectTrustDecision::Trusted
        } else {
            ProjectTrustDecision::Untrusted
        };
        saved_decision.decision == decision && &saved_decision.path == saved_path
    }

    fn update_list(&mut self) {
        self.list_container.clear();
        for (index, option) in self.trust_options.iter().enumerate() {
            let is_selected = index == self.selected_index;
            let is_current = self.is_saved_option(option);
            let checkmark = if is_current {
                with_theme(|theme| theme.fg(ThemeColor::Success, " ✓"))
            } else {
                JsString::default()
            };
            let prefix = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "→ "))
            } else {
                JsString::from("  ")
            };
            let label = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, &option.label))
            } else {
                with_theme(|theme| theme.fg(ThemeColor::Text, &option.label))
            };
            self.list_container.add_child(component(Text::new(
                js_format!("{}{}{}", prefix, label, checkmark),
                1,
                0,
            )));
        }
    }
}

impl Component for TrustSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = Vec::new();
        for child in &self.prefix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines.extend(self.list_container.render(width));
        for child in &self.suffix_children {
            lines.extend(child.borrow_mut().render(width));
        }
        lines
    }

    fn handle_input(&mut self, key_data: &JsString) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "tui.select.up") || key_data == "k" {
            self.selected_index = self.selected_index.saturating_sub(1);
            self.update_list();
        } else if keybindings.matches(key_data, "tui.select.down") || key_data == "j" {
            self.selected_index = (self.selected_index + 1).min(self.trust_options.len().saturating_sub(1));
            self.update_list();
        } else if keybindings.matches(key_data, "tui.select.confirm") || key_data == "\n" {
            if let Some(selected) = self.trust_options.get(self.selected_index) {
                (self.on_select)(TrustSelection {
                    trusted: selected.trusted,
                    updates: selected.updates.clone(),
                });
            }
        } else if keybindings.matches(key_data, "tui.select.cancel") {
            (self.on_cancel)();
        }
    }

    fn invalidate(&mut self) {
        for child in &self.prefix_children {
            child.borrow_mut().invalidate();
        }
        self.list_container.invalidate();
        for child in &self.suffix_children {
            child.borrow_mut().invalidate();
        }
    }
}
