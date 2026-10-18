//! `list_sessions` tool (`core/tools/list-sessions.ts`).

use std::sync::OnceLock;

use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::result::text_result;
use crate::core::extensions::types::ExtensionContext;
use crate::core::session_directory::{SessionDirectoryOptions, list_sessions};
use crate::core::session_health::SessionDirectoryEntry;

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
        json!({
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

fn format_session_line(session: &SessionDirectoryEntry) -> String {
    let name = session
        .name
        .as_ref()
        .map(|name| format!(" name={name}"))
        .unwrap_or_default();
    let goal = session
        .goal
        .as_ref()
        .map(|goal| {
            let encoded = serde_json::to_string(goal).expect("goal string must serialize");
            format!(" goal={encoded}")
        })
        .unwrap_or_default();
    let cwd = session
        .cwd
        .as_ref()
        .map(|cwd| format!(" cwd={cwd}"))
        .unwrap_or_default();
    let pid = match session.pid {
        Some(pid) => format!("pid={pid}"),
        None => "pid=null".to_owned(),
    };
    format!(
        "{} status={} check={} eligible={} {pid}{name}{goal}{cwd}",
        session.session_id,
        session.status.as_str(),
        session.check_status.as_str(),
        session.eligible_to_receive
    )
}

impl AgentHarnessTool<ExtensionContext> for ListSessionsTool {
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
        Some(ToolExecutionMode::Parallel)
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
                .ok_or_else(|| ToolError::new("list_sessions requires a control database path"))?;
            let params: ListSessionsParams =
                serde_json::from_value(params).map_err(|error| ToolError::new(error.to_string()))?;
            let sessions = list_sessions(
                &control_db_path,
                &SessionDirectoryOptions {
                    include_ended: Some(params.include_ended.unwrap_or(true)),
                    ..Default::default()
                },
            )
            .map_err(|error| ToolError::new(error.to_string()))?;
            let lines: Vec<String> = if sessions.is_empty() {
                vec!["No sessions found.".to_owned()]
            } else {
                sessions
                    .iter()
                    .enumerate()
                    .map(|(index, session)| format!("{}. {}", index + 1, format_session_line(session)))
                    .collect()
            };
            let details = json!({ "sessions": sessions });
            Ok(text_result(lines.join("\n"), details))
        })
    }
}
