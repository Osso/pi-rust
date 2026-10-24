//! Shared delegation for the three native CustomEditor subclass examples.
//! Expose the composed base to the host's CustomEditor callback wiring.
use pi_coding_agent::modes::interactive::components::custom_editor::CustomEditor;
use pi_js_compat::js_json::JsString;
use pi_tui::EditorComponent;
use pi_tui::autocomplete::AutocompleteProvider;
use pi_tui::components::loader::LoaderIndicatorOptions;
use pi_tui::components::text::StyleFn;
use pi_tui::editor_component::{EditorHistoryError, EditorInput};
use pi_tui::tui::{Component, Focusable};
use std::rc::Rc;

pub trait EditorBehavior {
    fn input(&mut self, editor: &mut CustomEditor, data: &JsString) -> EditorInput {
        editor.handle_input_deferred(data)
    }
    fn render(&mut self, editor: &mut CustomEditor, width: usize) -> Vec<JsString>;
}

pub struct ComposedEditor<B> {
    pub editor: CustomEditor,
    pub behavior: B,
}
impl<B: EditorBehavior> Component for ComposedEditor<B> {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.behavior.render(&mut self.editor, width)
    }
    fn handle_input(&mut self, data: &JsString) {
        self.handle_input_deferred(data).run(|resume| resume(self));
    }
    fn invalidate(&mut self) {
        self.editor.invalidate();
    }
    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut self.editor)
    }
}
impl<B: EditorBehavior> Focusable for ComposedEditor<B> {
    fn focused(&self) -> bool {
        self.editor.focused()
    }
    fn set_focused(&mut self, focused: bool) {
        self.editor.set_focused(focused);
    }
}
impl<B: EditorBehavior> EditorComponent for ComposedEditor<B> {
    fn get_text(&self) -> Result<JsString, pi_js_compat::callback_error::CallbackError> {
        Ok(self.editor.get_text())
    }
    fn set_text(&mut self, text: &JsString) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.editor.set_text(text);
        Ok(())
    }
    fn handle_input_deferred(&mut self, data: &JsString) -> EditorInput {
        self.behavior.input(&mut self.editor, data)
    }
    fn add_to_history(&mut self, text: &JsString) -> Result<(), EditorHistoryError> {
        EditorComponent::add_to_history(&mut self.editor, text)
    }
    fn insert_text_at_cursor(&mut self, text: &JsString) {
        self.editor.insert_text_at_cursor(text);
    }
    fn get_expanded_text(&self) -> Result<JsString, pi_js_compat::callback_error::CallbackError> {
        self.editor.get_expanded_text()
    }
    fn set_autocomplete_provider(&mut self, provider: Rc<dyn AutocompleteProvider>) {
        self.editor.set_autocomplete_provider(provider);
    }
    fn set_border_color(&mut self, color: StyleFn) {
        self.editor.set_border_color(color);
    }
    fn set_padding_x(&mut self, padding: usize) {
        self.editor.set_padding_x(padding);
    }
    fn set_autocomplete_max_visible(&mut self, count: usize) {
        self.editor.set_autocomplete_max_visible(count);
    }
    fn supports_working_prompt(&self) -> bool {
        self.editor.supports_working_prompt()
    }
    fn set_working(&mut self, working: bool) {
        self.editor.set_working(working);
    }
    fn set_working_indicator(&mut self, options: Option<LoaderIndicatorOptions>) {
        self.editor.set_working_indicator(options);
    }
    fn set_screen_origin(&mut self, row: i64, col: i64) {
        self.editor.set_screen_origin(row, col);
    }
    fn clear_screen_origin(&mut self) {
        self.editor.clear_screen_origin();
    }
}
