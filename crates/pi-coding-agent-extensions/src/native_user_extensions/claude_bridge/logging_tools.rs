//! Source index.ts extraction/delivery diagnostics, without changing actor maps.
use super::{logging::Logs, stream::string};
use indexmap::IndexMap;
use pi_ai::types::{Context, Message, ToolResultMessage};
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{self, JsString, JsValue},
};
use std::collections::{HashMap, HashSet};

fn tail_results(context: &Context) -> (Vec<&ToolResultMessage>, Option<usize>) {
    let stop = context
        .messages
        .iter()
        .rposition(|m| matches!(m, Message::Assistant(_)));
    let results = context.messages[stop.map_or(0, |i| i + 1)..]
        .iter()
        .filter_map(|m| match m {
            Message::ToolResult(result) => Some(result),
            _ => None,
        })
        .collect();
    (results, stop)
}
fn mcp_content(result: &ToolResultMessage) -> Result<JsValue, CallbackError> {
    let value = js_json::to_value(&result.content).map_err(|e| CallbackError::new(e.to_string()))?;
    Ok(super::convert::tool_result_to_mcp_content(&value))
}
impl Logs {
    pub(super) fn debug_extract_results(&self, context: &Context) -> Result<(), CallbackError> {
        let (results, stop) = tail_results(context);
        self.debug(format!(
            "extractAllToolResults: {} results from {} msgs, stopped at index {}",
            results.len(),
            context.messages.len(),
            stop.map_or_else(|| "-1".into(), |i| i.to_string())
        ))?;
        let messages = context
            .messages
            .iter()
            .map(|m| js_json::to_value(m).map_err(|e| CallbackError::new(e.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        self.debug(pi_js_compat::js_format!(
            "extractAllToolResults: all msg roles: {}",
            JsString::join(
                messages
                    .iter()
                    .enumerate()
                    .map(|(i, m)| pi_js_compat::js_format!("[{}]{}", i, string(m, "role"))),
                " "
            )
        ))?;
        for (i, result) in results.iter().enumerate() {
            self.debug(pi_js_compat::js_format!(
                "extractAllToolResults: result[{}] id={}{} preview: {}",
                i,
                result.tool_call_id,
                if result.is_error { " ERROR" } else { "" },
                JsString::from(js_json::stringify_lossless(&mcp_content(result)?)).slice(0, 150)
            ))?;
        }
        Ok(())
    }
    pub(super) fn debug_deliver_results(
        &self,
        context: &Context,
        calls: &IndexMap<String, JsValue>,
        queued: &HashMap<String, JsValue>,
    ) -> Result<(), CallbackError> {
        let (results, _) = tail_results(context);
        let mut pending = calls.keys().cloned().collect::<HashSet<_>>();
        let mut queued = queued.keys().cloned().collect::<HashSet<_>>();
        self.debug(format!(
            "provider: tool results, {} results, {} waiting handlers, ctx.msgs={}",
            results.len(),
            pending.len(),
            context.messages.len()
        ))?;
        for result in &results {
            let id = &result.tool_call_id;
            if pending.remove(id) {
                let name = calls
                    .get(id)
                    .and_then(|c| c.get("request"))
                    .and_then(|r| r.get("message"))
                    .and_then(|m| m.get("params"))
                    .map(|p| string(p, "name"))
                    .unwrap_or_default();
                self.debug(pi_js_compat::js_format!(
                    "provider: resolving {} [{}]{} {}",
                    name,
                    id,
                    if result.is_error { " (error)" } else { "" },
                    JsString::from(js_json::stringify_lossless(&mcp_content(result)?)).slice(0, 200)
                ))?;
            } else if !id.is_empty() {
                queued.insert(id.clone());
                self.debug(format!("provider: queued result [{id}] ({} pending)", queued.len()))?;
            } else {
                self.debug("WARNING: tool result without toolCallId, cannot match")?;
            }
            if !pending.is_empty() && !queued.is_empty() {
                self.debug(format!(
                    "BUG: both maps non-empty! handlers={} results={}",
                    pending.len(),
                    queued.len()
                ))?;
            }
        }
        if !pending.is_empty() {
            self.debug(format!(
                "WARNING: {} MCP handlers still waiting after delivering {} results",
                pending.len(),
                results.len()
            ))?;
        }
        Ok(())
    }
}
