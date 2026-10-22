//! Ported from `coding-agent/src/modes/interactive/components/extension-editor.ts`.

use std::rc::Rc;

use pi_tui::components::editor::{Editor, EditorHost, EditorOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, Focusable, TuiExt, TuiRenderer, component};

use super::dynamic_border::dynamic_border;
use crate::core::keybindings::KeybindingsManager as AppKeybindingsManager;
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::external_editor::{
    ExternalEditorOptions, ExternalEditorResult, edit_in_external_editor,
};
use crate::modes::interactive::theme::{ThemeColor, get_editor_theme, with_theme};

/// Host surface needed to suspend the TUI around an external editor.
pub trait ExtensionEditorHost: EditorHost {
    fn suspend(&self);
    fn resume(&self);
    fn request_forced_render(&self);
}

impl ExtensionEditorHost for Rc<TuiRenderer> {
    fn suspend(&self) {
        TuiExt::stop(self.as_ref());
    }

    fn resume(&self) {
        let _ = TuiExt::start(self.as_ref());
    }

    fn request_forced_render(&self) {
        TuiExt::request_render(self.as_ref(), true);
    }
}

/// Multi-line editor component for extensions.
pub struct ExtensionEditorComponent {
    container: Container,
    editor: ComponentRef,
    tui: Rc<dyn ExtensionEditorHost>,
    keybindings: AppKeybindingsManager,
    external_editor_command: String,
    on_cancel: Rc<dyn Fn()>,
    focused: bool,
}

impl ExtensionEditorComponent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tui: Rc<dyn ExtensionEditorHost>,
        keybindings: AppKeybindingsManager,
        title: &str,
        prefill: Option<&str>,
        on_submit: Rc<dyn Fn(String)>,
        on_cancel: Rc<dyn Fn()>,
        options: EditorOptions,
        external_editor_command: Option<String>,
    ) -> Self {
        let external_editor_command = external_editor_command
            .filter(|command| !command.is_empty())
            .or_else(|| pi_js_compat::process_env::get("VISUAL").filter(|value| !value.is_empty()))
            .or_else(|| pi_js_compat::process_env::get("EDITOR").filter(|value| !value.is_empty()))
            .unwrap_or_else(|| {
                if cfg!(target_os = "windows") {
                    "notepad".to_owned()
                } else {
                    "nano".to_owned()
                }
            });

        let mut container = Container::new();
        container.add_child(component(dynamic_border()));
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Accent, title)),
            1,
            0,
        )));
        container.add_child(component(Spacer::new(1)));

        let host: Rc<dyn EditorHost> = tui.clone();
        let mut editor = Editor::new(host, get_editor_theme(), options);
        if let Some(prefill) = prefill {
            editor.set_text(prefill);
        }
        let submit = Rc::clone(&on_submit);
        editor.on_submit = Some(Rc::new(move |text: &str| submit(text.to_owned())));
        let editor = component(editor);
        container.add_child(Rc::clone(&editor));

        container.add_child(component(Spacer::new(1)));
        container.add_child(component(Text::new(
            format!(
                "{}  {}  {}  {}",
                key_hint("tui.select.confirm", "submit"),
                key_hint("tui.input.newLine", "newline"),
                key_hint("tui.select.cancel", "cancel"),
                key_hint("app.editor.external", "external editor")
            ),
            1,
            0,
        )));
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(dynamic_border()));

        Self {
            container,
            editor,
            tui,
            keybindings,
            external_editor_command,
            on_cancel,
            focused: false,
        }
    }

    fn editor_text(&mut self) -> String {
        self.editor
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Editor>())
            .map(|editor| editor.get_text())
            .unwrap_or_default()
    }

    fn set_editor_text(&mut self, text: &str) {
        if let Some(editor) = self
            .editor
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Editor>())
        {
            editor.set_text(text);
        }
    }

    fn handle_open_external_editor(&mut self) {
        let content = self.editor_text();
        self.tui.suspend();
        let result = edit_in_external_editor(&ExternalEditorOptions {
            command: self.external_editor_command.clone(),
            content,
        })
        .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error));
        if let ExternalEditorResult::Complete { content } = result {
            self.set_editor_text(&content);
        }
        self.tui.resume();
        self.tui.request_forced_render();
    }
}

impl Component for ExtensionEditorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.container.render(width)
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = get_keybindings();
        if keybindings.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
            return;
        }

        if self.keybindings.matches(data, "app.editor.external") {
            self.handle_open_external_editor();
            return;
        }

        self.editor.borrow_mut().handle_input(data);
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}

impl Focusable for ExtensionEditorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(editor) = self
            .editor
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Editor>())
        {
            editor.set_focused(focused);
        }
    }
}
