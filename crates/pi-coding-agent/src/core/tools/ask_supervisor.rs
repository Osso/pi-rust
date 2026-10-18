//! `ask_supervisor` tool (`core/tools/ask-supervisor.ts`).

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
use crate::core::session_control_db::{SupervisorRequestKind, SupervisorResponse};
use crate::supervisor::client::{RequestSupervisorDecisionInput, advisory_payload, request_supervisor_decision};
use crate::supervisor::project_resolver::{DEFAULT_SUPERVISOR_KB_DIR, resolve_supervisor_project_for_cwd};

const SUPERVISOR_ADVISORY_TIMEOUT_MS: i64 = 3 * 60 * 1_000;

pub const NAME: &str = "ask_supervisor";
pub const LABEL: &str = "ask_supervisor";
pub const DESCRIPTION: &str = "Ask the resident Supervisor for bounded advisory guidance.";
pub const PROMPT_SNIPPET: &str = "Ask the resident Supervisor";
pub const PROMPT_GUIDELINES: [&str; 3] = [
    "Use this tool for direct advisory questions to the Supervisor.",
    "Supervisor answers are advisory text and do not mutate goals, policies, sessions, processes, or agents.",
    "Requests are durable and wait up to three minutes for a response.",
];

#[derive(Debug, Deserialize)]
struct AskSupervisorParams {
    question: String,
    context: Option<String>,
}

/// `createAskSupervisorToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AskSupervisorTool;

impl AskSupervisorTool {
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
                "question": {
                    "type": "string",
                    "description": "Bounded advisory question for the resident Supervisor.",
                    "maxLength": 4_000
                },
                "context": {
                    "type": "string",
                    "description": "Optional bounded evidence relevant to the question.",
                    "maxLength": 8_000
                }
            },
            "required": ["question"]
        })
    })
}

impl AgentHarnessTool<ExtensionContext> for AskSupervisorTool {
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
            assert_main_session(&context, NAME)?;
            let control_db_path = require_control_db_path(&context, NAME)?;
            let sender_session_id = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?
                .get_session_id()
                .to_owned();
            if sender_session_id.is_empty() {
                return Err(ToolError::new("ask_supervisor requires a persisted session id"));
            }
            let params: AskSupervisorParams =
                serde_json::from_value(params).map_err(|error| ToolError::new(error.to_string()))?;
            let cwd = context.cwd().map_err(|error| ToolError::new(error.message))?;
            let project_id =
                resolve_supervisor_project_for_cwd(&cwd, DEFAULT_SUPERVISOR_KB_DIR).map_err(ToolError::new)?;
            let response = request_supervisor_decision(&RequestSupervisorDecisionInput {
                control_db_path: control_db_path.to_owned(),
                kind: SupervisorRequestKind::SupervisorAdvisory,
                payload: advisory_payload(&params.question, params.context.as_deref()),
                project_id: project_id.clone(),
                sender_session_id: sender_session_id.to_owned(),
                timeout_ms: SUPERVISOR_ADVISORY_TIMEOUT_MS,
                max_attempts: None,
                poll_interval_ms: None,
                retry_delay_ms: None,
                retry_jitter_ratio: None,
            })
            .await
            .map_err(|error| ToolError::new(error.to_string()))?;
            let answer = match response {
                SupervisorResponse::Advisory { answer } => answer,
                SupervisorResponse::Error { reason } => return Err(ToolError::new(reason)),
                _ => return Err(ToolError::new("Invalid Supervisor advisory response")),
            };
            Ok(text_result(
                answer.clone(),
                json!({
                    "answer": answer,
                    "projectId": project_id,
                    "senderSessionId": sender_session_id
                }),
            ))
        })
    }
}

fn assert_main_session(context: &ExtensionContext, tool_name: &str) -> Result<(), ToolError> {
    let agent_id = context
        .multi_agent_agent_id()
        .map_err(|error| ToolError::new(error.message))?;
    let requires_agent_id = context
        .multi_agent_requires_agent_id()
        .map_err(|error| ToolError::new(error.message))?;
    if agent_id.is_some() || requires_agent_id {
        return Err(ToolError::new(format!(
            "{tool_name} is only available from main sessions"
        )));
    }
    Ok(())
}

fn require_control_db_path(context: &ExtensionContext, tool_name: &str) -> Result<String, ToolError> {
    context
        .control_db_path()
        .map_err(|error| ToolError::new(error.message))?
        .ok_or_else(|| ToolError::new(format!("{tool_name} requires a control database path")))
}
