//! Tool result constructors (`extensions/agents-core/src/tool-results.ts`).

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

pub fn tool_result(text: impl Into<JsString>, details: impl Into<JsValue>) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(details.into()),
        is_error: None,
        terminate: None,
    }
}

pub fn error_tool_result(text: impl Into<JsString>, details: impl Into<JsValue>) -> AgentToolResult {
    tool_result(text, details)
}

pub fn empty_tool_result() -> AgentToolResult {
    AgentToolResult {
        content: Vec::new(),
        details: Some(js_json!({})),
        is_error: None,
        terminate: None,
    }
}
