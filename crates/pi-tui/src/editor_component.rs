//! Ported from `packages/tui/src/editor-component.ts`.
//!
//! The source interface exposes optional capabilities through optional methods.
//! Rust models them as default-implemented trait methods.

use pi_js_compat::js_json::JsString;

use std::rc::Rc;

use crate::autocomplete::AutocompleteProvider;
use crate::components::loader::LoaderIndicatorOptions;
use crate::components::text::StyleFn;
use crate::tui::Component;

/// Failure from the optional persistent editor-history boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorHistoryError {
    pub message: JsString,
}

/// Continuation that borrows the editor only after an input callback returns.
pub type EditorInputResume = Box<dyn FnOnce(&mut dyn EditorComponent) -> EditorInput>;

/// Input dispatch alternates editor mutations and callbacks that may re-enter it.
pub enum EditorInput {
    Complete,
    Callback(Box<dyn FnOnce() -> EditorInput>),
    Resume(EditorInputResume),
}

impl EditorInput {
    pub fn run(mut self, mut resume: impl FnMut(EditorInputResume) -> EditorInput) {
        loop {
            self = match self {
                Self::Complete => return,
                Self::Callback(callback) => callback(),
                Self::Resume(continuation) => resume(continuation),
            };
        }
    }

    pub fn then(self, callback: impl FnOnce() + 'static) -> Self {
        match self {
            Self::Complete => Self::Callback(Box::new(move || {
                callback();
                Self::Complete
            })),
            Self::Callback(next) => Self::Callback(Box::new(move || next().then(callback))),
            Self::Resume(next) => Self::Resume(Box::new(move |editor| next(editor).then(callback))),
        }
    }
}

/// Custom editor implementation accepted by the core application.
pub trait EditorComponent: Component {
    fn get_text(&self) -> JsString;

    fn set_text(&mut self, text: &JsString);

    /// Rust-only dispatch protocol: callbacks run outside the host's editor
    /// borrow; continuations resume under a fresh borrow after their results.
    fn handle_input_deferred(&mut self, data: &JsString) -> EditorInput {
        self.handle_input(data);
        EditorInput::Complete
    }

    fn add_to_history(&mut self, _text: &JsString) -> Result<(), EditorHistoryError> {
        Ok(())
    }

    fn insert_text_at_cursor(&mut self, _text: &JsString) {}

    fn get_expanded_text(&self) -> JsString {
        self.get_text()
    }

    fn set_autocomplete_provider(&mut self, _provider: Rc<dyn AutocompleteProvider>) {}

    fn set_border_color(&mut self, _border_color: StyleFn) {}

    fn set_padding_x(&mut self, _padding: usize) {}

    fn set_autocomplete_max_visible(&mut self, _max_visible: usize) {}

    /// Whether the editor implements the working-prompt animation surface
    /// (the source's `setWorking`/`setWorkingIndicator`/`setScreenOrigin`/
    /// `clearScreenOrigin` presence check).
    fn supports_working_prompt(&self) -> bool {
        false
    }

    fn set_working(&mut self, _working: bool) {}

    fn set_working_indicator(&mut self, _options: Option<LoaderIndicatorOptions>) {}

    fn set_screen_origin(&mut self, _row: i64, _col: i64) {}

    fn clear_screen_origin(&mut self) {}
}

impl EditorComponent for crate::components::editor::Editor {
    fn get_text(&self) -> JsString {
        crate::components::editor::Editor::get_text(self)
    }

    fn set_text(&mut self, text: &JsString) {
        crate::components::editor::Editor::set_text(self, text);
    }

    fn add_to_history(&mut self, text: &JsString) -> Result<(), EditorHistoryError> {
        crate::components::editor::Editor::add_to_history(self, text);
        Ok(())
    }

    fn insert_text_at_cursor(&mut self, text: &JsString) {
        crate::components::editor::Editor::insert_text_at_cursor(self, text);
    }

    fn get_expanded_text(&self) -> JsString {
        crate::components::editor::Editor::get_expanded_text(self)
    }

    fn set_autocomplete_provider(&mut self, provider: Rc<dyn AutocompleteProvider>) {
        crate::components::editor::Editor::set_autocomplete_provider(self, provider);
    }

    fn set_border_color(&mut self, border_color: StyleFn) {
        crate::components::editor::Editor::set_border_color(self, border_color);
    }

    fn set_padding_x(&mut self, padding: usize) {
        crate::components::editor::Editor::set_padding_x(self, padding);
    }

    fn set_autocomplete_max_visible(&mut self, max_visible: usize) {
        crate::components::editor::Editor::set_autocomplete_max_visible(self, max_visible);
    }

    fn supports_working_prompt(&self) -> bool {
        true
    }

    fn set_working(&mut self, working: bool) {
        crate::components::editor::Editor::set_working(self, working);
    }

    fn set_working_indicator(&mut self, options: Option<LoaderIndicatorOptions>) {
        crate::components::editor::Editor::set_working_indicator(self, options)
            .expect("working indicator frames must occupy one cell; callers validate first");
    }

    fn set_screen_origin(&mut self, row: i64, col: i64) {
        crate::components::editor::Editor::set_screen_origin(self, row, col);
    }

    fn clear_screen_origin(&mut self) {
        crate::components::editor::Editor::clear_screen_origin(self);
    }
}
