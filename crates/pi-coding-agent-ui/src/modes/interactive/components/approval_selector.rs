//! Ported from `packages/coding-agent/src/modes/interactive/components/approval-selector.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::rc::Rc;

use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, component};

use super::super::theme::{ThemeColor, with_theme};
use super::dynamic_border::dynamic_border;
use super::keybinding_hints::{key_hint, raw_key_hint};
use crate::core::permissions::presets::{APPROVAL_PRESETS, ApprovalPresetName};

/// Where the chosen approval preset is persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalScope {
    Global,
    Project,
}

impl ApprovalScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApprovalSelectorSelection {
    pub preset: ApprovalPresetName,
    pub scope: ApprovalScope,
}

pub struct ApprovalSelectorOptions {
    pub current_preset: ApprovalPresetName,
    pub on_select: Rc<dyn Fn(ApprovalSelectorSelection)>,
    pub on_cancel: Rc<dyn Fn()>,
}

/// Approval preset list with a global/project save scope.
pub struct ApprovalSelectorComponent {
    selected_index: usize,
    scope: ApprovalScope,
    list_container: Container,
    on_select: Rc<dyn Fn(ApprovalSelectorSelection)>,
    on_cancel: Rc<dyn Fn()>,
    current_preset: ApprovalPresetName,
    prefix_children: Vec<ComponentRef>,
    suffix_children: Vec<ComponentRef>,
}

impl ApprovalSelectorComponent {
    pub fn new(options: ApprovalSelectorOptions) -> Self {
        let selected_index = APPROVAL_PRESETS
            .iter()
            .position(|preset| preset.name == options.current_preset)
            .unwrap_or(0);

        let prefix_children = vec![
            component(dynamic_border()),
            component(Spacer::new(1)),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, theme.bold("Approval presets"))),
                1,
                0,
            )),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Muted, "Tab switches save scope: global/project")),
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
                    + &raw_key_hint("Tab", "scope")
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
            selected_index,
            scope: ApprovalScope::Global,
            list_container: Container::new(),
            on_select: options.on_select,
            on_cancel: options.on_cancel,
            current_preset: options.current_preset,
            prefix_children,
            suffix_children,
        };
        selector.update_list();
        selector
    }

    fn update_list(&mut self) {
        self.list_container.clear();
        self.list_container.add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Muted, &js_format!("Save scope: {}", self.scope.as_str()))),
            1,
            0,
        )));
        self.list_container.add_child(component(Spacer::new(1)));

        for (index, preset) in APPROVAL_PRESETS.iter().enumerate() {
            let is_selected = index == self.selected_index;
            let is_current = preset.name == self.current_preset;
            let prefix = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "→ "))
            } else {
                JsString::from("  ")
            };
            let label = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, preset.label))
            } else {
                with_theme(|theme| theme.fg(ThemeColor::Text, preset.label))
            };
            let checkmark = if is_current {
                with_theme(|theme| theme.fg(ThemeColor::Success, " ✓"))
            } else {
                JsString::default()
            };
            self.list_container.add_child(component(Text::new(
                js_format!("{}{}{}", prefix, label, checkmark),
                1,
                0,
            )));
        }
    }

    pub fn scope(&self) -> ApprovalScope {
        self.scope
    }
}

impl Component for ApprovalSelectorComponent {
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
            self.selected_index = (self.selected_index + 1).min(APPROVAL_PRESETS.len() - 1);
            self.update_list();
        } else if key_data == "\t" {
            self.scope = match self.scope {
                ApprovalScope::Global => ApprovalScope::Project,
                ApprovalScope::Project => ApprovalScope::Global,
            };
            self.update_list();
        } else if keybindings.matches(key_data, "tui.select.confirm") || key_data == "\n" {
            if let Some(selected) = APPROVAL_PRESETS.get(self.selected_index) {
                (self.on_select)(ApprovalSelectorSelection {
                    preset: selected.name,
                    scope: self.scope,
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
