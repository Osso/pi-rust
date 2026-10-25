//! `ask_supervisor` tool (`core/tools/ask-supervisor.ts`).

use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{Arc, OnceLock};

use crate::core::extensions::types::ContextTool;
use pi_agent_core::{
    AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_tui::tui::ComponentRef;
use serde::Deserialize;
use serde_json::{Value, json};

use super::result::text_result;
use crate::core::extensions::types::{
    ExtensionContext, HandlerError, ToolDefinition, ToolRenderContext, ToolRenderResultOptions,
};
use crate::core::session_control_db::{SupervisorRequestKind, SupervisorResponse};
use crate::modes::interactive::theme::{Theme, ThemeColor};
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
    question: JsString,
    context: Option<JsString>,
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

impl ContextTool<Option<ExtensionContext>> for AskSupervisorTool {
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
            let context = context.ok_or_else(|| ToolError::new("ask_supervisor requires a control database path"))?;
            if context.no_supervisor().map_err(|error| ToolError::new(error.message))? {
                return Err(ToolError::new(
                    "Supervisor advisory is unavailable in standalone worker mode",
                ));
            }
            assert_main_session(&context, NAME)?;
            let control_db_path = require_control_db_path(&context, NAME)?;
            let session_manager = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let sender_session_id = session_manager.get_session_id().clone();
            if sender_session_id.is_empty() {
                return Err(ToolError::new("ask_supervisor requires a persisted session id"));
            }
            let params: AskSupervisorParams =
                pi_js_compat::js_json::from_value(&params).map_err(|error| ToolError::new(error.to_string()))?;
            let cwd = context.cwd().map_err(|error| ToolError::new(error.message))?;
            let project_id =
                resolve_supervisor_project_for_cwd(&cwd, &DEFAULT_SUPERVISOR_KB_DIR.into()).map_err(ToolError::new)?;
            let response = request_supervisor_decision(&RequestSupervisorDecisionInput {
                control_db_path: control_db_path.clone(),
                kind: SupervisorRequestKind::SupervisorAdvisory,
                payload: advisory_payload(&params.question, params.context.as_ref()),
                project_id: project_id.clone(),
                sender_session_id: sender_session_id.clone(),
                timeout_ms: SUPERVISOR_ADVISORY_TIMEOUT_MS as f64,
                signal: None,
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
                pi_js_compat::js_json!({
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
    if agent_id.as_ref().is_some_and(|id| !id.is_empty()) || requires_agent_id {
        return Err(ToolError::new(format!(
            "{tool_name} is only available from main sessions"
        )));
    }
    Ok(())
}

fn require_control_db_path(context: &ExtensionContext, tool_name: &str) -> Result<JsString, ToolError> {
    context
        .control_db_path()
        .map_err(|error| ToolError::new(error.message))?
        .filter(|path| !path.is_empty())
        .ok_or_else(|| ToolError::new(format!("{tool_name} requires a control database path")))
}

fn format_call(theme: &Theme) -> JsString {
    theme.fg(ThemeColor::ToolTitle, theme.bold("ask_supervisor"))
}

/// `renderCall` for the ask_supervisor tool.
pub fn render_call(_args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
    super::render_utils::set_text_component(format_call(theme), context)
}

/// `renderResult` for the ask_supervisor tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    super::render_utils::set_text_component(super::render_tool_output(result, theme), context)
}

/// `create_ask_supervisor_tool_definition()`.
pub fn create_ask_supervisor_tool_definition() -> ToolDefinition {
    let mut definition = crate::core::extensions::wrapper::context_tool_definition(Arc::new(AskSupervisorTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition.render_call = Some(Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}
