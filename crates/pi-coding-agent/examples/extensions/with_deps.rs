//! Opt-in native `with-deps/index.ts`: embed this file and register its factory explicitly.
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::{ExtensionApi, HandlerError, ToolDefinition};
use serde_json::json;

pub fn with_deps_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_tool(ToolDefinition::define(ToolDefinition {
        name: "parse_duration".into(),
        label: "Parse Duration".into(),
        description: "Parse a human-readable duration string (e.g., '2 days', '1h', '5m') to milliseconds".into(),
        parameters: json!({"type":"object","properties":{"duration":{"type":"string","description":"Duration string like '2 days', '1h', '5m'"}},"required":["duration"]}),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_id, params, _signal, _on_update, _ctx| Box::pin(async move {
            let duration = params["duration"].as_string().ok_or_else(|| HandlerError::new("duration must be a string"))?;
            let value = parse_duration(duration)?;
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new(js_format!("{} = {} milliseconds", duration, pi_js_compat::number::js_number_to_string(value))))],
                details: Some(JsValue::from(json!({}))),
                is_error: None,
                terminate: None,
            })
        })),
        render_call: None,
        render_result: None,
    }))
}

// Only ms's string overload is exercised by the Type.String tool schema.
fn parse_duration(text: &JsString) -> Result<f64, HandlerError> {
    if text.is_empty() {
        return Err(HandlerError::new(
            "val is not a non-empty string or a valid number. val=\"\"",
        ));
    }
    let invalid = || HandlerError::new(js_format!("Invalid duration: \"{}\"", text));
    if text.encode_utf16().count() > 100 {
        return Err(invalid());
    }
    let pattern = pi_js_compat::js_regex::JsRegex::with_flags(r"^(-?(?:[0-9]+)?\.?[0-9]+) *(milliseconds?|msecs?|ms|seconds?|secs?|s|minutes?|mins?|m|hours?|hrs?|h|days?|d|weeks?|w|years?|yrs?|y)?$", "i").expect("constant ms pattern");
    let found = pattern.captures(text).ok_or_else(invalid)?;
    let digits = found
        .get(1)
        .expect("duration number capture")
        .text()
        .to_utf8()
        .expect("duration pattern captures ASCII digits only");
    let n: f64 = digits.parse().map_err(|_| invalid())?;
    let factor = match found
        .get(2)
        .map(|m| m.text().to_utf8().expect("duration unit is ASCII").to_ascii_lowercase())
        .as_deref()
        .unwrap_or("ms")
    {
        "y" | "yr" | "yrs" | "year" | "years" => 31_557_600_000.0,
        "w" | "week" | "weeks" => 604_800_000.0,
        "d" | "day" | "days" => 86_400_000.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000.0,
        "m" | "min" | "mins" | "minute" | "minutes" => 60_000.0,
        "s" | "sec" | "secs" | "second" | "seconds" => 1000.0,
        _ => 1.0,
    };
    Ok(n * factor)
}
