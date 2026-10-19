//! `broadcast` tool (`core/tools/broadcast.ts`).

use std::sync::{Arc, OnceLock};

use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_tui::tui::ComponentRef;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::result::text_result;
use crate::core::extensions::types::{
    ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call,
    infallible_render_result,
};
use crate::core::session_directory::{
    BroadcastSessionFilters, BroadcastSessionsInput, SessionDirectoryOptions, broadcast_to_sessions,
};
use crate::core::session_health::{SessionBroadcastOutcome, SessionBroadcastResult, SessionLiveStatus};
use crate::core::session_manager::lock_session_manager;
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
    message: String,
    session_ids: Option<Vec<String>>,
    cwd: Option<String>,
    name: Option<String>,
    status: Option<Vec<String>>,
}

/// `createBroadcastToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct BroadcastTool;

impl BroadcastTool {
    pub fn new() -> Self {
        Self
    }
}

pub fn parameters() -> &'static Value {
    static PARAMETERS: OnceLock<Value> = OnceLock::new();
    PARAMETERS.get_or_init(|| {
        json!({
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
                    "items": { "type": "string", "enum": ["running", "idle", "ended"] },
                    "description": "Optional status filter."
                }
            },
            "required": ["message"]
        })
    })
}

fn parse_statuses(status: Option<Vec<String>>) -> Option<Vec<SessionLiveStatus>> {
    status.map(|statuses| {
        statuses
            .into_iter()
            .filter_map(|status| match status.as_str() {
                "running" => Some(SessionLiveStatus::Running),
                "idle" => Some(SessionLiveStatus::Idle),
                "ended" => Some(SessionLiveStatus::Ended),
                _ => None,
            })
            .collect()
    })
}

fn format_broadcast_result(result: &SessionBroadcastResult) -> String {
    let error = result
        .error
        .as_ref()
        .map(|error| {
            let encoded = serde_json::to_string(error).expect("error string must serialize");
            format!(" error={encoded}")
        })
        .unwrap_or_default();
    format!(
        "{} outcome={} check={}{error}",
        result.session_id,
        result.outcome.as_str(),
        result.check_status.as_str()
    )
}

impl AgentHarnessTool<ExtensionContext> for BroadcastTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &Value {
        parameters()
    }

    fn label(&self) -> &str {
        LABEL
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Sequential)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: ExtensionContext,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let control_db_path = context
                .control_db_path()
                .map_err(|error| ToolError::new(error.message))?
                .ok_or_else(|| ToolError::new("broadcast requires a control database path"))?;
            let session = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let (session_id, session_path) = {
                let manager = lock_session_manager(&session);
                let session_path = manager
                    .get_session_file()
                    .ok_or_else(|| ToolError::new("broadcast requires a persisted session file"))?
                    .to_owned();
                (manager.get_session_id().to_owned(), session_path)
            };
            let params: BroadcastParams =
                serde_json::from_value(params).map_err(|error| ToolError::new(error.to_string()))?;
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
            let lines: Vec<String> = if results.is_empty() {
                vec!["No sessions matched.".to_owned()]
            } else {
                let mut lines = vec![format!("sent={sent} skipped={skipped} failed={failed}")];
                lines.extend(
                    results
                        .iter()
                        .enumerate()
                        .map(|(index, result)| format!("{}. {}", index + 1, format_broadcast_result(result))),
                );
                lines
            };
            let details = json!({
                "results": results,
                "sent": sent,
                "skipped": skipped,
                "failed": failed
            });
            Ok(text_result(lines.join("\n"), details))
        })
    }
}

fn format_call(args: &Value, theme: &Theme) -> String {
    let mut filters = Vec::new();
    if let Some(ids) = args
        .get("session_ids")
        .and_then(Value::as_array)
        .filter(|ids| !ids.is_empty())
    {
        filters.push(format!("ids={}", ids.len()));
    }
    if let Some(cwd) = args.get("cwd").and_then(Value::as_str).filter(|cwd| !cwd.is_empty()) {
        filters.push(format!("cwd={cwd}"));
    }
    if let Some(name) = args.get("name").and_then(Value::as_str).filter(|name| !name.is_empty()) {
        filters.push(format!("name={name}"));
    }
    if let Some(status) = args
        .get("status")
        .and_then(Value::as_array)
        .filter(|status| !status.is_empty())
    {
        let status = status
            .iter()
            .map(|value| value.as_str().map_or_else(|| value.to_string(), str::to_owned))
            .collect::<Vec<_>>()
            .join(",");
        filters.push(format!("status={status}"));
    }
    let filter_text = if filters.is_empty() {
        String::new()
    } else {
        format!(" {}", theme.fg(ThemeColor::Dim, &filters.join(" ")))
    };
    format!(
        "{}{filter_text}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("broadcast"))
    )
}

/// `renderCall` for the broadcast tool.
pub fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
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
    let mut definition = crate::core::extensions::wrapper::harness_tool_definition(Arc::new(BroadcastTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.to_owned());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).to_owned()).collect());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}
