//! `list_sessions` tool (`core/tools/list-sessions.ts`).

use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::{Arc, OnceLock};

use crate::core::extensions::types::ContextTool;
use pi_agent_core::{
    AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_tui::tui::ComponentRef;
use serde::Deserialize;
use serde_json::Value;

use super::result::text_result;
use crate::core::extensions::types::{
    ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call,
    infallible_render_result,
};
use crate::core::session_directory::{SessionDirectoryOptions, list_sessions};
use crate::core::session_health::SessionDirectoryEntry;
use crate::modes::interactive::theme::{Theme, ThemeColor};

pub const NAME: &str = "list_sessions";
pub const LABEL: &str = "list_sessions";
pub const DESCRIPTION: &str = "List known non-archived Pi sessions with purpose metadata and sticky liveness checks. Use this before messaging other sessions.";
pub const PROMPT_SNIPPET: &str = "List running/known non-archived sessions with purpose and liveness";
pub const PROMPT_GUIDELINES: [&str; 4] = [
    "Use list_sessions to discover other non-archived Pi sessions and inspect whether they are eligible to receive messages.",
    "Archived sessions are excluded regardless of include_ended.",
    "Prefer name and goal fields to understand what a session is for; ids alone are usually not useful.",
    "Sessions marked checkStatus=dead for the current agentGeneration are sticky-dead and should be skipped until a new agent starts for that session.",
];

#[derive(Debug, Default, Deserialize)]
struct ListSessionsParams {
    include_ended: Option<bool>,
}

/// `createListSessionsToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ListSessionsTool;

impl ListSessionsTool {
    pub fn new() -> Self {
        Self
    }
}

pub fn parameters() -> &'static Value {
    static PARAMETERS: OnceLock<Value> = OnceLock::new();
    PARAMETERS.get_or_init(|| {
        serde_json::json!({
            "type": "object",
            "properties": {
                "include_ended": {
                    "type": "boolean",
                    "description": "Include ended/sticky-dead non-archived sessions. Defaults to true so failures remain inspectable."
                }
            }
        })
    })
}

fn format_session_line(session: &SessionDirectoryEntry) -> JsString {
    let name = session
        .name
        .as_ref()
        .map(|name| js_format!(" name={}", name))
        .unwrap_or_default();
    let goal = session
        .goal
        .as_ref()
        .map(|goal| {
            let encoded = js_json::stringify_lossless(&JsValue::String(goal.clone()));
            js_format!(" goal={}", encoded)
        })
        .unwrap_or_default();
    let cwd = session
        .cwd
        .as_ref()
        .map(|cwd| js_format!(" cwd={}", cwd))
        .unwrap_or_default();
    let pid = match session.pid {
        Some(pid) => format!("pid={pid}"),
        None => "pid=null".to_owned(),
    };
    js_format!(
        "{} status={} check={} eligible={} {}{}{}{}",
        session.session_id,
        session.status.as_str(),
        session.check_status.as_str(),
        session.eligible_to_receive.to_string(),
        pid,
        name,
        goal,
        cwd
    )
}

impl ContextTool<Option<ExtensionContext>> for ListSessionsTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.into()
    }

    fn parameters(&self) -> &Value {
        parameters()
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        LABEL.into()
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Parallel)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        _signal: Option<AbortSignal>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: Option<ExtensionContext>,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let context = context.ok_or_else(|| ToolError::new("list_sessions requires a control database path"))?;
            let control_db_path = context
                .control_db_path()
                .map_err(|error| ToolError::new(error.message))?
                .ok_or_else(|| ToolError::new("list_sessions requires a control database path"))?;
            let params: ListSessionsParams =
                pi_js_compat::js_json::from_value(&params).map_err(|error| ToolError::new(error.to_string()))?;
            let sessions = list_sessions(
                &control_db_path,
                &SessionDirectoryOptions {
                    include_ended: Some(params.include_ended.unwrap_or(true)),
                    ..Default::default()
                },
            )
            .map_err(|error| ToolError::new(error.to_string()))?;
            let lines: Vec<JsString> = if sessions.is_empty() {
                vec!["No sessions found.".into()]
            } else {
                sessions
                    .iter()
                    .enumerate()
                    .map(|(index, session)| js_format!("{}. {}", index + 1, format_session_line(session)))
                    .collect()
            };
            let details = pi_js_compat::js_json!({ "sessions": sessions });
            Ok(text_result(JsString::join(lines, "\n"), details))
        })
    }
}

fn format_call(theme: &Theme) -> JsString {
    theme.fg(ThemeColor::ToolTitle, theme.bold("list_sessions"))
}

/// `renderCall` for the list_sessions tool.
pub fn render_call(_args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, format_call(theme))
}

/// `renderResult` for the list_sessions tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    super::render_text_component(context, super::render_tool_output(result, theme))
}

/// `create_list_sessions_tool_definition()`.
pub fn create_list_sessions_tool_definition() -> ToolDefinition {
    let mut definition = crate::core::extensions::wrapper::context_tool_definition(Arc::new(ListSessionsTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

#[cfg(test)]
mod native_output_tests {
    use super::*;

    #[test]
    fn session_line_keeps_raw_identity_name_and_cwd_and_quotes_goal() {
        let wire = js_json::parse_lossless(r#"{"sessionId":"id-\ud800","sessionPath":"/session","pid":null,"status":"idle","startedAt":"now","name":"name-\ud801","goal":"goal-\ud802","cwd":"cwd-\ud803","checkStatus":"never","agentGeneration":0,"eligibleToReceive":false}"#).unwrap();
        let session: SessionDirectoryEntry = js_json::from_value(&wire).unwrap();
        let expected = js_json::parse_lossless(r#""id-\ud800 status=idle check=never eligible=false pid=null name=name-\ud801 goal=\"goal-\\ud802\" cwd=cwd-\ud803""#).unwrap();
        assert_eq!(format_session_line(&session), expected.as_string().unwrap().clone());
    }
}
