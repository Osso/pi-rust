//! Opt-in native port of `modal-editor.ts`.
use super::editor_composition::{ComposedEditor, EditorBehavior};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::components::custom_editor::CustomEditor;
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
    fn input(&mut self, editor: &mut CustomEditor, data: &str) -> Vec<Rc<dyn Fn()>> {
        if matches_key(data, "escape") {
            if self.normal {
                return editor.handle_input_deferred(data);
            }
            self.normal = true;
            return Vec::new();
        }
        if !self.normal {
            return editor.handle_input_deferred(data);
        }
        let sequence = match data {
            "h" => "\x1b[D",
            "j" => "\x1b[B",
            "k" => "\x1b[A",
            "l" => "\x1b[C",
            "0" => "\x01",
            "$" => "\x05",
            "x" => "\x1b[3~",
            "i" => {
                self.normal = false;
                return Vec::new();
            }
            "a" => {
                self.normal = false;
                "\x1b[C"
            }
            _ if data.encode_utf16().count() == 1 && data.chars().next().is_some_and(|c| c >= ' ') => {
                return Vec::new();
            }
            _ => data,
        };
        editor.handle_input_deferred(sequence)
    }
    fn render(&mut self, editor: &mut CustomEditor, width: usize) -> Vec<String> {
        let mut lines = editor.render(width);
        let label = if self.normal { " NORMAL " } else { " INSERT " };
        if let Some(last) = lines.last_mut()
            && visible_width(last) >= label.len()
        {
            *last = format!(
                "{}{label}",
                truncate_to_width_with(last, width.saturating_sub(label.len()), "", false)
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
