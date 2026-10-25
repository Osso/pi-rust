//! Ported from `coding-agent/src/modes/interactive/components/bordered-loader.ts`.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use std::sync::Arc;

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::JsString;
use pi_tui::components::cancellable_loader::CancellableLoader;
use pi_tui::components::dynamic_border::DynamicBorder;
use pi_tui::components::loader::{Loader, LoaderIndicatorOptions};
use pi_tui::components::text::StyleFn;
use pi_tui::tui::{Component, ComponentRef, Container, component};
use pi_tui::{Spacer, Text};

use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};

/// `BorderedLoader` options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderedLoaderOptions {
    pub cancellable: Option<bool>,
}

enum LoaderHandle {
    Cancellable(Rc<RefCell<CancellableLoader>>),
    Plain(Rc<RefCell<Loader>>),
}

impl LoaderHandle {
    fn component(&self) -> ComponentRef {
        match self {
            Self::Cancellable(loader) => loader.clone(),
            Self::Plain(loader) => loader.clone(),
        }
    }

    fn dispose(&self) {
        match self {
            Self::Cancellable(loader) => loader.borrow_mut().dispose(),
            Self::Plain(loader) => loader.borrow_mut().stop(),
        }
    }
}

/// Loader wrapped with borders for extension UI.
pub struct BorderedLoader {
    container: Container,
    loader: LoaderHandle,
    signal: AbortSignal,
}

impl BorderedLoader {
    pub fn new(
        request_render: Rc<dyn Fn()>,
        theme: Arc<Theme>,
        message: impl Into<JsString>,
        options: BorderedLoaderOptions,
    ) -> Self {
        let cancellable = options.cancellable.unwrap_or(true);
        let border_color = theme_color(&theme, ThemeColor::Border);
        let (loader, signal) = create_loader(request_render, &theme, message.into(), cancellable);
        let mut container = Container::new();
        container.add_child(component(DynamicBorder::new(border_color.clone())));
        container.add_child(loader.component());
        if cancellable {
            container.add_child(component(Spacer::new(1)));
            container.add_child(component(Text::new(key_hint("tui.select.cancel", "cancel"), 1, 0)));
        }
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(DynamicBorder::new(border_color)));
        Self {
            container,
            loader,
            signal,
        }
    }

    /// `signal`: fires when the user cancels a cancellable loader.
    pub fn signal(&self) -> AbortSignal {
        self.signal.clone()
    }

    pub fn set_on_abort(&mut self, handler: Option<Rc<dyn Fn()>>) {
        if let LoaderHandle::Cancellable(loader) = &self.loader {
            loader.borrow_mut().on_abort = handler;
        }
    }

    /// Return the cancellation callback for invocation after shared caller guards are released.
    pub fn abort_for_input(&self, data: &JsString) -> Option<Rc<dyn Fn()>> {
        match &self.loader {
            LoaderHandle::Cancellable(loader) => loader.borrow_mut().abort_for_input(data),
            LoaderHandle::Plain(_) => None,
        }
    }

    pub fn dispose(&mut self) {
        self.loader.dispose();
    }
}

fn theme_color(theme: &Arc<Theme>, color: ThemeColor) -> StyleFn {
    let theme = theme.clone();
    Rc::new(move |text| theme.fg(color, text))
}

fn create_loader(
    request_render: Rc<dyn Fn()>,
    theme: &Arc<Theme>,
    message: JsString,
    cancellable: bool,
) -> (LoaderHandle, AbortSignal) {
    let indicator = Some(LoaderIndicatorOptions {
        frames: Some(Vec::new()),
        interval_ms: None,
    });
    let spinner_color = theme_color(theme, ThemeColor::Accent);
    let message_color = theme_color(theme, ThemeColor::Muted);
    if cancellable {
        let loader = CancellableLoader::new(request_render, spinner_color, message_color, message, indicator);
        let signal = loader.signal();
        (LoaderHandle::Cancellable(Rc::new(RefCell::new(loader))), signal)
    } else {
        let loader = Loader::new(request_render, spinner_color, message_color, message, indicator);
        (
            LoaderHandle::Plain(Rc::new(RefCell::new(loader))),
            AbortSignal::default(),
        )
    }
}

impl Deref for BorderedLoader {
    type Target = Container;

    fn deref(&self) -> &Self::Target {
        &self.container
    }
}

impl DerefMut for BorderedLoader {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.container
    }
}

impl Component for BorderedLoader {
    fn dispose(&mut self) {
        BorderedLoader::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        if let Some(on_abort) = self.abort_for_input(data) {
            on_abort();
        }
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
