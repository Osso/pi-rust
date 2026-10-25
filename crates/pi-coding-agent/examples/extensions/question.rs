//! Opt-in native port of `examples/extensions/question.ts`.
#[path = "forms.rs"]
mod forms;
use forms::{FormEditor, editor_lines, option_lines, wrap_prefixed};
use pi_agent_core::{AgentToolResult, ToolExecutionMode};
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::{
    core::extensions::*,
    modes::interactive::theme::{Theme, ThemeColor as C},
};
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use pi_tui::{
    Text,
    keys::matches_key,
    tui::{ComponentRef, component},
};
use std::{cell::RefCell, sync::Arc};

pub fn question_extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    api.register_tool(ToolDefinition {
        name:"question".into(),label:"Question".into(),
        description:"Ask the user a question and let them pick from options. Use when you need user input to proceed.".into(),
        parameters:serde_json::json!({"type":"object","properties":{"question":{"type":"string","description":"The question to ask the user"},"options":{"type":"array","description":"Options for the user to choose from","items":{"type":"object","properties":{"label":{"type":"string","description":"Display label for the option"},"description":{"type":"string","description":"Optional description shown below label"}},"required":["label"]}}},"required":["question","options"]}),
        prompt_snippet:None,prompt_guidelines:None,approval_required:None,approval_kind:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:Some(ToolExecutionMode::Sequential),
        execute:Arc::new(|_,params,_,_,ctx| Box::pin(async move { execute(params,pi_coding_agent::core::extensions::types::require_extension_context(ctx)?).await })),
        render_call:Some(infallible_render_call(|args,theme,_| text(render_call(args,theme)))),
        render_result:Some(infallible_render_result(|result,_,theme,_|text(render_result(result,theme)))),
    })
}
fn result(message: JsString, details: Value) -> AgentToolResult {
    let mut result = AgentToolResult::text(message);
    result.details = Some(details);
    result
}
async fn execute(params: Value, ctx: ExtensionContext) -> Result<AgentToolResult, HandlerError> {
    let labels = params["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["label"].clone())
        .collect::<Vec<_>>();
    let mut details = json!({"question":params["question"],"options":labels,"answer":null});
    if ctx.mode()? != ExtensionMode::Tui {
        return Ok(result(
            "Error: UI not available (running in non-interactive mode)".into(),
            details,
        ));
    }
    if labels.is_empty() {
        return Ok(result("Error: No options provided".into(), details));
    }
    let answer = ctx
        .ui()?
        .custom(
            Arc::new(move |context, done| {
                pi_coding_agent::core::extensions::ready_custom_component(Box::new(QuestionForm {
                    theme: context.theme.clone(),
                    tui: context.tui.clone(),
                    done,
                    params: params.clone(),
                    state: RefCell::new(State {
                        option: 0,
                        editing: false,
                        cache: None,
                        editor: FormEditor::new(&context),
                    }),
                }))
            }),
            None,
        )
        .await?;
    let Some(answer) = answer.filter(|a| !a.is_null()) else {
        return Ok(result("User cancelled the selection".into(), details));
    };
    details["answer"] = answer["answer"].clone();
    details["wasCustom"] = answer["wasCustom"].clone();
    let message = if answer["wasCustom"] == pi_js_compat::js_json!(true) {
        js_format!("User wrote: {}", answer["answer"].as_string().unwrap())
    } else {
        js_format!(
            "User selected: {}. {}",
            pi_js_compat::js_json::to_js_string(&answer["index"]).expect("answer index converts to JS text"),
            answer["answer"].as_string().unwrap()
        )
    };
    Ok(result(message, details))
}
struct State {
    option: usize,
    editing: bool,
    cache: Option<Vec<JsString>>,
    editor: FormEditor,
}
struct QuestionForm {
    theme: Arc<Theme>,
    tui: ExtensionUiTui,
    done: ExtensionUiDone,
    params: Value,
    state: RefCell<State>,
}
impl QuestionForm {
    fn refresh(&self, state: &mut State) {
        state.cache = None;
        self.tui.request_render();
    }
}
impl ExtensionUiComponent for QuestionForm {
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        let mut state = self.state.borrow_mut();
        if state.editing {
            if matches_key(data, "escape") {
                state.editing = false;
                state.editor.editor.set_text("");
                self.refresh(&mut state);
                return;
            }
            if let Some(value) = state.editor.input(data) {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    state.editing = false;
                    state.editor.editor.set_text("");
                    self.refresh(&mut state);
                } else {
                    self.done.call(Some(json!({"answer":trimmed,"wasCustom":true})));
                }
            }
            self.refresh(&mut state);
            return;
        }
        if matches_key(data, "up") {
            state.option = state.option.saturating_sub(1);
            self.refresh(&mut state);
            return;
        }
        if matches_key(data, "down") {
            state.option = (state.option + 1).min(self.params["options"].as_array().unwrap().len());
            self.refresh(&mut state);
            return;
        }
        if matches_key(data, "enter") {
            if state.option == self.params["options"].as_array().unwrap().len() {
                state.editing = true;
                self.refresh(&mut state);
            } else {
                self.done.call(Some(json!({"answer":self.params["options"][state.option]["label"],"wasCustom":false,"index":state.option+1})));
            }
        } else if matches_key(data, "escape") {
            self.done.call(Some(Value::Null));
        }
    }
    fn invalidate(&self) {
        self.state.borrow_mut().cache = None;
    }
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let mut state = self.state.borrow_mut();
        if let Some(cache) = &state.cache {
            return cache.clone().into_iter().collect::<Vec<_>>();
        }
        let width = width.max(1);
        let theme = &self.theme;
        let mut lines = vec![theme.fg(C::Accent, "─".repeat(width))];
        wrap_prefixed(
            &mut lines,
            width,
            " ",
            theme.fg(C::Text, self.params["question"].as_string().unwrap()),
        );
        lines.push(JsString::default());
        let mut options = self.params["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                (
                    o["label"].as_string().unwrap().to_owned(),
                    o["description"].as_string().cloned(),
                    false,
                )
            })
            .collect::<Vec<_>>();
        options.push(("Type something.".into(), None, true));
        option_lines(&mut lines, width, theme, &options, state.option as i64, state.editing);
        if state.editing {
            editor_lines(&mut lines, width, theme, &mut state.editor.editor);
        }
        lines.push(JsString::default());
        wrap_prefixed(
            &mut lines,
            width,
            " ",
            theme.fg(
                C::Dim,
                if state.editing {
                    "Enter to submit • Esc to go back"
                } else {
                    "↑↓ navigate • Enter to select • Esc to cancel"
                },
            ),
        );
        lines.push(theme.fg(C::Accent, "─".repeat(width)));
        state.cache = Some(lines.clone());
        lines
    }
}
fn text(value: JsString) -> ComponentRef {
    component(Text::new(value, 0, 0))
}
fn render_call(args: &Value, theme: &Theme) -> JsString {
    let mut text = theme.fg(C::ToolTitle, theme.bold("question "))
        + &theme.fg(
            C::Muted,
            args["question"]
                .as_string()
                .cloned()
                .unwrap_or_else(|| JsString::from("undefined")),
        );
    if let Some(options) = args["options"].as_array().filter(|o| !o.is_empty()) {
        let mut labels = options
            .iter()
            .map(|o| o["label"].as_string().unwrap().to_owned())
            .collect::<Vec<_>>();
        labels.push("Type something.".into());
        let numbered = JsString::join(
            labels
                .iter()
                .enumerate()
                .map(|(i, label)| js_format!("{}. {}", i + 1, label))
                .collect::<Vec<_>>(),
            ", ",
        );
        text.push_str(&js_format!(
            "\n{}",
            theme.fg(C::Dim, &js_format!("  Options: {}", numbered))
        ));
    }
    text
}
fn render_result(result: &AgentToolResult, theme: &Theme) -> JsString {
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
    let Some(answer) = details["answer"].as_string() else {
        return theme.fg(C::Warning, "Cancelled");
    };
    if details["wasCustom"] == pi_js_compat::js_json!(true) {
        return theme.fg(C::Success, "✓ ") + &theme.fg(C::Muted, "(wrote) ") + &theme.fg(C::Accent, answer);
    }
    let index = details["options"]
        .as_array()
        .unwrap()
        .iter()
        .position(|v| v.as_string() == Some(answer));
    let display = index.map_or_else(|| answer.to_owned(), |i| js_format!("{}. {}", i + 1, answer));
    theme.fg(C::Success, "✓ ") + &theme.fg(C::Accent, &display)
}
