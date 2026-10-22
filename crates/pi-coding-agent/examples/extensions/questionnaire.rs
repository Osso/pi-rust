//! Opt-in native port of `examples/extensions/questionnaire.ts`.
#[path = "forms.rs"]
mod forms;
use forms::{FormEditor, editor_lines, option_lines, wrap_prefixed};
use indexmap::IndexMap;
use pi_agent_core::AgentToolResult;
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::{
    core::extensions::*,
    modes::interactive::theme::{Theme, ThemeBg as B, ThemeColor as C},
};
use pi_js_compat::string::js_trim;
use pi_tui::{
    Text,
    keys::matches_key,
    tui::{ComponentRef, component},
};
use serde_json::{Value, json};
use std::{cell::RefCell, sync::Arc};

pub fn questionnaire_extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    api.register_tool(ToolDefinition {
  name:"questionnaire".into(),label:"Questionnaire".into(),description:"Ask the user one or more questions. Use for clarifying requirements, getting preferences, or confirming decisions. For single questions, shows a simple option list. For multiple questions, shows a tab-based interface.".into(),
  parameters:json!({"type":"object","properties":{"questions":{"type":"array","description":"Questions to ask the user","items":{"type":"object","properties":{"id":{"type":"string","description":"Unique identifier for this question"},"label":{"type":"string","description":"Short contextual label for tab bar, e.g. 'Scope', 'Priority' (defaults to Q1, Q2)"},"prompt":{"type":"string","description":"The full question text to display"},"options":{"type":"array","description":"Available options to choose from","items":{"type":"object","properties":{"value":{"type":"string","description":"The value returned when selected"},"label":{"type":"string","description":"Display label for the option"},"description":{"type":"string","description":"Optional description shown below label"}},"required":["value","label"]}},"allowOther":{"type":"boolean","description":"Allow 'Type something' option (default: true)"}},"required":["id","prompt","options"]}}},"required":["questions"]}),
  prompt_snippet:None,prompt_guidelines:None,approval_required:None,approval_kind:None,constrained_sampling:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:None,
  execute:Arc::new(|_,params,_,_,ctx|Box::pin(async move{execute(params,ctx).await})),
  render_call:Some(infallible_render_call(|args,theme,_|text(render_call(args,theme)))),
  render_result:Some(infallible_render_result(|result,_,theme,_|text(render_result(result,theme)))),
 }).map_err(HandlerError::new)
}
fn result(message: String, details: Value) -> AgentToolResult {
    let mut result = AgentToolResult::text(message);
    result.details = Some(details);
    result
}
fn error(message: &str) -> AgentToolResult {
    result(message.into(), json!({"questions":[],"answers":[],"cancelled":true}))
}
async fn execute(params: Value, ctx: ExtensionContext) -> Result<AgentToolResult, HandlerError> {
    if ctx.mode()? != ExtensionMode::Tui {
        return Ok(error("Error: UI not available (running in non-interactive mode)"));
    }
    let mut questions = params["questions"].as_array().unwrap().clone();
    if questions.is_empty() {
        return Ok(error("Error: No questions provided"));
    }
    for (i, q) in questions.iter_mut().enumerate() {
        if q["label"].as_str().is_none_or(str::is_empty) {
            q["label"] = json!(format!("Q{}", i + 1));
        }
        q["allowOther"] = json!(q["allowOther"] != false);
    }
    let source = questions.clone();
    let details = ctx
        .ui()?
        .custom(
            Arc::new(move |context, done| {
                pi_coding_agent::core::extensions::ready_custom_component(Box::new(Questionnaire {
                    theme: context.theme.clone(),
                    tui: context.tui.clone(),
                    done,
                    questions: source.clone(),
                    state: RefCell::new(State {
                        tab: 0,
                        option: 0,
                        input_id: None,
                        cache: None,
                        answers: IndexMap::new(),
                        editor: FormEditor::new(&context),
                    }),
                }))
            }),
            None,
        )
        .await?
        .ok_or_else(|| HandlerError::new("questionnaire custom UI returned no result"))?;
    if details["cancelled"] == true {
        return Ok(result("User cancelled the questionnaire".into(), details));
    }
    let message = details["answers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|answer| {
            let label = questions
                .iter()
                .find(|q| q["id"] == answer["id"])
                .and_then(|q| q["label"].as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(answer["id"].as_str().unwrap());
            if answer["wasCustom"] == true {
                format!("{label}: user wrote: {}", answer["label"].as_str().unwrap())
            } else {
                format!(
                    "{label}: user selected: {}. {}",
                    answer["index"],
                    answer["label"].as_str().unwrap()
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(result(message, details))
}
struct State {
    tab: usize,
    option: i64,
    input_id: Option<String>,
    cache: Option<Vec<String>>,
    answers: IndexMap<String, Value>,
    editor: FormEditor,
}
struct Questionnaire {
    theme: Arc<Theme>,
    tui: ExtensionUiTui,
    done: ExtensionUiDone,
    questions: Vec<Value>,
    state: RefCell<State>,
}
impl Questionnaire {
    fn refresh(&self, state: &mut State) {
        state.cache = None;
        self.tui.request_render();
    }
    fn all_answered(&self, state: &State) -> bool {
        self.questions
            .iter()
            .all(|q| state.answers.contains_key(q["id"].as_str().unwrap()))
    }
    fn submit(&self, state: &State, cancelled: bool) {
        self.done.call(Some(json!({"questions":self.questions,"answers":state.answers.values().collect::<Vec<_>>(),"cancelled":cancelled})));
    }
    fn advance(&self, state: &mut State) {
        if self.questions.len() == 1 {
            self.submit(state, false);
        } else {
            state.tab = (state.tab + 1).min(self.questions.len());
            state.option = 0;
            self.refresh(state);
        }
    }
    fn options(&self, tab: usize) -> Vec<Value> {
        let Some(q) = self.questions.get(tab) else {
            return Vec::new();
        };
        let mut options = q["options"].as_array().unwrap().clone();
        if q["allowOther"] == true {
            options.push(json!({"value":"__other__","label":"Type something.","isOther":true}));
        }
        options
    }
    fn input_editor(&self, state: &mut State, data: &str) {
        if matches_key(data, "escape") {
            state.input_id = None;
            state.editor.editor.set_text("");
            self.refresh(state);
            return;
        }
        if let Some(value) = state.editor.input(data)
            && let Some(id) = state.input_id.clone().filter(|s| !s.is_empty())
        {
            let trimmed = js_trim(&value);
            let text = if trimmed.is_empty() { "(no response)" } else { trimmed };
            state
                .answers
                .insert(id.clone(), json!({"id":id,"value":text,"label":text,"wasCustom":true}));
            state.input_id = None;
            state.editor.editor.set_text("");
            self.advance(state);
        }
        self.refresh(state);
    }
    fn render_tabs(&self, lines: &mut Vec<String>, width: usize, state: &State) {
        let theme = &self.theme;
        let mut tabs = "← ".to_owned();
        for (i, q) in self.questions.iter().enumerate() {
            let answered = state.answers.contains_key(q["id"].as_str().unwrap());
            let text = format!(
                " {} {} ",
                if answered { "■" } else { "□" },
                q["label"].as_str().unwrap()
            );
            let styled = if i == state.tab {
                theme.bg(B::SelectedBg, &theme.fg(C::Text, &text))
            } else {
                theme.fg(if answered { C::Success } else { C::Muted }, &text)
            };
            tabs += &format!("{styled} ");
        }
        let submit = if state.tab == self.questions.len() {
            theme.bg(B::SelectedBg, &theme.fg(C::Text, " ✓ Submit "))
        } else {
            theme.fg(if self.all_answered(state) { C::Success } else { C::Dim }, " ✓ Submit ")
        };
        tabs += &format!("{submit} →");
        wrap_prefixed(lines, width, " ", &tabs);
        lines.push(String::new());
    }
    fn render_submit(&self, lines: &mut Vec<String>, width: usize, state: &State) {
        let theme = &self.theme;
        wrap_prefixed(lines, width, " ", &theme.fg(C::Accent, &theme.bold("Ready to submit")));
        lines.push(String::new());
        for q in &self.questions {
            if let Some(answer) = state.answers.get(q["id"].as_str().unwrap()) {
                let summary = theme.fg(C::Muted, &format!("{}: ", q["label"].as_str().unwrap()))
                    + &theme.fg(
                        C::Text,
                        &format!(
                            "{}{}",
                            if answer["wasCustom"] == true { "(wrote) " } else { "" },
                            answer["label"].as_str().unwrap()
                        ),
                    );
                wrap_prefixed(lines, width, " ", &summary);
            }
        }
        lines.push(String::new());
        if self.all_answered(state) {
            wrap_prefixed(lines, width, " ", &theme.fg(C::Success, "Press Enter to submit"));
        } else {
            let missing = self
                .questions
                .iter()
                .filter(|q| !state.answers.contains_key(q["id"].as_str().unwrap()))
                .map(|q| q["label"].as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", ");
            wrap_prefixed(
                lines,
                width,
                " ",
                &theme.fg(C::Warning, &format!("Unanswered: {missing}")),
            );
        }
    }
}
impl ExtensionUiComponent for Questionnaire {
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        let mut state = self.state.borrow_mut();
        if state.input_id.is_some() {
            self.input_editor(&mut state, data);
            return;
        }
        if self.questions.len() > 1 {
            let total = self.questions.len() + 1;
            let next = if matches_key(data, "tab") || matches_key(data, "right") {
                Some((state.tab + 1) % total)
            } else if matches_key(data, "shift+tab") || matches_key(data, "left") {
                Some((state.tab + total - 1) % total)
            } else {
                None
            };
            if let Some(tab) = next {
                state.tab = tab;
                state.option = 0;
                self.refresh(&mut state);
                return;
            }
        }
        if state.tab == self.questions.len() {
            if matches_key(data, "enter") && self.all_answered(&state) {
                self.submit(&state, false);
            } else if matches_key(data, "escape") {
                self.submit(&state, true);
            }
            return;
        }
        if matches_key(data, "up") {
            state.option = (state.option - 1).max(0);
            self.refresh(&mut state);
            return;
        }
        if matches_key(data, "down") {
            state.option = (state.option + 1).min(self.options(state.tab).len() as i64 - 1).max(0);
            self.refresh(&mut state);
            return;
        }
        if matches_key(data, "enter") {
            let options = self.options(state.tab);
            let Some(option) = usize::try_from(state.option).ok().and_then(|i| options.get(i)) else {
                return;
            };
            let id = self.questions[state.tab]["id"].as_str().unwrap().to_owned();
            if option["isOther"] == true {
                state.input_id = Some(id);
                state.editor.editor.set_text("");
                self.refresh(&mut state);
                return;
            }
            let index = state.option + 1;
            state.answers.insert(
                id.clone(),
                json!({"id":id,"value":option["value"],"label":option["label"],"wasCustom":false,"index":index}),
            );
            self.advance(&mut state);
        } else if matches_key(data, "escape") {
            self.submit(&state, true);
        }
    }
    fn invalidate(&self) {
        self.state.borrow_mut().cache = None;
    }
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let mut state = self.state.borrow_mut();
        if let Some(cache) = &state.cache {
            return cache.clone();
        }
        let width = width.max(1);
        let theme = &self.theme;
        let mut lines = vec![theme.fg(C::Accent, &"─".repeat(width))];
        if self.questions.len() > 1 {
            self.render_tabs(&mut lines, width, &state);
        }
        if let Some(q) = self.questions.get(state.tab) {
            wrap_prefixed(
                &mut lines,
                width,
                " ",
                &theme.fg(C::Text, q["prompt"].as_str().unwrap()),
            );
            lines.push(String::new());
            let options = self
                .options(state.tab)
                .iter()
                .map(|o| {
                    (
                        o["label"].as_str().unwrap().to_owned(),
                        o["description"].as_str().map(str::to_owned),
                        o["isOther"] == true,
                    )
                })
                .collect::<Vec<_>>();
            option_lines(
                &mut lines,
                width,
                theme,
                &options,
                state.option,
                state.input_id.is_some(),
            );
            if state.input_id.is_some() {
                editor_lines(&mut lines, width, theme, &mut state.editor.editor);
                lines.push(String::new());
                wrap_prefixed(
                    &mut lines,
                    width,
                    " ",
                    &theme.fg(C::Dim, "Enter to submit • Esc to cancel"),
                );
            }
        } else {
            self.render_submit(&mut lines, width, &state);
        }
        lines.push(String::new());
        if state.input_id.is_none() {
            wrap_prefixed(
                &mut lines,
                width,
                " ",
                &theme.fg(
                    C::Dim,
                    if self.questions.len() > 1 {
                        "Tab/←→ navigate • ↑↓ select • Enter confirm • Esc cancel"
                    } else {
                        "↑↓ navigate • Enter select • Esc cancel"
                    },
                ),
            );
        }
        lines.push(theme.fg(C::Accent, &"─".repeat(width)));
        state.cache = Some(lines.clone());
        lines
    }
}
fn text(value: String) -> ComponentRef {
    component(Text::new(value, 0, 0))
}
fn render_call(args: &Value, theme: &Theme) -> String {
    let questions = args["questions"].as_array().cloned().unwrap_or_default();
    let count = questions.len();
    let labels = questions
        .iter()
        .map(|q| {
            q["label"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(q["id"].as_str().unwrap())
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut text = theme.fg(C::ToolTitle, &theme.bold("questionnaire "))
        + &theme.fg(
            C::Muted,
            &format!("{count} question{}", if count != 1 { "s" } else { "" }),
        );
    if !labels.is_empty() {
        text += &theme.fg(C::Dim, &format!(" ({labels})"));
    }
    text
}
fn render_result(result: &AgentToolResult, theme: &Theme) -> String {
    let Some(details) = result.details.as_ref() else {
        return result
            .content
            .first()
            .and_then(|c| match c {
                TextOrImageContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .unwrap_or_default();
    };
    if details["cancelled"] == true {
        return theme.fg(C::Warning, "Cancelled");
    }
    details["answers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            let prefix = theme.fg(C::Success, "✓ ") + &theme.fg(C::Accent, a["id"].as_str().unwrap()) + ": ";
            if a["wasCustom"] == true {
                prefix + &theme.fg(C::Muted, "(wrote) ") + a["label"].as_str().unwrap()
            } else {
                prefix
                    + &a["index"].as_i64().filter(|i| *i != 0).map_or_else(
                        || a["label"].as_str().unwrap().to_owned(),
                        |i| format!("{i}. {}", a["label"].as_str().unwrap()),
                    )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
