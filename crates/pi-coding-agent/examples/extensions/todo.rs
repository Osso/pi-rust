//! Opt-in todo tool; state is reconstructed only from current-branch tool results.
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor as C};
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::JsValue as Value;
use pi_tui::{
    Text,
    keys::matches_key,
    tui::{ComponentRef, component},
    utils::truncate_to_width,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
#[derive(Clone, Serialize, Deserialize)]
struct Todo {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    id: f64,
    text: JsString,
    done: bool,
}
struct State {
    todos: Vec<Todo>,
    next_id: f64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            todos: vec![],
            next_id: 1.0,
        }
    }
}
fn number(n: f64) -> String {
    pi_js_compat::number::js_number_to_string(n)
}

pub fn todo_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let state = Arc::new(Mutex::new(State::default()));
    let start = state.clone();
    pi.on_session_start(
        move |_: SessionStartEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = start.clone();
            Box::pin(async move {
                restore(&state, &ctx)?;
                Ok(None)
            })
        },
    )?;
    let tree = state.clone();
    pi.on_session_tree(
        move |_: SessionTreeEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = tree.clone();
            Box::pin(async move {
                restore(&state, &ctx)?;
                Ok(None)
            })
        },
    )?;
    let execute = state.clone();
    pi.register_tool(ToolDefinition {
        name:"todo".into(),label:"Todo".into(),description:"Manage a todo list. Actions: list, add (text), toggle (id), clear".into(),
        parameters:serde_json::json!({"type":"object","properties":{"action":{"type":"string","enum":["list","add","toggle","clear"]},"text":{"type":"string","description":"Todo text (for add)"},"id":{"type":"number","description":"Todo ID (for toggle)"}},"required":["action"]}),
        prompt_snippet:None,prompt_guidelines:None,approval_required:None,approval_kind:None,constrained_sampling:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:None,
        execute:Arc::new(move|_,params,_,_,_|{let state=execute.clone();Box::pin(async move {Ok(execute_todo(&mut state.lock().unwrap(),&params))})}),
        render_call:Some(infallible_render_call(|args,th,_| {
            let mut text=th.fg(C::ToolTitle,&th.bold("todo "))+&th.fg(C::Muted,args["action"].as_string().unwrap_or(&JsString::from("undefined")));
            if let Some(value)=args["text"].as_string().filter(|v|!v.is_empty()){text.push_str(&js_format!(" {}", th.fg(C::Dim,&js_format!("\"{}\"", value))));}
            if let Some(id)=args["id"].as_f64(){text.push_str(&js_format!(" {}", th.fg(C::Accent,&js_format!("#{}", number(id)))));}
            text_component(text)
        })),
        render_result:Some(infallible_render_result(|result,options,th,_|text_component(render_result(result,options.expanded,th)))),
    })?;
    pi.register_command(
        "todos",
        Some("Show all todos on the current branch".into()),
        None,
        Arc::new(move |_, ctx| {
            let state = state.clone();
            Box::pin(async move {
                if ctx.mode()? != ExtensionMode::Tui {
                    ctx.ui()?.notify(
                        &JsString::from("/todos requires interactive mode"),
                        Some(NotifyType::Error),
                    )?;
                    return Ok(());
                }
                let todos = state.lock().unwrap().todos.clone();
                ctx.ui()?
                    .custom(
                        Arc::new(move |context, done| {
                            pi_coding_agent::core::extensions::ready_custom_component(Box::new(TodoList {
                                todos: todos.clone(),
                                theme: context.theme,
                                done,
                            }))
                        }),
                        None,
                    )
                    .await?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
fn restore(state: &Mutex<State>, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let entries = ctx.session_manager()?.get_branch(None);
    let mut restored = State::default();
    for entry in entries {
        let value = pi_js_compat::js_json::to_value(&entry).map_err(|e| HandlerError::new(e.to_string()))?;
        let message = &value["message"];
        if message["role"] != json!("toolResult")
            || message["toolName"] != json!("todo")
            || message["details"].is_null()
        {
            continue;
        }
        restored.todos = pi_js_compat::js_json::from_value(&message["details"]["todos"])
            .map_err(|e| HandlerError::new(e.to_string()))?;
        restored.next_id = message["details"]["nextId"]
            .as_f64()
            .ok_or_else(|| HandlerError::new("todo nextId must be a number"))?;
    }
    *state.lock().unwrap() = restored;
    Ok(())
}
fn execute_todo(state: &mut State, params: &Value) -> AgentToolResult {
    let action = params["action"]
        .as_string()
        .cloned()
        .unwrap_or_else(|| JsString::from("undefined"));
    let (text, error) = match &action {
        action if action == "list" => (
            if state.todos.is_empty() {
                "No todos".into()
            } else {
                JsString::join(
                    state
                        .todos
                        .iter()
                        .map(|t| js_format!("[{}] #{}: {}", if t.done { "x" } else { " " }, number(t.id), t.text))
                        .collect::<Vec<_>>(),
                    "\n",
                )
            },
            None,
        ),
        action if action == "add" => match params["text"].as_string().filter(|s| !s.is_empty()) {
            None => ("Error: text required for add".into(), Some("text required".into())),
            Some(text) => {
                let id = state.next_id;
                state.next_id += 1.0;
                state.todos.push(Todo {
                    id,
                    text: text.into(),
                    done: false,
                });
                (js_format!("Added todo #{}: {}", number(id), text), None)
            }
        },
        action if action == "toggle" => toggle(state, params),
        action if action == "clear" => {
            let count = state.todos.len();
            *state = State::default();
            (js_format!("Cleared {} todos", count), None)
        }
        other => (
            js_format!("Unknown action: {}", other),
            Some(js_format!("unknown action: {}", other)),
        ),
    };
    let action = if ["list", "add", "toggle", "clear"].iter().any(|name| action == *name) {
        action
    } else {
        JsString::from("list")
    };
    let mut details = json!({"action":action,"todos":state.todos,"nextId":state.next_id});
    if let Some(error) = error {
        details["error"] = json!(error);
    }
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(JsValue::from(details)),
        is_error: None,
        terminate: None,
    }
}
fn toggle(state: &mut State, params: &Value) -> (JsString, Option<JsString>) {
    let Some(id) = params["id"].as_f64() else {
        return ("Error: id required for toggle".into(), Some("id required".into()));
    };
    let Some(todo) = state.todos.iter_mut().find(|t| t.id == id) else {
        return (
            js_format!("Todo #{} not found", number(id)),
            Some(js_format!("#{} not found", number(id))),
        );
    };
    todo.done = !todo.done;
    (
        js_format!(
            "Todo #{} {}",
            number(id),
            if todo.done { "completed" } else { "uncompleted" }
        ),
        None,
    )
}
fn text_component(text: JsString) -> ComponentRef {
    component(Text::new(text, 0, 0))
}
fn first_text(result: &AgentToolResult) -> JsString {
    match result.content.first() {
        Some(TextOrImageContent::Text(t)) => t.text.clone(),
        _ => JsString::default(),
    }
}
fn render_result(result: &AgentToolResult, expanded: bool, th: &Theme) -> JsString {
    let Some(details) = &result.details else {
        return first_text(result).into();
    };
    if details.is_null() {
        return first_text(result).into();
    }
    if let Some(error) = details["error"].as_string().filter(|s| !s.is_empty()) {
        return th.fg(C::Error, &js_format!("Error: {}", error));
    }
    let todos: Vec<Todo> = pi_js_compat::js_json::from_value(&details["todos"]).expect("todo details");
    match details["action"].as_string().unwrap_or(&JsString::from("")) {
        action if action == "list" => render_list(&todos, expanded, th),
        action if action == "add" => {
            let t = todos.last().expect("added todo");
            th.fg(C::Success, "✓ Added ")
                + &th.fg(C::Accent, &js_format!("#{}", number(t.id)))
                + " "
                + &th.fg(C::Muted, &t.text)
        }
        action if action == "toggle" => th.fg(C::Success, "✓ ") + &th.fg(C::Muted, first_text(result)),
        action if action == "clear" => th.fg(C::Success, "✓ ") + &th.fg(C::Muted, "Cleared all todos"),
        _ => JsString::default(),
    }
}
fn render_list(todos: &[Todo], expanded: bool, th: &Theme) -> JsString {
    if todos.is_empty() {
        return th.fg(C::Dim, "No todos");
    }
    let mut text = th.fg(C::Muted, &js_format!("{} todo(s):", todos.len()));
    for t in todos.iter().take(if expanded { todos.len() } else { 5 }) {
        let check = if t.done {
            th.fg(C::Success, "✓")
        } else {
            th.fg(C::Dim, "○")
        };
        let item = th.fg(if t.done { C::Dim } else { C::Muted }, &t.text);
        text.push_str(&js_format!(
            "\n{} {} {}",
            check,
            th.fg(C::Accent, &js_format!("#{}", number(t.id))),
            item
        ));
    }
    if !expanded && todos.len() > 5 {
        text.push_str(&js_format!(
            "\n{}",
            th.fg(C::Dim, &js_format!("... {} more", todos.len() - 5))
        ));
    }
    text
}
struct TodoList {
    todos: Vec<Todo>,
    theme: Arc<Theme>,
    done: ExtensionUiDone,
}
impl ExtensionUiComponent for TodoList {
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        if matches_key(data, "escape") || matches_key(data, "ctrl+c") {
            self.done.call(None);
        }
    }
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let th = &self.theme;
        let header = th.fg(C::BorderMuted, "───")
            + &th.fg(C::Accent, " Todos ")
            + &th.fg(C::BorderMuted, &"─".repeat(width.saturating_sub(10)));
        let mut lines = vec![
            JsString::default(),
            truncate_to_width(&header, width),
            JsString::default(),
        ];
        if self.todos.is_empty() {
            lines.push(truncate_to_width(
                &js_format!("  {}", th.fg(C::Dim, "No todos yet. Ask the agent to add some!")),
                width,
            ));
        } else {
            let done = self.todos.iter().filter(|t| t.done).count();
            lines.push(truncate_to_width(
                &js_format!(
                    "  {}",
                    th.fg(C::Muted, &js_format!("{}/{} completed", done, self.todos.len()))
                ),
                width,
            ));
            lines.push(JsString::default());
            for t in &self.todos {
                let check = if t.done {
                    th.fg(C::Success, "✓")
                } else {
                    th.fg(C::Dim, "○")
                };
                let id = th.fg(C::Accent, &js_format!("#{}", number(t.id)));
                let text = th.fg(if t.done { C::Dim } else { C::Text }, &t.text);
                lines.push(truncate_to_width(&js_format!("  {} {} {}", check, id, text), width));
            }
        }
        lines.push(JsString::default());
        lines.push(truncate_to_width(
            &js_format!("  {}", th.fg(C::Dim, "Press Escape to close")),
            width,
        ));
        lines.push(JsString::default());
        lines
    }
}
