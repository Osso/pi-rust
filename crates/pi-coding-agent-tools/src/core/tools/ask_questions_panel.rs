//! Frozen tabbed `ask-questions-panel.ts`; UI-local state, no session/global IO.
use super::ask_questions::Question;
use crate::core::extensions::types::{ExtensionUiComponent, ExtensionUiDone, ExtensionUiFactoryContext};
use crate::modes::interactive::theme::{ThemeBg, ThemeColor};
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value};
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::tui::{Component, Focusable};
use pi_tui::utils::{truncate_to_width, wrap_text_with_ansi};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Default)]
struct Answer {
    selected: Vec<JsString>,
    custom: Option<JsString>,
}
impl Answer {
    fn has_answer(&self) -> bool {
        !self.selected.is_empty() || self.custom.is_some()
    }
    fn format(&self, question: &Question) -> JsString {
        if question.options.is_none() || question.multi_select != Some(true) {
            return self
                .custom
                .clone()
                .or_else(|| self.selected.first().cloned())
                .unwrap_or_default();
        }
        let mut selected = question
            .options
            .iter()
            .flatten()
            .filter(|option| self.selected.contains(&option.label))
            .map(|option| option.label.clone())
            .collect::<Vec<_>>();
        selected.extend(self.custom.clone());
        JsString::join(selected, ", ")
    }
}
#[cfg(test)]
mod raw_answer_tests {
    use super::super::ask_questions::QuestionOption;
    use super::*;

    #[test]
    fn custom_and_multi_select_answers_keep_original_units() {
        let raw = JsString::from_utf16(vec![0xd800]);
        let label = JsString::from("choice-") + &raw;
        let custom = JsString::from("other-") + &raw;
        let question = Question {
            question: "question".into(),
            header: None,
            options: Some(vec![QuestionOption {
                label: label.clone(),
                description: None,
            }]),
            multi_select: Some(true),
        };
        let answer = Answer {
            selected: vec![label.clone()],
            custom: Some(custom.clone()),
        };
        assert_eq!(answer.format(&question), label + ", " + custom);
    }
}

enum InputAction {
    Submit(JsString),
    Escape,
}
struct State {
    answers: Vec<Answer>,
    cursors: Vec<usize>,
    inputs: Vec<Input>,
    tab: usize,
    editing: bool,
    finished: bool,
}
pub struct QuestionPanel {
    questions: Vec<Question>,
    context: ExtensionUiFactoryContext,
    done: ExtensionUiDone,
    focused: Cell<bool>,
    state: RefCell<State>,
    action: Rc<RefCell<Option<InputAction>>>,
    has_submit: bool,
}
impl QuestionPanel {
    pub fn new(questions: Vec<Question>, context: ExtensionUiFactoryContext, done: ExtensionUiDone) -> Self {
        let action = Rc::new(RefCell::new(None));
        let inputs = questions
            .iter()
            .map(|_| {
                let mut input = Input::new(InputOptions::default());
                let submit = action.clone();
                input.on_submit = Some(Rc::new(move |text| {
                    *submit.borrow_mut() = Some(InputAction::Submit(text.to_owned()))
                }));
                let escape = action.clone();
                input.on_escape = Some(Rc::new(move || *escape.borrow_mut() = Some(InputAction::Escape)));
                input
            })
            .collect();
        let state = State {
            answers: questions.iter().map(|_| Answer::default()).collect(),
            cursors: vec![0; questions.len()],
            inputs,
            tab: 0,
            editing: questions.first().is_some_and(|question| question.options.is_none()),
            finished: false,
        };
        let has_submit = questions.len() > 1
            || questions
                .first()
                .is_some_and(|question| question.multi_select == Some(true));
        Self {
            questions,
            context,
            done,
            focused: Cell::new(false),
            state: RefCell::new(state),
            action,
            has_submit,
        }
    }
    fn tab_count(&self) -> usize {
        self.questions.len() + usize::from(self.has_submit)
    }
    fn is_submit(&self, state: &State) -> bool {
        self.has_submit && state.tab == self.questions.len()
    }
    fn is_free_text(&self, state: &State) -> bool {
        !self.is_submit(state) && self.questions[state.tab].options.is_none()
    }
    fn can_submit(&self, state: &State) -> bool {
        self.questions.iter().zip(&state.answers).all(|(question, answer)| {
            (question.options.is_some() && question.multi_select == Some(true)) || answer.has_answer()
        })
    }
    fn finish(&self, state: &mut State, cancelled: bool) {
        if state.finished {
            return;
        }
        state.finished = true;
        let mut answers = Map::new();
        for (question, answer) in self.questions.iter().zip(&state.answers) {
            // Frozen collects with assignment on {}, whose inherited __proto__ setter
            // ignores string values instead of creating an own answer property.
            if question.question != "__proto__" && (!cancelled || answer.has_answer()) {
                answers.insert(question.question.clone(), Value::String(answer.format(question)));
            }
        }
        self.done
            .call(Some(json!({"answers":Value::Object(answers),"cancelled":cancelled})));
    }
    fn move_to_tab(&self, state: &mut State, next: usize) {
        state.tab = next.min(self.tab_count() - 1);
        state.editing = self.is_free_text(state);
    }
    fn advance(&self, state: &mut State) {
        if !self.has_submit {
            self.finish(state, false);
        } else {
            self.move_to_tab(state, state.tab + 1);
        }
    }
    fn activate_row(&self, state: &mut State) {
        let tab = state.tab;
        let option = self.questions[tab]
            .options
            .as_ref()
            .and_then(|options| options.get(state.cursors[tab]));
        let Some(option) = option else {
            state.editing = true;
            state.inputs[tab].set_value(state.answers[tab].custom.clone().unwrap_or_default());
            return;
        };
        if self.questions[tab].multi_select == Some(true) {
            let selected = &mut state.answers[tab].selected;
            if let Some(index) = selected.iter().position(|label| label == &option.label) {
                selected.remove(index);
            } else {
                selected.push(option.label.clone());
            }
        } else {
            state.answers[tab] = Answer {
                selected: vec![option.label.clone()],
                custom: None,
            };
            self.advance(state);
        }
    }
    fn edit_answer(&self, state: &mut State, data: &JsString) {
        let free = self.is_free_text(state);
        let kb = &self.context.keybindings;
        if free && kb.matches(data, "app.questions.next") && !kb.matches(data, "tui.editor.cursorRight") {
            self.move_to_tab(state, state.tab + 1);
            return;
        }
        if free && kb.matches(data, "app.questions.previous") && !kb.matches(data, "tui.editor.cursorLeft") {
            self.move_to_tab(state, state.tab.saturating_sub(1));
            return;
        }
        let tab = state.tab;
        state.inputs[tab].handle_input(data);
        let action = self.action.borrow_mut().take();
        match action {
            Some(InputAction::Submit(value)) => {
                let text = value.trim();
                state.answers[tab].custom = (!text.is_empty()).then_some(text.clone());
                if free && text.is_empty() {
                    return;
                }
                state.editing = false;
                if self.questions[tab].multi_select != Some(true) && !text.is_empty() {
                    state.answers[tab].selected.clear();
                    self.advance(state);
                }
                self.context.tui.request_render();
            }
            Some(InputAction::Escape) => {
                if free {
                    self.finish(state, true);
                } else {
                    state.editing = false;
                }
                self.context.tui.request_render();
            }
            None => {}
        }
        if free {
            let text = state.inputs[tab].get_value().trim();
            state.answers[tab].custom = (!text.is_empty()).then(|| text.to_owned());
        }
    }
    fn handle_key(&self, state: &mut State, data: &JsString) {
        if state.editing {
            self.edit_answer(state, data);
            return;
        }
        let kb = &self.context.keybindings;
        if kb.matches(data, "app.questions.next") {
            self.move_to_tab(state, state.tab + 1);
        } else if kb.matches(data, "app.questions.previous") {
            self.move_to_tab(state, state.tab.saturating_sub(1));
        } else if kb.matches(data, "tui.select.cancel") {
            self.finish(state, true);
        } else if self.is_submit(state) {
            if kb.matches(data, "tui.select.confirm") && self.can_submit(state) {
                self.finish(state, false);
            }
        } else if kb.matches(data, "tui.select.up") {
            state.cursors[state.tab] = state.cursors[state.tab].saturating_sub(1);
        } else if kb.matches(data, "tui.select.down") {
            state.cursors[state.tab] =
                (state.cursors[state.tab] + 1).min(self.questions[state.tab].options.as_ref().map_or(0, Vec::len));
        } else if kb.matches(data, "tui.select.confirm") {
            self.activate_row(state);
        }
    }
    fn render_tab_bar(&self, state: &State) -> JsString {
        let theme = &self.context.theme;
        let mut labels = self
            .questions
            .iter()
            .enumerate()
            .map(|(index, question)| {
                let marker = if state.answers[index].has_answer() {
                    "■"
                } else {
                    "□"
                };
                let header = question.header.clone().unwrap_or_else(|| js_format!("Q{}", index + 1));
                let text = js_format!(" {} {} ", marker, header);
                if index == state.tab {
                    theme.bg(ThemeBg::SelectedBg, theme.fg(ThemeColor::Text, &text))
                } else {
                    theme.fg(ThemeColor::Muted, &text)
                }
            })
            .collect::<Vec<_>>();
        let text = " ✓ Submit ";
        labels.push(if self.is_submit(state) {
            theme.bg(ThemeBg::SelectedBg, theme.fg(ThemeColor::Text, text))
        } else {
            theme.fg(
                if self.can_submit(state) {
                    ThemeColor::Success
                } else {
                    ThemeColor::Dim
                },
                text,
            )
        });
        js_format!(" {}", JsString::join(&labels, " "))
    }
    fn render_options(&self, state: &State, width: usize) -> Vec<JsString> {
        let question = &self.questions[state.tab];
        let Some(options) = &question.options else {
            return Vec::new();
        };
        let answer = &state.answers[state.tab];
        let theme = &self.context.theme;
        let mut rows = options
            .iter()
            .map(|option| {
                let selected = answer.selected.contains(&option.label);
                let marker = if question.multi_select == Some(true) {
                    if selected { "[x] " } else { "[ ] " }
                } else if selected {
                    "● "
                } else {
                    "○ "
                };
                let description = option
                    .description
                    .as_ref()
                    .filter(|text| !text.is_empty())
                    .map(|text| theme.fg(ThemeColor::Muted, &js_format!(" — {}", text)))
                    .unwrap_or_default();
                (marker, option.label.clone(), description)
            })
            .enumerate()
            .map(|(index, (marker, label, description))| {
                js_format!("{}{}. {}{}", marker, index + 1, label, description)
            })
            .collect::<Vec<_>>();
        rows.push(
            answer
                .custom
                .as_ref()
                .map(|text| js_format!("✎ Other: {}", text))
                .unwrap_or_else(|| "  Other".into()),
        );
        rows.into_iter()
            .enumerate()
            .map(|(index, text)| {
                let active = index == state.cursors[state.tab];
                let prefix = if active {
                    theme.fg(ThemeColor::Accent, "> ")
                } else {
                    "  ".into()
                };
                let text = if active {
                    theme.fg(ThemeColor::Accent, &text)
                } else {
                    text
                };
                truncate_to_width(&js_format!(" {}{}", prefix, text), width)
            })
            .collect()
    }
    fn render_body(&self, state: &mut State, width: usize) -> Vec<JsString> {
        let theme = &self.context.theme;
        if self.is_submit(state) {
            let mut lines = self
                .questions
                .iter()
                .zip(&state.answers)
                .map(|(question, answer)| {
                    let value = if answer.has_answer() {
                        answer.format(question)
                    } else {
                        theme.fg(ThemeColor::Warning, "—")
                    };
                    truncate_to_width(
                        &js_format!(
                            "   {}→ {}",
                            theme.fg(ThemeColor::Muted, &js_format!("{} ", question.question)),
                            value
                        ),
                        width,
                    )
                })
                .collect::<Vec<_>>();
            let status = if self.can_submit(state) {
                theme.fg(ThemeColor::Success, "Enter to submit")
            } else {
                theme.fg(ThemeColor::Warning, "Answer every question to submit")
            };
            lines.extend([JsString::default(), js_format!("   {}", status)]);
            return lines;
        }
        let mut lines = wrap_text_with_ansi(
            theme.fg(ThemeColor::Text, &self.questions[state.tab].question),
            width.saturating_sub(2).max(1),
        )
        .into_iter()
        .map(|line| js_format!(" {}", line))
        .collect::<Vec<_>>();
        lines.push(JsString::default());
        lines.extend(self.render_options(state, width));
        if state.editing {
            let input = &mut state.inputs[state.tab];
            input.set_focused(self.focused.get());
            lines.extend([
                JsString::default(),
                js_format!(" {}", theme.fg(ThemeColor::Muted, "Your answer:")),
            ]);
            lines.extend(input.render(width.saturating_sub(2).max(1)));
        }
        lines
    }
    fn help(&self, state: &State) -> JsString {
        if self.is_free_text(state) {
            return js_format!(
                "{}Enter save • Esc cancel",
                if self.tab_count() > 1 {
                    "Tab/Shift+Tab questions • "
                } else {
                    ""
                }
            );
        }
        if state.editing {
            return "Enter save • Esc back".into();
        }
        js_format!(
            "{}↑↓ move • Enter {} • Esc cancel",
            if self.tab_count() > 1 {
                "←→ questions • "
            } else {
                ""
            },
            if self
                .questions
                .get(state.tab)
                .is_some_and(|question| question.multi_select == Some(true))
            {
                "toggle"
            } else {
                "select"
            }
        )
    }
}
impl ExtensionUiComponent for QuestionPanel {
    fn focused(&self) -> Option<bool> {
        Some(self.focused.get())
    }
    fn set_focused(&self, value: bool) {
        self.focused.set(value);
    }
    fn handle_input(&self, data: &JsString) {
        self.handle_key(&mut self.state.borrow_mut(), data);
        self.context.tui.request_render();
    }
    fn render(&self, width: usize) -> Vec<JsString> {
        let mut state = self.state.borrow_mut();
        let rule = self.context.theme.fg(ThemeColor::Accent, "─".repeat(width.max(1)));
        let mut lines = vec![rule.clone()];
        if self.tab_count() > 1 {
            lines.extend([self.render_tab_bar(&state), JsString::default()]);
        }
        lines.extend(self.render_body(&mut state, width));
        lines.extend([
            JsString::default(),
            js_format!(" {}", self.context.theme.fg(ThemeColor::Dim, self.help(&state))),
            rule,
        ]);
        lines
    }
}
