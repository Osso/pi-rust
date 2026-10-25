//! Frozen `session-autoname`: the model names persisted main sessions through a tool.

use std::sync::{Arc, Mutex};

use pi_agent_core::agent::types::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::string::is_js_whitespace_unit;
use serde_json::json;

use crate::core::extensions::types::{ExtensionApi, ExtensionContext, HandlerError, ToolDefinition};

const MAX_NAME_CHARS: usize = 80;
type SharedToolName = Arc<Mutex<Option<JsString>>>;

pub fn session_autoname_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let name_set_by_tool: SharedToolName = Arc::new(Mutex::new(None));
    let extension = pi.clone();
    pi.register_tool(ToolDefinition {
        name: "set_session_name".into(),
        label: "Set Session Name".into(),
        description: "Set this session's display name to a short title describing the user's request. Call it once, early in the session.".into(),
        prompt_snippet: None,
        prompt_guidelines: Some(vec!["Call set_session_name once with a concise 2-4 word title describing the user's request, before doing substantial work.".into()]),
        approval_required: Some(false),
        approval_kind: None,
        parameters: json!({"type": "object", "properties": {"name": {"type": "string", "description": "Short 2-4 word session title."}}, "required": ["name"]}),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_id, params, _signal, _update, ctx| {
            let extension = extension.clone();
            let name_set_by_tool = name_set_by_tool.clone();
            Box::pin(async move {
                let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
                execute_name_tool(&extension, &ctx, &params, &name_set_by_tool)
            })
        }),
        render_call: None,
        render_result: None,
    })
}

fn execute_name_tool(
    pi: &ExtensionApi,
    ctx: &ExtensionContext,
    params: &JsValue,
    name_set_by_tool: &SharedToolName,
) -> Result<AgentToolResult, HandlerError> {
    let raw_name = params
        .get("name")
        .and_then(JsValue::as_string)
        .ok_or_else(|| HandlerError::new("Session name must be a string."))?;
    let name = normalize_name(raw_name);
    let previous = name_set_by_tool
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Some(refusal) = describe_refusal(ctx, &name, previous.as_ref())? {
        return Ok(name_result(refusal, JsString::default(), Some(true)));
    }
    pi.set_session_name(&name)?;
    *name_set_by_tool
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(name.clone());
    let message = JsString::from("Session name set to \"") + &name + "\".";
    Ok(name_result(message, name, None))
}

fn describe_refusal(
    ctx: &ExtensionContext,
    name: &JsString,
    name_set_by_tool: Option<&JsString>,
) -> Result<Option<JsString>, HandlerError> {
    if name.is_empty() {
        return Ok(Some("Session name must be a non-empty string.".into()));
    }
    let manager = ctx.session_manager()?;
    if ctx.multi_agent_agent_id()?.is_some_and(|id| !id.is_empty()) || manager.is_subagent_session() {
        return Ok(Some("Child agent sessions cannot be named.".into()));
    }
    let manager = manager;
    if manager.get_session_file().is_none() {
        return Ok(Some("Only persisted sessions can be named.".into()));
    }
    let current_name = manager.get_session_name();
    if current_name.is_none() && manager.has_session_name_state() {
        return Ok(Some(
            "The session name was cleared explicitly; leave it unnamed.".into(),
        ));
    }
    if let Some(current) = current_name.as_ref()
        && Some(current) != name_set_by_tool
    {
        return Ok(Some(
            JsString::from("The session is already named \"") + current + "\".",
        ));
    }
    Ok(None)
}

fn normalize_name(raw_name: &JsString) -> JsString {
    let words = raw_name
        .units()
        .split(|unit| is_js_whitespace_unit(*unit))
        .filter(|word| !word.is_empty())
        .map(|word| JsString::from_utf16(word.to_vec()));
    let collapsed = JsString::join(words, " ");
    if collapsed.len() <= MAX_NAME_CHARS {
        return collapsed;
    }
    let bounded = collapsed.slice(0, MAX_NAME_CHARS + 1);
    let end = bounded
        .units()
        .iter()
        .rposition(|unit| *unit == u16::from(b' '))
        .filter(|index| *index > 0)
        .unwrap_or(MAX_NAME_CHARS);
    bounded.slice(0, end).trim()
}

fn name_result(message: JsString, name: JsString, is_error: Option<bool>) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(message))],
        details: Some(pi_js_compat::js_json!({"name": name})),
        is_error,
        terminate: None,
    }
}
