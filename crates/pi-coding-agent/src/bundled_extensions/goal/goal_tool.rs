//! Port of `packages/coding-agent/extensions/goal/src/goal-tool.ts`.

use std::sync::Arc;

use pi_agent_core::AgentToolResult;
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
    pub objective: Option<String>,
    pub reason: Option<String>,
    pub completion_report: Option<String>,
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

fn parse_action(value: Option<&Value>) -> Option<ManageGoalAction> {
    match value?.as_str()? {
        "set" => Some(ManageGoalAction::Set),
        "pause" => Some(ManageGoalAction::Pause),
        "resume" => Some(ManageGoalAction::Resume),
        "complete" => Some(ManageGoalAction::Complete),
        "clear" => Some(ManageGoalAction::Clear),
        "status" => Some(ManageGoalAction::Status),
        _ => None,
    }
}

/// Parses the tool arguments into the typed params the executor consumes.
pub fn manage_goal_params(params: &Value) -> ManageGoalParams {
    let Some(record) = params.as_object() else {
        return ManageGoalParams::default();
    };
    ManageGoalParams {
        action: parse_action(record.get("action")),
        objective: record.get("objective").and_then(Value::as_str).map(str::to_owned),
        reason: record.get("reason").and_then(Value::as_str).map(str::to_owned),
        completion_report: record
            .get("completionReport")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

/// `registerManageGoalTool`.
pub fn register_manage_goal_tool(pi: &ExtensionApi, execute: ManageGoalExecute) -> Result<(), String> {
    let tool_execute: ToolExecute = Arc::new(move |_tool_call_id, params, _signal, _on_update, ctx| {
        let execute = Arc::clone(&execute);
        let params = manage_goal_params(&params);
        Box::pin(async move { execute(params, ctx).await })
    });
    pi.register_tool(ToolDefinition {
        name: "manage_goal".to_owned(),
        label: "Manage Goal".to_owned(),
        description: "Manage the active long-running /goal objective. Pause requires a concrete reason; complete requires a proof-backed completion report.".to_owned(),
        prompt_snippet: None,
        prompt_guidelines: Some(Vec::new()),
        approval_required: Some(false),
        approval_kind: None,
        parameters: manage_goal_parameters(),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: tool_execute,
        render_call: None,
        render_result: None,
    })
}
