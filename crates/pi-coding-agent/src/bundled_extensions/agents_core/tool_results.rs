//! Tool result constructors (`extensions/agents-core/src/tool-results.ts`).

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::{Value, json};

pub fn tool_result(text: impl Into<String>, details: Value) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(details),
        is_error: None,
        terminate: None,
    }
}

pub fn error_tool_result(text: impl Into<String>, details: Value) -> AgentToolResult {
    tool_result(text, details)
}

pub fn empty_tool_result() -> AgentToolResult {
    AgentToolResult {
        content: Vec::new(),
        details: Some(json!({})),
        is_error: None,
        terminate: None,
    }
}
