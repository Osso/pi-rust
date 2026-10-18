//! Ported from `coding-agent/src/modes/interactive/components/approval-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;

use pi_tui::tui::component;
use pi_tui::{Component, Container, Spacer, Text};

use crate::core::permissions::presets::{APPROVAL_PRESETS, ApprovalPresetName};
use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::{key_hint, raw_key_hint};

use crate::modes::interactive::theme::{Theme, ThemeColor};

/// Save scope for an approval preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalSelectorScope {
    Global,
    Project,
}

impl ApprovalSelectorScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }
}

/// Selection emitted when the user confirms a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalSelectorSelection {
    pub preset: ApprovalPresetName,
    pub scope: ApprovalSelectorScope,
}

/// Constructor options.
pub struct ApprovalSelectorOptions {
    pub current_preset: ApprovalPresetName,
    pub on_select: Rc<dyn Fn(ApprovalSelectorSelection)>,
    pub on_cancel: Rc<dyn Fn()>,
}

/// Approval preset selector with global/project save scope.
pub struct ApprovalSelectorComponent {
    selected_index: usize,
    scope: ApprovalSelectorScope,
    list_container: Container,
    on_select: Rc<dyn Fn(ApprovalSelectorSelection)>,
    on_cancel: Rc<dyn Fn()>,
    current_preset: ApprovalPresetName,
    top: Container,
    bottom: Container,
    theme: Arc<Theme>,
}

impl ApprovalSelectorComponent {
    pub fn new(options: ApprovalSelectorOptions, theme: Arc<Theme>) -> Self {
        let selected_index = APPROVAL_PRESETS
            .iter()
            .position(|preset| preset.name == options.current_preset)
            .unwrap_or(0);

        let mut top = Container::new();
        top.add_child(component(dynamic_border(&theme)));
        top.add_child(component(Spacer::new(1)));
        top.add_child(component(Text::new(
            theme.fg(ThemeColor::Accent, &theme.bold("Approval presets")),
            1,
            0,
        )));
        top.add_child(component(Text::new(
            theme.fg(ThemeColor::Muted, "Tab switches save scope: global/project"),
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
            selected_index,
            scope: ApprovalSelectorScope::Global,
            list_container: Container::new(),
            on_select: options.on_select,
            on_cancel: options.on_cancel,
            current_preset: options.current_preset,
            top,
            bottom,
            theme,
        };
        selector.update_list();
        selector
    }

    fn update_list(&mut self) {
        self.list_container.clear();
        self.list_container.add_child(component(Text::new(
            self.theme
                .fg(ThemeColor::Muted, &format!("Save scope: {}", self.scope.as_str())),
            1,
            0,
        )));
        self.list_container.add_child(component(Spacer::new(1)));

        for (index, preset) in APPROVAL_PRESETS.iter().enumerate() {
            let is_selected = index == self.selected_index;
            let is_current = preset.name == self.current_preset;
            let prefix = if is_selected {
                self.theme.fg(ThemeColor::Accent, "→ ")
            } else {
                "  ".to_string()
            };
            let label = if is_selected {
                self.theme.fg(ThemeColor::Accent, preset.label)
            } else {
                self.theme.fg(ThemeColor::Text, preset.label)
            };
            let checkmark = if is_current {
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

    pub fn scope(&self) -> ApprovalSelectorScope {
        self.scope
    }
}

impl Component for ApprovalSelectorComponent {
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
            self.selected_index = (self.selected_index + 1).min(APPROVAL_PRESETS.len() - 1);
            self.update_list();
        } else if data == "\t" {
            self.scope = match self.scope {
                ApprovalSelectorScope::Global => ApprovalSelectorScope::Project,
                ApprovalSelectorScope::Project => ApprovalSelectorScope::Global,
            };
            self.update_list();
        } else if keybindings.matches(data, "tui.select.confirm") || data == "\n" {
            if let Some(selected) = APPROVAL_PRESETS.get(self.selected_index) {
                (self.on_select)(ApprovalSelectorSelection {
                    preset: selected.name,
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
