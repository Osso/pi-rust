//! Ported from `packages/tui/src/components/cancellable-loader.ts`.
//!
//! The native signal preserves abort reasons; pressing the
//! `tui.select.cancel` key cancels it and invokes `on_abort`.

use std::rc::Rc;
use std::time::Instant;

use pi_js_compat::abort::{AbortController, AbortSignal};

use super::loader::{Loader, LoaderIndicatorOptions};
use super::text::StyleFn;
use crate::keybindings::get_keybindings;
use pi_js_compat::js_json::JsString;

use crate::tui::Component;

pub struct CancellableLoader {
    loader: Loader,
    abort_controller: AbortController,
    pub on_abort: Option<Rc<dyn Fn()>>,
}

impl CancellableLoader {
    pub fn new(
        request_render: Rc<dyn Fn()>,
        spinner_color_fn: StyleFn,
        message_color_fn: StyleFn,
        message: impl Into<JsString>,
        indicator: Option<LoaderIndicatorOptions>,
    ) -> Self {
        Self {
            loader: Loader::new(request_render, spinner_color_fn, message_color_fn, message, indicator),
            abort_controller: AbortController::new(),
            on_abort: None,
        }
    }

    /// Abort signal that fires when the user presses Escape.
    pub fn signal(&self) -> AbortSignal {
        self.abort_controller.signal()
    }

    pub fn is_aborted(&self) -> bool {
        self.abort_controller.signal().aborted()
    }

    pub fn start(&mut self) {
        self.loader.start();
    }

    pub fn stop(&mut self) {
        self.loader.stop();
    }

    pub fn set_message(&mut self, message: impl Into<JsString>) {
        self.loader.set_message(message);
    }

    pub fn set_indicator(&mut self, indicator: Option<LoaderIndicatorOptions>) {
        self.loader.set_indicator(indicator);
    }

    pub fn poll(&mut self, now: Instant) -> bool {
        self.loader.poll(now)
    }

    /// Cancel matching input and return the callback without invoking it, so shared callers can release borrows first.
    pub fn abort_for_input(&mut self, data: &JsString) -> Option<Rc<dyn Fn()>> {
        if !get_keybindings().matches(data, "tui.select.cancel") {
            return None;
        }
        self.abort_controller.abort(None);
        self.on_abort.clone()
    }

    /// Stop the animation and release resources.
    pub fn dispose(&mut self) {
        self.loader.stop();
    }
}

impl Component for CancellableLoader {
    fn dispose(&mut self) {
        CancellableLoader::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.loader.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        if let Some(on_abort) = self.abort_for_input(data) {
            on_abort();
        }
    }

    fn invalidate(&mut self) {
        self.loader.invalidate();
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
