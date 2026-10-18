//! Ported from `coding-agent/src/modes/interactive/components/trust-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_tui::tui::component;
use pi_tui::{Component, Container, Spacer, Text};

use crate::core::trust_manager::{
    ProjectTrustDecision, ProjectTrustOption, ProjectTrustStoreEntry, ProjectTrustUpdate, get_project_trust_options,
};
use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::{key_hint, raw_key_hint};

use crate::modes::interactive::theme::{Theme, ThemeColor};

/// Selection emitted when the user confirms a trust decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustSelection {
    pub trusted: bool,
    pub updates: Vec<ProjectTrustUpdate>,
}

/// Constructor options.
pub struct TrustSelectorOptions {
    pub cwd: String,
    pub saved_decision: Option<ProjectTrustStoreEntry>,
    pub project_trusted: bool,
    pub on_select: Rc<dyn Fn(TrustSelection)>,
    pub on_cancel: Rc<dyn Fn()>,
}

fn decision_label(decision: ProjectTrustDecision) -> &'static str {
    // `Unknown` is `null` in trust.json, which is falsy in the source's
    // `decision ? "trusted" : "untrusted"` expression.
    match decision {
        ProjectTrustDecision::Trusted => "trusted",
        ProjectTrustDecision::Untrusted | ProjectTrustDecision::Unknown => "untrusted",
    }
}

fn format_decision(trust_path: Option<&str>, decision: Option<&ProjectTrustStoreEntry>) -> String {
    let Some(decision) = decision else {
        return "none".to_string();
    };
    let label = decision_label(decision.decision);
    if let Some(trust_path) = trust_path
        && decision.path != trust_path
    {
        return format!("{label} (inherited from {})", decision.path);
    }
    format!("{label} ({})", decision.path)
}

/// Project trust selector.
pub struct TrustSelectorComponent {
    selected_index: usize,
    list_container: Container,
    trust_options: Vec<ProjectTrustOption>,
    saved_decision: Option<ProjectTrustStoreEntry>,
    on_select: Rc<dyn Fn(TrustSelection)>,
    on_cancel: Rc<dyn Fn()>,
    top: Container,
    bottom: Container,
    theme: Arc<Theme>,
}

impl TrustSelectorComponent {
    pub fn new(options: TrustSelectorOptions, theme: Arc<Theme>) -> Self {
        let trust_options = get_project_trust_options(&options.cwd, false);
        let saved_decision = options.saved_decision;
        let mut selector = Self {
            selected_index: 0,
            list_container: Container::new(),
            trust_options,
            saved_decision,
            on_select: options.on_select,
            on_cancel: options.on_cancel,
            top: Container::new(),
            bottom: Container::new(),
            theme,
        };
        selector.selected_index = selector
            .trust_options
            .iter()
            .position(|option| selector.is_saved_option(option))
            .unwrap_or(0);
        selector.build_layout(&options.cwd, options.project_trusted);
        selector.update_list();
        selector
    }

    fn build_layout(&mut self, cwd: &str, project_trusted: bool) {
        self.top.add_child(component(dynamic_border(&self.theme)));
        self.top.add_child(component(Spacer::new(1)));
        self.top.add_child(component(Text::new(
            self.theme.fg(ThemeColor::Accent, &self.theme.bold("Project trust")),
            1,
            0,
        )));
        self.top
            .add_child(component(Text::new(self.theme.fg(ThemeColor::Muted, cwd), 1, 0)));
        self.top.add_child(component(Spacer::new(1)));
        let saved_path = self
            .trust_options
            .first()
            .and_then(|option| option.saved_path.as_deref());
        self.top.add_child(component(Text::new(
            self.theme.fg(
                ThemeColor::Muted,
                &format!(
                    "Saved decision: {}",
                    format_decision(saved_path, self.saved_decision.as_ref())
                ),
            ),
            1,
            0,
        )));
        self.top.add_child(component(Text::new(
            self.theme.fg(
                ThemeColor::Muted,
                &format!(
                    "Current session: {}",
                    if project_trusted { "trusted" } else { "untrusted" }
                ),
            ),
            1,
            0,
        )));
        self.top.add_child(component(Spacer::new(1)));

        self.bottom.add_child(component(Spacer::new(1)));
        let controls = raw_key_hint(&self.theme, "↑↓", "navigate")
            + "  "
            + &key_hint(&self.theme, "tui.select.confirm", "save")
            + "  "
            + &key_hint(&self.theme, "tui.select.cancel", "cancel");
        self.bottom.add_child(component(Text::new(controls, 1, 0)));
        self.bottom.add_child(component(Spacer::new(1)));
        self.bottom.add_child(component(dynamic_border(&self.theme)));
    }

    fn is_saved_option(&self, option: &ProjectTrustOption) -> bool {
        let Some(saved_path) = option.saved_path.as_deref() else {
            return false;
        };
        let Some(saved_decision) = &self.saved_decision else {
            return false;
        };
        let expected = if option.trusted {
            ProjectTrustDecision::Trusted
        } else {
            ProjectTrustDecision::Untrusted
        };
        saved_decision.decision == expected && saved_decision.path == saved_path
    }

    fn update_list(&mut self) {
        self.list_container.clear();
        for (index, option) in self.trust_options.iter().enumerate() {
            let is_selected = index == self.selected_index;
            let is_current = self.is_saved_option(option);
            let checkmark = if is_current {
                self.theme.fg(ThemeColor::Success, " ✓")
            } else {
                String::new()
            };
            let prefix = if is_selected {
                self.theme.fg(ThemeColor::Accent, "→ ")
            } else {
                "  ".to_string()
            };
            let label = if is_selected {
                self.theme.fg(ThemeColor::Accent, &option.label)
            } else {
                self.theme.fg(ThemeColor::Text, &option.label)
            };
            self.list_container
                .add_child(component(Text::new(format!("{prefix}{label}{checkmark}"), 1, 0)));
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }
}

impl Component for TrustSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.list_container.render_children(width));
        lines.extend(self.bottom.render_children(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "tui.select.up") || data == "k" {
            self.selected_index = self.selected_index.saturating_sub(1);
            self.update_list();
        } else if keybindings.matches(data, "tui.select.down") || data == "j" {
            self.selected_index = (self.selected_index + 1).min(self.trust_options.len() - 1);
            self.update_list();
        } else if keybindings.matches(data, "tui.select.confirm") || data == "\n" {
            if let Some(selected) = self.trust_options.get(self.selected_index) {
                (self.on_select)(TrustSelection {
                    trusted: selected.trusted,
                    updates: selected.updates.clone(),
                });
            }
        } else if keybindings.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        }
    }

    fn invalidate(&mut self) {
        self.top.invalidate_children();
        self.list_container.invalidate_children();
        self.bottom.invalidate_children();
    }
}
