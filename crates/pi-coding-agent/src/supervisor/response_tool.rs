//! Supervisor structured response tool (`supervisor/response-tool.ts`).

use std::sync::Arc;

use pi_agent_core::agent::AgentToolResult;
use pi_agent_core::harness::ToolExecutionMode;
use pi_ai::types::TextOrImageContent;
use serde_json::{Value, json};

use crate::core::extensions::types::ToolDefinition;

pub const SUPERVISOR_RESPONSE_TOOL_NAME: &str = "supervisor_response";

fn string_property(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

/// TypeBox schema for the response parameters.
pub fn supervisor_response_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {
                "anyOf": [
                    {"type": "string", "const": "approve"},
                    {"type": "string", "const": "reject"},
                    {"type": "string", "const": "complete"},
                    {"type": "string", "const": "continue"},
                    {"type": "string", "const": "pause"},
                    {"type": "string", "const": "wait"},
                    {"type": "string", "const": "set"},
                    {"type": "string", "const": "advisory"},
                    {"type": "string", "const": "error"},
                ]
            },
            "reason": string_property("Why this decision applies."),
            "instructions": string_property(
                "Generic active-goal reminder, or the smallest evidence-backed corrective action."
            ),
            "objective": string_property("Additive objective for goal set decisions."),
            "answer": string_property("Text answer for advisory decisions."),
        },
        "required": ["kind"]
    })
}

/// Text returned by the tool for one parameter object.
pub fn supervisor_response_text(params: &Value) -> String {
    let answer = params.get("answer").and_then(Value::as_str);
    match params.get("kind").and_then(Value::as_str) {
        Some("advisory") if answer.is_some_and(|answer| !pi_js_compat::string::js_trim(answer).is_empty()) => {
            answer.unwrap_or_default().to_owned()
        }
        _ => "Supervisor response recorded.".to_owned(),
    }
}

/// `createSupervisorResponseTool`.
pub fn create_supervisor_response_tool() -> ToolDefinition {
    ToolDefinition {
        name: SUPERVISOR_RESPONSE_TOOL_NAME.to_owned(),
        label: "Supervisor response".to_owned(),
        description: "Submit exactly one structured Supervisor decision and end the current request.".to_owned(),
        prompt_snippet: Some("Submit the final structured Supervisor decision".to_owned()),
        prompt_guidelines: Some(vec![
            "Call supervisor_response exactly once as the final action for every request.".to_owned(),
            "Do not emit assistant text, JSON, markdown, or call end_turn before or after supervisor_response."
                .to_owned(),
        ]),
        approval_required: None,
        approval_kind: None,
        parameters: supervisor_response_parameters(),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: Some(ToolExecutionMode::Sequential),
        execute: Arc::new(|_tool_call_id, params, _signal, _on_update, _context| {
            Box::pin(async move {
                let text = supervisor_response_text(&params);
                Ok(AgentToolResult {
                    content: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(text))],
                    details: Some(params),
                    is_error: None,
                    terminate: Some(true),
                })
            })
        }),
        render_call: None,
        render_result: None,
    }
}
