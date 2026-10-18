//! Ported from `coding-agent/src/modes/interactive/components/custom-entry.ts`.

use std::rc::Rc;

use pi_tui::components::spacer::Spacer;
use pi_tui::tui::{Component, ComponentRef, Container, component};

use crate::core::extensions::types::{EntryRenderOptions, EntryRenderer};
use crate::core::session_manager::CustomEntry;
use crate::modes::interactive::theme::current_theme;

/// Renders a custom session entry from an extension.
pub struct CustomEntryComponent {
    entry: CustomEntry,
    renderer: EntryRenderer,
    custom_component: Option<ComponentRef>,
    expanded: bool,
    container: Container,
}

impl CustomEntryComponent {
    pub fn new(entry: CustomEntry, renderer: EntryRenderer) -> Self {
        let mut component = Self {
            entry,
            renderer,
            custom_component: None,
            expanded: false,
            container: Container::new(),
        };
        component.rebuild();
        component
    }

    pub fn has_content(&self) -> bool {
        self.custom_component.is_some()
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        if self.expanded != expanded {
            self.expanded = expanded;
            self.rebuild();
        }
    }

    fn rebuild(&mut self) {
        self.container.clear();
        self.custom_component = None;

        let theme = current_theme();
        let rendered = (self.renderer)(
            &self.entry,
            &EntryRenderOptions {
                expanded: self.expanded,
            },
            theme.as_ref(),
        );

        let Some(rendered) = rendered else {
            return;
        };
        self.custom_component = Some(Rc::clone(&rendered));
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(rendered);
    }
}

impl Component for CustomEntryComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.container.render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
        self.rebuild();
    }
}
