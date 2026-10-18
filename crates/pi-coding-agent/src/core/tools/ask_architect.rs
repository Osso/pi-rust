//! `ask_architect` tool (`core/tools/ask-architect.ts`).

use std::path::Path;
use std::sync::OnceLock;

use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::string::js_trim;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::result::text_result;
use crate::core::extensions::types::ExtensionContext;
use crate::core::session_control_db::{PostArchitectRequestInput, post_architect_request};

pub const NAME: &str = "ask_architect";
pub const LABEL: &str = "ask_architect";
pub const DESCRIPTION: &str = "Queue a durable request for the resident Architect.";
pub const PROMPT_SNIPPET: &str = "Ask the resident Architect";
pub const PROMPT_GUIDELINES: [&str; 3] = [
    "Use this tool for direct Architect requests; do not use channel_post for Architect requests.",
    "Set project_path to the project directory, such as the active worktree, when it differs from the session cwd.",
    "Requests remain queued across Architect restarts until processed.",
];

#[derive(Debug, Deserialize)]
struct AskArchitectParams {
    message: String,
    project_path: Option<String>,
}

/// `createAskArchitectToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AskArchitectTool;

impl AskArchitectTool {
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
                    "description": "Question or diagnostic request for the resident Architect."
                },
                "project_path": {
                    "type": "string",
                    "description": "Absolute project directory, such as the active worktree. Defaults to the session cwd."
                }
            },
            "required": ["message"]
        })
    })
}

fn resolve_project_path(project_path: Option<&str>, session_cwd: &str) -> Result<String, ToolError> {
    let Some(project_path) = project_path else {
        return Ok(session_cwd.to_owned());
    };
    if js_trim(project_path).is_empty() || !Path::new(project_path).is_absolute() {
        return Err(ToolError::new(
            "ask_architect project_path must be a non-empty absolute path",
        ));
    }
    let canonical_path = std::fs::canonicalize(project_path)
        .map_err(|_| ToolError::new("ask_architect project_path must reference an existing directory"))?;
    if !canonical_path.is_dir() {
        return Err(ToolError::new("ask_architect project_path must reference a directory"));
    }
    Ok(canonical_path.to_string_lossy().into_owned())
}

impl AgentHarnessTool<ExtensionContext> for AskArchitectTool {
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
            let agent_id = context
                .multi_agent_agent_id()
                .map_err(|error| ToolError::new(error.message))?;
            let requires_agent_id = context
                .multi_agent_requires_agent_id()
                .map_err(|error| ToolError::new(error.message))?;
            if agent_id.is_some() || requires_agent_id {
                return Err(ToolError::new("ask_architect is only available from main sessions"));
            }
            let control_db_path = context
                .control_db_path()
                .map_err(|error| ToolError::new(error.message))?
                .ok_or_else(|| ToolError::new("ask_architect requires a control database path"))?;
            let sender_session_id = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?
                .get_session_id()
                .to_owned();
            if sender_session_id.is_empty() {
                return Err(ToolError::new("ask_architect requires a persisted session id"));
            }
            let params: AskArchitectParams =
                serde_json::from_value(params).map_err(|error| ToolError::new(error.to_string()))?;
            let cwd = context.cwd().map_err(|error| ToolError::new(error.message))?;
            let project_cwd = resolve_project_path(params.project_path.as_deref(), &cwd)?;
            let request_id = post_architect_request(
                &control_db_path,
                &PostArchitectRequestInput {
                    sender_session_id: sender_session_id.to_owned(),
                    project_cwd,
                    body: params.message,
                },
            )
            .map_err(|error| ToolError::new(error.to_string()))?;
            Ok(text_result(
                format!("Architect request queued: {request_id}"),
                json!({ "requestId": request_id, "senderSessionId": sender_session_id }),
            ))
        })
    }
}
