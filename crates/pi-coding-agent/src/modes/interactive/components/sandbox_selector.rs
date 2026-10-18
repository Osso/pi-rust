//! Ported from `coding-agent/src/modes/interactive/components/sandbox-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_tui::tui::component;
use pi_tui::{Component, Container, Spacer, Text};

use crate::core::permissions::presets::{SANDBOX_PROFILES, SandboxProfileName, SandboxProfileScope};
use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::{key_hint, raw_key_hint};

use crate::modes::interactive::theme::{Theme, ThemeColor};

/// Selection emitted when the user confirms a sandbox profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxSelectorSelection {
    Persistent {
        profile: SandboxProfileName,
        scope: SandboxProfileScope,
    },
    Session {
        profile: Option<SandboxProfileName>,
    },
}

/// Constructor options.
pub struct SandboxSelectorOptions {
    pub current_profile: SandboxProfileName,
    pub session_profile: Option<SandboxProfileName>,
    pub on_select: Rc<dyn Fn(SandboxSelectorSelection)>,
    pub on_cancel: Rc<dyn Fn()>,
}

struct SandboxSelectorOption {
    label: &'static str,
    profile: Option<SandboxProfileName>,
}

const SESSION_INHERIT_OPTION: SandboxSelectorOption = SandboxSelectorOption {
    label: "Inherit global/project",
    profile: None,
};

const SANDBOX_SCOPES: [SandboxProfileScope; 3] = [
    SandboxProfileScope::Global,
    SandboxProfileScope::Project,
    SandboxProfileScope::Session,
];

fn scope_name(scope: SandboxProfileScope) -> &'static str {
    match scope {
        SandboxProfileScope::Global => "global",
        SandboxProfileScope::Project => "project",
        SandboxProfileScope::Session => "session",
    }
}

/// Sandbox profile selector with global/project/session save scope.
pub struct SandboxSelectorComponent {
    selected_index: usize,
    scope: SandboxProfileScope,
    list_container: Container,
    on_select: Rc<dyn Fn(SandboxSelectorSelection)>,
    on_cancel: Rc<dyn Fn()>,
    current_profile: SandboxProfileName,
    session_profile: Option<SandboxProfileName>,
    top: Container,
    bottom: Container,
    theme: Arc<Theme>,
}

impl SandboxSelectorComponent {
    pub fn new(options: SandboxSelectorOptions, theme: Arc<Theme>) -> Self {
        let mut top = Container::new();
        top.add_child(component(dynamic_border(&theme)));
        top.add_child(component(Spacer::new(1)));
        top.add_child(component(Text::new(
            theme.fg(ThemeColor::Accent, &theme.bold("Sandbox profiles")),
            1,
            0,
        )));
        top.add_child(component(Text::new(
            theme.fg(ThemeColor::Muted, "Tab switches save scope: global/project/session"),
            1,
            0,
        )));
        top.add_child(component(Spacer::new(1)));

        let mut bottom = Container::new();
        bottom.add_child(component(Spacer::new(1)));
        let controls = [
            raw_key_hint(&theme, "↑↓", "navigate"),
            raw_key_hint(&theme, "Tab", "scope"),
            key_hint(&theme, "tui.select.confirm", "save"),
            key_hint(&theme, "tui.select.cancel", "cancel"),
        ]
        .join("  ");
        bottom.add_child(component(Text::new(controls, 1, 0)));
        bottom.add_child(component(Spacer::new(1)));
        bottom.add_child(component(dynamic_border(&theme)));

        let mut selector = Self {
            selected_index: 0,
            scope: SandboxProfileScope::Global,
            list_container: Container::new(),
            on_select: options.on_select,
            on_cancel: options.on_cancel,
            current_profile: options.current_profile,
            session_profile: options.session_profile,
            top,
            bottom,
            theme,
        };
        selector.selected_index = selector.selected_index_for_scope();
        selector.update_list();
        selector
    }

    fn options_for_scope(&self) -> Vec<SandboxSelectorOption> {
        let profiles = SANDBOX_PROFILES
            .iter()
            .map(|profile| SandboxSelectorOption {
                label: profile.label,
                profile: Some(profile.name),
            })
            .collect::<Vec<_>>();
        if self.scope == SandboxProfileScope::Session {
            let mut options = vec![SESSION_INHERIT_OPTION];
            options.extend(profiles);
            options
        } else {
            profiles
        }
    }

    fn active_profile(&self) -> Option<SandboxProfileName> {
        if self.scope == SandboxProfileScope::Session {
            self.session_profile
        } else {
            Some(self.current_profile)
        }
    }

    fn selected_index_for_scope(&self) -> usize {
        let active_profile = self.active_profile();
        self.options_for_scope()
            .iter()
            .position(|option| option.profile == active_profile)
            .unwrap_or(0)
    }

    fn cycle_scope(&mut self) {
        let current_index = SANDBOX_SCOPES
            .iter()
            .position(|scope| *scope == self.scope)
            .expect("current scope is one of SANDBOX_SCOPES");
        self.scope = SANDBOX_SCOPES[(current_index + 1) % SANDBOX_SCOPES.len()];
        self.selected_index = self.selected_index_for_scope();
        self.update_list();
    }

    fn update_list(&mut self) {
        self.list_container.clear();
        self.list_container.add_child(component(Text::new(
            self.theme
                .fg(ThemeColor::Muted, &format!("Save scope: {}", scope_name(self.scope))),
            1,
            0,
        )));
        self.list_container.add_child(component(Spacer::new(1)));
        let active_profile = self.active_profile();
        for (index, option) in self.options_for_scope().iter().enumerate() {
            let is_selected = index == self.selected_index;
            let prefix = if is_selected {
                self.theme.fg(ThemeColor::Accent, "→ ")
            } else {
                "  ".to_string()
            };
            let label = if is_selected {
                self.theme.fg(ThemeColor::Accent, option.label)
            } else {
                self.theme.fg(ThemeColor::Text, option.label)
            };
            let checkmark = if option.profile == active_profile {
                self.theme.fg(ThemeColor::Success, " ✓")
            } else {
                String::new()
            };
            self.list_container
                .add_child(component(Text::new(format!("{prefix}{label}{checkmark}"), 1, 0)));
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn scope(&self) -> SandboxProfileScope {
        self.scope
    }
}

impl Component for SandboxSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.list_container.render_children(width));
        lines.extend(self.bottom.render_children(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        let options = self.options_for_scope();
        if keybindings.matches(data, "tui.select.up") || data == "k" {
            self.selected_index = self.selected_index.saturating_sub(1);
            self.update_list();
        } else if keybindings.matches(data, "tui.select.down") || data == "j" {
            self.selected_index = (self.selected_index + 1).min(options.len() - 1);
            self.update_list();
        } else if data == "\t" {
            self.cycle_scope();
        } else if keybindings.matches(data, "tui.select.confirm") || data == "\n" {
            let Some(selected) = options.get(self.selected_index) else {
                return;
            };
            if self.scope == SandboxProfileScope::Session {
                (self.on_select)(SandboxSelectorSelection::Session {
                    profile: selected.profile,
                });
            } else if let Some(profile) = selected.profile {
                (self.on_select)(SandboxSelectorSelection::Persistent {
                    profile,
                    scope: self.scope,
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
