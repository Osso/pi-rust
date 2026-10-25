//! Port of `packages/coding-agent/extensions/goal/src/goal-tool.ts`.

use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::{Value, json};

use crate::core::extensions::types::{
    BoxFuture, ExtensionApi, ExtensionContext, HandlerError, ToolDefinition, ToolExecute,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManageGoalAction {
    Set,
    Pause,
    Resume,
    Complete,
    Clear,
    Status,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManageGoalParams {
    pub action: Option<ManageGoalAction>,
    pub objective: Option<JsString>,
    pub reason: Option<JsString>,
    pub completion_report: Option<JsString>,
}

pub type ManageGoalExecute = Arc<
    dyn Fn(ManageGoalParams, ExtensionContext) -> BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        + Send
        + Sync,
>;

fn action_literal(action: &str) -> Value {
    json!({"const": action, "type": "string"})
}

fn manage_goal_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "anyOf": [
                    action_literal("set"),
                    action_literal("pause"),
                    action_literal("resume"),
                    action_literal("complete"),
                    action_literal("clear"),
                    action_literal("status")
                ]
            },
            "objective": {"type": "string"},
            "reason": {
                "type": "string",
                "description": "Required when pausing; used to explain why work stopped."
            },
            "completionReport": {
                "type": "string",
                "description": "Required when completing; concise Markdown proof covering implementation, tests/checks, commits, deployment, smoke tests, and remaining blockers."
            }
        },
        "required": ["action"]
    })
}

fn parse_action(value: Option<&JsValue>) -> Option<ManageGoalAction> {
    match value?.as_string()? {
        value if value == "set" => Some(ManageGoalAction::Set),
        value if value == "pause" => Some(ManageGoalAction::Pause),
        value if value == "resume" => Some(ManageGoalAction::Resume),
        value if value == "complete" => Some(ManageGoalAction::Complete),
        value if value == "clear" => Some(ManageGoalAction::Clear),
        value if value == "status" => Some(ManageGoalAction::Status),
        _ => None,
    }
}

/// Parses the tool arguments into the typed params the executor consumes.
pub fn manage_goal_params(params: &JsValue) -> ManageGoalParams {
    let Some(record) = params.as_object() else {
        return ManageGoalParams::default();
    };
    ManageGoalParams {
        action: parse_action(record.get("action")),
        objective: record.get("objective").and_then(JsValue::as_string).cloned(),
        reason: record.get("reason").and_then(JsValue::as_string).cloned(),
        completion_report: record.get("completionReport").and_then(JsValue::as_string).cloned(),
    }
}

/// `registerManageGoalTool`.
pub fn register_manage_goal_tool(
    pi: &ExtensionApi,
    execute: ManageGoalExecute,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    let tool_execute: ToolExecute = Arc::new(move |_tool_call_id, params, _signal, _on_update, ctx| {
        let execute = Arc::clone(&execute);
        let params = manage_goal_params(&params);
        Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
            execute(params, ctx).await
        })
    });
    pi.register_tool(ToolDefinition {
        name: "manage_goal".to_owned(),
        label: "Manage Goal".into(),
        description: "Manage the active long-running /goal objective. Pause requires a concrete reason; complete requires a proof-backed completion report.".into(),
        prompt_snippet: None,
        prompt_guidelines: Some(Vec::new()),
        approval_required: Some(false),
        approval_kind: None,
        parameters: manage_goal_parameters(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: tool_execute,
        render_call: None,
        render_result: None,
    })
}
