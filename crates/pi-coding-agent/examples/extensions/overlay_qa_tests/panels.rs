use super::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::utils::{truncate_to_width_with, visible_width};
use pi_tui::{Component, Focusable};
use std::rc::Rc;
pub(super) const LABELS: [&str; 3] = ["Alpha", "Beta", "Gamma"];
pub(super) const STREAM_LABELS: [&str; 3] = ["Panel A", "Panel B", "Panel C"];
pub(super) const COLORS: [ThemeColor; 3] = [ThemeColor::Error, ThemeColor::Success, ThemeColor::Accent];
pub(super) struct Entry {
    pub index: usize,
    pub closed: Rc<Cell<bool>>,
    pub handle: Rc<dyn ExtensionUiOverlayHandle>,
}
struct Panel {
    controller: Weak<Model>,
    index: usize,
    focused: Cell<bool>,
    input: RefCell<Input>,
    typed: RefCell<Vec<u16>>,
    inputs: RefCell<Vec<JsString>>,
    handle: Rc<RefCell<Option<Rc<dyn ExtensionUiOverlayHandle>>>>,
}
pub(super) fn mount(model: &Rc<Model>) {
    let count = if model.demo == Demo::Passive { 1 } else { 3 };
    for index in 0..count {
        let slot = Rc::new(RefCell::new(None));
        let component = Panel {
            controller: Rc::downgrade(model),
            index,
            focused: Cell::new(false),
            input: RefCell::new(Input::new(InputOptions::default())),
            typed: RefCell::new(Vec::new()),
            inputs: RefCell::new(Vec::new()),
            handle: slot.clone(),
        };
        let mut options = ExtensionUiOverlayOptions {
            non_capturing: true,
            ..Default::default()
        };
        match model.demo {
            Demo::Passive => {
                options.anchor = Some(Anchor::TopRight);
                options.width = absolute(22.);
                options.margin = Some(OverlayMargin {
                    top: Some(1),
                    right: Some(2),
                    ..Default::default()
                });
            }
            Demo::Focus => {
                options.width = absolute(34.);
                options.row = absolute([2., 5., 8.][index]);
                options.col = absolute([4., 28., 52.][index]);
            }
            Demo::Streaming => {
                options.width = absolute(35.);
                options.row = absolute((1 + index * 9) as f64);
                options.col = absolute(2.);
            }
            _ => unreachable!(),
        }
        let handle = model.tui.show_overlay(Box::new(component), Some(options));
        *slot.borrow_mut() = Some(handle.clone());
        model.entries.borrow_mut().push(Entry {
            index,
            closed: Rc::new(Cell::new(false)),
            handle,
        });
    }
    if model.demo == Demo::Focus {
        model.focus_first();
    }
}
impl Panel {
    fn focus_input(&self, model: &Model, data: &JsString) {
        if matches_key(data, "tab") {
            model.focus_next(self.index, 1);
            return;
        }
        if matches_key(data, "shift+tab") {
            model.focus_next(self.index, -1);
            return;
        }
        if matches_key(data, "escape") || matches_key(data, "ctrl+d") {
            model.dismiss(self.index);
            return;
        }
        if matches_key(data, "ctrl+c") {
            model.close();
            return;
        }
        let label = if matches_key(data, "return") {
            "Enter".into()
        } else if matches_key(data, "up") {
            "↑".into()
        } else if matches_key(data, "down") {
            "↓".into()
        } else {
            self.input.borrow_mut().handle_input(data);
            if matches_key(data, "left") {
                "←".into()
            } else if matches_key(data, "right") {
                "→".into()
            } else if matches_key(data, "backspace") {
                "Backspace".into()
            } else {
                serde_json::to_string(data).expect("input JSON")
            }
        };
        self.inputs.borrow_mut().push(JsString::from(label));
    }
    fn focus_render(&self, model: &Model, width: usize) -> Vec<JsString> {
        let inner = width.saturating_sub(2).max(1);
        let theme = &model.theme;
        let focused = self.focused.get();
        let color = if focused { COLORS[self.index] } else { ThemeColor::Dim };
        let fg = |c, s: &str| theme.fg(c, s);
        let mut input = self.input.borrow_mut();
        input.set_focused(focused);
        let input_line = input
            .render(inner.saturating_sub(8).max(1))
            .first()
            .cloned()
            .unwrap_or_default();
        let keys = self.inputs.borrow();
        let recent = if keys.is_empty() {
            "(none)".into()
        } else {
            JsString::join(&keys[keys.len().saturating_sub(6)..], " ")
        };
        let content = [
            js_format!(
                " {} {}",
                fg(COLORS[self.index], LABELS[self.index]),
                if focused {
                    fg(ThemeColor::Success, "FOCUSED")
                } else {
                    fg(ThemeColor::Dim, "visible")
                }
            ),
            "".into(),
            js_format!(" Input: {}", input_line),
            js_format!(" Keys: {}", recent),
            fg(ThemeColor::Dim, " Tab/Shift+Tab focus"),
            fg(ThemeColor::Dim, " Esc/Ctrl+D dismiss"),
        ];
        let mut result = vec![fg(color, &format!("╭{}╮", "─".repeat(inner)))];
        result.extend(content.iter().map(|s| render::row(model, s, inner, color)));
        result.push(fg(color, &format!("╰{}╯", "─".repeat(inner))));
        result
    }
    fn streaming_render(&self, model: &Model, width: usize) -> Vec<JsString> {
        let inner = width.saturating_sub(2).max(1);
        let theme = &model.theme;
        let color = COLORS[self.index];
        let fg = |c, s: &str| theme.fg(c, s);
        let focused = self.handle.borrow().as_ref().is_some_and(|h| h.is_focused());
        let typed = self.typed.borrow();
        let display = if typed.is_empty() {
            fg(ThemeColor::Dim, "(type here)")
        } else {
            JsString::from_utf16(typed.clone())
        };
        let truncated = truncate_to_width_with(&js_format!(" > {}", display), inner, "...", true);
        let content = [
            js_format!(" {}", fg(ThemeColor::Accent, STREAM_LABELS[self.index])),
            "".into(),
            if focused {
                fg(ThemeColor::Success, " ● FOCUSED")
            } else {
                fg(ThemeColor::Dim, " ○ unfocused")
            },
            if focused {
                fg(ThemeColor::Dim, " (receiving input)")
            } else {
                "".into()
            },
            truncated,
            "".into(),
            fg(ThemeColor::Dim, " Tab | Esc"),
        ];
        let mut result = vec![fg(color, &format!("╭{}╮", "─".repeat(inner)))];
        result.extend(content.iter().map(|s| {
            js_format!(
                "{}{}{}{}",
                fg(color, "│"),
                s,
                " ".repeat(inner.saturating_sub(visible_width(s))),
                fg(color, "│")
            )
        }));
        result.push(fg(color, &format!("╰{}╯", "─".repeat(inner))));
        result
    }
}
impl ExtensionUiComponent for Panel {
    fn focused(&self) -> Option<bool> {
        self.controller
            .upgrade()
            .filter(|m| m.demo == Demo::Focus)
            .map(|_| self.focused.get())
    }
    fn set_focused(&self, focused: bool) {
        self.focused.set(focused);
    }
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        let Some(model) = self.controller.upgrade() else { return };
        match model.demo {
            Demo::Focus => self.focus_input(&model, data),
            Demo::Streaming => {
                if matches_key(data, "tab") {
                    model.cycle_streaming();
                } else if matches_key(data, "escape") || matches_key(data, "ctrl+c") {
                    model.close();
                } else {
                    edit_text(&mut self.typed.borrow_mut(), data);
                }
            }
            _ => {}
        }
    }
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let Some(model) = self.controller.upgrade() else {
            return Vec::new();
        };
        match model.demo {
            Demo::Passive => {
                let seconds = model.state.borrow().seconds;
                (render::boxed(
                    &model,
                    vec![
                        js_format!(
                            " {}",
                            model
                                .theme
                                .fg(ThemeColor::Accent, format!("{:02}:{:02}", seconds / 60, seconds % 60))
                        ),
                        model.theme.fg(ThemeColor::Dim, " nonCapturing: true"),
                    ],
                    width,
                    "Timer",
                ))
                .into_iter()
                .collect::<Vec<_>>()
            }
            Demo::Focus => self.focus_render(&model, width).into_iter().collect::<Vec<_>>(),
            Demo::Streaming => self.streaming_render(&model, width).into_iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        }
    }
    fn invalidate(&self) {}
}
