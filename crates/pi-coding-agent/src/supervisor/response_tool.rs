//! Port of `packages/coding-agent/src/supervisor/response-tool.ts`.
//!
//! Tool registration is blocked on the Rust extension API; the schema, prompt metadata, and
//! execute result are ported here.

use serde_json::{Value, json};

pub const SUPERVISOR_RESPONSE_TOOL_NAME: &str = "supervisor_response";
pub const SUPERVISOR_RESPONSE_TOOL_LABEL: &str = "Supervisor response";
pub const SUPERVISOR_RESPONSE_TOOL_DESCRIPTION: &str =
    "Submit exactly one structured Supervisor decision and end the current request.";
pub const SUPERVISOR_RESPONSE_PROMPT_SNIPPET: &str = "Submit the final structured Supervisor decision";
pub const SUPERVISOR_RESPONSE_PROMPT_GUIDELINES: [&str; 2] = [
    "Call supervisor_response exactly once as the final action for every request.",
    "Do not emit assistant text, JSON, markdown, or call end_turn before or after supervisor_response.",
];

/// `supervisorResponseParameters`: TypeBox object with source property order.
pub fn supervisor_response_parameters() -> Value {
    let string_property = |description: &str| json!({"description": description, "type": "string"});
    json!({
        "type": "object",
        "properties": {
            "kind": {
                "anyOf": [
                    {"const": "approve", "type": "string"},
                    {"const": "reject", "type": "string"},
                    {"const": "complete", "type": "string"},
                    {"const": "continue", "type": "string"},
                    {"const": "pause", "type": "string"},
                    {"const": "wait", "type": "string"},
                    {"const": "set", "type": "string"},
                    {"const": "advisory", "type": "string"},
                    {"const": "error", "type": "string"},
                ]
            },
            "reason": string_property("Why this decision applies."),
            "instructions": string_property(
                "Generic active-goal reminder, or the smallest evidence-backed corrective action.",
            ),
            "objective": string_property("Additive objective for goal set decisions."),
            "answer": string_property("Text answer for advisory decisions."),
        },
        "required": ["kind"],
    })
}

/// `execute` result: advisory answers are trimmed, everything else is acknowledged.
pub fn supervisor_response_text(params: &Value) -> String {
    if params.get("kind").and_then(Value::as_str) == Some("advisory") {
        let answer = params.get("answer").and_then(Value::as_str).unwrap_or_default();
        if !answer.trim().is_empty() {
            return answer.to_owned();
        }
    }
    "Supervisor response recorded.".to_owned()
}
