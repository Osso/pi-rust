//! Opt-in native plan-mode example. Explicit registration only.
pub mod utils;
use pi_coding_agent::{core::extensions::*, modes::interactive::theme::ThemeColor as C};
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::{self, JsValue as Value};
use std::sync::{Arc, Mutex};
use utils::{TodoItem, extract_todo_items, is_safe_command, mark_completed_steps};

const PLAN: &[&str] = &["read", "bash", "grep", "find", "ls", "ask_questions"];
const NORMAL: &[&str] = &["read", "bash", "edit", "write"];
const PLAN_CONTEXT: &str = "[PLAN MODE ACTIVE]\nYou are in plan mode - a read-only exploration mode for safe code analysis.\n\nRestrictions:\n- Built-in edit and write tools are disabled\n- Other currently active tools remain available\n- Bash is restricted to an allowlist of read-only commands\n\nAsk all user questions through ask_questions, never chat. Always specify blocking: true when planning depends on an answer, false to continue independent read-only exploration while answers arrive through the inbox. Omit options for free-text questions.\nUse brave-search skill via bash for web research.\n\nCreate a detailed numbered plan under a \"Plan:\" header:\n\nPlan:\n1. First step description\n2. Second step description\n...\n\nDo NOT attempt to make changes - just describe what you would do.";
#[derive(Default)]
struct State {
    enabled: bool,
    executing: bool,
    todos: Vec<TodoItem>,
    tools: Option<Vec<String>>,
}
type Shared = Arc<Mutex<State>>;

pub fn plan_mode_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let state: Shared = Arc::default();
    pi.register_flag(
        "plan",
        Some("Start in plan mode (read-only exploration)".into()),
        FlagType::Boolean,
        Some(FlagValue::Boolean(false)),
    )?;
    let api = pi.clone();
    let shared = state.clone();
    pi.register_command(
        "plan",
        Some("Toggle plan mode (read-only exploration)".into()),
        None,
        Arc::new(move |_, ctx| {
            let api = api.clone();
            let shared = shared.clone();
            Box::pin(async move { toggle(&api, &shared, &ctx) })
        }),
    )?;
    let shared = state.clone();
    pi.register_command(
        "todos",
        Some("Show current plan todo list".into()),
        None,
        Arc::new(move |_, ctx| {
            let shared = shared.clone();
            Box::pin(async move {
                let s = shared.lock().unwrap();
                if s.todos.is_empty() {
                    ctx.ui()?.notify(
                        &JsString::from("No todos. Create a plan first with /plan"),
                        Some(NotifyType::Info),
                    )?;
                } else {
                    let list = JsString::join(
                        s.todos
                            .iter()
                            .enumerate()
                            .map(|(i, t)| js_format!("{}. {} {}", i + 1, if t.completed { "✓" } else { "○" }, t.text)),
                        "\n",
                    );
                    ctx.ui()?
                        .notify(&js_format!("Plan Progress:\n{}", list), Some(NotifyType::Info))?;
                }
                Ok(())
            })
        }),
    )?;
    let api = pi.clone();
    let shared = state.clone();
    pi.register_shortcut(
        "ctrl+alt+p".into(),
        Some("Toggle plan mode".into()),
        Arc::new(move |ctx| {
            let api = api.clone();
            let shared = shared.clone();
            Box::pin(async move { toggle(&api, &shared, &ctx) })
        }),
    )?;
    let shared = state.clone();
    pi.on_tool_call(move|event:ToolCallEvent,_:ExtensionContext|->BoxFuture<'static,Result<Option<ToolCallEventResult>,HandlerError>>{
        let enabled=shared.lock().unwrap().enabled;
        Box::pin(async move {
            if !enabled || event.tool_name != ToolName::Bash { return Ok(None); }
            let command=event.input["command"].as_string().cloned().unwrap_or_default();
            Ok((enabled&&event.tool_name==ToolName::Bash&&!is_safe_command(&command)).then(||ToolCallEventResult{block:Some(true),reason:Some(js_format!("Plan mode: command blocked (not allowlisted). Use /plan to disable plan mode first.\nCommand: {}", command)),..Default::default()}))
        })
    })?;
    let shared = state.clone();
    pi.on_context(
        move |event: ContextEvent,
              _: ExtensionContext|
              -> BoxFuture<'static, Result<Option<ContextEventResult>, HandlerError>> {
            let enabled = shared.lock().unwrap().enabled;
            Box::pin(async move {
                if enabled {
                    return Ok(None);
                }
                let messages = event
                    .messages
                    .into_iter()
                    .filter(|m| {
                        let v = js_json::to_value(m).expect("message JSON");
                        if v["customType"] == json!("plan-mode-context") {
                            return false;
                        }
                        if v["role"] != json!("user") {
                            return true;
                        }
                        if let Some(s) = v["content"].as_string() {
                            return !s.contains("[PLAN MODE ACTIVE]");
                        }
                        !v["content"].as_array().into_iter().flatten().any(|b| {
                            b["type"] == json!("text")
                                && b["text"].as_string().is_some_and(|s| s.contains("[PLAN MODE ACTIVE]"))
                        })
                    })
                    .collect();
                Ok(Some(ContextEventResult {
                    messages: Some(messages),
                }))
            })
        },
    )?;
    let shared = state.clone();
    pi.on_before_agent_start(move|_:BeforeAgentStartEvent,_:ExtensionContext|->BoxFuture<'static,Result<Option<BeforeAgentStartEventResult>,HandlerError>>{
        let s=shared.lock().unwrap();
        let message=if s.enabled {Some(custom_message("plan-mode-context",PLAN_CONTEXT.into(),false))}
        else if s.executing&&!s.todos.is_empty(){Some(custom_message("plan-execution-context",js_format!("[EXECUTING PLAN - Full tool access enabled]\n\nRemaining steps:\n{}\n\nExecute each step in order.\nAfter completing a step, include a [DONE:n] tag in your response.",remaining(&s.todos)),false))}else{None};
        Box::pin(async move{Ok(message.map(|message|BeforeAgentStartEventResult{message:Some(message),system_prompt:None}))})
    })?;
    let api = pi.clone();
    let shared = state.clone();
    pi.on_turn_end(
        move |event: TurnEndEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            let shared = shared.clone();
            Box::pin(async move {
                let mut s = shared.lock().unwrap();
                if !s.executing || s.todos.is_empty() {
                    return Ok(None);
                }
                let Some(text) = assistant_text(&js_json::to_value(&event.message).unwrap()) else {
                    return Ok(None);
                };
                if mark_completed_steps(&text, &mut s.todos) > 0 {
                    update(&s, &ctx)?;
                }
                persist(&api, &s)?;
                Ok(None)
            })
        },
    )?;
    let api = pi.clone();
    let shared = state.clone();
    pi.on_agent_end(
        move |event: AgentEndEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            let shared = shared.clone();
            Box::pin(async move {
                finish_turn(&api, &shared, &ctx, &js_json::to_value(&event.messages).unwrap()).await?;
                Ok(None)
            })
        },
    )?;
    let api = pi.clone();
    let shared = state.clone();
    pi.on_session_start(
        move |_: SessionStartEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            let shared = shared.clone();
            Box::pin(async move {
                restore(&api, &shared, &ctx)?;
                Ok(None)
            })
        },
    )?;
    Ok(())
}
fn custom_message(kind: &str, content: JsString, display: bool) -> CustomMessagePayload {
    CustomMessagePayload {
        custom_type: kind.into(),
        content: CustomMessageContent::Text(content),
        display,
        details: None,
    }
}
fn assistant_text(v: &Value) -> Option<JsString> {
    if v["role"] != json!("assistant") {
        return None;
    }
    Some(JsString::join(
        v["content"]
            .as_array()?
            .iter()
            .filter(|b| b["type"] == json!("text"))
            .filter_map(|b| b["text"].as_string()),
        "\n",
    ))
}
fn unique(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for n in names {
        if !out.contains(&n) {
            out.push(n)
        }
    }
    out
}
fn enable(pi: &ExtensionApi, s: &mut State) -> Result<(), HandlerError> {
    if s.tools.is_none() {
        s.tools = Some(pi.get_active_tools()?);
    }
    pi.set_active_tools(unique(
        s.tools
            .as_ref()
            .unwrap()
            .iter()
            .filter(|n| !matches!(n.as_str(), "edit" | "write"))
            .cloned()
            .chain(PLAN.iter().map(|n| n.to_string())),
    ))
}
fn normal(pi: &ExtensionApi, s: &mut State) -> Result<(), HandlerError> {
    let tools = match &s.tools {
        Some(tools) => tools.clone(),
        None => unique(
            NORMAL.iter().map(|n| n.to_string()).chain(
                pi.get_active_tools()?
                    .into_iter()
                    .filter(|n| !PLAN.contains(&n.as_str()) && !NORMAL.contains(&n.as_str())),
            ),
        ),
    };
    pi.set_active_tools(tools)?;
    s.tools = None;
    Ok(())
}
fn toggle(pi: &ExtensionApi, shared: &Shared, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let mut s = shared.lock().unwrap();
    s.enabled = !s.enabled;
    s.executing = false;
    s.todos.clear();
    if s.enabled {
        enable(pi, &mut s)?;
        ctx.ui()?.notify(
            &JsString::from("Plan mode enabled. Built-in write tools disabled."),
            None,
        )?;
    } else {
        normal(pi, &mut s)?;
        ctx.ui()?
            .notify(&JsString::from("Plan mode disabled. Full access restored."), None)?;
    }
    update(&s, ctx)?;
    persist(pi, &s)
}
fn update(s: &State, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    let th = ui.theme()?;
    let status = if s.executing && !s.todos.is_empty() {
        Some(th.fg(
            C::Accent,
            &format!(
                "📋 {}/{}",
                s.todos.iter().filter(|t| t.completed).count(),
                s.todos.len()
            ),
        ))
    } else if s.enabled {
        Some(th.fg(C::Warning, "⏸ plan"))
    } else {
        None
    };
    ui.set_status(&JsString::from("plan-mode"), status.map(ExtensionStatus::Text))?;
    let lines = (s.executing && !s.todos.is_empty()).then(|| {
        s.todos
            .iter()
            .map(|t| {
                if t.completed {
                    th.fg(C::Success, "☑ ") + &th.fg(C::Muted, th.strikethrough(&t.text))
                } else {
                    th.fg(C::Muted, "☐ ") + &t.text
                }
            })
            .collect::<Vec<_>>()
    });
    ui.set_widget(&JsString::from("plan-todos"), lines.as_deref(), None)?;
    Ok(())
}
fn persist(pi: &ExtensionApi, s: &State) -> Result<(), HandlerError> {
    let todos: Vec<_> = s
        .todos
        .iter()
        .map(|t| json!({"step":t.step,"text":t.text.clone(),"completed":t.completed}))
        .collect();
    let mut data = json!({"enabled":s.enabled,"executing":s.executing,"todos":todos});
    if let Some(tools) = &s.tools {
        data["toolsBeforePlanMode"] = json!(tools)
    }
    pi.append_entry(&JsString::from("plan-mode"), Some(data))
}
fn remaining(todos: &[TodoItem]) -> JsString {
    JsString::join(
        todos
            .iter()
            .filter(|t| !t.completed)
            .map(|t| js_format!("{}. {}", t.step, t.text)),
        "\n",
    )
}
async fn finish_turn(
    pi: &ExtensionApi,
    shared: &Shared,
    ctx: &ExtensionContext,
    messages: &Value,
) -> Result<(), HandlerError> {
    {
        let mut s = shared.lock().unwrap();
        if s.executing && !s.todos.is_empty() {
            if s.todos.iter().all(|t| t.completed) {
                let list = JsString::join(s.todos.iter().map(|t| js_format!("~~{}~~", t.text)), "\n");
                pi.send_message(
                    custom_message("plan-complete", js_format!("**Plan Complete!** ✓\n\n{}", list), true),
                    Some(SendMessageOptions {
                        trigger_turn: Some(false),
                        ..Default::default()
                    }),
                )?;
                s.executing = false;
                s.todos.clear();
                update(&s, ctx)?;
                persist(pi, &s)?;
            }
            return Ok(());
        }
        if !s.enabled || !ctx.has_ui()? {
            return Ok(());
        }
        if let Some(text) = messages.as_array().into_iter().flatten().rev().find_map(assistant_text) {
            let extracted = extract_todo_items(&text);
            if !extracted.is_empty() {
                s.todos = extracted;
            }
        }
        if s.todos.is_empty() {
            return Ok(());
        }
        persist(pi, &s)?;
    }
    let todos = shared.lock().unwrap().todos.clone();
    let list = JsString::join(
        todos
            .iter()
            .enumerate()
            .map(|(i, t)| js_format!("{}. ☐ {}", i + 1, t.text)),
        "\n",
    );
    let plan_message = custom_message(
        "plan-todo-list",
        js_format!("**Plan Steps ({}):**\n\n{}", todos.len(), list),
        true,
    );
    let choice = ctx
        .ui()?
        .select(
            &JsString::from("Plan mode - what next?"),
            &[
                "Execute the plan (track progress)".into(),
                "Stay in plan mode".into(),
                "Refine the plan".into(),
            ],
            None,
        )
        .await?;
    if choice.as_ref().is_some_and(|c| c.starts_with("Execute")) {
        let mut s = shared.lock().unwrap();
        s.enabled = false;
        s.executing = true;
        normal(pi, &mut s)?;
        update(&s, ctx)?;
        persist(pi, &s)?;
        let content = js_format!(
            "Execute the plan.\n\nRemaining steps:\n{}\n\nStart with: {}\nAfter completing a step, include a [DONE:n] tag in your response.",
            remaining(&s.todos),
            s.todos[0].text.clone()
        );
        pi.send_message(
            plan_message,
            Some(SendMessageOptions {
                deliver_as: Some(MessageDelivery::FollowUp),
                ..Default::default()
            }),
        )?;
        pi.send_message(
            custom_message("plan-mode-execute", content, true),
            Some(SendMessageOptions {
                trigger_turn: Some(true),
                deliver_as: Some(MessageDelivery::FollowUp),
            }),
        )?;
    } else if choice.as_ref() == Some(&JsString::from("Refine the plan"))
        && let Some(text) = ctx
            .ui()?
            .editor(&JsString::from("Refine the plan:"), Some(&JsString::from("")))
            .await?
            .filter(|s| !(s).trim().is_empty())
    {
        pi.send_message(
            plan_message,
            Some(SendMessageOptions {
                deliver_as: Some(MessageDelivery::FollowUp),
                ..Default::default()
            }),
        )?;
        pi.send_user_message(
            UserMessageContent::Text((&text).trim().into()),
            Some(SendUserMessageOptions {
                deliver_as: Some(UserMessageDelivery::FollowUp),
                ..Default::default()
            }),
        )?;
    }
    Ok(())
}
fn restore(pi: &ExtensionApi, shared: &Shared, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let entries = ctx.session_manager()?.get_entries();
    let entries = js_json::to_value(&entries).unwrap();
    let entries = entries.as_array().unwrap();
    let saved = entries
        .iter()
        .rev()
        .find(|e| e["type"] == json!("custom") && e["customType"] == json!("plan-mode"));
    let mut s = shared.lock().unwrap();
    if pi.get_flag("plan")? == Some(FlagValue::Boolean(true)) {
        s.enabled = true;
    }
    if let Some(data) = saved.map(|e| &e["data"]).filter(|d| !d.is_null()) {
        if let Some(v) = data["enabled"].as_bool() {
            s.enabled = v;
        }
        if let Some(v) = data["executing"].as_bool() {
            s.executing = v;
        }
        if let Some(todos) = data["todos"].as_array() {
            s.todos = todos
                .iter()
                .map(|t| TodoItem {
                    step: t["step"].as_u64().unwrap() as usize,
                    text: t["text"].as_string().unwrap().into(),
                    completed: t["completed"].as_bool().unwrap(),
                })
                .collect();
        }
        if !data["toolsBeforePlanMode"].is_null() {
            s.tools =
                Some(js_json::from_value(&data["toolsBeforePlanMode"]).map_err(|e| HandlerError::new(e.to_string()))?);
        }
    }
    if saved.is_some() && s.executing && !s.todos.is_empty() {
        let start = entries
            .iter()
            .rposition(|e| e["customType"] == json!("plan-mode-execute"))
            .map_or(0, |i| i + 1);
        let text = JsString::join(
            entries[start..]
                .iter()
                .filter(|e| e["type"] == json!("message"))
                .filter_map(|e| assistant_text(&e["message"])),
            "\n",
        );
        mark_completed_steps(&text, &mut s.todos);
    }
    if s.enabled {
        enable(pi, &mut s)?;
    }
    update(&s, ctx)
}
