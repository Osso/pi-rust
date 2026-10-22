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
///
/// A failed partial-update spill is recorded here: the update callback cannot
/// return an error, so the failure surfaces from the final `execute`, where the
/// TypeScript `writeFileSync` throw would have aborted the tool call.
#[derive(Debug, Default)]
pub struct ToolOutputSpill {
    pub full_output_path: Option<JsString>,
    pub error: Option<JsString>,
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

    let full_output_path = spill
        .full_output_path
        .get_or_insert_with(create_full_output_path)
        .clone();
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
    let prepare_execution_tool = Arc::clone(&tool);
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
        prepare_arguments: Some(Arc::new(move |args: &JsValue| {
            prepare_arguments_tool
                .prepare_arguments(args.clone())
                .map_err(|error| HandlerError::new(error.message))
        })),
        prepare_execution: Some(Arc::new(move |tool_call_id, signal, _context| {
            let tool = Arc::clone(&prepare_execution_tool);
            Box::pin(async move {
                tool.prepare_execution(tool_call_id, signal)
                    .await
                    .map_err(|error| HandlerError::new(error.message))
            })
        })),
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

/// Supplies the extension context for a wrapped definition (`ctxFactory`).
pub type ToolContextFactory = Arc<dyn Fn() -> ExtensionContext + Send + Sync>;

/// `createToolExecutionContext`: the base context plus the execution identity.
pub fn create_tool_execution_context(
    factory: &ToolContextFactory,
    execution: Option<&AgentToolExecutionContext>,
) -> ExtensionContext {
    let mut context = factory();
    context.tool_execution_started_at = execution.map(|execution| execution.started_at);
    context.tool_execution_agent_id = execution.and_then(|execution| execution.agent_id.clone());
    context
}

/// `wrapToolDefinition`: a [`ToolDefinition`] exposed as an [`AgentTool`].
pub struct DefinitionTool {
    definition: ToolDefinition,
    context_factory: ToolContextFactory,
}

impl DefinitionTool {
    pub fn new(definition: ToolDefinition, context_factory: ToolContextFactory) -> Self {
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

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters(&self) -> &Value {
        &self.definition.parameters
    }

    fn label(&self) -> &str {
        &self.definition.label
    }

    fn constrained_sampling(&self) -> Option<pi_ai::types::ConstrainedSampling> {
        self.definition.constrained_sampling.clone()
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

    fn prepare_execution(
        &self,
        tool_call_id: String,
        signal: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<'static, Result<Option<pi_ai::types::ToolCallExecution>, ToolError>> {
        let Some(prepare) = self.definition.prepare_execution.clone() else {
            return Box::pin(async { Ok(None) });
        };
        let context = (self.context_factory)();
        Box::pin(async move {
            prepare(tool_call_id, signal, context)
                .await
                .map_err(|error| ToolError::new(error.message))
        })
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        signal: Option<tokio_util::sync::CancellationToken>,
        on_update: pi_agent_core::agent::AgentToolUpdateCallback,
        execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let definition = self.definition.clone();
        let context = create_tool_execution_context(&self.context_factory, execution.as_ref());
        let spill = Arc::new(Mutex::new(ToolOutputSpill::default()));
        let update_spill = Arc::clone(&spill);
        let capped_update: pi_agent_core::agent::AgentToolUpdateCallback = Arc::new(move |partial| {
            let mut spill = update_spill.lock().expect("tool output spill lock");
            match cap_tool_result_output(partial, &mut spill) {
                Ok(capped) => (on_update)(capped),
                Err(error) => spill.error = Some(error.message),
            }
        });
        Box::pin(async move {
            let result = (definition.execute)(tool_call_id, params, signal, Some(capped_update), context)
                .await
                .map_err(|error| ToolError::new(error.message))?;
            let mut spill = spill.lock().expect("tool output spill lock");
            if let Some(message) = spill.error.take() {
                return Err(ToolError::new(message));
            }
            cap_tool_result_output(result, &mut spill)
        })
    }
}

/// Wrap one definition into an [`AgentTool`] with the shared result cap.
pub fn wrap_definition(definition: ToolDefinition, context_factory: ToolContextFactory) -> Arc<dyn AgentTool> {
    Arc::new(DefinitionTool::new(definition, context_factory))
}

/// Wrap definitions into [`AgentTool`]s with the shared result cap.
pub fn wrap_definitions(
    definitions: Vec<ToolDefinition>,
    context_factory: ToolContextFactory,
) -> Vec<Arc<dyn AgentTool>> {
    definitions
        .into_iter()
        .map(|definition| wrap_definition(definition, Arc::clone(&context_factory)))
        .collect()
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

    fn prepare_arguments(&self, args: JsValue) -> Result<JsValue, pi_agent_core::agent::ToolError> {
        self.inner.prepare_arguments(args)
    }

    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        signal: Option<tokio_util::sync::CancellationToken>,
        on_update: pi_agent_core::agent::AgentToolUpdateCallback,
        execution: Option<pi_agent_core::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, pi_agent_core::agent::ToolError>> {
        let spill = Arc::clone(&self.spill);
        let update_spill = Arc::clone(&spill);
        let capped_update: pi_agent_core::agent::AgentToolUpdateCallback = Arc::new(move |partial| {
            let mut spill = update_spill.lock().expect("tool output spill lock");
            match cap_tool_result_output(partial, &mut spill) {
                Ok(capped) => (on_update)(capped),
                Err(error) => spill.error = Some(error.message),
            }
        });
        let result = self
            .inner
            .execute(tool_call_id, params, signal, capped_update, execution);
        Box::pin(async move {
            let result = result.await?;
            let mut spill = spill.lock().expect("tool output spill lock");
            if let Some(message) = spill.error.take() {
                return Err(ToolError::new(message));
            }
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
        _ => JsObject::new(),
    };
    merged.insert(
        "truncation".into(),
        pi_js_compat::js_json::to_value(truncation).expect("truncation metadata must serialize"),
    );
    merged.insert("fullOutputPath".into(), JsValue::String(full_output_path.into()));
    JsValue::Object(merged)
}

fn create_full_output_path() -> JsString {
    let id = pi_ai::utils::random_uuid::random_uuid().replace('-', "");
    let id = &id[..16];
    crate::utils::paths::node_join(
        std::env::temp_dir().to_string_lossy().into_owned(),
        format!("pi-tool-{id}.log"),
    )
}
