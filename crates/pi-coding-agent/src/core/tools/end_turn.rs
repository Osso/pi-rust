//! `end_turn` tool (`core/tools/end-turn.ts`).

use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::OnceLock;

use crate::core::extensions::types::ContextTool;
use pi_agent_core::{
    AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::result::terminating_text_result;
use crate::core::extensions::types::ExtensionContext;

pub const NAME: &str = "end_turn";
pub const LABEL: &str = "end_turn";
pub const DESCRIPTION: &str =
    "End the current model turn. Provide a concise reason explaining why the turn is finished.";
pub const PROMPT_SNIPPET: &str = "End the current model turn with a required reason";
pub const PROMPT_GUIDELINES: [&str; 3] = [
    "Call end_turn only when the task is complete, progress requires user input, or the user explicitly asks you to stop. If work remains and progress is possible, continue working instead of calling end_turn. Assistant text alone does not finish the turn.",
    "Provide one concise, non-empty reason.",
    "Thinking is never shown to the user. When responding to the user, write the reply as assistant text before calling end_turn; never leave an answer only in thinking.",
];

#[derive(Debug, Deserialize)]
struct EndTurnParams {
    reason: JsString,
}

/// `createEndTurnToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct EndTurnTool;

impl EndTurnTool {
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
                "reason": {
                    "type": "string",
                    "description": "Required non-empty reason for ending the current turn."
                }
            },
            "required": ["reason"]
        })
    })
}

impl ContextTool<ExtensionContext> for EndTurnTool {
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
        _signal: Option<CancellationToken>,
        _on_update: Option<AgentToolUpdateCallback>,
        _context: ExtensionContext,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let params: EndTurnParams =
                pi_js_compat::js_json::from_value(&params).map_err(|error| ToolError::new(error.to_string()))?;
            if params.reason.trim().is_empty() {
                return Err(ToolError::new("end_turn reason must be a non-empty string"));
            }
            Ok(terminating_text_result(
                pi_js_compat::js_format!("Turn ended: {}", params.reason),
                pi_js_compat::js_json!({ "reason": params.reason }),
            ))
        })
    }
}

/// `createEndTurnToolDefinition()`.
pub fn create_end_turn_tool_definition() -> crate::core::extensions::types::ToolDefinition {
    let mut definition =
        crate::core::extensions::wrapper::context_tool_definition(std::sync::Arc::new(EndTurnTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition
}
