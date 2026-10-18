//! Ported from `coding-agent/src/modes/interactive/components/bordered-loader.ts`.

use std::rc::Rc;

use pi_tui::components::cancellable_loader::CancellableLoader;
use pi_tui::components::dynamic_border::DynamicBorder;
use pi_tui::components::loader::{Loader, LoaderIndicatorOptions};
use pi_tui::tui::{Component, ComponentRef, Container, component};
use pi_tui::{Spacer, Text};
use tokio_util::sync::CancellationToken;

use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{ThemeColor, with_theme};

/// `BorderedLoader` options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderedLoaderOptions {
    pub cancellable: Option<bool>,
}

/// Loader wrapped with borders for extension UI.
pub struct BorderedLoader {
    container: Container,
    loader: ComponentRef,
    cancellable: bool,
    signal: CancellationToken,
}

impl BorderedLoader {
    pub fn new(request_render: Rc<dyn Fn()>, message: impl Into<String>, options: BorderedLoaderOptions) -> Self {
        let cancellable = options.cancellable.unwrap_or(true);
        let border_color = Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Border, text)));
        let mut container = Container::new();
        container.add_child(component(DynamicBorder::new(border_color.clone())));

        let message = message.into();
        let (loader, signal) = if cancellable {
            let loader = CancellableLoader::new(
                Rc::clone(&request_render),
                Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
                Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
                message,
                Some(LoaderIndicatorOptions {
                    frames: Some(Vec::new()),
                    interval_ms: None,
                }),
            );
            let signal = loader.signal();
            (component(loader), signal)
        } else {
            let loader = Loader::new(
                request_render,
                Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
                Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
                message,
                Some(LoaderIndicatorOptions {
                    frames: Some(Vec::new()),
                    interval_ms: None,
                }),
            );
            (component(loader), CancellationToken::new())
        };

        container.add_child(Rc::clone(&loader));
        if cancellable {
            container.add_child(component(Spacer::new(1)));
            container.add_child(component(Text::new(key_hint("tui.select.cancel", "cancel"), 1, 0)));
        }
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(DynamicBorder::new(border_color)));

        Self {
            container,
            loader,
            cancellable,
            signal,
        }
    }

    /// `signal`: fires when the user cancels a cancellable loader.
    pub fn signal(&self) -> CancellationToken {
        self.signal.clone()
    }

    pub fn set_on_abort(&mut self, handler: Option<Rc<dyn Fn()>>) {
        if !self.cancellable {
            return;
        }
        if let Some(loader) = self
            .loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<CancellableLoader>())
        {
            loader.on_abort = handler;
        }
    }

    pub fn dispose(&mut self) {
        if let Some(loader) = self
            .loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<CancellableLoader>())
        {
            loader.dispose();
            return;
        }
        if let Some(loader) = self
            .loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Loader>())
        {
            loader.stop();
        }
    }
}

impl Component for BorderedLoader {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &str) {
        if self.cancellable {
            self.loader.borrow_mut().handle_input(data);
        }
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}
