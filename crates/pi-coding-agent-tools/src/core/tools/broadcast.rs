//! `broadcast` tool (`core/tools/broadcast.ts`).

use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, OnceLock};

use crate::core::extensions::types::ContextTool;
use pi_agent_core::{
    AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_tui::tui::ComponentRef;
use serde::Deserialize;
use serde_json::Value as SchemaValue;

use super::result::text_result;
use crate::core::extensions::types::{
    ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call,
    infallible_render_result,
};
use crate::core::session_directory::{
    BroadcastSessionFilters, BroadcastSessionsInput, SessionDirectoryOptions, broadcast_to_sessions,
};
use crate::core::session_health::{SessionBroadcastOutcome, SessionBroadcastResult, SessionLiveStatus};
use crate::modes::interactive::theme::{Theme, ThemeColor};

pub const NAME: &str = "broadcast";
pub const LABEL: &str = "broadcast";
pub const DESCRIPTION: &str = "Send one message to eligible Pi sessions after sticky health checks. Dead sessions for the current agent generation are skipped until a newer agent starts.";
pub const PROMPT_SNIPPET: &str = "Broadcast a message to eligible sessions after liveness checks";
pub const PROMPT_GUIDELINES: [&str; 3] = [
    "Use list_sessions first when you need purpose/context for destination selection.",
    "broadcast never targets sticky-dead sessions for the current agent generation.",
    "Prefer filters (session_ids, cwd, name, status) over unfiltered broadcasts.",
];

#[derive(Debug, Deserialize)]
struct BroadcastParams {
    message: JsString,
    session_ids: Option<Vec<JsString>>,
    cwd: Option<JsString>,
    name: Option<JsString>,
    status: Option<Vec<JsString>>,
}

/// `createBroadcastToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct BroadcastTool;

impl BroadcastTool {
    pub fn new() -> Self {
        Self
    }
}

pub fn parameters() -> &'static SchemaValue {
    static PARAMETERS: OnceLock<SchemaValue> = OnceLock::new();
    PARAMETERS.get_or_init(|| {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Message body to deliver to eligible sessions."
                },
                "session_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional exact session id allowlist. When omitted, all inventory sessions are candidates."
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional exact cwd filter."
                },
                "name": {
                    "type": "string",
                    "description": "Optional exact session name filter."
                },
                "status": {
                    "type": "array",
                    "items": {
                        "anyOf": [
                            {"type": "string", "const": "running"},
                            {"type": "string", "const": "idle"},
                            {"type": "string", "const": "ended"}
                        ]
                    },
                    "description": "Optional status filter."
                }
            },
            "required": ["message"]
        })
    })
}

fn parse_statuses(status: Option<Vec<JsString>>) -> Option<Vec<SessionLiveStatus>> {
    status.map(|statuses| {
        statuses
            .into_iter()
            .filter_map(|status| match status.to_utf8().as_deref() {
                Ok("running") => Some(SessionLiveStatus::Running),
                Ok("idle") => Some(SessionLiveStatus::Idle),
                Ok("ended") => Some(SessionLiveStatus::Ended),
                _ => None,
            })
            .collect()
    })
}

fn format_broadcast_result(result: &SessionBroadcastResult) -> JsString {
    let error = result
        .error
        .as_ref()
        .map(|error| {
            let encoded = json::stringify_lossless(&Value::String(error.clone()));
            js_format!(" error={}", encoded)
        })
        .unwrap_or_default();
    js_format!(
        "{} outcome={} check={}{}",
        result.session_id,
        result.outcome.as_str(),
        result.check_status.as_str(),
        error
    )
}

impl ContextTool<Option<ExtensionContext>> for BroadcastTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.into()
    }

    fn parameters(&self) -> &SchemaValue {
        parameters()
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        LABEL.into()
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Sequential)
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
            let context = context.ok_or_else(|| ToolError::new("broadcast requires a control database path"))?;
            let control_db_path = context
                .control_db_path()
                .map_err(|error| ToolError::new(error.message))?
                .ok_or_else(|| ToolError::new("broadcast requires a control database path"))?;
            let session = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let (session_id, session_path) = {
                let manager = session;
                let session_path = manager
                    .get_session_file()
                    .ok_or_else(|| ToolError::new("broadcast requires a persisted session file"))?
                    .to_owned();
                (manager.get_session_id().to_owned(), session_path)
            };
            let params: BroadcastParams =
                pi_js_compat::js_json::from_value(&params).map_err(|error| ToolError::new(error.to_string()))?;
            let results = broadcast_to_sessions(
                &control_db_path,
                &BroadcastSessionsInput {
                    message: params.message,
                    filters: Some(BroadcastSessionFilters {
                        session_ids: params.session_ids,
                        cwd: params.cwd,
                        name: params.name,
                        status: parse_statuses(params.status),
                    }),
                    sender_session_id: session_id,
                    sender_session_path: session_path,
                    sender_agent_id: context
                        .multi_agent_agent_id()
                        .map_err(|error| ToolError::new(error.message))?,
                },
                &SessionDirectoryOptions::default(),
            )
            .map_err(|error| ToolError::new(error.to_string()))?;

            let sent = results
                .iter()
                .filter(|result| result.outcome == SessionBroadcastOutcome::Sent)
                .count();
            let failed = results
                .iter()
                .filter(|result| {
                    matches!(
                        result.outcome,
                        SessionBroadcastOutcome::SendFailed
                            | SessionBroadcastOutcome::CheckFailed
                            | SessionBroadcastOutcome::Timeout
                    )
                })
                .count();
            let skipped = results.len() - sent - failed;
            let lines: Vec<JsString> = if results.is_empty() {
                vec!["No sessions matched.".into()]
            } else {
                let mut lines = vec![js_format!("sent={} skipped={} failed={}", sent, skipped, failed)];
                lines.extend(
                    results
                        .iter()
                        .enumerate()
                        .map(|(index, result)| js_format!("{}. {}", index + 1, format_broadcast_result(result))),
                );
                lines
            };
            let details = json!({
                "results": results,
                "sent": sent,
                "skipped": skipped,
                "failed": failed
            });
            Ok(text_result(JsString::join(lines, "\n"), details))
        })
    }
}

fn format_call(args: &JsValue, theme: &Theme) -> JsString {
    let mut filters = Vec::new();
    if let Some(ids) = args
        .get("session_ids")
        .and_then(Value::as_array)
        .filter(|ids| !ids.is_empty())
    {
        filters.push(js_format!("ids={}", ids.len()));
    }
    if let Some(cwd) = args.get("cwd").and_then(Value::as_string).filter(|cwd| !cwd.is_empty()) {
        filters.push(js_format!("cwd={}", cwd));
    }
    if let Some(name) = args
        .get("name")
        .and_then(Value::as_string)
        .filter(|name| !name.is_empty())
    {
        filters.push(js_format!("name={}", name));
    }
    if let Some(status) = args
        .get("status")
        .and_then(Value::as_array)
        .filter(|status| !status.is_empty())
    {
        let status: Vec<JsString> = status
            .iter()
            .map(|value| {
                value
                    .as_string()
                    .cloned()
                    .unwrap_or_else(|| json::stringify_lossless(value).into())
            })
            .collect();
        filters.push(js_format!("status={}", JsString::join(status, ",")));
    }
    let filter_text = if filters.is_empty() {
        JsString::default()
    } else {
        js_format!(" {}", theme.fg(ThemeColor::Dim, JsString::join(filters, " ")))
    };
    js_format!(
        "{}{}",
        theme.fg(ThemeColor::ToolTitle, theme.bold("broadcast")),
        filter_text
    )
}

/// `renderCall` for the broadcast tool.
pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, format_call(args, theme))
}

/// `renderResult` for the broadcast tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    super::render_text_component(context, super::render_tool_output(result, theme))
}

/// `createBroadcastToolDefinition()`.
pub fn create_broadcast_tool_definition() -> ToolDefinition {
    let mut definition = crate::core::extensions::wrapper::context_tool_definition(Arc::new(BroadcastTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

#[cfg(test)]
mod native_filter_tests {
    use super::*;

    #[test]
    fn broadcast_filters_accept_raw_units_without_repair() {
        let wire = json::parse_lossless(r#"{"message":"body-\ud800","session_ids":["id-\ud801"],"cwd":"cwd-\ud802","name":"name-\ud803","status":["idle","unknown-\ud804"]}"#).unwrap();
        let params: BroadcastParams = json::from_value(&wire).unwrap();
        assert_eq!(params.message, wire["message"].as_string().unwrap().clone());
        assert_eq!(
            params.session_ids.unwrap()[0],
            wire["session_ids"][0].as_string().unwrap().clone()
        );
        assert_eq!(params.cwd.as_ref(), wire["cwd"].as_string());
        assert_eq!(params.name.as_ref(), wire["name"].as_string());
        assert_eq!(parse_statuses(params.status), Some(vec![SessionLiveStatus::Idle]));
    }
}
