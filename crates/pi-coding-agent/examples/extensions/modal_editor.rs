//! Opt-in native port of `modal-editor.ts`.
use super::editor_composition::{ComposedEditor, EditorBehavior};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::components::custom_editor::CustomEditor;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_tui::editor_component::EditorInput;
use pi_tui::tui::Component;
use pi_tui::utils::truncate_to_width_with;
use pi_tui::{EditorComponent, matches_key, visible_width};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

struct Modal {
    normal: bool,
}
impl EditorBehavior for Modal {
    fn input(&mut self, editor: &mut CustomEditor, data: &JsString) -> EditorInput {
        if matches_key(data, "escape") {
            if self.normal {
                return editor.handle_input_deferred(&JsString::from(data));
            }
            self.normal = true;
            return EditorInput::Complete;
        }
        if !self.normal {
            return editor.handle_input_deferred(&JsString::from(data));
        }
        let sequence = match data {
            data if data == "h" => JsString::from("\x1b[D"),
            data if data == "j" => JsString::from("\x1b[B"),
            data if data == "k" => JsString::from("\x1b[A"),
            data if data == "l" => JsString::from("\x1b[C"),
            data if data == "0" => JsString::from("\x01"),
            data if data == "$" => JsString::from("\x05"),
            data if data == "x" => JsString::from("\x1b[3~"),
            data if data == "i" => {
                self.normal = false;
                return EditorInput::Complete;
            }
            data if data == "a" => {
                self.normal = false;
                JsString::from("\x1b[C")
            }
            _ if data.len() == 1 && data.units()[0] >= u16::from(b' ') => {
                return EditorInput::Complete;
            }
            _ => data.clone(),
        };
        editor.handle_input_deferred(&sequence)
    }
    fn render(&mut self, editor: &mut CustomEditor, width: usize) -> Vec<JsString> {
        let mut lines = editor.render(width);
        let label = if self.normal { " NORMAL " } else { " INSERT " };
        if let Some(last) = lines.last_mut()
            && visible_width(&*last) >= label.len()
        {
            *last = js_format!(
                "{}{}",
                truncate_to_width_with(&*last, width.saturating_sub(label.len()), "", false),
                label
            );
        }
        lines
    }
}

pub fn modal_editor_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_session_start(
        |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                ctx.ui()?.set_editor_component(Some(Arc::new(|tui, theme, kb| {
                    Rc::new(RefCell::new(ComposedEditor {
                        editor: CustomEditor::new(Rc::new(tui), theme, kb, Default::default())
                            .expect("default editor has no history DB"),
                        behavior: Modal { normal: false },
                    }))
                })));
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
