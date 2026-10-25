//! Opt-in native structured output tool with a terminating result.
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::ThemeColor;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::{components::text::Text, tui::component};
use serde_json::json;
use std::sync::Arc;

pub fn structured_output_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_tool(ToolDefinition::define(ToolDefinition {
        name: "structured_output".into(), label: "Structured Output".into(),
        description: "Return a final structured answer. Use this as your last action when the user asks for structured output or a machine-readable summary.".into(),
        prompt_snippet: Some("Emit a final structured answer as a terminating tool result".into()),
        prompt_guidelines: Some(vec![
            "Use structured_output as your final action when the user asks for structured output, JSON-like output, or a machine-readable summary.".into(),
            "After calling structured_output, do not emit another assistant response in the same turn.".into(),
        ]),
        parameters: json!({"type":"object","properties":{
            "headline":{"type":"string","description":"Short title for the result"},
            "summary":{"type":"string","description":"One-paragraph summary"},
            "actionItems":{"type":"array","items":{"type":"string"},"description":"Concrete next steps or key bullets"}},
            "required":["headline","summary","actionItems"]}),
        approval_required: None, approval_kind: None, render_shell: None,
        prepare_arguments: None, prepare_execution: None, execution_mode: None, render_call: None,
        execute: Arc::new(|_, params, _, _, _| Box::pin(async move {
            let headline = params["headline"].as_string().ok_or_else(|| HandlerError::new("headline must be a string"))?;
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new(js_format!("Saved structured output: {}", headline)))],
                details: Some(JsValue::from(json!({"headline":params["headline"],"summary":params["summary"],"actionItems":params["actionItems"]}))),
                terminate: Some(true), is_error: None,
            })
        })),
        render_result: Some(Arc::new(|result, _, theme, _| {
            let Some(details) = result.details.as_ref().filter(|value| !value.is_null()) else {
                let text = match result.content.first() {
                    Some(TextOrImageContent::Text(text)) => text.text.clone(),
                    _ => JsString::default(),
                };
                return Ok(component(Text::new(text, 0, 0)));
            };
            let headline = details["headline"].as_string().ok_or_else(|| HandlerError::new("headline must be a string"))?;
            let summary = details["summary"].as_string().ok_or_else(|| HandlerError::new("summary must be a string"))?;
            let items = details["actionItems"].as_array().ok_or_else(|| HandlerError::new("actionItems must be an array"))?;
            let mut lines = vec![theme.fg(ThemeColor::ToolTitle, &theme.bold(headline)), theme.fg(ThemeColor::Text, summary), JsString::default()];
            for (index, item) in items.iter().enumerate() {
                let item = item.as_string().ok_or_else(|| HandlerError::new("action item must be a string"))?;
                lines.push(theme.fg(ThemeColor::Muted, &pi_js_compat::js_format!("{}. {}", index + 1, item)));
            }
            Ok(component(Text::new(JsString::join(lines, "\n"), 0, 0)))
        })),
    }))
}
