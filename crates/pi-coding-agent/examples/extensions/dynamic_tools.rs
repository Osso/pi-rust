//! Opt-in native `dynamic-tools.ts`; embed and register its factory explicitly.
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::{
    BoxFuture, ExtensionApi, ExtensionContext, HandlerError, NotifyType, SessionStartEvent, ToolDefinition,
};
use pi_js_compat::string::js_trim;
use serde_json::json;

fn normalize_tool_name(input: &JsString) -> Option<String> {
    let name = input.trim().to_lowercase();
    let valid = !name.is_empty()
        && name
            .units()
            .iter()
            .all(|unit| *unit == u16::from(b'_') || u8::try_from(*unit).is_ok_and(|byte| byte.is_ascii_alphanumeric()));
    valid.then(|| name.to_utf8().expect("validated tool name is ASCII"))
}

fn register_echo_tool(
    pi: &ExtensionApi,
    names: &Mutex<HashSet<String>>,
    name: &str,
    label: &str,
    prefix: &str,
) -> Result<bool, HandlerError> {
    if !names
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(name.into())
    {
        return Ok(false);
    }
    let tool_name = name.to_owned();
    let tool_prefix = prefix.to_owned();
    pi.register_tool(ToolDefinition::define(ToolDefinition {
        name: name.into(),
        label: label.into(),
        description: format!("Echo a message with prefix: {prefix}").into(),
        prompt_snippet: Some(JsString::from(format!("Echo back user-provided text with {} prefix", js_trim(prefix)))),
        prompt_guidelines: Some(vec!["Use echo_session when the user asks for exact echo output.".into()]),
        parameters: json!({"type":"object","properties":{"message":{"type":"string","description":"Message to echo"}},"required":["message"]}),
        approval_required: None,
        approval_kind: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_id, params, _signal, _on_update, _ctx| {
            let name = tool_name.clone();
            let prefix = tool_prefix.clone();
            Box::pin(async move {
                let message = params["message"].as_string().ok_or_else(|| HandlerError::new("message must be a string"))?;
                Ok(AgentToolResult {
                    content: vec![TextOrImageContent::Text(TextContent::new(js_format!("{}{}", prefix, message)))],
                    details: Some(pi_js_compat::js_json!({"tool":name,"prefix":prefix})),
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: None,
        render_result: None,
    }))?;
    Ok(true)
}

pub fn dynamic_tools_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let names = Arc::new(Mutex::new(HashSet::new()));
    let start_api = pi.clone();
    let start_names = names.clone();
    pi.on_session_start(move |_event: SessionStartEvent, ctx: ExtensionContext| {
        let pi = start_api.clone();
        let names = start_names.clone();
        Box::pin(async move {
            register_echo_tool(&pi, &names, "echo_session", "Echo Session", "[session] ")?;
            ctx.ui()?.notify(
                &JsString::from("Registered dynamic tool: echo_session"),
                Some(NotifyType::Info),
            )?;
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    let command_api = pi.clone();
    pi.register_command(
        "add-echo-tool",
        Some("Register a new echo tool dynamically: /add-echo-tool <tool_name>".into()),
        None,
        Arc::new(move |args, ctx| {
            let pi = command_api.clone();
            let names = names.clone();
            Box::pin(async move {
                let Some(name) = normalize_tool_name(&args) else {
                    ctx.ui()?.notify(
                        &JsString::from("Usage: /add-echo-tool <tool_name> (lowercase, numbers, underscores)"),
                        Some(NotifyType::Warning),
                    )?;
                    return Ok(());
                };
                let created = register_echo_tool(&pi, &names, &name, &format!("Echo {name}"), &format!("[{name}] "))?;
                if !created {
                    ctx.ui()?.notify(
                        &js_format!("Tool already registered: {}", name),
                        Some(NotifyType::Warning),
                    )?;
                    return Ok(());
                }
                ctx.ui()?
                    .notify(&js_format!("Registered dynamic tool: {}", name), Some(NotifyType::Info))?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
