//! Ported from `coding-agent/src/modes/interactive/components/custom-editor.ts`.

use std::rc::Rc;

use indexmap::IndexMap;
use pi_js_compat::js_json::{JsString, JsValue as Value, encode_node_utf8, parse_lossless};
use pi_tui::components::editor::{Editor, EditorHost, EditorOptions, EditorTheme};
use pi_tui::editor_component::{EditorHistoryError, EditorInput};
use pi_tui::tui::Component;

use crate::core::keybindings::KeybindingsManager;
use crate::core::session_control_db::{read_or_migrate_prompt_history, record_prompt_history_entry};

/// App action handler: `true` means the key was handled (TypeScript `!== false`).
pub type AppActionHandler = Rc<dyn Fn() -> bool>;

/// Prompt-history filter callback.
pub type PromptHistoryFilter = Rc<dyn Fn(&JsString) -> bool>;

/// Extension-registered shortcut: returns true when handled.
pub type ExtensionShortcut = Rc<dyn Fn(&JsString) -> bool>;

/// `CustomEditorOptions`.
#[derive(Default)]
pub struct CustomEditorOptions {
    pub editor: EditorOptions,
    pub legacy_prompt_history_path: Option<JsString>,
    pub prompt_history_control_db_path: Option<JsString>,
    pub prompt_history_filter: Option<PromptHistoryFilter>,
}

/// Editor that handles app-level keybindings for coding-agent.
pub struct CustomEditor {
    editor: Editor,
    keybindings: KeybindingsManager,
    prompt_history_control_db_path: Option<JsString>,
    pub action_handlers: IndexMap<JsString, AppActionHandler>,
    pub on_escape: Option<Rc<dyn Fn()>>,
    pub on_ctrl_d: Option<Rc<dyn Fn()>>,
    pub on_paste_image: Option<Rc<dyn Fn()>>,
    /// Extension-registered shortcuts. Returns true if handled.
    pub on_extension_shortcut: Option<ExtensionShortcut>,
}

impl CustomEditor {
    pub fn new(
        tui: Rc<dyn EditorHost>,
        theme: EditorTheme,
        keybindings: KeybindingsManager,
        options: CustomEditorOptions,
    ) -> Result<Self, crate::core::session_control_db::ControlDbError> {
        let mut editor = Editor::new(tui, theme, options.editor);
        let prompt_history_control_db_path = options.prompt_history_control_db_path.filter(|path| !path.is_empty());
        if let Some(control_db_path) = &prompt_history_control_db_path {
            let history = load_prompt_history(control_db_path, options.legacy_prompt_history_path.as_ref())?;
            let history = match &options.prompt_history_filter {
                Some(filter) => history.into_iter().filter(|entry| filter(entry)).collect(),
                None => history,
            };
            editor.set_history(history);
        }
        Ok(Self {
            editor,
            keybindings,
            prompt_history_control_db_path,
            action_handlers: IndexMap::new(),
            on_escape: None,
            on_ctrl_d: None,
            on_paste_image: None,
            on_extension_shortcut: None,
        })
    }

    /// Register a handler for an app action.
    pub fn on_action(&mut self, action: impl Into<JsString>, handler: AppActionHandler) {
        self.action_handlers.insert(action.into(), handler);
    }

    pub fn get_text(&self) -> JsString {
        self.editor.get_text()
    }

    pub fn set_text(&mut self, text: &JsString) {
        self.editor.set_text(text);
    }

    pub fn get_history(&self) -> Vec<JsString> {
        self.editor.get_history()
    }

    pub fn get_padding_x(&self) -> usize {
        self.editor.get_padding_x()
    }

    pub fn get_autocomplete_max_visible(&self) -> usize {
        self.editor.get_autocomplete_max_visible()
    }

    pub fn get_border_color(&self) -> pi_tui::components::text::StyleFn {
        self.editor.get_border_color()
    }

    pub fn set_history(&mut self, history: Vec<JsString>) {
        self.editor.set_history(history);
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.editor
    }

    /// `addToHistory`: append and persist the newest entry.
    pub fn add_to_history(&mut self, text: &JsString) -> Result<(), crate::core::session_control_db::ControlDbError> {
        let before = self.editor.get_history();
        self.editor.add_to_history(text);
        self.record_newest_history_entry(&before)
    }

    pub fn add_to_history_without_persistence(&mut self, text: &JsString) {
        self.editor.add_to_history(text);
    }

    fn record_newest_history_entry(
        &mut self,
        previous_history: &[JsString],
    ) -> Result<(), crate::core::session_control_db::ControlDbError> {
        let history = self.editor.get_history();
        let Some(newest_history_entry) = history.first() else {
            return Ok(());
        };
        let Some(control_db_path) = &self.prompt_history_control_db_path else {
            return Ok(());
        };
        if !same_history(previous_history, &history) {
            record_prompt_history_entry(control_db_path, newest_history_entry)?;
        }
        Ok(())
    }
}

fn load_prompt_history(
    control_db_path: &JsString,
    legacy_prompt_history_path: Option<&JsString>,
) -> Result<Vec<JsString>, crate::core::session_control_db::ControlDbError> {
    let legacy_prompt_history = read_legacy_prompt_history(legacy_prompt_history_path);
    read_or_migrate_prompt_history(control_db_path, &legacy_prompt_history, 100)
}

fn read_legacy_prompt_history(path: Option<&JsString>) -> Vec<JsString> {
    let Some(path) = path.filter(|path| std::path::Path::new(&encode_node_utf8(path)).exists()) else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(encode_node_utf8(path)) else {
        return Vec::new();
    };
    let content = JsString::from(std::string::String::from_utf8_lossy(&bytes).as_ref());
    let Ok(parsed) = parse_lossless(&content) else {
        return Vec::new();
    };
    let Value::Array(entries) = parsed else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            Value::String(text) => Some(text),
            _ => None,
        })
        .take(100)
        .collect()
}

fn as_action_fn(handler: &AppActionHandler) -> Rc<dyn Fn()> {
    let handler = Rc::clone(handler);
    Rc::new(move || {
        handler();
    })
}

fn dispatch_consumed(handler: Option<Rc<dyn Fn()>>) -> EditorInput {
    match handler {
        Some(handler) => EditorInput::Callback(Box::new(move || {
            handler();
            EditorInput::Complete
        })),
        None => EditorInput::Complete,
    }
}

fn resume_custom_editor(resume: impl FnOnce(&mut CustomEditor) -> EditorInput + 'static) -> EditorInput {
    EditorInput::Resume(Box::new(move |editor| {
        let editor = editor
            .as_any_mut()
            .and_then(|editor| editor.downcast_mut::<CustomEditor>())
            .expect("CustomEditor dispatch resumes on its composed base editor");
        resume(editor)
    }))
}

fn same_history(left: &[JsString], right: &[JsString]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(left, right)| left == right)
}

impl CustomEditor {
    /// Pause at callbacks so a mounted editor can release its RefCell borrow.
    pub fn dispatch_input(&mut self, data: &JsString) -> EditorInput {
        let Some(shortcut) = self.on_extension_shortcut.clone() else {
            return self.dispatch_app_binding(data);
        };
        let data = data.clone();
        EditorInput::Callback(Box::new(move || {
            if shortcut(&data) {
                EditorInput::Complete
            } else {
                resume_custom_editor(move |editor| editor.dispatch_app_binding(&data))
            }
        }))
    }

    fn dispatch_app_binding(&mut self, data: &JsString) -> EditorInput {
        if self.keybindings.matches(data, "app.clipboard.pasteImage") {
            return dispatch_consumed(self.on_paste_image.clone());
        }

        if self.keybindings.matches(data, "app.interrupt") {
            if !self.editor.is_showing_autocomplete() {
                let handler = self
                    .on_escape
                    .clone()
                    .or_else(|| self.action_handlers.get("app.interrupt").map(as_action_fn));
                if handler.is_some() {
                    return dispatch_consumed(handler);
                }
            }
            self.editor.handle_input(data);
            return EditorInput::Complete;
        }

        if self.keybindings.matches(data, "app.exit") && self.editor.get_text().is_empty() {
            let handler = self
                .on_ctrl_d
                .clone()
                .or_else(|| self.action_handlers.get("app.exit").map(as_action_fn));
            return dispatch_consumed(handler);
        }

        self.dispatch_action(data, 0)
    }

    fn dispatch_action(&mut self, data: &JsString, start: usize) -> EditorInput {
        for (index, (action, handler)) in self.action_handlers.iter().enumerate().skip(start) {
            if action == "app.interrupt" || action == "app.exit" || !self.keybindings.matches(data, action) {
                continue;
            }
            let handler = handler.clone();
            let data = data.clone();
            return EditorInput::Callback(Box::new(move || {
                if handler() {
                    EditorInput::Complete
                } else {
                    resume_custom_editor(move |editor| editor.dispatch_action(&data, index + 1))
                }
            }));
        }
        self.editor.handle_input(data);
        EditorInput::Complete
    }
}

impl Component for CustomEditor {
    fn as_focusable_mut(&mut self) -> Option<&mut dyn pi_tui::tui::Focusable> {
        Some(self)
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.editor.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.dispatch_input(data).run(|resume| resume(self));
    }

    fn invalidate(&mut self) {
        self.editor.invalidate();
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl pi_tui::EditorComponent for CustomEditor {
    fn handle_input_deferred(&mut self, data: &JsString) -> EditorInput {
        self.dispatch_input(data)
    }

    fn get_text(&self) -> Result<JsString, pi_js_compat::callback_error::CallbackError> {
        Ok(self.editor.get_text())
    }

    fn set_text(&mut self, text: &JsString) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.editor.set_text(text);
        Ok(())
    }

    fn add_to_history(&mut self, text: &JsString) -> Result<(), EditorHistoryError> {
        CustomEditor::add_to_history(self, text).map_err(|error| EditorHistoryError {
            message: error.message().clone(),
        })
    }

    fn insert_text_at_cursor(&mut self, text: &JsString) {
        self.editor.insert_text_at_cursor(text);
    }

    fn get_expanded_text(&self) -> Result<JsString, pi_js_compat::callback_error::CallbackError> {
        Ok(self.editor.get_expanded_text())
    }

    fn set_autocomplete_provider(&mut self, provider: Rc<dyn pi_tui::autocomplete::AutocompleteProvider>) {
        self.editor.set_autocomplete_provider(provider);
    }

    fn set_border_color(&mut self, border_color: pi_tui::components::text::StyleFn) {
        self.editor.set_border_color(border_color);
    }

    fn set_padding_x(&mut self, padding: usize) {
        self.editor.set_padding_x(padding);
    }

    fn set_autocomplete_max_visible(&mut self, max_visible: usize) {
        self.editor.set_autocomplete_max_visible(max_visible);
    }

    fn supports_working_prompt(&self) -> bool {
        true
    }

    fn set_working(&mut self, working: bool) {
        self.editor.set_working(working);
    }

    fn set_working_indicator(&mut self, options: Option<pi_tui::components::loader::LoaderIndicatorOptions>) {
        self.editor
            .set_working_indicator(options)
            .expect("working indicator frames must occupy one cell; callers validate first");
    }

    fn set_screen_origin(&mut self, row: i64, col: i64) {
        self.editor.set_screen_origin(row, col);
    }

    fn clear_screen_origin(&mut self) {
        self.editor.clear_screen_origin();
    }
}

impl pi_tui::tui::Focusable for CustomEditor {
    fn focused(&self) -> bool {
        pi_tui::tui::Focusable::focused(&self.editor)
    }

    fn set_focused(&mut self, focused: bool) {
        pi_tui::tui::Focusable::set_focused(&mut self.editor, focused);
    }
}

#[cfg(test)]
mod raw_legacy_history_tests {
    use super::*;

    #[test]
    fn legacy_json_import_retains_unmatched_units_and_filters_non_strings() {
        let directory = tempfile::tempdir().unwrap();
        let path = JsString::from(directory.path().join("history.json").display().to_string());
        std::fs::write(encode_node_utf8(&path), r#"["\ud800",42,"\udc00","😀"]"#).unwrap();
        assert_eq!(
            read_legacy_prompt_history(Some(&path)),
            vec![
                JsString::from_utf16(vec![0xd800]),
                JsString::from_utf16(vec![0xdc00]),
                JsString::from("😀"),
            ]
        );
    }
}
