//! Opt-in native port of `rainbow-editor.ts`.
use super::editor_composition::{ComposedEditor, EditorBehavior};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::components::custom_editor::CustomEditor;
use pi_js_compat::js_json::JsString;
use pi_tui::editor_component::EditorInput;
use pi_tui::{Component, EditorComponent};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

const COLORS: [[u8; 3]; 7] = [
    [233, 137, 115],
    [228, 186, 103],
    [141, 192, 122],
    [102, 194, 179],
    [121, 157, 207],
    [157, 134, 195],
    [206, 130, 172],
];
struct Rainbow {
    owner: std::rc::Weak<RefCell<ComposedEditor<Rainbow>>>,
    tui: ExtensionUiTui,
    frame: Arc<AtomicUsize>,
    timer: Option<tokio::task::JoinHandle<()>>,
}
fn colorize(text: &JsString, shine: Option<usize>) -> JsString {
    let mut result = JsString::default();
    for (i, unit) in text.units().iter().enumerate() {
        let factor = match shine.map(|p| i.abs_diff(p)) {
            Some(0) => 0.7,
            Some(1) => 0.35,
            _ => 0.0,
        };
        let rgb = COLORS[i % COLORS.len()].map(|c| (f64::from(c) + (255.0 - f64::from(c)) * factor).round() as u8);
        result.push_str(pi_js_compat::js_format!("\x1b[38;2;{};{};{}m", rgb[0], rgb[1], rgb[2]));
        result.push_unit(*unit);
    }
    result.push_str("\x1b[0m");
    result
}
impl Rainbow {
    fn update_animation(&mut self, text: &JsString) {
        let has_word = pi_js_compat::js_regex::JsRegex::with_flags("ultrathink", "i")
            .expect("constant word pattern")
            .is_match(text);
        if has_word && self.timer.is_none() {
            let frame = self.frame.clone();
            let tui = self.tui.clone();
            self.timer = Some(tokio::spawn(async move {
                let period = Duration::from_millis(60);
                let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
                loop {
                    ticks.tick().await;
                    frame.fetch_add(1, Ordering::SeqCst);
                    tui.request_render();
                }
            }));
        } else if !has_word && let Some(timer) = self.timer.take() {
            timer.abort();
        }
    }
}
impl EditorBehavior for Rainbow {
    fn input(&mut self, editor: &mut CustomEditor, data: &JsString) -> EditorInput {
        let input = editor.handle_input_deferred(data);
        if matches!(input, EditorInput::Complete) {
            self.update_animation(&editor.get_text());
            return input;
        }
        // Inspect text only after every callback and base-editor continuation.
        let owner = self.owner.clone();
        input.then(move || {
            if let Some(owner) = owner.upgrade() {
                let mut owner = owner.borrow_mut();
                let text = owner.editor.get_text();
                owner.behavior.update_animation(&text);
            }
        })
    }
    fn render(&mut self, editor: &mut CustomEditor, width: usize) -> Vec<JsString> {
        let cycle = self.frame.load(Ordering::SeqCst) % 20;
        let shine = (cycle < 10).then_some(cycle);
        editor
            .render(width)
            .into_iter()
            .map(|line| {
                // ASCII case-insensitive matching preserves original spelling and ANSI.
                let pattern =
                    pi_js_compat::js_regex::JsRegex::with_flags("ultrathink", "i").expect("constant word pattern");
                let mut result = JsString::default();
                let mut from = 0;
                while let Some(found) = pattern.find_from(&line, from) {
                    let start = found.start();
                    result.push_str(line.slice(from, start));
                    from = start + 10;
                    result.push_str(colorize(&line.slice(start, from), shine));
                }
                result.push_str(line.slice(from, line.len()));
                result
            })
            .collect()
    }
}

pub fn rainbow_editor_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_session_start(
        |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                ctx.ui()?.set_editor_component(Some(Arc::new(|tui, theme, kb| {
                    Rc::new_cyclic(|owner| {
                        RefCell::new(ComposedEditor {
                            editor: CustomEditor::new(Rc::new(tui.clone()), theme, kb, Default::default())
                                .expect("default editor has no history DB"),
                            behavior: Rainbow {
                                owner: owner.clone(),
                                tui,
                                frame: Arc::new(AtomicUsize::new(0)),
                                timer: None,
                            },
                        })
                    })
                })));
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
