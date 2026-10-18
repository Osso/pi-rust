//! Ported from `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts`.
//!
//! The wrapper caps tool results before they reach the model: when the concatenated text
//! exceeds the tail-truncation limits, the full text is spilled to a temp file and the
//! result is replaced with the truncated tail plus a footer. In Rust the coding-agent tools
//! already implement [`AgentTool`], so [`wrap_agent_tool`] applies the same capping instead
//! of adapting a separate `ToolDefinition` type (that type belongs to `core/extensions`).

use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::{Map, Value};

use super::truncate::{TruncationOptions, format_size, truncate_tail};
use crate::core::extensions::types::{HandlerError, ToolDefinition};

/// Shared spill state for one tool call: the temp file is created at most once.
#[derive(Debug, Default)]
pub struct ToolOutputSpill {
    pub full_output_path: Option<String>,
}

/// `capToolResultOutput`: spill truncated text and replace the result with its tail.
pub fn cap_tool_result_output(mut result: AgentToolResult, spill: &mut ToolOutputSpill) -> AgentToolResult {
    if has_existing_output_details(result.details.as_ref()) {
        return result;
    }

    let full_text = result
        .content
        .iter()
        .filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(text.text.as_str()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let truncation = truncate_tail(&full_text, TruncationOptions::default());
    if !truncation.truncated {
        return result;
    }

    let full_output_path = spill
        .full_output_path
        .get_or_insert_with(create_full_output_path)
        .clone();
    std::fs::write(&full_output_path, &full_text).expect("tool output spill file must be writable");
    let footer = format!(
        "[Showing last {} of {}. Full output: {}]",
        format_size(truncation.output_bytes),
        format_size(truncation.total_bytes),
        full_output_path
    );
    result.content = vec![TextOrImageContent::Text(TextContent::new(format!(
        "{}\n\n{footer}",
        truncation.content
    )))];
    result.details = Some(merge_output_details(result.details, &truncation, &full_output_path));
    result
}

/// `createToolDefinitionFromAgentTool`: synthesize a minimal definition from an [`AgentTool`].
pub fn create_tool_definition_from_agent_tool(tool: Arc<dyn AgentTool>) -> ToolDefinition {
    let execute_tool = Arc::clone(&tool);
    ToolDefinition {
        name: tool.name().to_owned(),
        label: tool.label().to_owned(),
        description: tool.description().to_owned(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: tool.parameters().clone(),
        constrained_sampling: tool.constrained_sampling(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: tool.execution_mode(),
        execute: Arc::new(move |tool_call_id, params, signal, on_update, _context| {
            let tool = Arc::clone(&execute_tool);
            let on_update = on_update.unwrap_or_else(|| Arc::new(|_result| {}));
            Box::pin(async move {
                tool.execute(tool_call_id, params, signal, on_update, None)
                    .await
                    .map_err(|error| HandlerError::new(error.message))
            })
        }),
        render_call: None,
        render_result: None,
    }
}

/// `wrapToolDefinition`: cap partial updates and the final result of an [`AgentTool`].
pub struct CappedTool {
    inner: Arc<dyn AgentTool>,
    spill: Arc<Mutex<ToolOutputSpill>>,
}

impl CappedTool {
    pub fn new(inner: Arc<dyn AgentTool>) -> Self {
        Self {
            inner,
            spill: Arc::new(Mutex::new(ToolOutputSpill::default())),
        }
    }
}

impl AgentTool for CappedTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> &Value {
        self.inner.parameters()
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn constrained_sampling(&self) -> Option<pi_ai::types::ConstrainedSampling> {
        self.inner.constrained_sampling()
    }

    fn execution_mode(&self) -> Option<pi_agent_core::harness::types::ToolExecutionMode> {
        self.inner.execution_mode()
    }

    fn prepare_arguments(&self, args: Value) -> Value {
        self.inner.prepare_arguments(args)
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: Value,
        signal: Option<tokio_util::sync::CancellationToken>,
        on_update: pi_agent_core::agent::AgentToolUpdateCallback,
        execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, pi_agent_core::agent::ToolError>> {
        let spill = Arc::clone(&self.spill);
        let update_spill = Arc::clone(&spill);
        let capped_update: pi_agent_core::agent::AgentToolUpdateCallback = Arc::new(move |partial| {
            let mut spill = update_spill.lock().expect("tool output spill lock");
            (on_update)(cap_tool_result_output(partial, &mut spill));
        });
        let result = self
            .inner
            .execute(tool_call_id, params, signal, capped_update, execution);
        Box::pin(async move {
            let result = result.await?;
            let mut spill = spill.lock().expect("tool output spill lock");
            Ok(cap_tool_result_output(result, &mut spill))
        })
    }
}

/// Wrap a tool so its results are capped like `wrapToolDefinition` does.
pub fn wrap_agent_tool(inner: Arc<dyn AgentTool>) -> Arc<dyn AgentTool> {
    Arc::new(CappedTool::new(inner))
}

fn has_existing_output_details(details: Option<&Value>) -> bool {
    match details {
        Some(Value::Object(details)) => details.contains_key("fullOutputPath") || details.contains_key("truncation"),
        _ => false,
    }
}

fn merge_output_details(
    details: Option<Value>,
    truncation: &super::truncate::TruncationResult,
    full_output_path: &str,
) -> Value {
    let mut merged = match details {
        Some(Value::Object(details)) => details,
        _ => Map::new(),
    };
    merged.insert(
        "truncation".to_owned(),
        serde_json::to_value(truncation).expect("truncation metadata must serialize"),
    );
    merged.insert("fullOutputPath".to_owned(), Value::String(full_output_path.to_owned()));
    Value::Object(merged)
}

fn create_full_output_path() -> String {
    let id = pi_ai::utils::uuid::random_uuid().replace('-', "");
    let id = &id[..16];
    std::env::temp_dir()
        .join(format!("pi-tool-{id}.log"))
        .to_string_lossy()
        .into_owned()
}
