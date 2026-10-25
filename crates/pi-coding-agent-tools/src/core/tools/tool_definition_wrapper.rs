//! Ported from `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts`.
//!
//! The wrapper converts an extension `ToolDefinition` into an [`AgentTool`]: it injects the
//! runner context and execution identity, caps partial updates plus the final result, and
//! spills oversized text to a temp file. Built-in tools already implement [`AgentTool`], so
//! [`wrap_agent_tool`] applies the same capping without a definition adapter.

use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolExecutionContext;
use pi_agent_core::agent::{AgentTool, AgentToolResult, ToolError};
use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::Value;

use super::node_fs::node_fs_error;
use super::truncate::{TruncationOptions, format_size, truncate_tail};
use crate::core::extensions::types::{ExtensionContext, HandlerError, ToolDefinition};

/// Shared spill state for one tool call: the temp file is created at most once.
#[derive(Debug, Default)]
pub struct ToolOutputSpill {
    pub full_output_path: Option<JsString>,
}

/// `capToolResultOutput`: spill truncated text and replace the result with its tail.
pub fn cap_tool_result_output(
    mut result: AgentToolResult,
    spill: &mut ToolOutputSpill,
) -> Result<AgentToolResult, ToolError> {
    if has_existing_output_details(result.details.as_ref()) {
        return Ok(result);
    }

    let full_text = JsString::join(
        result.content.iter().filter_map(|entry| match entry {
            TextOrImageContent::Text(text) => Some(&text.text),
            TextOrImageContent::Image(_) => None,
        }),
        "\n",
    );
    let truncation = truncate_tail(&full_text, TruncationOptions::default());
    if !truncation.truncated {
        return Ok(result);
    }

    if spill.full_output_path.is_none() {
        spill.full_output_path = Some(create_full_output_path()?);
    }
    let full_output_path = spill.full_output_path.as_ref().expect("spill path initialized").clone();
    std::fs::write(encode_node_utf8(&full_output_path), encode_node_utf8(&full_text))
        .map_err(|error| ToolError::new(node_fs_error(&error, "open", &full_output_path)))?;
    let footer = pi_js_compat::js_format!(
        "[Showing last {} of {}. Full output: {}]",
        format_size(truncation.output_bytes as f64),
        format_size(truncation.total_bytes as f64),
        full_output_path
    );
    result.content = vec![TextOrImageContent::Text(TextContent::new(
        truncation.content.clone() + "\n\n" + footer,
    ))];
    result.details = Some(merge_output_details(result.details, &truncation, &full_output_path));
    Ok(result)
}

/// `createToolDefinitionFromAgentTool`: synthesize a minimal definition from an [`AgentTool`].
pub fn create_tool_definition_from_agent_tool(tool: Arc<dyn AgentTool>) -> ToolDefinition {
    let execute_tool = Arc::clone(&tool);
    let prepare_arguments_tool = Arc::clone(&tool);
    let prepare_execution = tool.prepare_execution().map(|prepare| {
        let callback: crate::core::extensions::types::PrepareExecution =
            Arc::new(move |tool_call_id, signal, _context| {
                let prepare = Arc::clone(&prepare);
                Box::pin(async move { prepare(tool_call_id, signal).await.map_err(HandlerError::from) })
            });
        callback
    });
    ToolDefinition {
        name: tool.name().to_owned(),
        label: tool.label(),
        description: tool.description(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: tool.parameters().clone(),
        render_shell: None,
        prepare_arguments: Some(Arc::new(move |args: &JsValue| {
            prepare_arguments_tool
                .prepare_arguments(args.clone())
                .map_err(|error| HandlerError::new(error.message))
        })),
        prepare_execution,
        execution_mode: tool.execution_mode(),
        execute: Arc::new(move |tool_call_id, params, signal, on_update, _context| {
            let tool = Arc::clone(&execute_tool);
            let on_update = on_update.unwrap_or_else(|| Arc::new(|_result| Ok(())));
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

/// Supplies the extension context for a wrapped definition (`ctxFactory`).
pub type ToolContextFactory = Arc<dyn Fn() -> ExtensionContext + Send + Sync>;

/// `createToolExecutionContext`: the base context plus the execution identity.
pub fn create_tool_execution_context(
    factory: Option<&ToolContextFactory>,
    execution: Option<&AgentToolExecutionContext>,
) -> Option<ExtensionContext> {
    let mut context = factory?();
    context.tool_execution_started_at = execution.map(|execution| execution.started_at);
    context.tool_execution_agent_id = execution.and_then(|execution| execution.agent_id.clone());
    Some(context)
}

/// `wrapToolDefinition`: a [`ToolDefinition`] exposed as an [`AgentTool`].
pub struct DefinitionTool {
    definition: ToolDefinition,
    context_factory: Option<ToolContextFactory>,
}

impl DefinitionTool {
    pub fn new(definition: ToolDefinition, context_factory: Option<ToolContextFactory>) -> Self {
        Self {
            definition,
            context_factory,
        }
    }
}

impl AgentTool for DefinitionTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        self.definition.description.clone()
    }

    fn parameters(&self) -> &Value {
        &self.definition.parameters
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        self.definition.label.clone()
    }

    fn execution_mode(&self) -> Option<pi_agent_core::harness::types::ToolExecutionMode> {
        self.definition.execution_mode
    }

    fn prepare_arguments(&self, args: JsValue) -> Result<JsValue, ToolError> {
        match &self.definition.prepare_arguments {
            Some(prepare) => prepare(&args).map_err(|error| ToolError::new(error.message)),
            None => Ok(args),
        }
    }

    fn prepare_execution(&self) -> Option<pi_agent_core::agent::types::AgentToolPrepareExecution> {
        let prepare = self.definition.prepare_execution.clone()?;
        let context_factory = self.context_factory.clone();
        Some(Arc::new(move |tool_call_id, signal| {
            let prepare = Arc::clone(&prepare);
            let context = context_factory.as_ref().map(|factory| factory());
            Box::pin(async move {
                prepare(tool_call_id, signal, context)
                    .await
                    .map_err(|error| ToolError::new(error.message))
            })
        }))
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        signal: Option<pi_js_compat::abort::AbortSignal>,
        on_update: pi_agent_core::agent::AgentToolUpdateCallback,
        execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let definition = self.definition.clone();
        let context = create_tool_execution_context(self.context_factory.as_ref(), execution.as_ref());
        let spill = Arc::new(Mutex::new(ToolOutputSpill::default()));
        let update_spill = Arc::clone(&spill);
        let capped_update: pi_agent_core::agent::AgentToolUpdateCallback = Arc::new(move |partial| {
            let capped = cap_tool_result_output(partial, &mut update_spill.lock().expect("tool output spill lock"))?;
            (on_update)(capped)
        });
        Box::pin(async move {
            let result = (definition.execute)(tool_call_id, params, signal, Some(capped_update), context)
                .await
                .map_err(|error| project_callback_tool_error(error.into()))?;
            let mut spill = spill.lock().expect("tool output spill lock");
            cap_tool_result_output(result, &mut spill)
        })
    }
}

/// Frozen agent-loop.ts:845-850 projects Error.message or String(rejection).
pub(super) fn project_callback_tool_error(error: pi_js_compat::callback_error::CallbackError) -> ToolError {
    match error.error_or_string_message() {
        Ok(message) => ToolError::new(message),
        // Approved ingress (c) gap, 2026-10-24: ToolError cannot carry a loop-fatal
        // coercion rejection. Return that TypeError explicitly, never exit the process.
        Err(conversion_error) => ToolError::new(conversion_error.message),
    }
}

/// Wrap one definition into an [`AgentTool`] with the shared result cap.
pub fn wrap_definition(definition: ToolDefinition, context_factory: Option<ToolContextFactory>) -> Arc<dyn AgentTool> {
    Arc::new(DefinitionTool::new(definition, context_factory))
}

/// Wrap definitions into [`AgentTool`]s with the shared result cap.
pub fn wrap_definitions(
    definitions: Vec<ToolDefinition>,
    context_factory: Option<ToolContextFactory>,
) -> Vec<Arc<dyn AgentTool>> {
    definitions
        .into_iter()
        .map(|definition| wrap_definition(definition, context_factory.clone()))
        .collect()
}

/// `wrapToolDefinition`: cap partial updates and the final result of an [`AgentTool`].
pub struct CappedTool {
    inner: Arc<dyn AgentTool>,
}

impl CappedTool {
    pub fn new(inner: Arc<dyn AgentTool>) -> Self {
        Self { inner }
    }
}

impl AgentTool for CappedTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        self.inner.description()
    }

    fn parameters(&self) -> &Value {
        self.inner.parameters()
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        self.inner.label()
    }

    fn execution_mode(&self) -> Option<pi_agent_core::harness::types::ToolExecutionMode> {
        self.inner.execution_mode()
    }

    fn prepare_arguments(&self, args: JsValue) -> Result<JsValue, pi_agent_core::agent::ToolError> {
        self.inner.prepare_arguments(args)
    }

    fn prepare_execution(&self) -> Option<pi_agent_core::agent::types::AgentToolPrepareExecution> {
        self.inner.prepare_execution()
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        signal: Option<pi_js_compat::abort::AbortSignal>,
        on_update: pi_agent_core::agent::AgentToolUpdateCallback,
        execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, pi_agent_core::agent::ToolError>> {
        let spill = Arc::new(Mutex::new(ToolOutputSpill::default()));
        let update_spill = Arc::clone(&spill);
        let capped_update: pi_agent_core::agent::AgentToolUpdateCallback = Arc::new(move |partial| {
            let capped = cap_tool_result_output(partial, &mut update_spill.lock().expect("tool output spill lock"))?;
            (on_update)(capped)
        });
        let result = self
            .inner
            .execute(tool_call_id, params, signal, capped_update, execution);
        Box::pin(async move {
            let result = result.await?;
            let mut spill = spill.lock().expect("tool output spill lock");
            cap_tool_result_output(result, &mut spill)
        })
    }
}

/// Wrap a tool so its results are capped like `wrapToolDefinition` does.
pub fn wrap_agent_tool(inner: Arc<dyn AgentTool>) -> Arc<dyn AgentTool> {
    Arc::new(CappedTool::new(inner))
}

fn has_existing_output_details(details: Option<&JsValue>) -> bool {
    match details {
        Some(JsValue::Object(details)) => details.contains_key("fullOutputPath") || details.contains_key("truncation"),
        _ => false,
    }
}

fn merge_output_details(
    details: Option<JsValue>,
    truncation: &super::truncate::TruncationResult,
    full_output_path: &JsString,
) -> JsValue {
    let mut merged = match details {
        Some(JsValue::Object(details)) => details,
        Some(JsValue::Array(details)) => details
            .into_iter()
            .enumerate()
            .map(|(index, value)| (index.to_string().into(), value))
            .collect(),
        _ => JsObject::new(),
    };
    merged.insert(
        "truncation".into(),
        pi_js_compat::js_json::to_value(truncation).expect("truncation metadata must serialize"),
    );
    merged.insert("fullOutputPath".into(), JsValue::String(full_output_path.into()));
    JsValue::Object(merged)
}

fn create_full_output_path() -> Result<JsString, ToolError> {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes).map_err(|error| ToolError::new(error.to_string()))?;
    let id = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    Ok(crate::utils::paths::node_join(
        pi_js_compat::node_os::tmpdir(),
        format!("pi-tool-{id}.log"),
    ))
}
