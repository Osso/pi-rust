//! Frozen `extensions/session-autoname/src/index.ts`: model-driven session naming.
//! No separate title-provider request or lifecycle hooks.

use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::js_json::JsString;
use pi_js_compat::string::is_js_whitespace_unit;
use serde_json::json;

use crate::core::extensions::types::{ExtensionApi, ExtensionContext, HandlerError, ToolDefinition};
use crate::core::session_manager::lock_session_manager;

const MAX_NAME_CHARS: usize = 80;

/// Closure ownership lasts for this factory instance, including session switches.
pub fn session_autoname_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let name_set_by_tool = Arc::new(Mutex::new(None::<String>));
    let extension = pi.clone();
    pi.register_tool(ToolDefinition {
        name: "set_session_name".to_owned(),
        label: "Set Session Name".to_owned(),
        description: "Set this session's display name to a short title describing the user's request. Call it once, early in the session.".to_owned(),
        prompt_snippet: None,
        prompt_guidelines: Some(vec!["Call set_session_name once with a concise 2-4 word title describing the user's request, before doing substantial work.".to_owned()]),
        approval_required: Some(false),
        approval_kind: None,
        parameters: json!({
            "type": "object",
            "properties": {"name": {"type": "string", "description": "Short 2-4 word session title."}},
            "required": ["name"]
        }),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_id, params, _signal, _on_update, ctx| {
            let extension = extension.clone();
            let ownership = Arc::clone(&name_set_by_tool);
            Box::pin(async move {
                let raw = params.get("name").and_then(serde_json::Value::as_str)
                    .ok_or_else(|| HandlerError::new("set_session_name requires a string name"))?;
                let name = normalize_name(raw);
                let mut previous = ownership.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(refusal) = describe_refusal(&ctx, &name, previous.as_deref())? {
                    return Ok(name_result(&refusal, "", Some(true)));
                }
                // The existing extension setter and tool-result fields require UTF-8.
                // Do not silently renew the old lossy helper waiver at this boundary.
                let name = name.to_utf8().map_err(|_| HandlerError::new(
                    "set_session_name UTF-16 result is not representable by the current extension API; parity pending approval",
                ))?;
                extension.set_session_name(&name)?;
                *previous = Some(name.clone());
                Ok(name_result(&format!("Session name set to \"{name}\"."), &name, None))
            })
        }),
        render_call: None,
        render_result: None,
    }).map_err(HandlerError::new)
}

fn describe_refusal(
    ctx: &ExtensionContext,
    name: &JsString,
    name_set_by_tool: Option<&str>,
) -> Result<Option<String>, HandlerError> {
    if name.units().is_empty() {
        return Ok(Some("Session name must be a non-empty string.".to_owned()));
    }
    let manager = ctx.session_manager()?;
    let manager = lock_session_manager(&manager);
    if ctx.multi_agent_agent_id()?.is_some_and(|id| !id.is_empty()) || manager.is_subagent_session() {
        return Ok(Some("Child agent sessions cannot be named.".to_owned()));
    }
    if manager.get_session_file().is_none() {
        return Ok(Some("Only persisted sessions can be named.".to_owned()));
    }
    let current_name = manager.get_session_name();
    if current_name.is_none() && manager.has_session_name_state() {
        return Ok(Some(
            "The session name was cleared explicitly; leave it unnamed.".to_owned(),
        ));
    }
    if let Some(current) = current_name.filter(|current| Some(current.as_str()) != name_set_by_tool) {
        return Ok(Some(format!("The session is already named \"{current}\".")));
    }
    Ok(None)
}

fn normalize_name(raw: &str) -> JsString {
    let mut collapsed = Vec::new();
    let mut pending_space = false;
    for unit in raw.encode_utf16() {
        if is_js_whitespace_unit(unit) {
            pending_space = !collapsed.is_empty();
        } else {
            if pending_space {
                collapsed.push(0x20);
                pending_space = false;
            }
            collapsed.push(unit);
        }
    }
    if collapsed.len() > MAX_NAME_CHARS {
        let bounded = &collapsed[..MAX_NAME_CHARS + 1];
        let end = bounded
            .iter()
            .rposition(|unit| *unit == 0x20)
            .filter(|index| *index > 0)
            .unwrap_or(MAX_NAME_CHARS);
        collapsed.truncate(end);
        while collapsed.last().is_some_and(|unit| is_js_whitespace_unit(*unit)) {
            collapsed.pop();
        }
    }
    JsString::from_utf16(collapsed)
}

fn name_result(text: &str, name: &str, is_error: Option<bool>) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(json!({"name": name})),
        usage: None,
        added_tool_names: None,
        is_error,
        terminate: None,
    }
}
