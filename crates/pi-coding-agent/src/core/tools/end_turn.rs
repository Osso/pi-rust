//! `end_turn` tool (`core/tools/end-turn.ts`).

use pi_js_compat::js_json::JsValue;
use std::sync::OnceLock;

use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::string::js_trim;
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
pub const PROMPT_GUIDELINES: [&str; 2] = [
    "Call end_turn only when the task is complete, progress requires user input, or the user explicitly asks you to stop. If work remains and progress is possible, continue working instead of calling end_turn. Assistant text alone does not finish the turn.",
    "Provide one concise, non-empty reason.",
];

#[derive(Debug, Deserialize)]
struct EndTurnParams {
    reason: String,
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

impl AgentHarnessTool<ExtensionContext> for EndTurnTool {
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
        params: JsValue,
        _signal: Option<CancellationToken>,
        _on_update: Option<AgentToolUpdateCallback>,
        _context: ExtensionContext,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let params: EndTurnParams =
                pi_js_compat::js_json::from_value(&params).map_err(|error| ToolError::new(error.to_string()))?;
            if js_trim(&params.reason).is_empty() {
                return Err(ToolError::new("end_turn reason must be a non-empty string"));
            }
            Ok(terminating_text_result(
                format!("Turn ended: {}", params.reason),
                json!({ "reason": params.reason }),
            ))
        })
    }
}

/// `createEndTurnToolDefinition()`.
pub fn create_end_turn_tool_definition() -> crate::core::extensions::types::ToolDefinition {
    let mut definition =
        crate::core::extensions::wrapper::harness_tool_definition(std::sync::Arc::new(EndTurnTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition
}
