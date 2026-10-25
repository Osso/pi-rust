//! Ported from `coding-agent/src/modes/interactive/components/extension-editor.ts`.

use std::cell::RefCell;
use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::rc::Rc;

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use pi_tui::components::editor::{Editor, EditorHost, EditorOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, Focusable, TuiExt, TuiRenderer, component};

use super::dynamic_border::dynamic_border;
use crate::core::keybindings::KeybindingsManager as AppKeybindingsManager;
use crate::core::tools::node_fs::{node_fs_error, node_read_error};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{ThemeColor, get_editor_theme, with_theme};
use crate::utils::uncaught::throw_callback_uncaught;

/// Host surface needed to suspend the TUI around an external editor.
pub trait ExtensionEditorHost: EditorHost {
    fn suspend(&self) -> Result<(), CallbackError>;
    fn resume(&self) -> Result<(), CallbackError>;
    fn request_forced_render(&self);
}

impl ExtensionEditorHost for Rc<TuiRenderer> {
    fn suspend(&self) -> Result<(), CallbackError> {
        TuiExt::stop(self.as_ref())
    }

    fn resume(&self) -> Result<(), CallbackError> {
        TuiExt::start(self.as_ref())
    }

    fn request_forced_render(&self) {
        TuiExt::request_render(self.as_ref(), true);
    }
}

/// Multi-line editor component for extensions.
pub struct ExtensionEditorComponent {
    container: Container,
    editor: Rc<RefCell<Editor>>,
    tui: Rc<dyn ExtensionEditorHost>,
    keybindings: AppKeybindingsManager,
    external_editor_command: Option<JsString>,
    on_cancel: Rc<dyn Fn()>,
    focused: bool,
}

/// Editor configuration independent of the dialog callbacks.
pub struct ExtensionEditorOptions {
    pub editor: EditorOptions,
    pub external_editor_command: Option<JsString>,
}

impl ExtensionEditorComponent {
    pub fn new(
        tui: Rc<dyn ExtensionEditorHost>,
        keybindings: AppKeybindingsManager,
        title: &JsString,
        prefill: Option<&JsString>,
        on_submit: Rc<dyn Fn(JsString)>,
        on_cancel: Rc<dyn Fn()>,
        options: ExtensionEditorOptions,
    ) -> Self {
        let ExtensionEditorOptions {
            editor: options,
            external_editor_command,
        } = options;
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
        if let Some(prefill) = prefill.filter(|text| !text.is_empty()) {
            editor.set_text(prefill);
        }
        editor.on_submit = Some(Rc::new(move |text: &JsString| on_submit(text.clone())));
        let editor = Rc::new(RefCell::new(editor));
        let editor_child: ComponentRef = editor.clone();
        container.add_child(editor_child);

        container.add_child(component(Spacer::new(1)));
        // The frozen resolver always supplies a nonempty platform default.
        container.add_child(component(Text::new(
            js_format!(
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

    fn get_external_editor_command(&self) -> JsString {
        self.external_editor_command
            .clone()
            .filter(|command| !command.is_empty())
            .or_else(|| {
                pi_js_compat::process_env::get("VISUAL")
                    .filter(|value| !value.is_empty())
                    .map(JsString::from)
            })
            .or_else(|| {
                pi_js_compat::process_env::get("EDITOR")
                    .filter(|value| !value.is_empty())
                    .map(JsString::from)
            })
            .unwrap_or_else(|| {
                if cfg!(target_os = "windows") {
                    "notepad".into()
                } else {
                    "nano".into()
                }
            })
    }

    fn handle_open_external_editor(&mut self) {
        let command = self.get_external_editor_command();
        let content = self.editor.borrow().get_text();
        let millis = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_millis() as i128,
            Err(error) => -(error.duration().as_millis() as i128),
        };
        let name = format!("pi-extension-editor-{millis}.md");
        let file = extension_editor_temp_file(&name);
        if let Err(error) = begin_external_editor(self.tui.as_ref(), &file, &content, &command) {
            let error = match finish_external_editor(self.tui.as_ref(), &file) {
                Ok(()) => error,
                Err(finally_error) => finally_error,
            };
            // An async source function rejects later even when its pre-await work throws.
            tokio::task::spawn_local(async move {
                throw_callback_uncaught(error);
            });
            return;
        }
        let child = spawn_external_editor(&command, &file);
        let editor = Rc::clone(&self.editor);
        let tui = Rc::clone(&self.tui);
        tokio::task::spawn_local(complete_external_editor(child, file, editor, tui));
    }
}

fn extension_editor_temp_file(name: &str) -> PathBuf {
    let nonempty_env = |key| pi_js_compat::process_env::get(key).filter(|value| !value.is_empty());
    if cfg!(target_os = "windows") {
        let directory = nonempty_env("TEMP").or_else(|| nonempty_env("TMP")).unwrap_or_else(|| {
            let root = nonempty_env("SystemRoot").or_else(|| nonempty_env("windir"));
            // This is Node's literal undefined + '\\temp' result when both are absent.
            format!("{}\\temp", root.as_deref().unwrap_or("undefined"))
        });
        PathBuf::from(directory).join(name)
    } else {
        let directory = nonempty_env("TMPDIR")
            .or_else(|| nonempty_env("TMP"))
            .or_else(|| nonempty_env("TEMP"))
            .unwrap_or_else(|| "/tmp".into());
        let path = crate::utils::paths::node_join(directory, name);
        PathBuf::from(encode_node_utf8(&path))
    }
}

async fn complete_external_editor(
    child: Result<Option<tokio::process::Child>, CallbackError>,
    file: PathBuf,
    editor: Rc<RefCell<Editor>>,
    tui: Rc<dyn ExtensionEditorHost>,
) {
    let result = match child {
        Ok(child) => await_external_editor(child, &file).await,
        Err(error) => Err(error),
    };
    let result = result.map(|content| {
        if let Some(content) = content {
            editor.borrow_mut().set_text(content);
        }
    });
    let finally_result = finish_external_editor(tui.as_ref(), &file);
    if let Err(error) = finally_result.and(result) {
        throw_callback_uncaught(error);
    }
}

fn editor_failure(message: impl Into<JsString>) -> CallbackError {
    CallbackError::new(message)
}

fn begin_external_editor(
    tui: &dyn ExtensionEditorHost,
    file: &Path,
    content: &JsString,
    command: &JsString,
) -> Result<(), CallbackError> {
    std::fs::write(file, encode_node_utf8(content))
        .map_err(|error| editor_failure(node_fs_error(&error, "open", &file.display().to_string().into())))?;
    tui.suspend()?;
    std::io::stdout()
        .write_all(
            encode_node_utf8(&js_format!(
                "Launching external editor: {}\nPi will resume when the editor exits.\n",
                command
            ))
            .as_bytes(),
        )
        .map_err(|error| editor_failure(error.to_string()))?;
    Ok(())
}

fn validate_external_editor_arguments(executable: &JsString, arguments: &[JsString]) -> Result<(), CallbackError> {
    pi_js_compat::node_child_process::validate_spawn_arguments(executable, arguments)
        .map_err(|error| CallbackError::from(*error))
}

fn spawn_external_editor(command: &JsString, file: &Path) -> Result<Option<tokio::process::Child>, CallbackError> {
    let mut parts = command.split(' ');
    let executable = parts.next().expect("string splitting yields at least one part");
    let arguments: Vec<_> = parts.collect();
    validate_external_editor_arguments(&executable, &arguments)?;
    let mut process = if cfg!(target_os = "windows") {
        let mut process = tokio::process::Command::new("cmd");
        process
            .args(["/d", "/s", "/c"])
            .arg(encode_node_utf8(command))
            .arg(file);
        process
    } else {
        let mut process = tokio::process::Command::new(encode_node_utf8(&executable));
        process.args(arguments.iter().map(encode_node_utf8)).arg(file);
        process
    };
    let child = process
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();
    // Frozen child 'error' resolves null; only argument-validation throws reject.
    Ok(child.ok())
}

async fn await_external_editor(
    child: Option<tokio::process::Child>,
    file: &Path,
) -> Result<Option<JsString>, CallbackError> {
    let Some(mut child) = child else {
        return Ok(None);
    };
    let Ok(status) = child.wait().await else {
        return Ok(None);
    };
    if !status.success() {
        return Ok(None);
    }
    let bytes = std::fs::read(file)
        .map_err(|error| editor_failure(node_read_error(&error, &file.display().to_string().into())))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(Some(text.strip_suffix('\n').unwrap_or(&text).into()))
}

fn finish_external_editor(tui: &dyn ExtensionEditorHost, file: &Path) -> Result<(), CallbackError> {
    // Frozen finally deliberately ignores unlink failures.
    let _ = std::fs::remove_file(file);
    tui.resume()?;
    tui.request_forced_render();
    Ok(())
}

impl Deref for ExtensionEditorComponent {
    type Target = Container;
    fn deref(&self) -> &Self::Target {
        &self.container
    }
}
impl DerefMut for ExtensionEditorComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.container
    }
}
impl Component for ExtensionEditorComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }
    fn handle_input(&mut self, data: &JsString) {
        if get_keybindings().matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        } else if self.keybindings.matches(data, "app.editor.external") {
            self.handle_open_external_editor();
        } else {
            self.editor.borrow_mut().handle_input(data);
        }
    }
    fn invalidate(&mut self) {
        self.container.invalidate();
    }
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}
impl Focusable for ExtensionEditorComponent {
    fn focused(&self) -> bool {
        self.focused
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.editor.borrow_mut().set_focused(focused);
    }
}
