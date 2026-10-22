//! Ported from `coding-agent/src/modes/interactive/components/custom-entry.ts`.

use crate::core::extensions::types::{EntryRenderOptions, EntryRenderer};
use crate::core::session_manager::CustomEntry;
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, current_theme, with_theme};
use crate::utils::uncaught::{UncaughtError, catch_panics};
use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::Text;
use pi_tui::components::r#box::BoxComponent;
use pi_tui::components::spacer::Spacer;
use pi_tui::tui::{Component, ComponentRef, Container, component};
use std::{cell::RefCell, rc::Rc};

type Cleanup = Rc<dyn Fn()>;
type Cleanups = Rc<RefCell<Vec<Cleanup>>>;

/// The host binds `request_render` to this entry's scoped chat region.
pub struct CustomEntryComponentOptions {
    pub request_render: Rc<dyn Fn() -> bool>,
    pub session_id: JsString,
}

/// Renders a custom session entry from an extension.
pub struct CustomEntryComponent {
    entry: CustomEntry,
    renderer: EntryRenderer,
    options: CustomEntryComponentOptions,
    custom_component: Option<ComponentRef>,
    cleanup_callbacks: Cleanups,
    expanded: bool,
    container: Container,
}

impl CustomEntryComponent {
    pub fn new(entry: CustomEntry, renderer: EntryRenderer, options: CustomEntryComponentOptions) -> Self {
        let mut component = Self {
            entry,
            renderer,
            options,
            custom_component: None,
            cleanup_callbacks: Rc::default(),
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
        let cleanup_callbacks: Cleanups = Rc::default();
        let callbacks = cleanup_callbacks.clone();
        let options = EntryRenderOptions {
            expanded: self.expanded,
            session_id: self.options.session_id.clone(),
            request_render: self.options.request_render.clone(),
            register_cleanup: Rc::new(move |cleanup| {
                let mut callbacks = callbacks.borrow_mut();
                if !callbacks.iter().any(|registered| Rc::ptr_eq(registered, &cleanup)) {
                    callbacks.push(cleanup);
                }
            }),
        };
        let theme = current_theme();
        let rendered = match catch_panics(|| (self.renderer)(&self.entry, &options, theme.as_ref())) {
            Ok(component) => component,
            Err(error) => Some(renderer_error_component(&self.entry.custom_type, error.as_ref())),
        };
        // Invoke the replacement renderer before retiring the previous generation.
        self.dispose_rendered_component();
        self.cleanup_callbacks = cleanup_callbacks;
        self.container.clear();
        if let Some(rendered) = rendered {
            self.custom_component = Some(rendered.clone());
            self.container.add_child(component(Spacer::new(1)));
            self.container.add_child(rendered);
        }
    }
    fn dispose_rendered_component(&mut self) {
        // Preserve Set iteration order, including callbacks appended by a cleanup.
        let mut index = 0;
        loop {
            let cleanup = self.cleanup_callbacks.borrow().get(index).cloned();
            let Some(cleanup) = cleanup else {
                break;
            };
            cleanup();
            index += 1;
        }
        self.cleanup_callbacks.borrow_mut().clear();
        if let Some(component) = &self.custom_component {
            component.borrow_mut().dispose();
        }
        self.custom_component = None;
    }
}

fn renderer_error_component(custom_type: &JsString, error: &(dyn std::any::Any + Send)) -> ComponentRef {
    let message = if let Some(error) = error.downcast_ref::<UncaughtError>() {
        error.message.clone()
    } else if let Some(message) = error.downcast_ref::<JsString>() {
        message.clone()
    } else if let Some(message) = error.downcast_ref::<String>() {
        message.into()
    } else if let Some(message) = error.downcast_ref::<&str>() {
        (*message).into()
    } else {
        crate::utils::uncaught::format_uncaught(error)
    };
    let mut error_box = BoxComponent::new(
        1,
        1,
        Some(Rc::new(|text| {
            with_theme(|theme| theme.bg(ThemeBg::CustomMessageBg, text))
        })),
    );
    error_box.add_child(component(Text::new(
        with_theme(|theme| {
            theme.fg(
                ThemeColor::Error,
                &js_format!("[{}] renderer failed: {}", custom_type, message),
            )
        }),
        0,
        0,
    )));
    component(error_box)
}

impl Component for CustomEntryComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }
    fn wants_input(&self) -> bool {
        false
    }
    fn invalidate(&mut self) {
        self.container.invalidate();
        self.rebuild();
    }
    fn dispose(&mut self) {
        self.dispose_rendered_component();
        self.container.clear();
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
