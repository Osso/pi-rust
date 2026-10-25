//! Frozen `coding-agent/src/modes/interactive/components/coordination-message.ts`.
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use pi_js_compat::js_json::JsString;
use pi_tui::tui::ComponentRef;
use pi_tui::{Component, Container};

use super::custom_message::CustomMessageComponent;
use super::user_message::UserMessageComponent;

/// Retains message children while hiding only rendered lines.
pub struct CoordinationMessageComponent {
    container: Container,
    is_hidden: Rc<dyn Fn() -> bool>,
}

impl CoordinationMessageComponent {
    pub fn new(children: Vec<ComponentRef>, is_hidden: Rc<dyn Fn() -> bool>) -> Self {
        Self {
            container: Container::with_children(children),
            is_hidden,
        }
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        for child in self.container.children() {
            if let Some(message) = child
                .borrow_mut()
                .as_any_mut()
                .and_then(|child| child.downcast_mut::<CustomMessageComponent>())
            {
                message.set_expanded(expanded);
            }
        }
    }

    pub fn set_in_transit(&mut self, in_transit: bool) {
        for child in self.container.children() {
            if let Some(message) = child
                .borrow_mut()
                .as_any_mut()
                .and_then(|child| child.downcast_mut::<UserMessageComponent>())
            {
                message.set_in_transit(in_transit);
            }
        }
    }
}

impl Deref for CoordinationMessageComponent {
    type Target = Container;
    fn deref(&self) -> &Self::Target {
        &self.container
    }
}

impl DerefMut for CoordinationMessageComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.container
    }
}

impl Component for CoordinationMessageComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        if (self.is_hidden)() {
            Vec::new()
        } else {
            self.container.render(width)
        }
    }
    fn wants_input(&self) -> bool {
        false
    }
    fn invalidate(&mut self) {
        self.container.invalidate();
    }
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
