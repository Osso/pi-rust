//! Shared result constructors for first-party tools.

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::Value;

/// `{ content: [{ type: "text", text }], details }` with every optional field absent.
pub fn text_result(text: impl Into<String>, details: Value) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(details),
        is_error: None,
        terminate: None,
    }
}

/// A text result flagged as a tool error.
pub fn error_text_result(text: impl Into<String>, details: Value) -> AgentToolResult {
    AgentToolResult {
        is_error: Some(true),
        ..text_result(text, details)
    }
}

/// A text result that ends the current turn.
pub fn terminating_text_result(text: impl Into<String>, details: Value) -> AgentToolResult {
    AgentToolResult {
        terminate: Some(true),
        ..text_result(text, details)
    }
}
