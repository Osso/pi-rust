//! Ported from `packages/coding-agent/src/modes/interactive/components/sandbox-selector.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, component};

use super::super::theme::{ThemeColor, with_theme};
use super::dynamic_border::dynamic_border;
use super::keybinding_hints::{key_hint, raw_key_hint};
use crate::core::permissions::presets::{SANDBOX_PROFILES, SandboxProfileName, is_sandbox_platform_supported};
use crate::utils::pi_user_agent::node_platform_name;

/// Where the chosen sandbox profile is persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxScope {
    Global,
    Project,
    Session,
}

impl SandboxScope {
    const ALL: [Self; 3] = [Self::Global, Self::Project, Self::Session];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
            Self::Session => "session",
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL.iter().position(|scope| *scope == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxSelectorSelection {
    Saved {
        profile: SandboxProfileName,
        scope: SandboxScope,
    },
    Session {
        profile: Option<SandboxProfileName>,
    },
}

pub struct SandboxSelectorOptions {
    pub current_profile: SandboxProfileName,
    pub session_profile: Option<SandboxProfileName>,
    pub on_select: Rc<dyn Fn(SandboxSelectorSelection)>,
    pub on_cancel: Rc<dyn Fn()>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SandboxSelectorOption {
    label: JsString,
    profile: Option<SandboxProfileName>,
}

/// Sandbox profile list with a global/project/session save scope.
pub struct SandboxSelectorComponent {
    selected_index: usize,
    platform: String,
    scope: SandboxScope,
    container: Container,
    list_container: Rc<RefCell<Container>>,
    on_select: Rc<dyn Fn(SandboxSelectorSelection)>,
    on_cancel: Rc<dyn Fn()>,
    current_profile: SandboxProfileName,
    session_profile: Option<SandboxProfileName>,
}

impl SandboxSelectorComponent {
    pub fn new(options: SandboxSelectorOptions) -> Self {
        Self::new_for_platform(options, node_platform_name(std::env::consts::OS))
    }

    /// Use explicit process.platform authority, including unsupported-platform labels.
    pub fn new_for_platform(options: SandboxSelectorOptions, platform: &str) -> Self {
        let prefix_children = vec![
            component(dynamic_border()),
            component(Spacer::new(1)),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, theme.bold("Sandbox profiles"))),
                1,
                0,
            )),
            component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Muted, "Tab switches save scope: global/project/session")),
                1,
                0,
            )),
            component(Spacer::new(1)),
        ];
        let controls = JsString::join(
            [
                raw_key_hint("↑↓", "navigate"),
                raw_key_hint("Tab", "scope"),
                key_hint("tui.select.confirm", "save"),
                key_hint("tui.select.cancel", "cancel"),
            ],
            "  ",
        );
        let suffix_children = vec![
            component(Spacer::new(1)),
            component(Text::new(controls, 1, 0)),
            component(Spacer::new(1)),
            component(dynamic_border()),
        ];

        let list_container = Rc::new(RefCell::new(Container::new()));
        let list_child: ComponentRef = list_container.clone();
        let children = prefix_children
            .into_iter()
            .chain([list_child])
            .chain(suffix_children)
            .collect();
        let mut selector = Self {
            selected_index: 0,
            platform: platform.to_owned(),
            scope: SandboxScope::Global,
            container: Container::with_children(children),
            list_container,
            on_select: options.on_select,
            on_cancel: options.on_cancel,
            current_profile: options.current_profile,
            session_profile: options.session_profile,
        };
        selector.selected_index = selector.selected_index_for_scope();
        selector.update_list();
        selector
    }

    fn options_for_scope(&self) -> Vec<SandboxSelectorOption> {
        let profiles = SANDBOX_PROFILES
            .iter()
            .map(|profile| SandboxSelectorOption {
                label: if profile.name == SandboxProfileName::FullAccess
                    || is_sandbox_platform_supported(&self.platform)
                {
                    profile.label.into()
                } else {
                    js_format!("{} (unsupported on {}; Linux only)", profile.label, self.platform)
                },
                profile: Some(profile.name),
            })
            .collect::<Vec<_>>();
        if self.scope == SandboxScope::Session {
            let mut options = vec![SandboxSelectorOption {
                label: "Inherit global/project".into(),
                profile: None,
            }];
            options.extend(profiles);
            options
        } else {
            profiles
        }
    }

    fn selected_index_for_scope(&self) -> usize {
        let active_profile = match self.scope {
            SandboxScope::Session => self.session_profile,
            SandboxScope::Global | SandboxScope::Project => Some(self.current_profile),
        };
        self.options_for_scope()
            .iter()
            .position(|option| option.profile == active_profile)
            .unwrap_or(0)
    }

    fn cycle_scope(&mut self) {
        self.scope = self.scope.next();
        self.selected_index = self.selected_index_for_scope();
        self.update_list();
    }

    fn update_list(&mut self) {
        let mut list_container = self.list_container.borrow_mut();
        list_container.clear();
        list_container.add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Muted, &js_format!("Save scope: {}", self.scope.as_str()))),
            1,
            0,
        )));
        list_container.add_child(component(Spacer::new(1)));
        let active_profile = match self.scope {
            SandboxScope::Session => self.session_profile,
            SandboxScope::Global | SandboxScope::Project => Some(self.current_profile),
        };
        for (index, option) in self.options_for_scope().iter().enumerate() {
            let is_selected = index == self.selected_index;
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
            let checkmark = if option.profile == active_profile {
                with_theme(|theme| theme.fg(ThemeColor::Success, " ✓"))
            } else {
                JsString::default()
            };
            list_container.add_child(component(Text::new(
                js_format!("{}{}{}", prefix, label, checkmark),
                1,
                0,
            )));
        }
    }

    pub fn scope(&self) -> SandboxScope {
        self.scope
    }
}

impl Deref for SandboxSelectorComponent {
    type Target = Container;
    fn deref(&self) -> &Self::Target {
        &self.container
    }
}
impl DerefMut for SandboxSelectorComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.container
    }
}
impl Component for SandboxSelectorComponent {
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, key_data: &JsString) {
        let keybindings = get_keybindings();
        let options = self.options_for_scope();
        if keybindings.matches(key_data, "tui.select.up") || key_data == "k" {
            self.selected_index = self.selected_index.saturating_sub(1);
            self.update_list();
        } else if keybindings.matches(key_data, "tui.select.down") || key_data == "j" {
            self.selected_index = (self.selected_index + 1).min(options.len().saturating_sub(1));
            self.update_list();
        } else if key_data == "\t" {
            self.cycle_scope();
        } else if keybindings.matches(key_data, "tui.select.confirm") || key_data == "\n" {
            let Some(selected) = options.get(self.selected_index) else {
                return;
            };
            if self.scope == SandboxScope::Session {
                (self.on_select)(SandboxSelectorSelection::Session {
                    profile: selected.profile,
                });
            } else if let Some(profile) = selected.profile {
                (self.on_select)(SandboxSelectorSelection::Saved {
                    profile,
                    scope: self.scope,
                });
            }
        } else if keybindings.matches(key_data, "tui.select.cancel") {
            (self.on_cancel)();
        }
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}
