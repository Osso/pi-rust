//! Opt-in todo tool; state is reconstructed only from current-branch tool results.
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor as C};
use pi_tui::{
    Text,
    keys::matches_key,
    tui::{ComponentRef, component},
    utils::truncate_to_width,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
#[derive(Clone, Serialize, Deserialize)]
struct Todo {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    id: f64,
    text: String,
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
        parameters:json!({"type":"object","properties":{"action":{"type":"string","enum":["list","add","toggle","clear"]},"text":{"type":"string","description":"Todo text (for add)"},"id":{"type":"number","description":"Todo ID (for toggle)"}},"required":["action"]}),
        prompt_snippet:None,prompt_guidelines:None,approval_required:None,approval_kind:None,constrained_sampling:None,render_shell:None,prepare_arguments:None,prepare_execution:None,execution_mode:None,
        execute:Arc::new(move|_,params,_,_,_|{let state=execute.clone();Box::pin(async move {Ok(execute_todo(&mut state.lock().unwrap(),&params))})}),
        render_call:Some(infallible_render_call(|args,th,_| {
            let mut text=th.fg(C::ToolTitle,&th.bold("todo "))+&th.fg(C::Muted,args["action"].as_str().unwrap_or("undefined"));
            if let Some(value)=args["text"].as_str().filter(|v|!v.is_empty()){text+=&format!(" {}",th.fg(C::Dim,&format!("\"{value}\"")));}
            if let Some(id)=args["id"].as_f64(){text+=&format!(" {}",th.fg(C::Accent,&format!("#{}",number(id))));}
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
                    ctx.ui()?
                        .notify("/todos requires interactive mode", Some(NotifyType::Error));
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
    let entries = ctx.session_manager()?.lock().unwrap().get_branch(None);
    let mut restored = State::default();
    for entry in entries {
        let value = serde_json::to_value(entry).map_err(|e| HandlerError::new(e.to_string()))?;
        let message = &value["message"];
        if message["role"] != "toolResult" || message["toolName"] != "todo" || message["details"].is_null() {
            continue;
        }
        restored.todos = serde_json::from_value(message["details"]["todos"].clone())
            .map_err(|e| HandlerError::new(e.to_string()))?;
        restored.next_id = message["details"]["nextId"]
            .as_f64()
            .ok_or_else(|| HandlerError::new("todo nextId must be a number"))?;
    }
    *state.lock().unwrap() = restored;
    Ok(())
}
fn execute_todo(state: &mut State, params: &Value) -> AgentToolResult {
    let action = params["action"].as_str().unwrap_or("undefined");
    let (text, error) = match action {
        "list" => (
            if state.todos.is_empty() {
                "No todos".into()
            } else {
                state
                    .todos
                    .iter()
                    .map(|t| format!("[{}] #{}: {}", if t.done { "x" } else { " " }, number(t.id), t.text))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
            None,
        ),
        "add" => match params["text"].as_str().filter(|s| !s.is_empty()) {
            None => ("Error: text required for add".into(), Some("text required".into())),
            Some(text) => {
                let id = state.next_id;
                state.next_id += 1.0;
                state.todos.push(Todo {
                    id,
                    text: text.into(),
                    done: false,
                });
                (format!("Added todo #{}: {text}", number(id)), None)
            }
        },
        "toggle" => toggle(state, params),
        "clear" => {
            let count = state.todos.len();
            *state = State::default();
            (format!("Cleared {count} todos"), None)
        }
        other => (
            format!("Unknown action: {other}"),
            Some(format!("unknown action: {other}")),
        ),
    };
    let action = if ["list", "add", "toggle", "clear"].contains(&action) {
        action
    } else {
        "list"
    };
    let mut details = json!({"action":action,"todos":state.todos,"nextId":serde_json::from_str::<Value>(&number(state.next_id)).expect("finite nextId")});
    if let Some(error) = error {
        details["error"] = json!(error);
    }
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(details),
        is_error: None,
        terminate: None,
    }
}
fn toggle(state: &mut State, params: &Value) -> (String, Option<String>) {
    let Some(id) = params["id"].as_f64() else {
        return ("Error: id required for toggle".into(), Some("id required".into()));
    };
    let Some(todo) = state.todos.iter_mut().find(|t| t.id == id) else {
        return (
            format!("Todo #{} not found", number(id)),
            Some(format!("#{} not found", number(id))),
        );
    };
    todo.done = !todo.done;
    (
        format!(
            "Todo #{} {}",
            number(id),
            if todo.done { "completed" } else { "uncompleted" }
        ),
        None,
    )
}
fn text_component(text: String) -> ComponentRef {
    component(Text::new(text, 0, 0))
}
fn first_text(result: &AgentToolResult) -> &str {
    match result.content.first() {
        Some(TextOrImageContent::Text(t)) => &t.text,
        _ => "",
    }
}
fn render_result(result: &AgentToolResult, expanded: bool, th: &Theme) -> String {
    let Some(details) = &result.details else {
        return first_text(result).into();
    };
    if details.is_null() {
        return first_text(result).into();
    }
    if let Some(error) = details["error"].as_str().filter(|s| !s.is_empty()) {
        return th.fg(C::Error, &format!("Error: {error}"));
    }
    let todos: Vec<Todo> = serde_json::from_value(details["todos"].clone()).expect("todo details");
    match details["action"].as_str().unwrap_or("") {
        "list" => render_list(&todos, expanded, th),
        "add" => {
            let t = todos.last().expect("added todo");
            th.fg(C::Success, "✓ Added ")
                + &th.fg(C::Accent, &format!("#{}", number(t.id)))
                + " "
                + &th.fg(C::Muted, &t.text)
        }
        "toggle" => th.fg(C::Success, "✓ ") + &th.fg(C::Muted, first_text(result)),
        "clear" => th.fg(C::Success, "✓ ") + &th.fg(C::Muted, "Cleared all todos"),
        _ => String::new(),
    }
}
fn render_list(todos: &[Todo], expanded: bool, th: &Theme) -> String {
    if todos.is_empty() {
        return th.fg(C::Dim, "No todos");
    }
    let mut text = th.fg(C::Muted, &format!("{} todo(s):", todos.len()));
    for t in todos.iter().take(if expanded { todos.len() } else { 5 }) {
        let check = if t.done {
            th.fg(C::Success, "✓")
        } else {
            th.fg(C::Dim, "○")
        };
        let item = th.fg(if t.done { C::Dim } else { C::Muted }, &t.text);
        text += &format!("\n{check} {} {item}", th.fg(C::Accent, &format!("#{}", number(t.id))));
    }
    if !expanded && todos.len() > 5 {
        text += &format!("\n{}", th.fg(C::Dim, &format!("... {} more", todos.len() - 5)));
    }
    text
}
struct TodoList {
    todos: Vec<Todo>,
    theme: Arc<Theme>,
    done: ExtensionUiDone,
}
impl ExtensionUiComponent for TodoList {
    fn handle_input(&self, data: &str) {
        if matches_key(data, "escape") || matches_key(data, "ctrl+c") {
            self.done.call(None);
        }
    }
    fn render(&self, width: usize) -> Vec<String> {
        let th = &self.theme;
        let header = th.fg(C::BorderMuted, "───")
            + &th.fg(C::Accent, " Todos ")
            + &th.fg(C::BorderMuted, &"─".repeat(width.saturating_sub(10)));
        let mut lines = vec![String::new(), truncate_to_width(&header, width), String::new()];
        if self.todos.is_empty() {
            lines.push(truncate_to_width(
                &format!("  {}", th.fg(C::Dim, "No todos yet. Ask the agent to add some!")),
                width,
            ));
        } else {
            let done = self.todos.iter().filter(|t| t.done).count();
            lines.push(truncate_to_width(
                &format!(
                    "  {}",
                    th.fg(C::Muted, &format!("{done}/{} completed", self.todos.len()))
                ),
                width,
            ));
            lines.push(String::new());
            for t in &self.todos {
                let check = if t.done {
                    th.fg(C::Success, "✓")
                } else {
                    th.fg(C::Dim, "○")
                };
                let id = th.fg(C::Accent, &format!("#{}", number(t.id)));
                let text = th.fg(if t.done { C::Dim } else { C::Text }, &t.text);
                lines.push(truncate_to_width(&format!("  {check} {id} {text}"), width));
            }
        }
        lines.push(String::new());
        lines.push(truncate_to_width(
            &format!("  {}", th.fg(C::Dim, "Press Escape to close")),
            width,
        ));
        lines.push(String::new());
        lines
    }
}
